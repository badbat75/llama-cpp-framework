//! The versions the packages installed: one file per package,
//! `versions/<package>`, holding that package's own version on one line
//! (slice 13, `ARCHITECTURE.md`). The bff reads them for the web UI's About
//! place; the packages are `xtask package`'s (`components::of` gives each
//! file to the package whose `package()` it is).
//!
//! The directory is root's (`Program Files` on Windows), the same trust as the
//! binaries beside it.

use std::io::Read;
use std::path::Path;

/// The largest a version file is: a package's version is a line, and a file
/// over this is not one of the package's files.
const MAX_FILE_BYTES: usize = 128;

/// One GPU runtime of an engine, as its package's file names it: `cuda`,
/// `rocm`, `vulkan`. The web UI shows the name with its own string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    pub name: String,
    /// The version the file's first line holds; `None` for an empty file.
    pub version: Option<String>,
}

/// One engine and the runtimes installed with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    /// The engine's package name: `llama.cpp`.
    pub engine: String,
    pub version: Option<String>,
    /// The runtimes, sorted by name.
    pub backends: Vec<Backend>,
}

/// The engines installed under `dir`: every regular file named `llama.cpp` or
/// `llama.cpp-<a-z0-9>+`, at most 128 bytes, its first line trimmed and
/// printable ASCII, else the entry is skipped; an empty file is no version.
/// The engine is listed only when its own `llama.cpp` file is there, and its
/// backends are the other files. No directory, no engine: a tree build, an
/// older package, no engine installed. Read at each request, so an engine
/// upgrade shows without a restart of the reader.
pub fn installed(dir: &Path) -> Vec<Engine> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut version: Option<Option<String>> = None;
    let mut backends: Vec<Backend> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let backend = match backend_of(&name) {
            Some(backend) => backend,
            None => continue,
        };
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let version_read = match read_version(&entry.path()) {
            Some(version) => version,
            None => continue,
        };
        match backend {
            None => version = Some(version_read),
            Some(name) => backends.push(Backend {
                name: name.to_string(),
                version: version_read,
            }),
        }
    }
    let version = match version {
        Some(version) => version,
        None => return Vec::new(),
    };
    backends.sort_by(|a, b| a.name.cmp(&b.name));
    vec![Engine {
        engine: ENGINE.to_string(),
        version,
        backends,
    }]
}

/// The engine's package name, the one the files are named after.
const ENGINE: &str = "llama.cpp";

/// `Some(None)` is the engine's own file, `Some(Some(name))` the runtime
/// `name` of the engine, `None` a file that is not a package of the engine.
fn backend_of(name: &str) -> Option<Option<&str>> {
    if name == ENGINE {
        return Some(None);
    }
    let rest = name.strip_prefix(ENGINE)?.strip_prefix('-')?;
    if rest.is_empty()
        || !rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        return None;
    }
    Some(Some(rest))
}

/// The version a package's file holds: `Some(Some(text))` is its first line
/// trimmed, `Some(None)` is an empty file, `None` is not a version file (too
/// long, a line that is not printable ASCII, unreadable). At most
/// `MAX_FILE_BYTES` + 1 bytes are taken from it: the extra one is what shows a
/// file longer than the cap, without reading the file.
fn read_version(path: &Path) -> Option<Option<String>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return None,
    };
    let mut bytes: Vec<u8> = Vec::new();
    if file
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return None;
    }
    if bytes.len() > MAX_FILE_BYTES {
        return None;
    }
    let line: Vec<u8> = bytes.iter().take_while(|b| **b != b'\n').copied().collect();
    let text = String::from_utf8_lossy(&line).trim().to_string();
    if text.is_empty() {
        return Some(None);
    }
    if !text.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
        return None;
    }
    Some(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding the files named, each with its text.
    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).expect("file");
        }
        dir
    }

    fn engine(name: &str, version: Option<&str>, backends: Vec<(&str, Option<&str>)>) -> Engine {
        Engine {
            engine: name.to_string(),
            version: version.map(str::to_string),
            backends: backends
                .into_iter()
                .map(|(name, version)| Backend {
                    name: name.to_string(),
                    version: version.map(str::to_string),
                })
                .collect(),
        }
    }

    /// The engine with the runtimes the packages shipped, the versions as the
    /// packages render them (epoch, `+` suffixes).
    #[test]
    fn the_engine_and_the_runtimes_of_its_directory() {
        let dir = dir_with(&[
            ("llama.cpp", "1:1.0.0+pai2\n"),
            ("llama.cpp-vulkan", "1.4.363.1"),
            ("llama.cpp-cuda", "13.5.0+nccl.2.32.3\n"),
            ("llama.cpp-rocm", "10.1.0\n"),
        ]);
        assert_eq!(
            installed(dir.path()),
            vec![engine(
                "llama.cpp",
                Some("1:1.0.0+pai2"),
                vec![
                    ("cuda", Some("13.5.0+nccl.2.32.3")),
                    ("rocm", Some("10.1.0")),
                    ("vulkan", Some("1.4.363.1")),
                ]
            )],
            "the runtimes sorted by name, whatever order the directory gives"
        );
    }

    /// A CPU-only engine: its file alone, no runtime.
    #[test]
    fn an_engine_alone_has_no_backend() {
        let dir = dir_with(&[("llama.cpp", "1:1.0.0+pai2\n")]);
        assert_eq!(
            installed(dir.path()),
            vec![engine("llama.cpp", Some("1:1.0.0+pai2"), vec![])]
        );
    }

    /// A runtime with no engine file is no engine: the packages of a runtime
    /// sit where an engine of the same build is installed, and a directory
    /// with only a runtime names no version to show.
    #[test]
    fn a_runtime_without_the_engine_file_is_no_engine() {
        let dir = dir_with(&[("llama.cpp-cuda", "13.5.0\n")]);
        assert_eq!(installed(dir.path()), Vec::<Engine>::new());
    }

    /// No directory at all: a tree build, an older package, no engine
    /// installed.
    #[test]
    fn no_directory_is_no_engine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("versions");
        assert_eq!(installed(&missing), Vec::<Engine>::new());
        let empty = dir_with(&[]);
        assert_eq!(installed(empty.path()), Vec::<Engine>::new());
    }

    /// An empty file is the entry with no version: the package is there, its
    /// version is not known.
    #[test]
    fn an_empty_file_is_an_entry_with_no_version() {
        let dir = dir_with(&[("llama.cpp", ""), ("llama.cpp-rocm", "")]);
        assert_eq!(
            installed(dir.path()),
            vec![engine("llama.cpp", None, vec![("rocm", None)])]
        );
    }

    /// The first line, trimmed, is the version: a trailing newline, a blank
    /// line after it, spaces around it.
    #[test]
    fn the_first_line_trimmed_is_the_version() {
        let dir = dir_with(&[
            ("llama.cpp", "  1:1.0.0+pai2  \nthe package version\n"),
            ("llama.cpp-rocm", "\t10.1.0\t\r\n"),
        ]);
        assert_eq!(
            installed(dir.path()),
            vec![engine(
                "llama.cpp",
                Some("1:1.0.0+pai2"),
                vec![("rocm", Some("10.1.0"))]
            )]
        );
    }

    /// A file over the limit is not a version file, and a file of exactly the
    /// limit is.
    #[test]
    fn a_file_over_128_bytes_is_skipped() {
        let over = "1".repeat(129);
        let at = "1".repeat(128);
        let dir = dir_with(&[
            ("llama.cpp", &at),
            ("llama.cpp-cuda", &over),
            ("llama.cpp-rocm", "10.1.0\n"),
        ]);
        assert_eq!(
            installed(dir.path()),
            vec![engine(
                "llama.cpp",
                Some(at.as_str()),
                vec![("rocm", Some("10.1.0"))]
            )],
            "the oversized runtime is left out, the engine stays"
        );
    }

    /// A first line that is not printable ASCII is not a version.
    #[test]
    fn a_version_that_is_not_printable_ascii_is_skipped() {
        let dir = dir_with(&[
            ("llama.cpp", "1:1.0.0"),
            ("llama.cpp-cuda", "13.5.0\u{7}"),
            ("llama.cpp-rocm", "10.1.0\n\t\n"),
            ("llama.cpp-vulkan", "1.4.363.1+nö"),
        ]);
        assert_eq!(
            installed(dir.path()),
            vec![engine(
                "llama.cpp",
                Some("1:1.0.0"),
                vec![("rocm", Some("10.1.0"))]
            )],
            "the bell, the tab on its own line and the accented byte are not versions"
        );
    }

    /// A name that is not a package of the engine is not an entry, and
    /// neither is a directory: the rule is `llama.cpp` and
    /// `llama.cpp-<a-z0-9>+`.
    #[test]
    fn a_foreign_name_is_skipped() {
        let dir = dir_with(&[
            ("llama.cpp", "1:1.0.0"),
            ("private-ai", "2.99.1"),
            ("llama.cpp-CUDA", "13.5.0"),
            ("llama.cpp-", "1"),
            ("llama.cpp-cuda.deb", "1"),
            ("llama.cpp_cuda", "1"),
            ("README", "notes"),
        ]);
        std::fs::create_dir(dir.path().join("llama.cpp-vulkan")).expect("directory");
        assert_eq!(
            installed(dir.path()),
            vec![engine("llama.cpp", Some("1:1.0.0"), vec![])],
            "only the engine's own file is an entry"
        );
    }

    /// A file that cannot be read is left out: the reader has no error to
    /// give the web UI, so the entry is missing and the rest is shown. The
    /// mode of a file is Unix's, so this is a Unix test.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_is_left_out() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir_with(&[("llama.cpp", "1:1.0.0"), ("llama.cpp-rocm", "10.1.0")]);
        std::fs::set_permissions(
            dir.path().join("llama.cpp-rocm"),
            std::fs::Permissions::from_mode(0o000),
        )
        .expect("chmod");
        let got = installed(dir.path());
        if std::fs::read(dir.path().join("llama.cpp-rocm")).is_err() {
            assert_eq!(got, vec![engine("llama.cpp", Some("1:1.0.0"), vec![])]);
        }
        std::fs::set_permissions(
            dir.path().join("llama.cpp-rocm"),
            std::fs::Permissions::from_mode(0o644),
        )
        .expect("chmod back");
    }

    /// `bytes` at `rel`, the file grown to `len` after that. The bytes are
    /// sparse: the length is what the reader must judge, not what is on disk.
    fn grown(dir: &Path, rel: &str, bytes: &[u8], len: u64) {
        let path = dir.join(rel);
        std::fs::write(&path, bytes).expect("write");
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open")
            .set_len(len)
            .expect("set_len");
    }

    /// A runtime file of megabytes is left out, the engine stays: the reader
    /// has no reason to read a file of that size, it is not a package's
    /// version.
    #[test]
    fn a_file_of_megabytes_is_left_out() {
        let dir = dir_with(&[("llama.cpp", "1:1.0.0")]);
        grown(dir.path(), "llama.cpp-cuda", b"", 4 * 1024 * 1024);
        assert_eq!(
            installed(dir.path()),
            vec![engine("llama.cpp", Some("1:1.0.0"), vec![])],
            "the megabyte file is left out, not read"
        );
    }

    /// A megabyte file that STARTS with a version is still left out: the
    /// bound is the file's length, so a reader that read only its first
    /// bytes and parsed them would take this for CUDA's version.
    #[test]
    fn the_length_is_the_bound_even_when_the_first_line_is_a_version() {
        let dir = dir_with(&[("llama.cpp", "1:1.0.0")]);
        grown(
            dir.path(),
            "llama.cpp-vulkan",
            b"1.4.363.1\n",
            4 * 1024 * 1024,
        );
        assert_eq!(
            installed(dir.path()),
            vec![engine("llama.cpp", Some("1:1.0.0"), vec![])],
            "the length over the cap is what skips it, not the content"
        );
    }

    /// The reader is bounded: no version file is read whole, at most
    /// `MAX_FILE_BYTES` + 1 bytes are taken from it. The number of bytes a
    /// read takes is not visible from `installed`, so this pins the shape of
    /// the code; the behaviour it must keep is pinned above. Comments are
    /// stripped, a needle in a comment is not a hit.
    #[test]
    fn a_version_file_is_never_read_whole() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/versions.rs"))
            .expect("the reader's source");
        let prod = text
            .split("#[cfg(test)]")
            .next()
            .expect("the reader above its tests")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(line))
            .collect::<Vec<&str>>()
            .join("\n");
        assert!(
            !prod.contains("fs::read("),
            "the version file is read whole: open it and take at most {} bytes",
            MAX_FILE_BYTES + 1
        );
        assert!(
            prod.contains(".take("),
            "the read is not bounded: std::io::Read::take to {} bytes",
            MAX_FILE_BYTES + 1
        );
    }
}
