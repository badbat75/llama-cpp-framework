//! Slice AN2 of `ARCHITECTURE.md` ("About and node info"): the node's Vulkan
//! drivers, read by a child of the manager (`manager::service::VulkanProbe`).
//!
//! The child is this same executable run as `pai-manager vulkan-probe`: it
//! opens the platform's Vulkan loader, prints ONE JSON document on stdout, and
//! the manager parses that text as untrusted input, bounded by
//! `VULKAN_PROBE_TIMEOUT` and capped by the stream cap of `pai_core::proc`. A
//! child that hangs is killed and the answer is `failed`; a node with no loader
//! or no ICD answers `none`; text that is not the document answers `failed`.
//! The answer, whatever it is, is kept for `VULKAN_CACHE_SECONDS` so a polled
//! page spawns at most one child over the window, and callers that arrive
//! together share one run.
//!
//! The child is never the real one here: every test names a shell script in a
//! temp dir as the exe, so NO test of this file loads a Vulkan ICD, opens a GPU
//! or touches `paths::` (the exe and the bound are handed in; the production
//! wiring is pinned by reading the source's text). A POSIX shell is needed to
//! write a script, so those tests are `#[cfg(unix)]`; what Linux cannot prove
//! is said where it matters. The real `pai-manager vulkan-probe`, on this
//! machine's GPUs, is a hand run of the slice (the user's go-ahead), never a
//! test.
//!
//! Names the tests assume, the contract for the coder: `VulkanProbe::new(exe,
//! timeout)` for a test and `VulkanProbe::for_node()` for the manager, and
//! `probe(now)` with `now` in whole seconds since the Unix epoch (the unit
//! `NodeDevices`' `probed_at` uses), answering a `pai_core::vulkan::Probe`.

#[cfg(unix)]
use crate::manager::service::VulkanProbe;
use crate::manager::service::{VULKAN_CACHE_SECONDS, VULKAN_PROBE_TIMEOUT};
use std::time::Duration;
// The scripted child, its sandbox and the harness that calls the seam are
// Unix only, so everything they need is too.
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::time::Instant;

/// The bound a test gives a child that answers at once: far over any spawn, far
/// under any test.
#[cfg(unix)]
const QUICK: Duration = Duration::from_secs(10);
/// The wall-clock deadline of the harness itself: a probe that never returns is
/// a red naming the bound, never a hang. The probe runs on its own thread, so a
/// hung one is left behind when the test panics.
#[cfg(unix)]
const HARNESS: Duration = Duration::from_secs(30);

/// The document of a node with one amdgpu card, as the child prints it (the
/// shape `pai_core::vulkan::parse` documents).
#[cfg(unix)]
const DOC: &str = r#"{"vulkan":"ok","gpus":[{"pci":"0000:03:00.0","name":"AMD Radeon AI PRO R9700","type":"discrete-gpu","driver":"RADV","info":"Mesa 25.2.0"}]}"#;

#[cfg(unix)]
mod scripted {
    use super::*;

    /// A temp dir with the fake child and the file its script appends to.
    pub struct Sandbox {
        pub dir: tempfile::TempDir,
    }

    impl Sandbox {
        pub fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("tempdir"),
            }
        }

        /// The child the probe runs.
        pub fn exe(&self) -> PathBuf {
            self.dir.path().join("pai-manager")
        }

        /// One line per run of the child, its arguments on it.
        pub fn counter(&self) -> PathBuf {
            self.dir.path().join("runs.txt")
        }

        /// The pid the script writes when it hangs.
        pub fn pidfile(&self) -> PathBuf {
            self.dir.path().join("pid.txt")
        }

        /// Write `body` as the child and make it runnable; its path. Every body
        /// starts by logging its run, so the runs are counted.
        pub fn script(&self, body: &str) -> PathBuf {
            let exe = self.exe();
            let head = format!("echo \"run $*\" >> '{}'\n", self.counter().display());
            std::fs::write(&exe, format!("#!/bin/sh\n{head}{body}")).expect("script");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            exe
        }

        /// The lines the child appended: how often it ran, and with what
        /// arguments.
        pub fn runs(&self) -> Vec<String> {
            std::fs::read_to_string(self.counter())
                .unwrap_or_default()
                .lines()
                .map(|l| l.to_string())
                .collect()
        }
    }

    /// A child that prints `text` on stdout and exits 0.
    pub fn printing(sandbox: &Sandbox, text: &str) -> PathBuf {
        sandbox.script(&format!("cat <<'DOC'\n{text}\nDOC\nexit 0\n"))
    }

    /// The state letter of `pid` in `/proc`, `None` when it is gone. The command
    /// name may hold spaces and parens, so the state is the field after the
    /// last `)`.
    pub fn process_state(pid: u32) -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        stat.rsplit(')')
            .next()
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|state| state.chars().next())
    }
}

#[cfg(unix)]
use scripted::*;

/// One call of the seam, off the test's own thread: the bound is what is under
/// test, so a probe that ignores it must red as a bound failure and not hang the
/// suite. The answer, and the wall clock it took.
#[cfg(unix)]
fn call(probe: &Arc<VulkanProbe>, now: u64) -> (pai_core::vulkan::Probe, Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let one = Arc::clone(probe);
    std::thread::spawn(move || {
        let _ = tx.send(one.probe(now));
    });
    let at = Instant::now();
    let answer = match rx.recv_timeout(HARNESS) {
        Ok(answer) => answer,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
            "the probe never answered within {}: the bound of the child is not enforced",
            HARNESS.as_secs()
        ),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // A panic of the seam's own thread: the stub is `unimplemented!()`,
            // and the red reads as the harness's own death unless it is named.
            panic!(
                "the probe's `probe()` panicked (its own panic is printed above): the \
                 production Vulkan probe of the node is not implemented"
            )
        }
    };
    (answer, at.elapsed())
}

#[cfg(unix)]
fn vulkan(exe: &Path, timeout: Duration) -> Arc<VulkanProbe> {
    Arc::new(VulkanProbe::new(exe, timeout))
}

// ── The child answers ─────────────────────────────────────────────────────

#[test]
#[cfg(unix)]
fn the_fake_child_printing_a_document_gives_its_rows() {
    let box_ = Sandbox::new();
    let exe = printing(&box_, DOC);

    let (answer, _took) = call(&vulkan(&exe, QUICK), 0);

    assert_eq!(
        answer.status,
        pai_core::vulkan::Status::Ok,
        "the child read the node: {answer:?}"
    );
    assert_eq!(answer.devices.len(), 1, "one card, one device: {answer:?}");
    assert_eq!(answer.devices[0].pci.as_deref(), Some("0000:03:00.0"));
    assert_eq!(answer.devices[0].driver.as_deref(), Some("RADV"));
    assert_eq!(answer.devices[0].info.as_deref(), Some("Mesa 25.2.0"));
    let runs = box_.runs();
    assert_eq!(runs.len(), 1, "the child ran {:?}, once expected", runs);
    assert!(
        runs[0].contains("vulkan-probe"),
        "the child is this executable asked for its Vulkan drivers by that \
         subcommand, the hidden one the manager spawns: {runs:?}"
    );
}

#[test]
#[cfg(unix)]
fn a_child_printing_none_gives_none() {
    // No loader, or an ICD that refuses: the answer is `none` and every row of
    // the node's info keeps `vulkan: null`.
    let box_ = Sandbox::new();
    let exe = printing(&box_, r#"{"vulkan":"none"}"#);

    let (answer, _took) = call(&vulkan(&exe, QUICK), 0);

    assert_eq!(
        answer.status,
        pai_core::vulkan::Status::None,
        "a node with no Vulkan answers none: {answer:?}"
    );
    assert_eq!(answer.devices, Vec::new(), "none reads no GPU");
    assert_eq!(box_.runs().len(), 1, "the child ran once");
}

#[test]
#[cfg(unix)]
fn a_child_printing_what_is_not_the_document_gives_failed() {
    // A vendor's loader printing a banner on stdout, a crash printing nothing:
    // the text is not the document, so the read failed.
    let box_ = Sandbox::new();
    let exe = printing(&box_, "vulkaninfo: no devices (shipped by vulkan-tools)");

    let (answer, _took) = call(&vulkan(&exe, QUICK), 0);

    assert_eq!(
        answer.status,
        pai_core::vulkan::Status::Failed,
        "text that is not the document is failed: {answer:?}"
    );
    assert_eq!(answer.devices, Vec::new());
}

#[test]
#[cfg(unix)]
fn a_child_flooding_past_the_stream_cap_gives_failed() {
    // The child's stdout is capped by `proc`'s stream cap: a document over it is
    // kept only up to the cap, which cuts it, and a cut document is read as
    // nothing. The manager is never held by a loader that prints without end.
    let box_ = Sandbox::new();
    let row = r#"{"pci":"0000:03:00.0","name":"AMD Radeon AI PRO R9700","type":"discrete-gpu","driver":"RADV","info":"Mesa 25.2.0"},"#;
    let exe = box_.script(&format!(
        "printf '%s' '{head}'\nyes '{row}' | head -c 4194304\nprintf '%s' '{tail}'\nexit 0\n",
        head = r#"{"vulkan":"ok","gpus":[{"pci":"0000:03:00.0","name":"R7900","type":"discrete-gpu","driver":"RADV","info":"Mesa"},"#,
        tail = r#"{"pci":"0000:05:00.0"}]}"#,
    ));

    let (answer, _took) = call(&vulkan(&exe, QUICK), 0);

    assert_eq!(
        answer.status,
        pai_core::vulkan::Status::Failed,
        "a document over the stream cap is read as nothing: {answer:?}"
    );
    assert_eq!(answer.devices, Vec::new(), "a refused read names no GPU");
}

// ── The bound ─────────────────────────────────────────────────────────────

#[test]
#[cfg(unix)]
fn a_child_that_oversleeps_the_bound_is_failed_within_the_bound_plus_one() {
    let box_ = Sandbox::new();
    let (exe, pidfile) = (box_.exe(), box_.pidfile());
    std::fs::write(
        &exe,
        format!(
            "#!/bin/sh\necho \"run $*\" >> '{}'\necho $$ > '{}'\nsleep 120\n",
            box_.counter().display(),
            pidfile.display()
        ),
    )
    .expect("script");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let bound = Duration::from_secs(2);
    let probe = vulkan(&exe, bound);

    let (answer, took) = call(&probe, 0);

    assert_eq!(
        answer.status,
        pai_core::vulkan::Status::Failed,
        "a child that is killed reads nothing: {answer:?}"
    );
    assert!(
        took >= Duration::from_millis(1800),
        "the answer came back after {took:?}, under the bound of {bound:?}: the \
         child was not given its bound"
    );
    assert!(
        took < bound + Duration::from_secs(1),
        "the child sleeps 120 s: the probe waited {took:?} for it against a bound \
         of {bound:?}, over the bound plus one second"
    );

    // The child is stopped, not left behind: a page polled every few seconds
    // would fill the node with hung ICD loaders. A killed child the manager has
    // not reaped yet (state Z) counts as stopped.
    let pid: u32 = std::fs::read_to_string(&pidfile)
        .expect("the hanging child wrote its pid")
        .trim()
        .parse()
        .expect("a pid");
    #[cfg(target_os = "linux")]
    {
        let state = process_state(pid);
        assert!(
            state.is_none() || state == Some('Z'),
            "the killed child is still running (state {:?})",
            state
        );
    }
}

#[test]
fn the_probe_bound_is_the_eight_seconds_of_the_design() {
    assert_eq!(
        VULKAN_PROBE_TIMEOUT,
        Duration::from_secs(8),
        "ARCHITECTURE.md \"About and node info\": the child runs under an 8 s \
         bound, under the bff's 10 s for the whole node read"
    );
    assert_eq!(
        VULKAN_CACHE_SECONDS,
        10 * 60,
        "the answer is cached for 10 minutes, so a polled page spawns at most \
         one child over the window"
    );
}

// ── One child for many callers, and one per window ────────────────────────

#[test]
#[cfg(unix)]
fn two_calls_at_the_same_time_run_the_child_once() {
    // The child starts a loader and every ICD of the node, so two at once
    // contend for the cards they are there to read: the caller that arrives
    // while a child runs takes that child's answer, including when it failed.
    let box_ = Sandbox::new();
    let exe = box_.script(&format!("sleep 1\ncat <<'DOC'\n{DOC}\nDOC\nexit 0\n"));
    let probe = vulkan(&exe, QUICK);

    let first = {
        let one = Arc::clone(&probe);
        std::thread::spawn(move || one.probe(0))
    };
    // The first call is inside its child (the script sleeps a second); the
    // second arrives while that child runs.
    std::thread::sleep(Duration::from_millis(200));
    let (second, _took) = call(&probe, 0);

    let first = match first.join() {
        Ok(answer) => answer,
        Err(_) => panic!("the first call's thread panicked (its panic is above)"),
    };
    assert_eq!(first.status, pai_core::vulkan::Status::Ok, "{first:?}");
    assert_eq!(
        second, first,
        "the caller that queued behind the child takes its answer"
    );
    assert_eq!(
        box_.runs().len(),
        1,
        "two callers, one child: {:?}",
        box_.runs()
    );
}

#[test]
#[cfg(unix)]
fn a_call_inside_the_cache_window_runs_the_child_not_at_all() {
    let box_ = Sandbox::new();
    let exe = printing(&box_, DOC);
    let probe = vulkan(&exe, QUICK);

    let (first, _) = call(&probe, 0);
    assert_eq!(first.status, pai_core::vulkan::Status::Ok);
    assert_eq!(box_.runs().len(), 1, "the first call spawns the child");

    // A moment inside the window: the answer is kept, the child does not run.
    let (kept, _) = call(&probe, VULKAN_CACHE_SECONDS - 1);
    assert_eq!(
        kept, first,
        "the cached answer is the one the first call answered"
    );
    assert_eq!(
        box_.runs().len(),
        1,
        "a polled page inside the window spawns no child: {:?}",
        box_.runs()
    );

    // The window is over: the node is read again, so a driver upgrade shows.
    let (again, _) = call(&probe, VULKAN_CACHE_SECONDS);
    assert_eq!(again.status, pai_core::vulkan::Status::Ok);
    assert_eq!(
        box_.runs().len(),
        2,
        "the window is 10 minutes: at its end the child runs again: {:?}",
        box_.runs()
    );
}

#[test]
#[cfg(unix)]
fn a_failed_answer_is_cached_like_any_other() {
    // "The Vulkan answer (success, `none` or `failed`) is cached for 10
    // minutes": a hung ICD costs one child per window, not one per poll.
    let box_ = Sandbox::new();
    let exe = printing(&box_, "the loader printed no document");
    let probe = vulkan(&exe, QUICK);

    let (first, _) = call(&probe, 0);
    assert_eq!(first.status, pai_core::vulkan::Status::Failed);
    let (kept, _) = call(&probe, VULKAN_CACHE_SECONDS - 1);

    assert_eq!(kept, first, "the failed answer is the cached one");
    assert_eq!(
        box_.runs().len(),
        1,
        "a failed answer spawns no second child inside the window: {:?}",
        box_.runs()
    );
}

// ── The production wiring (the source's text, comments left out) ──────────

/// The manager's own probe runs THIS executable, under the design's constant,
/// and spawns it through the bounded reader. `for_node` is the wiring a test
/// cannot run: the child it names is the real one, reading this machine's GPUs,
/// which no test does. What the tests above pin is the behaviour of the seam as
/// such, over a script.
#[test]
fn the_production_probe_runs_the_managers_own_exe_bounded() {
    let service = code(include_str!("../manager/service.rs"));
    let vulkan_impl = impl_text(&service, "VulkanProbe");
    let for_node = method_text(&vulkan_impl, "for_node");

    // The reader works: the engine probe's own `for_node` is in this same file,
    // outside the slice's impl, and is read beside it.
    let gpus_for_node = method_text(&service, "for_node");
    assert!(
        gpus_for_node.contains("llama_server_exe()"),
        "the reader of an indented `for_node` found no engine exe: {gpus_for_node}"
    );

    let args = call_arguments(&for_node, "new").unwrap_or_else(|| {
        panic!("`VulkanProbe::for_node` builds no probe, over no exe:\n{for_node}")
    });
    assert!(
        args.contains("current_exe()"),
        "the child is the manager's own executable (`pai-manager vulkan-probe`), \
         read from `std::env::current_exe`: {args}"
    );
    assert!(
        args.contains("VULKAN_PROBE_TIMEOUT"),
        "the child is bound by the design's constant, not a figure of its own: {args}"
    );
    assert!(
        method_text(&vulkan_impl, "probe").contains("run_hidden_probe_within"),
        "the child is spawned through the bounded, capped reader of `proc` \
         (killed at the bound, its streams read while it runs)"
    );
}

// ── The source readers (the shape `tests/gpu_probe.rs` uses) ──────────────

/// `src` with the whole-line comments left out.
fn code(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of `impl <name>`, from its header to the brace that closes it at
/// column 0. A method read of the whole file cannot tell two impls apart (both
/// of the node's probes have a `for_node`), so the slice's methods are read
/// inside their own impl.
fn impl_text(src: &str, name: &str) -> String {
    for prefix in ["impl ", "impl<", "impl "] {
        let open = format!("\n{prefix}{name}");
        if let Some(at) = src.find(&open) {
            let body = &src[at..];
            let end = body
                .find("\n}\n")
                .unwrap_or_else(|| panic!("the end of `impl {name}`"));
            return body[..end].to_string();
        }
    }
    panic!("no `impl {name}` at column 0 in this source");
}

/// The text of an `fn <name>` indented in an `impl`, from its signature to the
/// brace that closes it at column 4. A method's own brace, not the `impl`'s: a
/// reader that stopped at the next column-0 brace would swallow every method
/// beside it and pass on a needle in a neighbour.
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

/// The arguments of the first `name(...)` call in `hay`, up to that call's own
/// closing paren (a nested call is counted, not followed); `None` when `hay`
/// never calls it.
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
