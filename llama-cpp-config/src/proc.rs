// Launch a child process without flashing a console window on Windows.
//
// Single home for the `CREATE_NO_WINDOW` dance the process probes share. Two
// entry points: `run_hidden` for the fire-and-forget probes (`devices`,
// `server_version`, the Windows branch of `runstate::is_running`/`stop`), and
// `hide_console` for callers that build the `Command` themselves — custom
// stdio/env then `spawn()` (`runstate::start`). On non-Windows both are no-ops
// beyond the plain command, so callers that differ only by that flag collapse.
// `combined_output` joins a probe's stdout+stderr — parse that, not one stream.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

/// Apply `CREATE_NO_WINDOW` to `cmd` on Windows so no console window pops up for
/// these background children; a no-op elsewhere. The single definition of the
/// flag, shared with `run_hidden`.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Run `exe` with `args`, capturing stdout/stderr. Returns `None` if the process
/// can't be spawned. On Windows the child gets `CREATE_NO_WINDOW` (via
/// `hide_console`) so no console window pops up for these background probes.
pub fn run_hidden<I, S>(exe: &Path, args: I) -> Option<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(exe);
    cmd.args(args);
    hide_console(&mut cmd);
    cmd.output().ok()
}

/// Join a child's stdout and stderr into one string. llama.cpp tools split
/// output across the two streams (`--version` prints to **stderr**,
/// `--list-devices` to stdout), so probes must parse the combination — reading
/// a single stream silently blanks them when upstream moves the output.
pub fn combined_output(output: &Output) -> String {
    let mut s = String::from_utf8_lossy(&output.stdout).into_owned();
    s.push('\n');
    s.push_str(&String::from_utf8_lossy(&output.stderr));
    s
}
