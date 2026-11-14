//! The address the bff listens on: `Listen` in the `[Bff]` section of
//! `bff.ini`, loopback only until LAN access has authentication. `parse`
//! reads the INI text, `load` the file at a path it is given (a tempdir
//! here, never the real system config).

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
#[test]
fn parse_refuses_a_value_that_is_not_an_address() {
    for value in [
        "not-an-address",
        "localhost:8590",
        "127.0.0.1",
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

/// Only loopback is served for now: every other address, the wildcard
/// included, is refused, whatever the port.
#[test]
fn parse_refuses_an_address_off_loopback() {
    for value in [
        "0.0.0.0:8590",
        "192.168.1.20:8590",
        "10.0.0.5:0",
        "[::]:8590",
        "[fe80::1]:8590",
    ] {
        let text = format!("[Bff]\nListen = {value}\n");
        assert_eq!(
            listen::parse(&text),
            Err(ListenError::NotLoopback(addr(value))),
            "Listen = {value:?}"
        );
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

/// `load` goes through the same checks as `parse`.
#[test]
fn load_refuses_a_bad_value_in_the_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bff.ini");
    std::fs::write(&path, "[Bff]\nListen = 0.0.0.0:8590\n").expect("write bff.ini");
    assert_eq!(
        listen::load(&path),
        Err(ListenError::NotLoopback(addr("0.0.0.0:8590")))
    );
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
