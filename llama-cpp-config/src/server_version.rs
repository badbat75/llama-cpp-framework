// Spawns `llama-server.exe --version` for the header version badge.

use std::process::Command;

use crate::paths;

pub fn probe() -> Option<String> {
    let exe = paths::llama_server_exe()?;
    let stdout = run(&exe)?;
    parse(&stdout)
}

#[cfg(windows)]
fn run(exe: &std::path::Path) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let output = Command::new(exe)
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
fn run(exe: &std::path::Path) -> Option<String> {
    let output = Command::new(exe).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Turn `"version: 9999 (abc12345)\n"` into `"9999-abc12345"`.
fn parse(s: &str) -> Option<String> {
    let line = s.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let stripped = line.strip_prefix("version: ").unwrap_or(line);
    // If there's a parenthetical commit hash, convert it to dash form
    if let Some((ver, rest)) = stripped.split_once(' ') {
        let hash = rest.trim_matches(|c| c == '(' || c == ')');
        if !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(format!("{ver}-{hash}"));
        }
    }
    let out = stripped.trim();
    if out.is_empty() {
        None
    } else {
        Some(out.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_version_with_hash() {
        assert_eq!(
            parse("version: 9999 (abc12345)\n").as_deref(),
            Some("9999-abc12345"),
        );
    }

    #[test]
    fn parses_version_no_hash() {
        assert_eq!(
            parse("version: 9999\n").as_deref(),
            Some("9999"),
        );
    }

    #[test]
    fn empty_input_is_none() {
        assert!(parse("").is_none());
    }
}
