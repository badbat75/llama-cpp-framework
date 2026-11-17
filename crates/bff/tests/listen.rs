//! The address the bff listens on: `Listen` in the `[Bff]` section of
//! `bff.ini`, any address the administrator chooses, the LAN and the
//! wildcard included, served plain HTTP. `parse` reads the INI text, `load`
//! the file at a path it is given (a tempdir here, never the real system
//! config).
//!
//! Beside the address, the names the `Host` guard answers: `parse_hosts` reads
//! the `Hosts` of the same section, and `host_names` builds the machine's own
//! names from a host name and a `resolv.conf` text, both given here as
//! fixtures: a test never reads the real host name nor `/etc/resolv.conf`. A
//! host name that is blank builds no name, and a `search` domain written as a
//! fully qualified name, with its trailing dot, builds the name without it.

use pai_bff::listen::{self, ListenError};
use std::net::SocketAddr;

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("a socket address")
}

fn default_addr() -> SocketAddr {
    addr("127.0.0.1:8590")
}

// ── Failure paths first ─────────────────────────────────────────────────

/// A value that is not `ip:port` is refused with the value as written: no
/// host name lookup, no default port, no port out of range, no empty value.
/// A name is refused wherever it sits, loopback included: accepting a LAN
/// address is not a name lookup.
#[test]
fn parse_refuses_a_value_that_is_not_an_address() {
    for value in [
        "not-an-address",
        "localhost:8590",
        "mypc.local:8590",
        "evil.example:8590",
        "HTTP://127.0.0.1:8590",
        "127.0.0.1",
        "0.0.0.0",
        "8590",
        "127.0.0.1:70000",
        "127.0.0.1:-1",
        "127.0.0.1:port",
        "",
    ] {
        let text = format!("[Bff]\nListen = {value}\n");
        assert_eq!(
            listen::parse(&text),
            Err(ListenError::Invalid(value.to_string())),
            "Listen = {value:?}"
        );
    }
}

/// Any address is served, whatever the port: the machine's LAN address, the
/// wildcard on both families, a link-local address, and port 0 (the OS picks
/// one). Off loopback is the administrator's choice, not a refusal.
#[test]
fn parse_accepts_an_address_off_loopback() {
    for value in [
        "0.0.0.0:8590",
        "192.168.1.10:8590",
        "10.0.0.5:0",
        "0.0.0.0:0",
        "[::]:8590",
        "[fe80::1]:8590",
    ] {
        let text = format!("[Bff]\nListen = {value}\n");
        assert_eq!(listen::parse(&text), Ok(addr(value)), "Listen = {value:?}");
    }
}

/// A `bff.ini` that exists and cannot be read is an error, not the default:
/// the service must not come up on an address the admin did not choose.
#[test]
fn load_refuses_a_file_it_cannot_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bff.ini");
    std::fs::create_dir(&path).expect("a directory where the file should be");
    match listen::load(&path) {
        Err(ListenError::Unreadable(detail)) => {
            assert!(!detail.is_empty(), "the IO error is kept")
        }
        other => panic!("a directory as bff.ini: {other:?}"),
    }
}

/// `load` goes through the same checks as `parse`: an address off loopback in
/// the file is the address it returns, a value that is not an address is the
/// refusal.
#[test]
fn load_refuses_a_bad_value_in_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bff.ini");
    std::fs::write(&path, "[Bff]\nListen = 0.0.0.0:8590\n").expect("write bff.ini");
    assert_eq!(listen::load(&path), Ok(addr("0.0.0.0:8590")));
    std::fs::write(&path, "[Bff]\nListen = 192.168.1.10:8590\n").expect("write bff.ini");
    assert_eq!(listen::load(&path), Ok(addr("192.168.1.10:8590")));
    std::fs::write(&path, "[Bff]\nListen = nonsense\n").expect("write bff.ini");
    assert_eq!(
        listen::load(&path),
        Err(ListenError::Invalid("nonsense".into()))
    );
}

// ── The default and what is accepted ────────────────────────────────────

/// No file is the default, `127.0.0.1:8590`.
#[test]
fn load_without_a_file_is_the_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bff.ini");
    assert_eq!(listen::load(&path), Ok(default_addr()));
    assert!(!path.exists(), "load creates nothing");
}

/// A text with no `Listen` in `[Bff]` is the default too: an empty file,
/// another section, another key.
#[test]
fn parse_without_listen_is_the_default() {
    for text in [
        "",
        "[Bff]\n",
        "[Other]\nListen = 192.168.1.20:8590\n",
        "[Bff]\nSomethingElse = 1\n",
        "; a comment\n[Bff]\n; Listen = 0.0.0.0:8590\n",
    ] {
        assert_eq!(listen::parse(text), Ok(default_addr()), "{text:?}");
    }
}

/// Any loopback address and port is accepted, port 0 (the OS picks one)
/// included, IPv6 as well as IPv4.
#[test]
fn parse_accepts_a_loopback_address() {
    for value in [
        "127.0.0.1:8590",
        "127.0.0.1:9000",
        "127.0.0.1:0",
        "127.0.0.2:8590",
        "[::1]:8590",
    ] {
        let text = format!("[Bff]\nListen = {value}\n");
        assert_eq!(listen::parse(&text), Ok(addr(value)), "Listen = {value:?}");
    }
}

/// The file as written by hand: whitespace around the value, CRLF line ends,
/// a UTF-8 BOM (Notepad), the section named in another case.
#[test]
fn parse_reads_a_hand_written_file() {
    for text in [
        "[Bff]\nListen =   127.0.0.1:9000   \n",
        "[Bff]\r\nListen = 127.0.0.1:9000\r\n",
        "\u{feff}[Bff]\nListen = 127.0.0.1:9000\n",
        "[bff]\nListen = 127.0.0.1:9000\n",
    ] {
        assert_eq!(listen::parse(text), Ok(addr("127.0.0.1:9000")), "{text:?}");
    }
}

/// `load` reads the file it is given.
#[test]
fn load_reads_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bff.ini");
    std::fs::write(&path, "[Bff]\nListen = 127.0.0.1:9000\n").expect("write bff.ini");
    assert_eq!(listen::load(&path), Ok(addr("127.0.0.1:9000")));
}

// ── The names the Host guard answers beside localhost and the IP literals ──

/// A list of names, sorted: the guard answers a name that is in the list, so
/// the list is a set and its order is not the contract.
fn sorted<S: AsRef<str>>(names: &[S]) -> Vec<String> {
    let mut out: Vec<String> = names.iter().map(|n| n.as_ref().to_string()).collect();
    out.sort();
    out
}

/// The host name of these fixtures: the machine's own name.
const HOSTNAME: &str = "aorus";

/// The `resolv.conf` of a machine on one search domain.
const RESOLV: &str = "search bombicci.local\n";

/// With no `[Bff] Hosts`, the guard answers the machine's own names: its host
/// name, that name with `.local` and that name under each domain the
/// `resolv.conf` text gives, a `search` line's list and a `domain` line alike,
/// from every line of its kind in the file.
#[test]
fn host_names_are_the_machine_and_its_search_domains() {
    for (resolv, want) in [
        (
            "search bombicci.local\n",
            &["aorus", "aorus.local", "aorus.bombicci.local"][..],
        ),
        (
            "domain x.lan\n",
            &["aorus", "aorus.local", "aorus.x.lan"][..],
        ),
        (
            "nameserver 8.8.8.8\nsearch bombicci.local office.lan\n",
            &[
                "aorus",
                "aorus.local",
                "aorus.bombicci.local",
                "aorus.office.lan",
            ][..],
        ),
        (
            "search a.lan\nsearch b.lan\n",
            &["aorus", "aorus.local", "aorus.a.lan", "aorus.b.lan"][..],
        ),
        (
            "search bombicci.local\tdomain-set.lan\t\n",
            &[
                "aorus",
                "aorus.local",
                "aorus.bombicci.local",
                "aorus.domain-set.lan",
            ][..],
        ),
    ] {
        assert_eq!(
            sorted(&listen::host_names(HOSTNAME, resolv, None)),
            sorted(want),
            "{resolv:?}"
        );
    }
}

/// What a `resolv.conf` text does not set: a comment in either spelling, a
/// blank line, a line of another keyword, and a `search` line with no domain
/// after it. The machine's name and its `.local` are always there.
#[test]
fn host_names_ignore_everything_that_is_not_a_search_line() {
    for (resolv, want) in [
        (
            "\n# a comment\n; another\nnameserver 8.8.8.8\n\noptions ndots:2\nsearch bombicci.local\n",
            &["aorus", "aorus.local", "aorus.bombicci.local"][..],
        ),
        (
            "; search evil.example\n# domain evil.example\n",
            &["aorus", "aorus.local"][..],
        ),
        ("search\n", &["aorus", "aorus.local"][..]),
        ("", &["aorus", "aorus.local"][..]),
        ("\n\n   \n", &["aorus", "aorus.local"][..]),
    ] {
        assert_eq!(
            sorted(&listen::host_names(HOSTNAME, resolv, None)),
            sorted(want),
            "{resolv:?}"
        );
    }
}

/// `[Bff] Hosts`, when the file sets it, replaces the machine's names: the
/// answer is exactly the names it lists, comma-separated, and not one of the
/// automatic ones. A value that is empty sets nothing, so the automatic names
/// stand, the same answer `parse_hosts` gives for that file.
#[test]
fn hosts_when_set_replace_the_machine_names() {
    for (hosts, want) in [
        ("aorus.bombicci.local", &["aorus.bombicci.local"][..]),
        ("pc.lan, office.example", &["pc.lan", "office.example"][..]),
        ("", &["aorus", "aorus.local", "aorus.bombicci.local"][..]),
    ] {
        assert_eq!(
            sorted(&listen::host_names(HOSTNAME, RESOLV, Some(hosts))),
            sorted(want),
            "Hosts = {hosts:?}"
        );
    }
    // The chain: the value `parse_hosts` reads is the value that replaces them.
    let text = "[Bff]\nListen = 0.0.0.0:8590\nHosts = aorus.bombicci.local\n";
    assert_eq!(
        sorted(&listen::host_names(
            HOSTNAME,
            RESOLV,
            listen::parse_hosts(text).as_deref()
        )),
        sorted(&["aorus.bombicci.local"]),
        "{text:?}"
    );
}

/// Every name is lowercase, because the guard reads the `Host` line in any
/// case: the host name, the domain of a `resolv.conf` line, and each name of a
/// `Hosts` value, which is also trimmed of the spaces and tabs around it, and
/// whose empty items (a trailing comma) are dropped.
#[test]
fn host_names_are_lowercase_and_trimmed() {
    assert_eq!(
        sorted(&listen::host_names(
            "Aorus",
            "search Bombicci.LOCAL\n",
            None
        )),
        sorted(&["aorus", "aorus.local", "aorus.bombicci.local"]),
        "the host name and its domain are lowercased"
    );
    assert_eq!(
        sorted(&listen::host_names(
            "aorus",
            RESOLV,
            Some(" Office.Example , LAN2.LAN ,\t")
        )),
        sorted(&["office.example", "lan2.lan"]),
        "each Hosts name is trimmed and lowercased"
    );
}

/// A host name that is blank names no machine, so it builds no name at all:
/// `""`, `".local"` and `".<domain>"` are not the machine's names, they are the
/// empty string with a suffix on it. The guard is then left with `localhost`
/// and the addresses, which is what a machine with no host name has. The
/// `[Bff] Hosts` of the file are the administrator's own list: they apply
/// whatever the host name is, blank included.
#[test]
fn a_blank_host_name_gives_no_automatic_names() {
    for hostname in ["", "  ", "\t"] {
        assert_eq!(
            listen::host_names(hostname, "search a.b", None),
            Vec::<String>::new(),
            "hostname = {hostname:?}"
        );
    }
    // No host name and no search domain: nothing either.
    assert_eq!(
        listen::host_names("", "", None),
        Vec::<String>::new(),
        "nothing to build from"
    );
    // The `Hosts` list stands on its own.
    for hostname in ["", "   "] {
        assert_eq!(
            sorted(&listen::host_names(
                hostname,
                "search a.b",
                Some("pc.lan, office.example")
            )),
            sorted(&["pc.lan", "office.example"]),
            "the Hosts list applies to a blank host name: {hostname:?}"
        );
    }
}

/// A `search` domain written as a fully qualified name, with its trailing dot
/// (the root of the DNS tree, the spelling `resolv.conf` and a Windows
/// connection's suffix list both carry), is that domain without the dot: the
/// name built from it has no trailing dot, and the same domain written twice,
/// dotted and bare, is one name and not two.
#[test]
fn a_search_domain_with_a_trailing_dot_builds_the_bare_name() {
    assert_eq!(
        sorted(&listen::host_names("aorus", "search example.com.", None)),
        sorted(&["aorus", "aorus.example.com", "aorus.local"]),
        "the dotted domain gives the name with no trailing dot"
    );
    assert_eq!(
        sorted(&listen::host_names(
            "aorus",
            "search example.com. example.com",
            None
        )),
        sorted(&["aorus", "aorus.example.com", "aorus.local"]),
        "the dotted domain is the bare one: one name, not two"
    );
    // The root alone, `search .`: appending it to a name adds nothing, so no
    // name carries a trailing dot. What the list holds beyond that is the
    // machine's own, which the rule above pins.
    for name in listen::host_names("aorus", "search .\n", None) {
        assert!(!name.ends_with('.'), "no name ends at the root: {name:?}");
    }
}

/// The `[Bff] Hosts` of an INI text, as written: the comma-separated list in
/// one value, the section named in any case, CRLF and a BOM included, beside
/// another key of the same section. The value is read by the same INI reader
/// as `Listen`, so it is trimmed and cut at the first `;`.
#[test]
fn parse_hosts_is_the_value_of_the_file() {
    for (text, want) in [
        (
            "[Bff]\nHosts = aorus.bombicci.local\n",
            Some("aorus.bombicci.local"),
        ),
        (
            "[Bff]\nHosts = aorus.bombicci.local, office.lan\n",
            Some("aorus.bombicci.local, office.lan"),
        ),
        ("[bff]\nHosts = aorus.local\n", Some("aorus.local")),
        ("[Bff]\r\nHosts = aorus.local\r\n", Some("aorus.local")),
        ("\u{feff}[Bff]\nHosts = aorus.local\n", Some("aorus.local")),
        (
            "[Bff]\nListen = 0.0.0.0:8590\nHosts = aorus.local\n",
            Some("aorus.local"),
        ),
        ("[Bff]\nHosts =   aorus.local  \n", Some("aorus.local")),
        (
            "[Bff]\nHosts = aorus.local ; the machine's name\n",
            Some("aorus.local"),
        ),
    ] {
        assert_eq!(listen::parse_hosts(text).as_deref(), want, "{text:?}");
    }
}

/// No `Hosts` is `None`, so the machine's own names stand: no file content,
/// another section, another key, a value that is empty or only spaces, a
/// commented-out line.
#[test]
fn parse_hosts_without_a_value_is_none() {
    for text in [
        "",
        "[Bff]\n",
        "[Bff]\nListen = 127.0.0.1:8590\n",
        "[Other]\nHosts = aorus.local\n",
        "[Bff]\nHosts =\n",
        "[Bff]\nHosts =   \n",
        "; a comment\n[Bff]\n; Hosts = aorus.local\n",
    ] {
        assert_eq!(listen::parse_hosts(text), None, "{text:?}");
    }
}
