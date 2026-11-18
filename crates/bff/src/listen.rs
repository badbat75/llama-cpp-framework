//! The address the bff listens on, from its own config file: any `ip:port` the
//! administrator chose, the machine's LAN address and the wildcard included,
//! served plain HTTP.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use windows_sys::Win32::System::SystemInformation::{
    ComputerNameDnsDomain, ComputerNameDnsHostname, GetComputerNameExW, COMPUTER_NAME_FORMAT,
};

/// Why a configured address is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenError {
    /// `Listen` is not an `ip:port` address (the value as written).
    Invalid(String),
    /// The file exists but cannot be read (the IO error).
    Unreadable(String),
}

/// The bff's config file.
pub fn config_path() -> PathBuf {
    pai_core::bff_cfg::config_path()
}

/// The address served when `bff.ini` sets none.
const DEFAULT: SocketAddr = pai_core::bff_cfg::DEFAULT_LISTEN;

/// The address an INI text sets: an `ip:port`, never a host name. The read and
/// the parse are `pai_core`'s, so the manager, which judges a server address
/// against this service's address, reads the same value.
pub fn parse(text: &str) -> Result<SocketAddr, ListenError> {
    pai_core::bff_cfg::parse_listen(text).map_err(ListenError::Invalid)
}

/// The `[Bff] Hosts` value of an INI text, as written: the names the `Host`
/// guard answers beside `localhost` and the IP literals, comma-separated. The
/// machine's own names are used when the file sets none, so a value that is
/// empty is no value.
pub fn parse_hosts(text: &str) -> Option<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    pai_core::ini::parse_sections(text)
        .into_iter()
        .filter(|s| s.id.eq_ignore_ascii_case("Bff"))
        .find_map(|s| s.keys.get("Hosts").cloned())
        .filter(|value| !value.trim().is_empty())
}

/// One name as the guard's list holds it: trimmed, lowercased, and with one
/// trailing dot removed, the root of the DNS tree a fully qualified name
/// carries, so no entry of the list ends with a dot.
fn list_name(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase();
    name.strip_suffix('.').unwrap_or(&name).to_string()
}

/// The domains of the `search` and `domain` lines of a `resolv.conf` text, in
/// the order they come, each one as `list_name` gives it: a comment in either
/// spelling, a blank line and every other keyword are ignored, a line whose
/// keyword has no domain after it gives none, and a domain that is only the
/// root `.` adds nothing.
fn search_domains(resolv_conf: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in resolv_conf.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let mut words = line.split_whitespace();
        if !matches!(words.next(), Some("search") | Some("domain")) {
            continue;
        }
        out.extend(words.map(list_name).filter(|domain| !domain.is_empty()));
    }
    out
}

/// The names the `Host` guard answers beside `localhost` and the IP literals,
/// from the machine's own facts: `hostname`, `<hostname>.local` and
/// `<hostname>.<domain>` for each `search`/`domain` of `resolv_conf`; `hosts`,
/// when it names at least one name, replaces them, an empty value setting none
/// as `parse_hosts` gives none for it. A host name that is empty once trimmed
/// names no machine, so it builds no name at all, and the `hosts` list applies
/// whatever the host name is. Every name is lowercase and carries no trailing
/// dot, as `list_name` gives it, and the guard reads the `Host` line in any
/// case and takes one trailing dot off the name it reads (`origin.rs`). Sorted
/// without duplicates: the guard answers a name that is in the list, so the
/// list is a set and its order is not the contract.
pub fn host_names(hostname: &str, resolv_conf: &str, hosts: Option<&str>) -> Vec<String> {
    if let Some(names) = hosts.map(split_hosts).filter(|names| !names.is_empty()) {
        return names;
    }
    let name = list_name(hostname);
    if name.is_empty() {
        return Vec::new();
    }
    let mut out = vec![name.clone(), format!("{name}.local")];
    out.extend(
        search_domains(resolv_conf)
            .into_iter()
            .map(|domain| format!("{name}.{domain}")),
    );
    out.sort();
    out.dedup();
    out
}

/// A `[Bff] Hosts` value as the list of names it is: split at the commas, each
/// name as `list_name` gives it (trimmed of spaces and tabs, lowercased, its
/// trailing dot removed), an empty item dropped.
fn split_hosts(hosts: &str) -> Vec<String> {
    hosts
        .split(',')
        .map(list_name)
        .filter(|name| !name.is_empty())
        .collect()
}

/// One of this computer's names as `GetComputerNameExW` answers it for `kind`
/// (the DNS host name, the DNS domain): the UTF-16 text the call wrote in a
/// buffer of the length a DNS name is capped at, without its terminating NUL.
/// None when the call fails.
#[cfg(windows)]
fn computer_name_ex(kind: COMPUTER_NAME_FORMAT) -> Option<String> {
    // A DNS name is at most 255 characters, plus the NUL the call writes.
    let mut name = vec![0u16; 256];
    let mut len = name.len() as u32;
    // SAFETY: a buffer of the length given.
    if unsafe { GetComputerNameExW(kind, name.as_mut_ptr(), &mut len) } == 0 {
        return None;
    }
    let len = (len as usize).min(name.len());
    Some(String::from_utf16_lossy(&name[..len]))
}

/// This machine's host name, by system call: `gethostname(2)` on Unix,
/// `GetComputerNameExW(ComputerNameDnsHostname)` on Windows. Empty when the
/// call fails: the guard is then handed no name of the machine, and answers
/// `localhost` and the addresses under the listen rule (`origin.rs`) and the
/// names of `bff.ini`.
pub fn hostname() -> String {
    #[cfg(unix)]
    {
        nix::unistd::gethostname()
            .map(|name| name.to_string_lossy().trim().to_string())
            .unwrap_or_default()
    }
    #[cfg(windows)]
    {
        computer_name_ex(ComputerNameDnsHostname).unwrap_or_default()
    }
}

/// The system's resolver configuration, as the `resolv.conf` text `host_names`
/// reads: `/etc/resolv.conf` on Unix, the DNS domain of a machine having no
/// system call there (`getdomainname(2)` is the NIS domain, not a DNS one). On
/// Windows the machine's DNS domain, so the text handed to `host_names` is
/// `search <domain>` when `GetComputerNameExW(ComputerNameDnsDomain)` answers a
/// domain, and empty when it answers none or fails. Empty text gives no domain.
pub fn resolv_conf() -> String {
    #[cfg(unix)]
    {
        std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default()
    }
    #[cfg(windows)]
    {
        match computer_name_ex(ComputerNameDnsDomain) {
            Some(domain) if !domain.is_empty() => format!("search {domain}\n"),
            _ => String::new(),
        }
    }
}

/// The address the file at `path` sets.
pub fn load(path: &Path) -> Result<SocketAddr, ListenError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DEFAULT),
        Err(e) => Err(ListenError::Unreadable(e.to_string())),
    }
}

/// The `[Bff] Hosts` of the file at `path`: `None` when it is missing, cannot
/// be read, or sets no value. A file that cannot be read is no refusal here:
/// the machine's own names stand, and `load` is the call that reports such a
/// file to the service.
pub fn load_hosts(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| parse_hosts(&text))
}
