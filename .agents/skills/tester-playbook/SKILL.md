---
name: tester-playbook
description: What the tester has learned about writing reliable tests. Use when writing or debugging tests.
---

Lessons only: the rules are in `AGENTS.md`.

- Never wait without a deadline. A test polling for a value hung forever on a stub in another thread: a
  test must fail, not hang.
- Check why a test is red, or why it is already green. A validation test once passed for the wrong reason,
  because the field did not exist yet. Recheck such tests once the code exists.
- Keep one fixture per test. An id taken from one temporary fixture will not match another, and the test
  fails with the wrong error.
- Use real names and values in fixtures, captured from the real system. They find what invented data hides.
- A fake holds whatever you write, while the real system can answer differently. Do not treat a passing
  test on a fake as proof about the real thing.
- In the configurator (package `pai-config` in `crates/config/`; filter with
  `cargo test -p pai-config <name>`), a test that needs `PRIVATE_AI_DATA_ROOT` (anything reading `paths::`) is not
  a `#[test]` of its own: write a `#[cfg(test)] pub(crate) fn <name>_e2e()` that calls
  `crate::paths::redirect_for_tests(dir.path())` on its own tempdir (it sets the variable and asserts the
  paths follow it before any IO, so a broken redirect fails instead of writing the real tree), and call it at the end of `editable_widgets_track_model_after_edit` in
  `src/tests/ui_bindings.rs`; filter on that name to run it. A phase of a module in pai-app
  (`crates/app`: presets, bench, server_control, migration, startup...) goes in pai-app's own runner,
  `data_root_e2e` in `crates/app/src/tests/mod.rs`, the same rule in that crate. Pure helpers stay plain unit tests. Nothing
  unsets the variable safely, so the real-path branches (`/etc`, `%ProgramData%`) go untested.
- Stubs with `todo!("<fn>")` give a red that names the missing function; prefer them to a compile error,
  which hides every other test in the binary.
- To pin a launch, build a `std::process::Command` and read it back with `get_program`, `get_args`,
  `get_envs` and `get_current_dir`; take the exe as a parameter, because `paths::llama_server_exe()` finds
  nothing from the test binary's directory. Compute the expected args with the existing `server_args()`
  instead of hard-coding the list.
- A single file can be formatted with `rustfmt --edition 2021 <file>`; `cargo fmt -p <crate> --check` then
  shows whether anything else is left.
- To pin a field a variant does not have yet without breaking the build, match with `Variant(p, ..)` (it
  compiles for one field or more) and check the new field through `{:?}`; tell the coder the intended
  shape. A compile error would hide every other test in the binary, the lints included.
- Since `src/lib.rs` exists, the binary roots (`main.rs`, `bin/*.rs`) are separate crates no module can
  call, while `lib.rs` is reachable from every module: a lint may exempt the former, never the latter.
- To make a write fail without a fake filesystem, chmod the directory `0o555` (Unix), probe that a write
  really fails (root ignores the mode) and put the mode back before asserting. Then check the old content
  and that the directory listing is unchanged, which also proves no temp file was left.
- A seam the code calls through `&self` (a service manager, polkit) gets a fake with `Mutex` logs (not
  `RefCell`: the manager is shared across threads, so its seams must be `Sync`) read
  back through an accessor on the struct under test; assert the recorded calls are empty to pin "nothing
  happened" on a refusal, as well as the returned error kind.
- Comments never cite PLAN.md step or slice numbers: steps are renumbered and removed when done. Name
  the feature instead ("running llama-server as a service").
- To test a zbus interface without a bus (zbus 5, `zbus::blocking`): `UnixStream::pair()`, the server end
  `Builder::async_io_unix_stream(s0).server(Guid::generate())?.p2p().serve_at(..)?.build()` on its own
  thread (both ends handshake at once; plain `unix_stream` is deprecated), the client end
  `.p2p().method_timeout(..)` (the deadline: a panicking method otherwise hangs the call). A p2p call
  carries no sender; to stamp one as the bus would, `client.inner().set_unique_name(":1.42")` after the
  build (dev feature `bus-impl`). Error replies named under `org.freedesktop.DBus.Error` come back as
  `zbus::Error::FDO`, any other name as `MethodError`.
- `git grep` searches only tracked files: after a move that is not yet `git add`ed (a new `crates/`), it
  finds nothing there. Use plain `grep -rn` on the new directory.
- Most xtask path code (`version::framework`, `strings_share`, the font licence in `licenses::stage_in`)
  has no test: after a path change, check the targets exist by hand and say so.
- To list or run the tests of an older commit: `git worktree add --detach build/<name> <rev>`, copy the
  gitignored `resources/llama.ico` (and `llama-brand-logo.svg`) into it, set
  `PRIVATE_AI_LLAMA_CPP_DIR=<repo>/build/linux/llama.cpp` (`LLAMA_FW_LLAMA_CPP_DIR` before commit
  7bddcff; build.rs reads the clone's `llama-arch.cpp`)
  and a separate `CARGO_TARGET_DIR`, since the repo's `.cargo/config.toml` would point the worktree at
  the main target dir. Compare `-- --list` names with `LC_ALL=C sort`, else `comm` complains.
- To check that a lint still bites after a refactor, plant a violation in a scratch copy (rsync the
  working tree into that worktree), never in the real tree. In binding_lint only the keys rules
  (`strings_t_keys_...`, `cli_only_code_...`) walk every crate's `src/`; `cli_print_paths_hold_no_language`
  reads only the files named in `CLI_PRINT_PATHS`, so a literal `println!` elsewhere is not caught.
- Several filters go in one run: `cargo test -p <crate> -- a b c` (libtest takes many). A loop of one
  `cargo test` per filter took minutes each on pai-config; one run with every filter took one.
- A names lint (`crates/config/src/tests/names_lint.rs`, `xtask/src/names_lint.rs`) flags old names in
  code lines; a test that must spell an old name (a negative assertion, the old-variable-is-ignored check)
  splits it with `concat!("llama-cpp", "-config")` so the lint stays clean.
- The names lints also flag `join("llama.cpp")`: a test building an old tree spells the directory
  `concat!("llama", ".cpp")`. Only `crates/core/src/migrate.rs` may name the old tree and login entry
  (`MIGRATION_MODULE` / `MIGRATION_NAMES` in the pai-config names lint); a fixture under a temp
  `core/src/migrate.rs` exercises the exemption, since `hits` matches the path by its tail.
- A stub reached only from tests fails `cargo clippy -D warnings` with dead code until the coder wires
  it in: say so in the report instead of adding `allow`s.
- To force a copy fallback without a second filesystem, inject the rename (`&dyn Fn(&Path, &Path)`)
  and fail it for one source path only, so the final temp-to-new rename still runs for real.
- `#[cfg(windows)]` test code cannot be type-checked from Linux: `cargo check --target
  x86_64-pc-windows-msvc` stops in libsqlite3-sys's build script (no MSVC compiler). Review it by
  reading, and give a registry test a guard whose `Drop` restores every value it touches (no panic in
  the `Drop`).
- docs.fedoraproject.org blocks scripted fetches (Anubis): read the packaging guidelines through
  `https://web.archive.org/web/2026/https://docs.fedoraproject.org/en-US/packaging-guidelines/` and say
  it is an archive copy. Check an rpm version bound with `rpm --eval '%{lua:
  print(rpm.vercmp("2.4.0+llama.cpp.v0.9.0", "2.4.1"))}'` (-1, 0, 1): `2.4.0+x` sorts above `2.4.0`.
- A text-pin test on a file the coder will rename or rewrite (the `.wxs`): also check it goes green on a
  candidate edit in a temporary copy, then delete the copy, so the red is not a broken helper.
- `benchmarks/schema.sql` is not pinned by digest: pai-config `include_str!`s it and
  `the_repo_database_matches_the_schema` compares only the columns with the committed bench.db. SQLite
  keeps each statement's text from `CREATE` on in `sqlite_master`, so a comment line between statements
  changes nothing recorded (check with `sqlite3 -readonly benchmarks/bench.db "select sql from
  sqlite_master"`); a comment inside a `CREATE` would.
- To pin "a slow seam holds up nobody else" (polkit waiting for a password), use `fakes::Gate` in
  `crates/manager/src/manager/service.rs` (`GateAuthority`, `GateResolver`): the test waits with
  `gate.arrived(n)`, runs the other calls through `on_thread(..)` + `recv_timeout`, opens the gate
  before panicking on a timeout, and checks `try_recv()` is still empty to prove the held call never
  finished early. A race is pinned deterministically by holding one call at a seam *inside* the
  critical section (the address check's lookup of another instance's host) and accepting either
  serialization as green.
- zbus 5 (async-io backend, the one this workspace builds) runs every task of a connection, the
  method dispatchers included, on one `zbus::Connection executor` thread
  (`zbus-5.19.0/src/connection/builder.rs` `start_internal_executor`); `spawn` is on by default
  (`object_server/mod.rs` `dispatch_message`), but a sync `fn` method runs inline in its task and
  blocks that thread for every other call on the connection.
- To skip one e2e phase for a red check, remove the `#[cfg(..)]` line with the call (a lone attribute
  does not compile), on a backup of the file restored after.
- `server_cfg::render` writes ModelsDir and StateDir RESOLVED, so an unset one goes through `paths::`
  (home dir, data root): a unit test that renders a `ServerConfig` sets both explicitly.
- Before handing over a large set of red tests, implement a throwaway candidate in place (copy the
  stub, the strings files and any wired file to /tmp, write the candidate, run, `cp` them all back,
  check `git status`): it shows the fixtures can go green, so the red is the missing code and not a
  wrong expectation.
- "No manager" over zbus 5 without a real bus: serve the interface at another path (the call gets
  `org.freedesktop.DBus.Error.UnknownObject` as `zbus::Error::FDO`), or drop the server connection (the
  next call fails at once with `zbus::Error::InputOutput`, no timeout); a call with destination
  `Some(BUS_NAME)` is accepted on p2p. No system bus at all: `DBUS_SYSTEM_BUS_ADDRESS=unix:path=<tempdir>/none`
  (zbus reads it in `Address::system`), in the one test of the binary that touches it.
- `recv_timeout` on `on_thread`'s receiver returns `Disconnected` at once when the thread panicked
  (a `todo!` stub): report that apart from `Timeout`, or the red reads as a hang.
- To pin the order of a call to llama-server against a call to a fake (the snapshot before the
  manager's stop), share one log: `server_control::fake::http_stub(&log)` answers `/models`, `/health`
  and a slot restore on a free loopback port and records `"<METHOD> <path>"` beside the
  `FakeClient`'s `"<method> <id>"`; point server.ini's `Port` at it in an e2e phase.
- In a test build `server_control::backend()` is `fake::current()`: the backend `fake::scoped(b, || ..)`
  set on this thread (not on threads it spawns), else `fake::default_backend()`, a manager over
  `DiskClient` (every action and probe `Unavailable`, so no test spawns or kills a llama-server; the
  config files at the per-user paths, so the e2e phases that read `paths::presets_ini()` keep working).
  Pin where the manager's bytes go with `FakeClient` (`with_file`, the shared `files`, `fail_write`,
  `fake::writes(&log)`); `fake::actions` leaves out status probes and file reads. `os_backend()` is the
  real OS choice.
- To prove a test run never dials the real system bus, start a Unix listener that logs each accept
  (a 10-line `python3 -I` script outside the repo), run `cargo test` with
  `DBUS_SYSTEM_BUS_ADDRESS=unix:path=<that socket>` and check the log is empty. The callers of
  `server_control::backend()` are in cli.rs, gui.rs, slot_state.rs and bench/; filtering on
  `bench:: cli:: slot_state gui::` plus the e2e covers every path that can reach it.
- To pin something that needs `PRIVATE_AI_DATA_ROOT` UNSET in pai-config (the e2e sets it on another
  thread), re-run one ignored test of the same binary in a child: `current_exe() --exact <path>
  --include-ignored --test-threads 1`, a marker env var, `env_remove` the redirect, a deadline on the
  child (`try_wait` loop, kill on timeout) and `"1 passed"` in its stdout
  (`system_connect_never_dials_from_a_test_binary`, `real_paths_follow_the_new_names` in pai-core).
- A source lint that must ignore test code uses `production_lines` in `tests/control_lint.rs`: it drops
  every `#[cfg(test)]` / `#[cfg(all(test ..` item by counting braces per line (not `any(test, ..)`,
  which is compiled in production too). Test code sits mid-file in gui.rs, presets.rs and
  integrations.rs, so "everything after the first `#[cfg(test)]`" is wrong.
- A fast throwaway candidate for "route this path-based IO through the seam": copy the seam's files
  into a temp dir, run the existing path-based function there, write back the files that changed, in
  the order the test expects. It showed the whole e2e can go green without writing the real refactor.
- A fake child process for an injected spawner: `sh -c '<printf ...>; exec sleep 30'` piped like the real
  one (`exec`, so the pid you hold is the one killed). "Killed and reaped" is `/proc/<pid>` gone (a zombie
  keeps it), polled with a deadline. Pass `'` inside the script as `'"'"'`.
- slint's `ElementHandle` has no visibility query: a `visible: false` element still answers
  `find_by_accessible_label`. Pin a note that shows only sometimes by asking for an `if` in the page and
  checking whether the search finds it. A `Text`'s accessible label is its text.
- journalctl's access-refusal texts are in systemd's `src/shared/journal-util.c`
  (`access_check_var_log_journal`, `journal_access_check_and_warn`); `strings $(which journalctl)
  /usr/lib/x86_64-linux-gnu/systemd/libsystemd-shared*.so` shows the installed version's wording.
- `FakeClient::fail_write_of` (name -> error) refuses one manager file and writes the others: the way
  to pin "a refusal in the middle of several writes stops there". A second fake over the same
  `files` (`c.files = files.clone()`) continues from what the first one wrote.
- A function over per-user files that takes the directory as a parameter (`migration::import_*`,
  `user_dir: &Path`) is tested by plain `#[test]`s on a tempdir, each with its own red, instead of one
  more e2e phase. Keep in the e2e only what reads `paths::` (a `config_path` under the redirect: the
  e2e changes `PRIVATE_AI_DATA_ROOT` on another thread, so a unit test comparing system paths races).
- The Linux package tests (`xtask/src/linux/package.rs`) check a spec's `%files` against a temp tree
  as rpmbuild does (`check_files`: every line takes a file, every file is taken, no system directory
  claimed); reuse it for any new packaged path. The ROCm libraries find their kpack archives at
  `<lib dir>/../.kpack` (`strings libhipblaslt.so | grep kpack`), so the runtimes' directory needs a
  parent the package owns.
- ggml (`ggml/src/ggml-backend-reg.cpp`, `ggml_backend_load_best`) looks for backends in the compiled-in
  `GGML_BACKEND_DIR`, then the executable's directory, then the cwd, keeping the first of equal
  score: a build with `GGML_BACKEND_DIR` prefers the installed backends over its own.
- A shared-library fixture is built as CMake lays it out (`std::os::unix::fs::symlink`: `.so -> .so.N
  -> .so.N.M.P`; a package ships only `.so.N` and the real file) and the found candidate is compared
  after `canonicalize`, so the test pins which library opens, not which of its names. Read the soname
  from the clone (`ggml/CMakeLists.txt` `GGML_VERSION_MAJOR`, `SOVERSION` in `ggml/src/CMakeLists.txt`)
  and `ls build/linux/llama.cpp-cmake/bin`, never from an older note.
- Packaging facts for maintainer scripts (checked on this machine): the local rpmbuild has no
  `%systemd_post` & co. (`rpm --eval '%systemd_post x'` echoes it back), so scriptlets are written out
  by hand; rpm 6 turns a sysusers.d `m user group` line into `Requires: group(<group>)`
  (`/usr/lib/rpm/fileattrs/sysusers.attr`, `_use_weak_usergroup_deps` makes it a Recommends). The
  debhelper snippets to mirror are in `/usr/share/debhelper/autoscripts/` (`postinst-systemd-enable`,
  `-restart`, `postinst-sysusers`, `prerm-systemd-restart`, `postrm-systemd`); dbus' dpkg trigger is
  `interest-noawait`, so a postinst cannot count on the bus policy being reloaded before it starts a
  Type=dbus unit. `man 5 systemd.exec` etc. are installed locally (systemd 259): cite them.
- A shell snippet in a test is checked with `sh -n` on stdin (parses, runs nothing). An `at(text,
  needle)` helper returning the position pins both presence and order of commands.
- A doc comment line starting with `1)` trips clippy's `doc_lazy_continuation`: reflow the sentence.
- A server or client over `impl Read + Write` is tested without threads by a scripted stream: a
  `Cursor` of the peer's frames for reads, an `Arc<Mutex<Vec<u8>>>` capturing writes, an optional
  `ErrorKind` failing every read or write. Compare JSON frames as `serde_json::Value`, never as bytes
  (key order). A real duplex for client-against-server is `UnixStream::pair()` with
  `set_read_timeout` on the client end as the deadline (`manager/pipe.rs`, `tests/pipe_client.rs`).
- Manager calls that are refused at authorization, or ask only `Status` with a valid id, never reach
  `paths::`, so they can be plain `#[test]`s; anything listing instances or touching a file is an
  e2e phase.
- A frame cap needs a test in both directions on each side: a client that sends an oversized request
  gets the server's hang-up, read as Unavailable or Other by timing, so the client refuses it first.
- Windows-only decisions are pinned as OS-neutral pure functions and constants in
  `crates/manager/src/manager/windows.rs` (SDDL, token facts, SCM spec, states, errors, service plan,
  log rotation) and `pai_core::client::pipe` (pipe name, client access and flags, open errors); only
  Win32 glue stays `cfg(windows)`. Win32 numeric values (access rights, error codes, flags) are read
  from the local windows-sys source (`~/.cargo/registry/src/*/windows-sys-0.61.2/src`, generated from
  Microsoft's metadata), the meaning from learn.microsoft.com. A crate's behaviour (windows-service
  quoting, which APIs it lacks) is read from its `.crate` at `static.crates.io`, unpacked in /tmp.
- A per-connection identity behind a shared `Authority` (the Windows pipe) is a thread-local set
  around the call (`TokenAuthority::with_caller`); pin that it is cleared on return and on panic, and
  that another thread sees none.
- The pai-config names lint refuses a bare `private-ai-manager` (the old executable) everywhere but
  in `MANAGER_SERVICE_NAMES` (the unit `.service`, the pipe `\pipe\private-ai-manager`) and the quoted
  string in `WINDOWS_SERVICE_MODULE` (`manager/src/manager/windows.rs`). A test that must name the
  manager's service elsewhere uses the constant, not the literal.
- pai-config's Windows code is compiled by nobody (Skia builds only with MSVC): pin each per-OS choice
  as a pure function of `server_control::Os` (`backend_kind`, `os_backend_for`, `log_source_for`,
  `log_rotate_note_key`) tested on Linux for every variant, with `Os::current()` written with `cfg!`
  (compiled on every OS) so only the Win32 call itself stays under `#[cfg(windows)]`.
- A fake one-shot runner returning `std::process::Output` needs an `ExitStatus`: on Unix
  `std::os::unix::process::ExitStatusExt::from_raw(code << 8)`; put such tests under `#[cfg(unix)]`,
  since the user's Windows `cargo test --workspace` compiles pai-config's tests too.
- `strings::t`/`fill` return the KEY itself when it is missing from the strings file: a test of a
  new message must assert its content (and `!text.starts_with("<key>")`), never only equality with
  `strings::fill("<new-key>", ..)`, which a stub using the same missing key would pass.
- "Only the seam may name X" is one more scan in `tests/control_lint.rs` (`production_lines` +
  a `*_hits(line)` helper + a scan test of the helper): `only_the_seam_reads_the_instances_log`
  for `paths::server_log` is the model.
- xtask's `windows/` module is `cfg(windows)`: nothing in it runs on Linux. Pin a Windows packaging
  choice as an item in `common/` (`common::msi::CARGO_EXES`) plus a text check that `windows/*.rs`
  uses it. The `.wxs` is checked as XML with roxmltree (xtask dev-dependency, `<?if?>` blocks parse as
  processing instructions, entities come back decoded): the helpers in `common/msi.rs` find a
  component's feature, and `dacl()` reads a `PermissionEx Sddl` into ACEs (aliases to SIDs, rights to
  masks) so an ACL is pinned by meaning, not spelling.
- WiX facts are read from docs.firegiant.com/wix/schema/wxs/<element>/ (the attribute table sits past
  character 100000: WebFetch with `offset: 100000`); what the compiler accepts where, from
  `src/wix/WixToolset.Core/Compiler.cs` at the pinned tag on GitHub (the element page's children list
  can omit one: CreateFolder accepts the core `PermissionEx`).
  What an MSI search sets is not on the WiX page: read the Windows Installer tables on learn.microsoft.com
  (`appsearch-table`: a signature with no Signature row is a directory search, its property set to the
  directory's path; `drlocator-table` for Depth).
- A new parameter on a core method (the caller on `Manager::read_file`): a stub that takes it and keeps
  the old body gives assertion reds in every phase, where a `todo!` would stop the one e2e `#[test]` at
  its first read. To see each later phase red, run a copy of `src/tests/mod.rs` calling only that phase
  (restore it after).
- A real chown(2) seam is testable without root: an owner may give its file any group it belongs to
  (`id -Gn`, the gid from `getent group <name>`); an unknown group name must come back `NotFound`.
- `#[cfg(test)]` is off in a library compiled as a dependency: an item another crate's tests need
  (a fake, an `_e2e` phase, the test branch of `server_control::backend()`) goes behind a
  `test-support` feature its dev-dependency turns on, as in pai-core. Without it, the dependent's
  tests silently take the real backend.
- The configurator's source lints read files by path: `control_lint` (`src_dirs`), `binding_lint`
  (`src_dirs`) and `names_lint` walk `crates/app/src` beside `crates/config/src`. A new crate holding
  configurator code must be added to all three, or they keep passing while scanning less.
- Cargo.lock writes each dependency on its own line (`"name"`, `"name version"` when two versions
  exist, plus `(source)`), and lists dev and build dependencies of every target: a walk over it
  over-reports, never misses (`crates/app/src/tests/deps_lint.rs`).
- After a move between crates, diff the test names against HEAD (`-- --list` of both crates now vs
  the old crate in a worktree): only renames should show, each accounted for.
- An executable is tested as a user runs it from `crates/<c>/tests/*.rs` with
  `env!("CARGO_BIN_EXE_<bin>")`: set `PRIVATE_AI_DATA_ROOT` (and a `DBUS_SYSTEM_BUS_ADDRESS` to a missing
  socket) on the CHILD only, so no in-process redirect races it, and wait with a `try_wait` deadline.
  Every start runs the 2.4 migration, which the redirect keeps in the temp tree (`crates/cli/tests/pai_binary.rs`).
  Run only commands that reach no backend: a non-test build takes the real manager.
- Who launches the binaries with which arguments: the login entry `gui [--minimized]` (pai-app
  `startup.rs`), the HW Monitor `current_exe() sdk-devices` (`hwmon/sdk.rs`), xtask's Linux and macOS
  package and install read `pai-config --version`, the MSI runs `control stop-and-close`
  (`installer/msi-stop-processes.ps1`). Check them before dropping an argument.
- A test helper two crates need goes in its own file without tests (`crates/app/src/tests/lock_walk.rs`),
  included by the other crate with `#[path = "..."] mod`.
- A stub at a file's move destination (`crates/cli/src/cli.rs`) keeps the new crate's tests compiling;
  tell the coder to replace it with `git mv -f`.
- A text pin on a source file that also holds the test (`"pai-cli"` in `common/configurator.rs`) must
  read only the code before `#[cfg(test)]\nmod tests`, or the test's own literal satisfies it.
- A new `Component` variant compiles with `todo!("...")` arms in `suffix`, `texts` and `version_of`
  (linux/package.rs): every test reaching `package()` then reds with the stub's name.
