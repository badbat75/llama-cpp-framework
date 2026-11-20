//! Slice 10b-3: the node's GPU totals from the engine's own probe
//! (`manager::service::ProbedGpus`, ARCHITECTURE.md (b) 2 "The GPUs" and
//! "No GPU, or no list").
//!
//! The probe runs `<exe> --list-devices` as the manager, kills it after a
//! bound, reduces the block to the physical pool with `pai_core::devices`,
//! caches that for the manager's life keyed by the exe's path and
//! modification time, and answers `None` when there is nothing to measure
//! against (no spawn, a timeout, no card): the memory check is then skipped.
//!
//! The exe is a shell script in a temp dir that appends one line per run to
//! a counter file and prints a block in the format `pai_core::devices::parse`
//! reads (the `SAMPLE` of that module's tests), so a test counts the runs and
//! reads the arguments the exe got. A POSIX shell is needed to write one:
//! those tests are `#[cfg(unix)]`, and what Linux cannot prove is said where
//! it matters.
//!
//! Nothing here touches `paths::` (the exe is named by the test, the
//! production wiring is pinned by reading the source's text), so these plain
//! `#[test]`s run in parallel beside the crate's `data_root_e2e`.
//!
//! Names the tests assume, the contract for the coder: the production seam is
//! `ProbedGpus::new(exe, timeout)` for a test and `ProbedGpus::for_node()`
//! for the manager, built over `paths::llama_server_exe()` under
//! `GPU_PROBE_TIMEOUT`.

use crate::manager::service::{GpuTotals, ProbedGpus, GPU_PROBE_TIMEOUT};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The probe bound a test gives an exe that answers at once: far over any
/// spawn, far under any test.
const QUICK: Duration = Duration::from_secs(10);
/// The wall-clock deadline of the harness itself: a probe that never returns
/// is a red naming the bound, never a hang. The probe runs on its own thread,
/// so a hung one is left behind when the test panics.
const HARNESS: Duration = Duration::from_secs(20);

#[cfg(unix)]
mod scripted {
    use super::*;

    /// The device block of this repo's test machine, as `llama-server
    /// --list-devices` reports it (the `SAMPLE` of `pai_core::devices`'
    /// tests).
    pub const BLOCK: &str = "Available devices:\n  CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  Vulkan0: AMD Radeon(TM) Graphics (33593 MiB, 31913 MiB free)\n  Vulkan1: NVIDIA GeForce RTX 4070 SUPER (11997 MiB, 10844 MiB free)\n";

    /// A probe that names no card: the CPU pseudo-device, the only entry a
    /// CPU-only node reports.
    pub const NO_CARD: &str =
        "Available devices:\n  CPU: AMD Ryzen 9 9900X (63090 MiB, 48233 MiB free)\n";

    /// Where the block goes.
    pub enum Stream {
        Stdout,
        Stderr,
        /// The exe prints nothing.
        Nothing,
    }

    /// The physical pool of [`BLOCK`] as `pai_core::devices::gpu_pool`
    /// reduces it (pinned there, slice 10b-1): the CUDA/Vulkan twin counted
    /// once on its CUDA id, the integrated card dropped because a discrete
    /// one exists.
    pub fn pool_of_block() -> Vec<pai_core::devices::Gpu> {
        vec![pai_core::devices::Gpu {
            id: "CUDA0".to_string(),
            name: "NVIDIA GeForce RTX 4070 SUPER".to_string(),
            total_mib: 12281,
        }]
    }

    /// A temp dir with the probe's exe and the files its script writes.
    pub struct Sandbox {
        pub dir: tempfile::TempDir,
    }

    impl Sandbox {
        pub fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("tempdir"),
            }
        }
        /// The fake engine the probe runs.
        pub fn exe(&self) -> PathBuf {
            self.dir.path().join("llama-server")
        }
        /// One line per run of the exe, the arguments on it.
        pub fn counter(&self) -> PathBuf {
            self.dir.path().join("runs.txt")
        }
        /// The pid the script writes when it hangs.
        pub fn pidfile(&self) -> PathBuf {
            self.dir.path().join("pid.txt")
        }
        /// While it exists the script hangs, so ONE exe and one mtime answer
        /// and hang at the test's will.
        pub fn modefile(&self) -> PathBuf {
            self.dir.path().join("hang")
        }
        /// Write `body` as the exe and make it runnable; the exe's path.
        pub fn script(&self, body: &str) -> PathBuf {
            let exe = self.exe();
            std::fs::write(&exe, format!("#!/bin/sh\n{body}")).expect("script");
            self.chmod_x(&exe);
            exe
        }
        /// Rewrite the exe with the SAME bytes, which moves its modification
        /// time: the cache key changes, the exe does not.
        pub fn touch(&self) -> (std::time::SystemTime, std::time::SystemTime) {
            let exe = self.exe();
            let before = mtime(&exe);
            let bytes = std::fs::read(&exe).expect("the exe to touch");
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(&exe, bytes).expect("touch");
            (before, mtime(&exe))
        }
        fn chmod_x(&self, path: &Path) {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod 755");
        }
        /// The lines the exe appended to the counter: how often it ran, and
        /// with what arguments.
        pub fn runs(&self) -> Vec<String> {
            std::fs::read_to_string(self.counter())
                .unwrap_or_default()
                .lines()
                .map(|l| l.to_string())
                .collect()
        }
    }

    /// The body of an exe: one line per run in `counter`, `block` on `stream`,
    /// `exit` as the status. With `hang` the pair `(pidfile, modefile)`, the
    /// script writes its own pid and replaces itself with `sleep 120` while
    /// the mode file exists.
    pub fn script(
        counter: &Path,
        block: &str,
        stream: Stream,
        exit: i32,
        hang: Option<(&Path, &Path)>,
    ) -> String {
        let mut body = format!("echo \"run $*\" >> '{}'\n", counter.display());
        if let Some((pid, mode)) = hang {
            body.push_str(&format!(
                "if [ -f '{mode}' ]; then echo $$ > '{pid}'; exec sleep 120; fi\n",
                mode = mode.display(),
                pid = pid.display()
            ));
        }
        if !block.is_empty() {
            let into = match stream {
                Stream::Stdout => "",
                Stream::Stderr => " 1>&2",
                Stream::Nothing => "",
            };
            body.push_str(&format!("cat <<'DEVICES'{into}\n{block}DEVICES\n"));
        }
        body.push_str(&format!("exit {exit}\n"));
        body
    }

    fn mtime(path: &Path) -> std::time::SystemTime {
        std::fs::metadata(path)
            .expect("metadata")
            .modified()
            .expect("mtime")
    }

    /// The state letter of `pid` in `/proc`, `None` when it is gone. The
    /// command name may hold spaces and parens, so the state is the field
    /// after the last `)`.
    pub fn process_state(pid: u32) -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit(')')
            .next()
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|state| state.chars().next())
    }
}

/// The scripted exe and its sandbox need a POSIX shell, so they exist on Unix
/// only: the tests that read them are `#[cfg(unix)]`, and the two cross-platform
/// tests below use `probe` and `probed` alone.
#[cfg(unix)]
use scripted::*;

/// One call of the seam, off the test's own thread: the bound is what is
/// under test, so a probe that ignores it must red as a bound failure and not
/// hang the suite. The answer, and the wall clock it took.
fn probe(gpus: &Arc<ProbedGpus>) -> (Option<Vec<pai_core::devices::Gpu>>, Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let one = Arc::clone(gpus);
    std::thread::spawn(move || {
        let _ = tx.send(one.gpus());
    });
    let at = Instant::now();
    let answer = match rx.recv_timeout(HARNESS) {
        Ok(answer) => answer,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
            "the probe never answered within {}: the bound of the probe is not enforced",
            HARNESS.as_secs()
        ),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // A panic of the seam's own thread: the stub is `unimplemented!()`,
            // and the red reads as the harness's own death unless it is named.
            panic!(
                "the probe's `gpus()` panicked (its own panic is printed above): the \
                 production probe of the node is not implemented"
            )
        }
    };
    (answer, at.elapsed())
}

fn probed(exe: &Path, timeout: Duration) -> Arc<ProbedGpus> {
    Arc::new(ProbedGpus::new(exe, timeout))
}

// ── The probe answers, once ───────────────────────────────────────────────

#[test]
#[cfg(unix)]
fn the_fake_exe_printing_a_device_block_gives_the_pool() {
    let box_ = Sandbox::new();
    let exe = box_.script(&script(&box_.counter(), BLOCK, Stream::Stdout, 0, None));

    let (answer, _took) = probe(&probed(&exe, QUICK));

    assert_eq!(
        answer,
        Some(pool_of_block()),
        "the block the exe printed is not the pool the check measures against"
    );
    let runs = box_.runs();
    assert_eq!(runs.len(), 1, "the exe ran {:?}, once expected", runs);
    assert!(
        runs[0].contains("--list-devices"),
        "the exe is asked for its devices by that argument, the block the parser reads: {runs:?}"
    );
}

#[test]
#[cfg(unix)]
fn two_calls_of_the_probe_run_the_exe_once() {
    let box_ = Sandbox::new();
    let exe = box_.script(&script(&box_.counter(), BLOCK, Stream::Stdout, 0, None));
    let gpus = probed(&exe, QUICK);

    let (first, _) = probe(&gpus);
    let (second, _) = probe(&gpus);

    assert_eq!(first, Some(pool_of_block()));
    assert_eq!(
        second, first,
        "the cached pool is the one the first call answered"
    );
    assert_eq!(
        box_.runs().len(),
        1,
        "the probe is cached for the manager's life: the exe ran {:?}",
        box_.runs()
    );
}

#[test]
#[cfg(unix)]
fn a_new_mtime_of_the_exe_probes_again() {
    let box_ = Sandbox::new();
    let exe = box_.script(&script(&box_.counter(), BLOCK, Stream::Stdout, 0, None));
    let gpus = probed(&exe, QUICK);

    let (first, _) = probe(&gpus);
    assert_eq!(first, Some(pool_of_block()));
    let (before, after) = box_.touch();
    assert!(
        after > before,
        "the harness moved the exe's mtime (a filesystem with a coarse mtime \
         granularity cannot prove this test): {before:?} then {after:?}"
    );

    let (again, _) = probe(&gpus);
    assert_eq!(again, Some(pool_of_block()), "the re-probe answers");
    assert_eq!(
        box_.runs().len(),
        2,
        "an engine upgrade replaces the file: a new mtime probes again"
    );

    // The cache holds the new key: a third call runs nothing.
    let (third, _) = probe(&gpus);
    assert_eq!(third, Some(pool_of_block()));
    assert_eq!(
        box_.runs().len(),
        2,
        "the re-probe is cached under the new mtime"
    );
}

// ── The probe fails: nothing to measure against, and nothing cached ───────

#[test]
#[cfg(unix)]
fn an_exe_that_oversleeps_the_bound_is_killed_and_gives_none() {
    let box_ = Sandbox::new();
    let (counter, pidfile, modefile) = (box_.counter(), box_.pidfile(), box_.modefile());
    std::fs::write(&modefile, "hang").expect("mode file");
    let exe = box_.script(&script(
        &counter,
        BLOCK,
        Stream::Stdout,
        0,
        Some((&pidfile, &modefile)),
    ));
    let bound = Duration::from_secs(2);
    let gpus = probed(&exe, bound);

    let (answer, took) = probe(&gpus);

    assert_eq!(answer, None, "a probe that is killed measures nothing");
    assert!(
        took >= Duration::from_millis(1800),
        "the answer came back after {took:?}, under the bound of {bound:?}: \
         the exe was not given its bound"
    );
    assert!(
        took < Duration::from_secs(8),
        "the exe sleeps 120 s: the probe waited {took:?} for it, so it was not killed"
    );

    // The child is stopped, not left running: a manager that probes on every
    // save would fill the node with hung engines. A killed child the manager
    // has not reaped yet (state Z) counts as stopped.
    let pid: u32 = std::fs::read_to_string(&pidfile)
        .expect("the hanging exe wrote its pid")
        .trim()
        .parse()
        .expect("a pid");
    #[cfg(target_os = "linux")]
    {
        let state = process_state(pid);
        assert!(
            state.is_none() || state == Some('Z'),
            "the killed exe is still running (state {:?})",
            state
        );
    }

    // A killed probe is not cached: the SAME probe, whose exe has not moved,
    // answers once the exe stops hanging.
    let _ = std::fs::remove_file(&modefile);
    let (answer, _) = probe(&gpus);
    assert_eq!(
        answer,
        Some(pool_of_block()),
        "the next call runs the exe again: a timeout is not cached"
    );
    assert_eq!(
        box_.runs().len(),
        2,
        "one run per probe, the failed one included: {:?}",
        box_.runs()
    );
}

#[test]
#[cfg(unix)]
fn a_probe_that_fails_is_not_cached() {
    let box_ = Sandbox::new();
    let counter = box_.counter();
    let exe = box_.script(&script(&counter, "", Stream::Nothing, 1, None));
    let gpus = probed(&exe, QUICK);

    let (first, _) = probe(&gpus);
    let (second, _) = probe(&gpus);

    assert_eq!(first, None, "an exe that exits 1 names no card");
    assert_eq!(second, None, "a failed probe keeps answering nothing");
    assert_eq!(
        box_.runs().len(),
        2,
        "a failed probe is not cached, so the fix of the engine is picked up: {:?}",
        box_.runs()
    );
}

#[test]
#[cfg(unix)]
fn a_probe_that_names_no_card_gives_none() {
    let box_ = Sandbox::new();
    let counter = box_.counter();
    let exe = box_.script(&script(&counter, NO_CARD, Stream::Stdout, 0, None));

    let (answer, _) = probe(&probed(&exe, QUICK));

    assert_eq!(
        answer, None,
        "a probe that names no card with a size is a node with no GPU: the \
         memory check is skipped, never measured on an empty pool"
    );
}

#[test]
#[cfg(unix)]
fn a_device_block_on_stderr_is_read_too() {
    // `pai_core::proc`'s rule for the llama-server probes: the tools split
    // their output across the two streams, so a probe parses the combination.
    let box_ = Sandbox::new();
    let exe = box_.script(&script(&box_.counter(), BLOCK, Stream::Stderr, 0, None));

    let (answer, _) = probe(&probed(&exe, QUICK));

    assert_eq!(
        answer,
        Some(pool_of_block()),
        "the block the exe printed on stderr is read as well as stdout"
    );
}

#[test]
#[cfg(unix)]
fn an_exe_flooding_its_pipe_past_the_buffer_is_not_stopped_by_it() {
    // The kernel's pipe buffer is far under 256 KiB (64 KiB on Linux,
    // pipe(7)): an exe writing 256 KiB of filler on stdout after the
    // block stops on its write unless the probe reads the pipes WHILE
    // the child runs. A stopped child never exits, so a probe that
    // reads the pipes only after its wait loop waits out the bound,
    // kills it and answers nothing: the node reads as having no GPU.
    let box_ = Sandbox::new();
    let counter = box_.counter();
    let exe = box_.script(&format!(
        "echo \"run $*\" >> '{}'\ncat <<'DEVICES'\n{BLOCK}DEVICES\nyes fill | head -c 262144\nexit 0\n",
        counter.display()
    ));
    let bound = Duration::from_secs(10);
    let gpus = probed(&exe, bound);

    let (answer, took) = probe(&gpus);

    assert_eq!(
        answer,
        Some(pool_of_block()),
        "the block is on stdout before the flood: an exe that floods its pipe \
         past the kernel buffer must be read while it runs, not stopped by the \
         full pipe and killed at the bound"
    );
    assert!(
        took < bound / 2,
        "a flooded child is read and reaped, not waited out: the probe \
         answered after {took:?} against a bound of {bound:?}"
    );
}

#[test]
#[cfg(unix)]
fn a_helper_holding_the_pipe_does_not_hang_the_probe() {
    // The engine spawns helpers of its own (a device probe's worker, a loader's
    // child) and they inherit its stdout and stderr. The engine prints the
    // block and exits at once, but its end of each pipe stays open in the
    // helper, so a reader that joins the two readers after the child's wait
    // blocks for the helper's life: the probe never answers, its bound is
    // never reached, and the manager's call hangs. The probe must answer
    // within its bound plus a short grace whatever the helper does, with the
    // pool (the block is there) or with nothing. The same shape on Windows is
    // an inherited handle, out of reach of this test.
    let box_ = Sandbox::new();
    let (counter, pidfile) = (box_.counter(), box_.pidfile());
    let exe = box_.script(&format!(
        "echo \"run $*\" >> '{}'\ncat <<'DEVICES'\n{BLOCK}DEVICES\nsleep 600 &\necho $! > '{}'\nexit 0\n",
        counter.display(),
        pidfile.display()
    ));
    let bound = Duration::from_secs(2);
    let gpus = probed(&exe, bound);

    let (answer, took) = probe(&gpus);

    // The helper is stopped before the asserts, so a red run leaves no
    // `sleep 600` behind.
    stop_the_helper(&pidfile);

    assert!(
        took < bound + Duration::from_secs(4),
        "the engine printed the block and exited at once, so a probe that \
         joins its pipe readers only after the child is gone waits for the \
         helper holding the pipes: the probe answered after {took:?} against \
         a bound of {bound:?}"
    );
    assert!(
        answer.is_none() || answer == Some(pool_of_block()),
        "the probe must answer with the pool of the block it read, or with \
         nothing: {answer:?}"
    );
}

/// Stops the background `sleep` a script left behind, whose pid it wrote to
/// `pidfile`. The kill goes through `sh` because `kill` is a shell builtin and
/// not a program.
#[cfg(unix)]
fn stop_the_helper(pidfile: &Path) {
    let pid = std::fs::read_to_string(pidfile)
        .unwrap_or_default()
        .trim()
        .to_string();
    if pid.is_empty() {
        return;
    }
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("kill {pid}"))
        .status();
}

#[test]
fn a_missing_exe_gives_none() {
    let box_ = tempfile::tempdir().expect("tempdir");
    let missing = box_.path().join("no-such-llama-server");
    let gpus = probed(&missing, QUICK);

    let (first, took) = probe(&gpus);
    let (second, _) = probe(&gpus);

    assert_eq!(
        first, None,
        "no engine installed: nothing to measure against"
    );
    assert_eq!(second, None, "and the answer is stable across calls");
    assert!(
        took < Duration::from_secs(5),
        "a spawn that cannot start is answered at once, not after the bound: {took:?}"
    );
}

// ── The wiring of the production manager ──────────────────────────────────

#[test]
fn the_probe_bound_is_the_thirty_seconds_of_the_design() {
    assert_eq!(
        GPU_PROBE_TIMEOUT,
        Duration::from_secs(30),
        "ARCHITECTURE.md (b) 2: killed after 30 s"
    );
}

/// The production manager probes the engine it launches, on both entry
/// points: the Linux D-Bus service and the Windows service. Like the other
/// pins of shared service glue this reads the source's TEXT, comments left
/// out (a comment naming a needle is not code), and
/// `manager/windows/entry.rs` is compiled by nobody on this machine: the pin
/// is the wiring, not the behaviour. What it cannot see: that the probe
/// really runs, which the tests above pin for `ProbedGpus` as such.
#[test]
fn the_production_manager_probes_the_engine_exe() {
    let dbus = fn_text(&code(include_str!("../manager/dbus.rs")), "serve");
    let entry = fn_text(
        &code(include_str!("../manager/windows/entry.rs")),
        "run_manager",
    );
    let service = code(include_str!("../manager/service.rs"));
    let for_node = method_text(&service, "for_node");

    // The readers find what is there today, so an empty text cannot pass.
    assert!(
        dbus.contains("Manager::new(") && dbus.contains("WriterLog::new("),
        "the reader of `serve` found no manager"
    );
    assert!(
        entry.contains("Manager::new(") && entry.contains("ScmService::new("),
        "the reader of `run_manager` found no manager"
    );
    assert!(
        method_text(&service, "with_gpus").contains("Box::new(gpus)"),
        "the reader of an indented method found no body"
    );

    for (name, text) in [("serve", &dbus), ("run_manager", &entry)] {
        let args = call_arguments(text, "with_gpus").unwrap_or_else(|| {
            panic!(
                "`{name}` builds its manager with no GPU pool, so the memory check of a \
                 real node measures against nothing. The manager of `{name}`:\n{text}"
            )
        });
        assert!(
            args.contains("ProbedGpus"),
            "`{name}` gives its manager `{args}`: the production probe of the engine is expected"
        );
    }

    let args = call_arguments(&for_node, "new").unwrap_or_else(|| {
        panic!("`for_node` builds no probe, over no exe:\n{for_node}");
    });
    assert!(
        args.contains("llama_server_exe()"),
        "the probe runs the exe the launch uses, `paths::llama_server_exe`: {args}"
    );
    assert!(
        args.contains("GPU_PROBE_TIMEOUT"),
        "the probe is bound by the design's constant, not a figure of its own: {args}"
    );
}

// ── The source readers ────────────────────────────────────────────────────

/// `src` with the whole-line comments left out.
fn code(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of the top-level `fn <name>` in `src`, from its signature to the
/// brace that closes it at column 0.
fn fn_text(src: &str, name: &str) -> String {
    for prefix in [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "async fn ",
        "unsafe fn ",
    ] {
        let open = format!("\n{prefix}{name}(");
        if let Some(at) = src.find(&open) {
            let body = &src[at..];
            let end = body
                .find("\n}\n")
                .unwrap_or_else(|| panic!("the end of `fn {name}`"));
            return body[..end].to_string();
        }
    }
    panic!("no top-level `fn {name}` in this source");
}

/// The text of an `fn <name>` indented in an `impl`, from its signature to
/// the brace that closes it at column 4. A method's own brace, not the
/// `impl`'s: a reader that stopped at the next column-0 brace would swallow
/// every method beside it and pass on a needle in a neighbour.
fn method_text(src: &str, name: &str) -> String {
    for prefix in ["fn ", "pub fn ", "pub(crate) fn "] {
        let open = format!("\n    {prefix}{name}(");
        if let Some(at) = src.find(&open) {
            let body = &src[at..];
            let end = body
                .find("\n    }")
                .unwrap_or_else(|| panic!("the end of `fn {name}`"));
            return body[..end].to_string();
        }
    }
    panic!("no `fn {name}` indented in an impl in this source");
}

/// The arguments of the first `name(...)` call in `hay`, up to that call's
/// own closing paren (a nested call is counted, not followed); `None` when
/// `hay` never calls it.
fn call_arguments(hay: &str, name: &str) -> Option<String> {
    let open = format!("{name}(");
    let at = hay.find(&open)?;
    let rest = &hay[at + open.len()..];
    let mut depth = 1usize;
    for (i, c) in rest.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(rest[..i].to_string());
            }
        }
    }
    None
}
