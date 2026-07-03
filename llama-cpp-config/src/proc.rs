// Launch a child process without flashing a console window on Windows.
//
// Single home for the `CREATE_NO_WINDOW` dance the process probes share
// (`devices`, `server_version`, and the Windows branch of `runstate`). On
// non-Windows it is a plain `Command::output()`, so callers that differ only by
// that flag collapse to one code path.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

/// Run `exe` with `args`, capturing stdout/stderr. Returns `None` if the process
/// can't be spawned. On Windows the child gets `CREATE_NO_WINDOW` so no console
/// window pops up for these background probes.
pub fn run_hidden<I, S>(exe: &Path, args: I) -> Option<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(exe);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output().ok()
}
