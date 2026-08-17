//! Spawns `llama-server.exe --version` for the footer's version badge.

use crate::paths;

pub fn probe() -> Option<String> {
    let exe = paths::llama_server_exe()?;
    let text = run(&exe)?;
    parse(&text)
}

/// `llama-server --version` prints to **stderr**, so parse the combined
/// streams (`proc::combined_output`); reading stdout alone yields "".
fn run(exe: &std::path::Path) -> Option<String> {
    let output = crate::proc::run_hidden_probe(exe, ["--version"])?;
    if !output.status.success() {
        return None;
    }
    Some(crate::proc::combined_output(&output))
}

/// The `--version` banner as the footer badge: `"0.1.0-dev · b10463"`.
///
/// TWO input shapes, because llama.cpp gave itself a semantic version in
/// **b10398** (`cmake : introduce semantic versioning`, #26839), which rewrote
/// the banner rather than extending it:
///
/// ```text
/// b10398+   version: 0.1.0-dev (build 10463, commit 7c35571e5)
/// older     version: 9870 (2d973636e)
/// ```
///
/// The badge keeps the version and the BUILD number and drops the commit: that
/// number is `bNNNN` without the `b`, i.e. the same identity the release tags,
/// the installer name and `LlamaBuild` in the registry all carry, so it is the
/// half a user can actually match against something. The version is prepended
/// only when the binary reports one, so an older llama-server reads `b9870`
/// rather than growing an invented `0.0.0`.
///
/// The `-dev` suffix is llama.cpp's own: `LLAMA_BUILD_IS_DEV` defaults ON and
/// upstream turns it off only when building from a release tag, so it is a fact
/// about the binary and is shown as-is rather than trimmed.
///
/// The input is the combined stdout+stderr, which can carry noise around the
/// version line (dynamic-backend builds print `load_backend: …` banners), so
/// prefer the line with the `version: ` prefix and only fall back to the first
/// non-empty line when no line carries it. Anything neither shape matches is
/// shown verbatim: a banner this build cannot read is still evidence of which
/// server is installed.
fn parse(s: &str) -> Option<String> {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("version: "))
        .or_else(|| s.lines().map(str::trim).find(|l| !l.is_empty()))?;
    let stripped = line.strip_prefix("version: ").unwrap_or(line);
    if let Some(badge) = versioned_form(stripped).or_else(|| legacy_form(stripped)) {
        return Some(badge);
    }
    let out = stripped.trim();
    if out.is_empty() {
        None
    } else {
        Some(out.to_string())
    }
}

/// `0.1.0-dev (build 10463, commit 7c35571e5)` → `0.1.0-dev · b10463`.
/// The commit is parsed only to prove the line really is this shape; a build
/// number that isn't a number means it is not, and the caller shows the line
/// raw instead of a badge assembled out of a bad guess.
fn versioned_form(s: &str) -> Option<String> {
    let (version, rest) = s.split_once(" (build ")?;
    let (build, commit) = rest.strip_suffix(')')?.split_once(", commit ")?;
    let (version, build, commit) = (version.trim(), build.trim(), commit.trim());
    let ok = !version.is_empty()
        && !build.is_empty()
        && build.chars().all(|c| c.is_ascii_digit())
        && !commit.is_empty()
        && commit.chars().all(|c| c.is_ascii_hexdigit());
    ok.then(|| format!("{version} · b{build}"))
}

/// Pre-b10398: `9870 (2d973636e)` → `b9870`. The leading field is the build
/// number the newer banner names explicitly, so it earns the same `b`.
fn legacy_form(s: &str) -> Option<String> {
    let (build, rest) = s.split_once(' ')?;
    let commit = rest.trim_matches(|c| c == '(' || c == ')');
    let ok = !build.is_empty()
        && build.chars().all(|c| c.is_ascii_digit())
        && !commit.is_empty()
        && commit.chars().all(|c| c.is_ascii_hexdigit());
    ok.then(|| format!("b{build}"))
}

#[cfg(test)]
mod tests {
    use super::parse;

    /// The shape llama.cpp has printed since b10398: the badge keeps the
    /// version and the build number and drops the commit.
    #[test]
    fn parses_the_versioned_banner() {
        assert_eq!(
            parse("\nversion: 0.1.0-dev (build 10463, commit 7c35571e5)\nbuilt with Clang 23.0.0 for Windows AMD64\n")
                .as_deref(),
            Some("0.1.0-dev · b10463"),
        );
    }

    /// The same banner off a release build, where llama.cpp's own
    /// `LLAMA_BUILD_IS_DEV=OFF` drops the suffix. Nothing here trims it: the
    /// badge repeats whatever the binary calls itself.
    #[test]
    fn keeps_a_release_version_and_a_dev_one_apart() {
        assert_eq!(
            parse("version: 0.1.0 (build 10463, commit 7c35571e5)\n").as_deref(),
            Some("0.1.0 · b10463"),
        );
    }

    /// A line that opens like the new banner but isn't (no commit half) must
    /// not be assembled into a badge out of a half-match; it comes out raw.
    #[test]
    fn a_malformed_versioned_banner_falls_back_to_the_raw_line() {
        assert_eq!(
            parse("version: 0.1.0-dev (build 10463)\n").as_deref(),
            Some("0.1.0-dev (build 10463)"),
        );
    }

    #[test]
    fn parses_version_with_hash() {
        assert_eq!(
            parse("version: 9999 (abc12345)\n").as_deref(),
            Some("b9999"),
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
            Some("b9870"),
        );
    }

    /// Dynamic-backend builds print `load_backend: …` banners around the
    /// version line; the parser must pick the `version: ` line, not just the
    /// first non-empty one.
    #[test]
    fn skips_backend_banner_lines() {
        assert_eq!(
            parse("load_backend: loaded CUDA backend from C:\\x\\ggml-cuda.dll\nversion: 9870 (2d973636e)\n")
                .as_deref(),
            Some("b9870"),
        );
    }
}
