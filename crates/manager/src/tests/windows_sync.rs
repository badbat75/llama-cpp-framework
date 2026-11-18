//! Windows: the SCM's services follow the instance record. The manager's
//! start plans the services against `instances.ini`, so a section of the
//! record gets a service and a `server@<profile>.ini` no section names has
//! none. A record that names no instance plans nothing: the entry syncs a list
//! only when it is not empty, and reports the empty one as it reports a read
//! that failed. The plan is the core's OS-neutral decision
//! (`windows::service_plan`), so it runs here on every OS; the Win32 glue that
//! applies it is `cfg(windows)` and is pinned from its text.

use crate::manager::service::fakes::{FakeAuthority, FakeResolver, FakeService};
use crate::manager::service::{Manager, INSTANCES_FILE};
use crate::manager::windows::service_plan;

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// The code of `fn run_manager` in entry.rs (the manager's service entry:
/// the one that serves the named pipe), from its signature to the brace that
/// closes it at column 0, whole-line comments left out. entry.rs is
/// `cfg(windows)`: type-checked on Windows only, so its wiring is pinned from
/// its text, and a comment naming a needle never greens it. It pins names and
/// wiring in the source, not behaviour.
fn run_manager_code() -> String {
    let entry = include_str!("../manager/windows/entry.rs")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let at = entry
        .find("\nfn run_manager(")
        .expect("entry.rs has `fn run_manager`");
    let body = &entry[at..];
    body[..body.find("\n}\n").expect("the end of `fn run_manager`")].to_string()
}

/// The position of `needle` in `hay`, present: an order pin is `at(a, x) <
/// at(b, y)`; a missing needle panics naming what is absent.
fn at(hay: &str, needle: &str) -> usize {
    hay.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is never in:\n{hay}"))
}

/// The `match` whose scrutinee holds `needle`, from the `match` keyword to the
/// brace that closes it: every arm is read inside it, so a `report(` of a call
/// outside the match is never taken for one of its arms.
fn match_on(code: &str, needle: &str) -> String {
    let at = code
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is never in:\n{code}"));
    let braces = code[at..]
        .find('{')
        .unwrap_or_else(|| panic!("no `{{` after `{needle}` in:\n{code}"));
    let body = &code[at + braces + 1..];
    let mut depth = 1usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return code[at..at + braces + 1 + i].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("the `match` on `{needle}` never closes:\n{code}")
}

/// The arms of a `match`: the text of each, pattern through body, split at the
/// lines of its own indentation (the arms of `run_manager`'s match are
/// indented 8, the code inside an arm 12 and more).
fn arms(m: &str) -> Vec<String> {
    let mut starts = Vec::new();
    let mut at = 0usize;
    for line in m.split_inclusive('\n') {
        let t = line.trim_start();
        if (t.starts_with("Ok(") || t.starts_with("Err(")) && line.len() - t.len() == 8 {
            starts.push(at);
        }
        at += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, from)| {
            let to = starts.get(i + 1).copied().unwrap_or(m.len());
            m[*from..to].trim().to_string()
        })
        .collect()
}

/// The pattern of an arm: the text before its `=>`.
fn pattern(arm: &str) -> &str {
    &arm[..arm
        .find("=>")
        .unwrap_or_else(|| panic!("an arm with no `=>`:\n{arm}"))]
}

/// The record on disk and the SCM's services: the plan the manager's start
/// applies. `gpu1` runs the `big` profile, so it gets a service; `lan` is a
/// profile no section names, so its service goes; a service that is not ours
/// is left alone.
pub(crate) fn manager_windows_sync_e2e() {
    let dir = tempfile::tempdir().expect("tempdir");
    crate::paths::redirect_for_tests(dir.path());
    let sys = crate::paths::engine_config_dir();
    std::fs::create_dir_all(&sys).expect("engine config dir");

    // Three server profiles on disk, two sections in the record: the `lan`
    // profile is nobody's, `gpu1` has no server@gpu1.ini.
    let ini = "[Server]\nPort = 8080\nHostname = 127.0.0.1\n";
    for name in ["server@default.ini", "server@big.ini", "server@lan.ini"] {
        std::fs::write(sys.join(name), ini).expect(name);
    }
    std::fs::write(
        sys.join(INSTANCES_FILE),
        "[default]\nServerProfile = default\n\n[gpu1]\nServerProfile = big\n",
    )
    .expect(INSTANCES_FILE);

    let m = Manager::new(
        FakeService::new(),
        FakeAuthority::new(true),
        FakeResolver::lan(),
    );
    assert_eq!(
        m.list_instances(),
        ids(&["default", "gpu1"]),
        "the sections of the record are the instances, not the profiles on disk"
    );

    // The SCM's services, as an enumeration of them would report them: a
    // service for `default`, and leftovers of instances the record no longer
    // names (`big`, `lan`), plus services that are not ours.
    let services = ids(&[
        "llama.cpp_default",
        "llama.cpp_big",
        "llama.cpp_lan",
        "Spooler",
        "Dnscache",
    ]);
    let plan = service_plan(&m.list_instances(), &services);
    assert_eq!(
        plan.create,
        ids(&["gpu1"]),
        "the record's new section gets a service"
    );
    assert_eq!(
        plan.keep,
        ids(&["default"]),
        "the section that already has one keeps it"
    );
    assert_eq!(
        plan.delete,
        ids(&["big", "lan"]),
        "the profiles no section names lose their services, nobody else's is touched"
    );

    // A record that reads and names no instance: an empty file, a write cut
    // mid-header, a file of comments, a section whose id the core refuses. The
    // fallible read answers the Ok arm with an empty list, exactly as a record
    // of sections answers it with a list, and the plan of an empty list
    // deletes the service of every instance: the entry cannot tell this answer
    // a plan, so it goes where a failed read goes.
    for (what, text) in [
        ("an empty record", ""),
        ("a record cut mid-header", "[defe"),
        ("a record of comments", "; the sections are gone\n\n"),
        (
            "a record of refused ids",
            "[Old]\nServerProfile = default\n",
        ),
    ] {
        std::fs::write(sys.join(INSTANCES_FILE), text).expect(what);
        let read = crate::paths::try_instance_ids().expect(what);
        assert_eq!(
            read,
            Vec::<String>::new(),
            "{what}: a record that reads lands in the Ok arm"
        );
        let empty = service_plan(&read, &services);
        assert_eq!(empty.create, Vec::<String>::new(), "{what}");
        assert_eq!(
            empty.delete,
            ids(&["default", "big", "lan"]),
            "{what}: a sync of this list deletes every service of ours"
        );
    }

    // The glue: the manager's service entry applies the plan through the
    // service, from the record, before it serves the pipe (the SCM starts the
    // services whose start type is automatic during that start).
    let run = run_manager_code();
    assert!(
        run.contains("service().sync("),
        "the plan is applied by the service, the SCM, not in the core:\n{run}"
    );
    // The ids come from the fallible read of the record: the record's
    // sections plan the services, not the files on disk, and not the
    // swallowing list that reads a record it cannot open as "no instances".
    assert!(
        run.contains("try_instance_ids("),
        "the entry reads the record fallibly, beside it acts only on Ok:\n{run}"
    );
    assert!(
        !run.contains("list_instances()"),
        "the entry must not feed sync the list that swallows a failed read:\n{run}"
    );
    // The match on the fallible read, arm by arm: one arm plans, every other
    // arm reports. The empty list is the answer of a record that READS (the
    // empty file, the cut one, the record of refused ids) as well as of a
    // record that names no instance, and a sync of it deletes the service of
    // every instance, so the entry plans only a list that is not empty and
    // reports the empty one where it reports a read that failed.
    // `sync_report` is told the io error of the read (`service.rs`:
    // `sync_report(&self, why: &std::io::Error)`), so every arm that reports
    // hands a borrowed io error: the empty arm has no read failure to hand, so
    // it makes one (a kind of its own, the record being readable).
    let m = match_on(&run, "match crate::paths::try_instance_ids(");
    assert!(
        at(&m, "try_instance_ids(") < at(&m, "Ok("),
        "the read is matched"
    );
    let arm_list = arms(&m);
    assert!(
        arm_list.len() > 2,
        "the empty list needs an arm of its own, beside the read that failed:\n{m}"
    );
    let syncing = arm_list.iter().filter(|a| a.contains("sync(")).count();
    assert_eq!(
        syncing, 1,
        "one arm plans and the rest report, and none of them a second time:\n{m}"
    );
    for arm in &arm_list {
        let pat = pattern(arm);
        if arm.contains("sync(") {
            assert!(
                pat.contains("if") && pat.contains("is_empty()") && pat.contains('!'),
                "the sync runs only for a list that is not empty:\n{arm}"
            );
        } else {
            assert!(
                arm.contains("manager.sync_report(&"),
                "a record with nothing to plan is reported, the manager still serves the pipe:\n{arm}"
            );
        }
    }
    // An `Ok` pattern with no guard accepts the empty record too, so it is one
    // that must plan nothing.
    for arm in &arm_list {
        let pat = pattern(arm);
        if pat.starts_with("Ok(") && !pat.contains("if") {
            assert!(
                !arm.contains("sync("),
                "the arm that accepts an empty record plans nothing:\n{arm}"
            );
        }
    }
    assert_eq!(
        run.matches("sync(").count(),
        1,
        "no second sync: an arm that reports names no list, so no list is synced"
    );
    assert!(
        run.find("sync(").expect("the sync") < run.find("pipe_server::serve(").expect("the pipe"),
        "the services are set up before the pipe serves"
    );
    assert!(
        run.contains("report(&failure)"),
        "a service the SCM refused is reported, the manager still serves the pipe"
    );

    // Where the new refusal text gets its arguments: the record's path
    // fills `{0}` and the id fills `{1}`, the order the strings file's text
    // is pinned to in this crate's strings tests.
    let cli = include_str!("../manager.rs");
    assert!(
        cli.contains(r#"strings::fill("manager-no-config", &[&p.display(), &id])"#),
        "manager-no-config is filled path first, id second"
    );
    // The uninstall keeps its sync against the empty list: that one deletes
    // every service on purpose. Only a read that FAILED at the manager's
    // start must plan nothing.
    assert!(
        cli.contains("sync(&[])"),
        "UninstallServices keeps syncing the empty list to delete the services"
    );
}
