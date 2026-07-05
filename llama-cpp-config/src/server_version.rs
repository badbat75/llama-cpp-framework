// Spawns `llama-server.exe --version` for the footer's version badge.

use crate::paths;

pub fn probe() -> Option<String> {
    let exe = paths::llama_server_exe()?;
    let text = run(&exe)?;
    parse(&text)
}

/// `llama-server --version` prints to **stderr**, so parse the combined
/// streams (`proc::combined_output`) — reading stdout alone yields "".
fn run(exe: &std::path::Path) -> Option<String> {
    let output = crate::proc::run_hidden_probe(exe, ["--version"])?;
    if !output.status.success() {
        return None;
    }
    Some(crate::proc::combined_output(&output))
}

/// Turn `"version: 9999 (abc12345)\n"` into `"9999-abc12345"`. The input is
/// the combined stdout+stderr, which can carry noise around the version line
/// (dynamic-backend builds print `load_backend: …` banners), so prefer the
/// line with the `version: ` prefix and only fall back to the first non-empty
/// line when no line carries it.
fn parse(s: &str) -> Option<String> {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("version: "))
        .or_else(|| s.lines().map(str::trim).find(|l| !l.is_empty()))?;
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
        assert_eq!(parse("version: 9999\n").as_deref(), Some("9999"),);
    }

    #[test]
    fn empty_input_is_none() {
        assert!(parse("").is_none());
        assert!(parse("\n\n").is_none());
    }

    /// The real shape: `--version` prints to stderr, so the combined
    /// stdout+stderr text starts with stdout's blank line.
    #[test]
    fn parses_combined_output_with_leading_blank_line() {
        assert_eq!(
            parse("\nversion: 9870 (2d973636e)\n").as_deref(),
            Some("9870-2d973636e"),
        );
    }

    /// Dynamic-backend builds print `load_backend: …` banners around the
    /// version line — the parser must pick the `version: ` line, not just the
    /// first non-empty one.
    #[test]
    fn skips_backend_banner_lines() {
        assert_eq!(
            parse("load_backend: loaded CUDA backend from C:\\x\\ggml-cuda.dll\nversion: 9870 (2d973636e)\n")
                .as_deref(),
            Some("9870-2d973636e"),
        );
    }
}
