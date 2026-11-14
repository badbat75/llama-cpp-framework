//! What the binary roots of pai-config do at start, checked on their text:
//! `main` and `gui::run` cannot be called from a test. The migration they run
//! lives in pai-app (`migration.rs`).

// `main` (a binary root no test can call) runs the migration before the
// GUI can read or write the user's tree, and before it reads its arguments,
// as before the command line moved to `pai` (whose own check is
// `crates/cli/src/tests/split_lint.rs`): a text check of the order of the
// calls in `main.rs`.
#[test]
fn main_migrates_before_the_gui_runs() {
    let main = include_str!("../main.rs");
    let at = |needle: &str| {
        main.find(needle)
            .unwrap_or_else(|| panic!("main.rs has no {needle}"))
    };
    let migrate = at("migration::run()");
    assert!(migrate < at("gui::run("), "migrate before the GUI");
    assert!(
        migrate < at("launch::parse("),
        "migrate before the arguments are read"
    );
}

/// The GUI asks at start (`gui::run`, which no test can call): a text
/// check that it decides with `import_offered` and hands over with
/// `import_user_config`. `main` runs the 2.4 tree move before the GUI
/// (`main_migrates_before_the_gui_runs`), so the per-user
/// files are found in their new place.
#[test]
fn the_gui_offers_the_import_at_start() {
    let gui = include_str!("../gui.rs");
    let run = &gui[gui.find("pub fn run(").expect("gui::run")..];
    let run = &run[..run.find("\n}\n").expect("end of gui::run")];
    assert!(
        run.contains("import_offered("),
        "gui::run decides with migration::import_offered"
    );
    assert!(
        gui.contains("import_user_config("),
        "the dialog's Import runs migration::import_user_config"
    );
}

// A login entry left stale by another build (an exe since removed) is read
// as enabled and would never be rewritten: `main` repairs it at the GUI's
// start (`startup::repair_entry`), after the 2.4 entry is migrated and only
// on the GUI launch (not `--version` or `sdk-devices`), before `gui::run`.
// A text check of the order of the calls in `main.rs`.
#[test]
fn main_repairs_a_stale_login_entry_before_the_gui_runs() {
    let main = include_str!("../main.rs");
    let at = |needle: &str| {
        main.find(needle)
            .unwrap_or_else(|| panic!("main.rs has no {needle}"))
    };
    let repair = at("startup::repair_entry(");
    assert!(
        at("migration::run()") < repair,
        "the 2.4 entry migrates first"
    );
    assert!(
        main.rfind("Launch::Moved").expect("Launch::Moved") < repair,
        "only on the GUI launch"
    );
    assert!(repair < at("gui::run("), "repair before the GUI");
}
