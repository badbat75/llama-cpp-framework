//! The isolated data-root test helper (TS2a): ARCHITECTURE.md "Isolated
//! data-root tests (TS2, decided 2026-11-28, test-only)".
//!
//! The data root is the process-wide `PRIVATE_AI_DATA_ROOT`, read again on
//! every call, so a test that redirects it must run alone in its process: that
//! is why every phase sits in the one `data_root_e2e` of `src/tests/mod.rs`.
//! `isolated` gives such a test its own process instead: the parent spawns this
//! test binary again with `--exact <its own test path>` and a marker naming it,
//! the child sees the marker, redirects the data root to a temp dir of its own
//! and runs the body. libtest runs the parents in parallel, so every
//! redirected test has its own root, runs alone, and fails on its own.
//!
//! The shape:
//! - the child, when the marker equals `test_path`: a `tempfile::tempdir()`,
//!   then `paths::redirect_for_tests` on it, then `body()`. The guard lives in
//!   the child, so the root is gone when the child exits.
//! - the parent: spawn `std::env::current_exe()` with `--exact <test_path>`,
//!   `--include-ignored` (a body that runs only as a child is `#[ignore]`d, so
//!   it fails nothing in the suite on its own) and `--nocapture`, with the
//!   marker set to `test_path`, forwarding the child's output, and fail unless
//!   the child exited 0 AND its output reports exactly one test passed: a
//!   mistyped path makes libtest run 0 tests and exit 0.

/// The marker `isolated` sets on the child it spawns, naming the test path it
/// re-runs there. A test of another path run in that process (by a filter, say)
/// sees a marker that is not its own and so never redirects its root.
const MARKER: &str = "PAI_TEST_ISOLATED_CHILD";

/// Run `body` in a child process of this test binary, with the data root
/// redirected to a temp dir of its own, and fail the calling test unless the
/// child ran exactly the test `test_path` (its own path) and passed.
pub fn isolated(test_path: &str, body: impl FnOnce()) {
    if std::env::var(MARKER).is_ok_and(|marked| marked == test_path) {
        // The child: the process this call spawned, running exactly this test,
        // so the body runs here, alone, under a root of its own.
        let dir = tempfile::tempdir().expect("the child's temp dir");
        crate::paths::redirect_for_tests(dir.path());
        body();
        // The guard drops here, so the child leaves no tree in the temp dir.
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            test_path,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MARKER, test_path)
        .output()
        .expect("spawn the child test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    let counts = passed_and_failed(&stdout);
    assert!(
        out.status.success() && counts == Some((1, 0)),
        "the isolated child for {test_path} did not run exactly one passing test \
         (exit {:?}, passed and failed {counts:?}):\n{stdout}{stderr}",
        out.status.code(),
    );
}

/// The `(passed, failed)` counts of one libtest summary line: the number
/// standing before each of the two words.
fn counts(line: &str) -> Option<(usize, usize)> {
    let count = |word: &str| {
        let mut before: Option<&str> = None;
        for token in line.split_whitespace() {
            if token == word {
                return before?.parse().ok();
            }
            before = Some(token);
        }
        None
    };
    Some((count("passed;")?, count("failed;")?))
}

/// The child's totals over every `test result:` line of its output, or `None`
/// when it carries no summary at all (the child died before writing one).
fn passed_and_failed(stdout: &str) -> Option<(usize, usize)> {
    let mut total: Option<(usize, usize)> = None;
    for line in stdout
        .lines()
        .filter(|line| line.starts_with("test result:"))
    {
        let (passed, failed) = counts(line)?;
        let (sum_passed, sum_failed) = total.unwrap_or((0, 0));
        total = Some((sum_passed + passed, sum_failed + failed));
    }
    total
}

#[cfg(test)]
#[path = "isolated_tests.rs"]
mod tests;
