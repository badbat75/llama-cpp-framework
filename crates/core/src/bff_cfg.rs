//! The web service's own address: the one place it is parsed, so that the
//! manager, which judges a server address against it, reads the same value
//! the service itself listens at.
//!
//! The service owns its file (`bff.ini`, in the system config directory) and
//! reads it with a `load` of its own (`crates/bff/src/listen.rs`), which
//! parses through [`parse_listen`] and reports a file it cannot read, or a
//! `Listen` it cannot parse, as its own failure: `pai-bff`'s `ListenError`.
//! [`listen_at`] is the manager's read of that same file.
//! The manager asks the same questions: whose address is in use is the
//! service's address. Where there is no file, there is no answer, and the
//! default is the right one: the service uses it too. A file that cannot be
//! read, or a value that cannot be parsed, answers nothing: the service will
//! not start on that file, and no address can be claimed from it.

use std::net::SocketAddr;
use std::path::Path;

/// The address the service listens at when the file says nothing: the loopback
/// port it was given. It is a [`std::net::SocketAddr`], so a value read from
/// the file and this one are the same kind, and the default answers the same
/// questions as a value that is there.
pub const DEFAULT_LISTEN: SocketAddr = SocketAddr::V4(std::net::SocketAddrV4::new(
    std::net::Ipv4Addr::LOCALHOST,
    8590,
));

/// The service's file: `bff.ini`, in the system config directory (it is the
/// service's own, so it stays there, not in the engine's tree).
pub fn config_path() -> std::path::PathBuf {
    crate::paths::system_config_dir().join("bff.ini")
}

/// The address an INI text sets: the `Listen` of its `[Bff]` section, an
/// `ip:port`, never a host name. No section, no key: the default, what the
/// service listens at when the file is silent. A key with nothing after the
/// `=` is a value, and a value that is not an address is refused.
///
/// The error is the value as written, which is not a socket address: the
/// caller puts it in a message of its own, the service naming its key.
pub fn parse_listen(text: &str) -> Result<SocketAddr, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value = crate::ini::parse_sections(text)
        .into_iter()
        .filter(|s| s.id.eq_ignore_ascii_case("Bff"))
        .find_map(|s| s.keys.get("Listen").cloned());
    let Some(value) = value else {
        return Ok(DEFAULT_LISTEN);
    };
    value.parse().map_err(|_| value)
}

/// The address the service listens at, read from `path`: the value's, the
/// file's, or the default when there is no file. `None` when the file cannot
/// be read, or names no address: nothing can be judged from a file the service
/// itself refuses to start on.
pub fn listen_at(path: &Path) -> Option<SocketAddr> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_listen(&text).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(DEFAULT_LISTEN),
        Err(_) => None,
    }
}

/// The address the service listens at, at its own file.
pub fn listen() -> Option<SocketAddr> {
    listen_at(&config_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file that is really absent is the default address: with no file the
    /// service listens at the default, so the default is the right answer. The
    /// file itself missing and a directory of the path missing are the same
    /// "no such file".
    #[test]
    fn no_file_is_the_default_address() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            listen_at(&dir.path().join("bff.ini")),
            Some(DEFAULT_LISTEN),
            "no bff.ini: the service is on the default address"
        );
        assert_eq!(
            listen_at(&dir.path().join("gone").join("bff.ini")),
            Some(DEFAULT_LISTEN),
            "a directory of the path is missing: the same answer"
        );
    }

    /// A file that cannot be read holds no address, the service answering by
    /// not starting: `crates/bff/src/listen.rs` reports the same file as its
    /// own `Unreadable` failure, so the manager, which reads through here,
    /// claims nothing for it. The file is a directory.
    #[test]
    fn a_file_that_cannot_be_read_holds_no_address() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bff.ini");
        std::fs::create_dir(&path).expect("a directory where the file should be");
        assert_eq!(
            listen_at(&path),
            None,
            "a directory is no file to read: no address"
        );
    }

    /// The same for a path whose parent is a regular file: the read fails
    /// because a component of it is no directory, and `Path::exists` is false
    /// for such a path, so "no file, so the default" is the wrong reading: the
    /// service will not start on it either.
    #[test]
    fn a_path_under_a_regular_file_holds_no_address() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blocker = dir.path().join("config");
        std::fs::write(&blocker, "[Bff]\nListen = 127.0.0.1:8590\n").expect("regular file");
        let path = blocker.join("bff.ini");
        assert!(
            !path.exists(),
            "fixture: the reader takes this path as no file"
        );
        assert_eq!(
            listen_at(&path),
            None,
            "a path through a regular file is unreadable, not absent"
        );
    }
}
