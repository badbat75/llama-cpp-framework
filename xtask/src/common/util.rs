//! Paths, processes and version comparison shared by every phase.

use anyhow::{bail, Context, Result};
use std::cmp::Ordering;
use std::ffi::OsStr;
#[cfg(not(windows))]
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The repository root: the parent of this crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits in the repo root")
        .to_path_buf()
}

/// "linux", "windows" or "macos": the per-OS level of build/ and of the dist
/// layout.
pub const OS_DIR: &str = if cfg!(windows) {
    "windows"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

/// build/<os>: everything this OS generates, never shared with the other.
pub fn build_root() -> PathBuf {
    repo_root().join("build").join(OS_DIR)
}

/// The user's home directory (USERPROFILE on Windows).
pub fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    PathBuf::from(std::env::var_os(var).unwrap_or_default())
}

/// An absolute, normalized path (`[IO.Path]::GetFullPath`): relative to the
/// current directory, `.` and `..` folded, no symlink resolved. Not
/// canonicalize(), which returns a `\\?\` verbatim path on Windows that
/// cmake, clang and the config file would all carry along.
#[cfg(windows)]
pub fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// An absolute, normalized path (`readlink -m`): relative to the current
/// directory, symlinks of the existing part resolved, `..` folded.
#[cfg(not(windows))]
pub fn absolute(p: &Path) -> PathBuf {
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    if let Ok(c) = joined.canonicalize() {
        return c;
    }
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    match (out.parent(), out.file_name()) {
        (Some(parent), Some(name)) if parent != out => match parent.canonicalize() {
            Ok(p) => p.join(name),
            Err(_) => out,
        },
        _ => out,
    }
}

/// The first executable named `name` on PATH. On Windows a bare name also
/// matches `<name>.exe`, `.cmd` and `.bat` (npm is `npm.cmd`), as the shell
/// would resolve it.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) && Path::new(name).extension().is_none() {
        &["exe", "cmd", "bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path)
        .flat_map(|d| exts.iter().map(move |e| if e.is_empty() { d.join(name) } else { d.join(name).with_extension(e) }))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
pub fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}
#[cfg(not(unix))]
pub fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// Puts `dir` in front of PATH, for this process and its children, unless it
/// is already on it (`only_if_absent`).
pub fn path_prepend(dir: &Path, only_if_absent: bool) {
    let cur = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = std::env::split_paths(&cur).collect();
    if only_if_absent && parts.iter().any(|p| p == dir) {
        return;
    }
    parts.insert(0, dir.to_path_buf());
    if let Ok(joined) = std::env::join_paths(parts) {
        std::env::set_var("PATH", joined);
    }
}

pub fn cmd<S: AsRef<OsStr>>(program: S) -> Command {
    Command::new(program)
}

fn describe(c: &Command) -> String {
    let mut s = c.get_program().to_string_lossy().into_owned();
    for a in c.get_args() {
        s.push(' ');
        s.push_str(&a.to_string_lossy());
    }
    s
}

/// Runs with inherited stdio; an error when it cannot start or fails.
pub fn run(c: &mut Command) -> Result<()> {
    let status = c.status().with_context(|| format!("cannot run {}", describe(c)))?;
    if !status.success() {
        bail!("{} failed ({status})", describe(c));
    }
    Ok(())
}

/// Did it run and succeed? Output discarded: a failure is an answer here.
pub fn succeeds(c: &mut Command) -> bool {
    c.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// stdout of a successful run, trimmed; None when it fails or cannot start.
/// stderr is discarded.
pub fn output(c: &mut Command) -> Option<String> {
    let out = c.stderr(Stdio::null()).stdin(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Version order as `sort -V` sees it: digit runs compared as numbers, the
/// rest as text, so 1.4.10 outranks 1.4.9 and v0.10.0 outranks v0.2.0.
pub fn vercmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    loop {
        match (a.is_empty(), b.is_empty()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let da = a[0].is_ascii_digit();
        let db = b[0].is_ascii_digit();
        if da && db {
            let na = a.iter().take_while(|c| c.is_ascii_digit()).count();
            let nb = b.iter().take_while(|c| c.is_ascii_digit()).count();
            let (ra, rb) = (&a[..na], &b[..nb]);
            let ta = trim_zeros(ra);
            let tb = trim_zeros(rb);
            let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
            if o != Ordering::Equal {
                return o;
            }
            a = &a[na..];
            b = &b[nb..];
        } else if da != db {
            return if da { Ordering::Less } else { Ordering::Greater };
        } else {
            let na = a.iter().take_while(|c| !c.is_ascii_digit()).count();
            let nb = b.iter().take_while(|c| !c.is_ascii_digit()).count();
            let o = a[..na].cmp(&b[..nb]);
            if o != Ordering::Equal {
                return o;
            }
            a = &a[na..];
            b = &b[nb..];
        }
    }
}

fn trim_zeros(d: &[u8]) -> &[u8] {
    let n = d.iter().take_while(|c| **c == b'0').count();
    &d[n..]
}

/// Is `a` strictly newer than `b`?
pub fn version_gt(a: &str, b: &str) -> bool {
    a != b && vercmp(a, b) == Ordering::Greater
}

/// `^[0-9]+(\.[0-9]+)+$`
pub fn is_dotted_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
}

/// `^v[0-9]+\.[0-9]+\.[0-9]+$`: a llama.cpp release tag, not a bNNNN
/// nightly, not a -rc.
pub fn is_release_tag(s: &str) -> bool {
    s.strip_prefix('v')
        .map(|v| v.split('.').count() == 3 && is_dotted_version(v))
        .unwrap_or(false)
}

/// Every `<prefix>X.Y.Z<suffix>` in `text`, strictly three numeric fields,
/// returned as (version, whole match).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn scan_versions(text: &str, prefix: &str, suffix: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(prefix) {
        let after = &rest[i + prefix.len()..];
        let ver_len = after
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(after.len());
        // The version ends where the suffix starts; trailing dots belong to it.
        if let Some(n) = (1..=ver_len).find(|&n| after[n..].starts_with(suffix)) {
            let v = &after[..n];
            if v.split('.').count() == 3 && is_dotted_version(v) {
                found.push((v.to_string(), format!("{prefix}{v}{suffix}")));
            }
        }
        rest = &rest[i + prefix.len()..];
    }
    found
}

/// The newest llama.cpp RELEASE tag of a clone: the highest vX.Y.Z among the
/// tags reachable from origin/master (--merged for reachability, version
/// order so v0.10.0 outranks v0.2.0, the strict pattern so a -rc pre-release
/// is not mistaken for a release and the bNNNN nightlies never count).
pub fn newest_release_tag(clone: &Path) -> Option<String> {
    let tags = output(cmd("git").arg("-C").arg(clone).args([
        "tag",
        "--list",
        "v[0-9]*",
        "--merged",
        "origin/master",
    ]))?;
    tags.lines()
        .map(str::trim)
        .filter(|t| is_release_tag(t))
        .max_by(|a, b| vercmp(a, b))
        .map(str::to_string)
}

/// `du -sm`/`du -sk`: what the scripts have always reported.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn du(flag: &str, p: &Path) -> String {
    output(cmd("du").arg(flag).arg(p))
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "?".into())
}

/// Removes a file or a directory tree, if there is one.
pub fn remove_all(p: &Path) -> Result<()> {
    match std::fs::symlink_metadata(p) {
        Err(_) => Ok(()),
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p).with_context(|| format!("removing {}", p.display())),
        Ok(_) => std::fs::remove_file(p).with_context(|| format!("removing {}", p.display())),
    }
}

/// The first `gfx<alnum>+` in a file name.
#[cfg(not(target_os = "macos"))]
pub fn gfx_arch(name: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(i) = name[from..].find("gfx") {
        let start = from + i;
        let tail = &name[start + 3..];
        let n = tail.bytes().take_while(|c| c.is_ascii_alphanumeric()).count();
        if n > 0 {
            return Some(&name[start..start + 3 + n]);
        }
        from = start + 3;
    }
    None
}

/// The file name of a path or URL, as text.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn basename(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// 3/4 of the logical cores, and never all of them: a core stays free during
/// the long CUDA/HIP compiles.
pub fn default_build_jobs() -> u32 {
    let n = std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1);
    let mut j = n * 3 / 4;
    if j > n - 1 {
        j = n - 1;
    }
    j.max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order() {
        assert!(version_gt("1.4.10", "1.4.9"));
        assert!(version_gt("v0.10.0", "v0.2.0"));
        assert!(!version_gt("1.0", "1.0"));
        assert!(version_gt("2.2.0+llama.cpp.v0.10.0-1", "2.2.0+llama.cpp.v0.7.0-1"));
        assert!(version_gt("2.2.1+llama.cpp.v0.1.0-1", "2.2.0+llama.cpp.v0.9.0-1"));
    }

    #[test]
    fn release_tags() {
        assert!(is_release_tag("v0.8.0"));
        assert!(!is_release_tag("v0.8.0-rc1"));
        assert!(!is_release_tag("b10566"));
        assert!(!is_release_tag("v0.8"));
    }

    #[test]
    fn scans_strict_versions() {
        let page = r#"x therock-dist-linux-multiarch-10.1.0.tar.gz y therock-dist-linux-multiarch-10.1.0a20260831.tar.gz
                      therock-dist-linux-multiarch-tests-10.2.0.tar.gz therock-dist-linux-multiarch-7.14.0.tar.gz"#;
        let v: Vec<String> = scan_versions(page, "therock-dist-linux-multiarch-", ".tar.gz")
            .into_iter()
            .map(|(v, _)| v)
            .collect();
        assert_eq!(v, ["10.1.0", "7.14.0"]);
        let idx = "redistrib_13.4.3.json redistrib_13.0.json";
        assert_eq!(scan_versions(idx, "redistrib_", ".json")[0].1, "redistrib_13.4.3.json");
    }
}
