//! What pai-config is asked to do at start. It parses the GUI's own arguments
//! and nothing else: the commands are `pai`'s (crates/cli).

/// One start of pai-config.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// Open the GUI: no arguments (the desktop entry, the MSI shortcut), `gui`
    /// or `gui --minimized` (the start-at-login entry, pai-app's `startup.rs`).
    Gui { minimized: bool },
    /// The HW Monitor's child, a hidden argument (pai-app's `hwmon/sdk.rs`
    /// runs this executable with it).
    SdkDevices,
    /// `--version`: `pai-config <version>`, which `xtask package` and `xtask
    /// install` read back.
    Version,
    /// Anything else: a command, which moved to `pai`.
    Moved,
}

/// The launch the arguments ask for, the program's name left out.
pub fn parse(args: &[String]) -> Launch {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args[..] {
        [] | ["gui"] => Launch::Gui { minimized: false },
        ["gui", "--minimized"] => Launch::Gui { minimized: true },
        ["sdk-devices"] => Launch::SdkDevices,
        ["--version"] => Launch::Version,
        _ => Launch::Moved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(args: &[&str]) -> Launch {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse(&args)
    }

    /// What every launcher passes still opens the GUI: the desktop entry and
    /// the MSI shortcut nothing, the start-at-login entry `gui` with or
    /// without `--minimized` (the entry's command, `startup.rs`).
    #[test]
    fn the_launchers_open_the_gui() {
        assert_eq!(launch(&[]), Launch::Gui { minimized: false });
        assert_eq!(launch(&["gui"]), Launch::Gui { minimized: false });
        assert_eq!(
            launch(&["gui", "--minimized"]),
            Launch::Gui { minimized: true }
        );
    }

    #[test]
    fn the_hw_monitor_child_and_the_version_are_kept() {
        assert_eq!(launch(&["sdk-devices"]), Launch::SdkDevices);
        assert_eq!(launch(&["--version"]), Launch::Version);
    }

    /// A command, a flag of `pai`, or a GUI argument out of place is no
    /// launch of the GUI: it points to `pai`.
    #[test]
    fn everything_else_moved_to_pai() {
        for args in [
            &["preset", "list"][..],
            &["control", "stop-and-close"],
            &["server"],
            &["bench", "run", "--preset", "p"],
            &["--help"],
            &["-h"],
            &["--minimized"],
            &["gui", "--bogus"],
            &["gui", "--minimized", "extra"],
            &["sdk-devices", "extra"],
            &["--version", "extra"],
            &[""],
        ] {
            assert_eq!(launch(args), Launch::Moved, "{args:?}");
        }
    }
}
