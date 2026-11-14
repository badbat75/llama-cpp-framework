//! The address the bff listens on, from its own config file.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Why a configured address is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenError {
    /// `Listen` is not an `ip:port` address (the value as written).
    Invalid(String),
    /// Not a loopback address: LAN access needs authentication first.
    NotLoopback(SocketAddr),
    /// The file exists but cannot be read (the IO error).
    Unreadable(String),
}

/// The bff's config file.
pub fn config_path() -> PathBuf {
    pai_core::paths::system_config_dir().join("bff.ini")
}

/// The address served when `bff.ini` sets none.
const DEFAULT: SocketAddr = SocketAddr::V4(std::net::SocketAddrV4::new(
    std::net::Ipv4Addr::LOCALHOST,
    8590,
));

/// The address an INI text sets.
pub fn parse(text: &str) -> Result<SocketAddr, ListenError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value = pai_core::ini::parse_sections(text)
        .into_iter()
        .filter(|s| s.id.eq_ignore_ascii_case("Bff"))
        .find_map(|s| s.keys.get("Listen").cloned());
    let Some(value) = value else {
        return Ok(DEFAULT);
    };
    let addr: SocketAddr = value.parse().map_err(|_| ListenError::Invalid(value))?;
    if !addr.ip().is_loopback() {
        return Err(ListenError::NotLoopback(addr));
    }
    Ok(addr)
}

/// The address the file at `path` sets.
pub fn load(path: &Path) -> Result<SocketAddr, ListenError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT),
        Err(e) => Err(ListenError::Unreadable(e.to_string())),
    }
}
