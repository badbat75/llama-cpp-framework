//! Cross-cutting tests of the crate.
//!
//! The phases that redirect the data tree (`paths::redirect_for_tests`, which
//! sets the process-wide `PRIVATE_AI_DATA_ROOT` and never restores it) run in
//! ONE `#[test]`, `data_root_e2e`, one after the other. Unit tests elsewhere
//! must not touch `paths::` (or anything that resolves the config dir): cargo
//! runs tests on parallel threads, so they would race this one.

mod deps_lint;
mod split_lint;

use clap::Parser;
use pai_core::paths;

#[test]
fn data_root_e2e() {
    // ── The `control` commands on the manager backend (cli.rs) ───────────
    crate::cli::cli_control_e2e();

    // ── `server set` and `preset enable` through the seam (cli.rs) ───────
    crate::cli::cli_config_e2e();

    // ── The per-user config handed to the manager (cli.rs) ───────────────
    crate::cli::cli_import_e2e();

    // ── `preset delete|disable|enable` on the default test backend ───────
    preset_commands_e2e();

    // ── `preset delete --delete-files` on disk (cli.rs, linked_files.rs) ─
    preset_delete_files_e2e();
}

/// `preset delete <typo>` errors instead of a false "Removed", and `preset
/// disable|enable` is the same move the Models tab's switch makes: a
/// case-insensitive lookup acting on the STORED id, opencode.json untouched
/// (only the Integrations tab's Save writes it). The default test backend keeps
/// the preset files at the per-user paths, under the redirect.
fn preset_commands_e2e() {
    use pai_app::presets;
    let dir = tempfile::tempdir().expect("tempdir");
    paths::redirect_for_tests(dir.path());
    let ini = paths::presets_ini();
    std::fs::create_dir_all(ini.parent().expect("config dir")).expect("config dir");
    std::fs::write(&ini, "[e2e]\nmodel = /models/e2e-Q8_0.gguf\n").expect("presets.ini");
    let opencode = paths::opencode_user_config();
    std::fs::create_dir_all(opencode.parent().expect("opencode dir")).expect("opencode dir");
    let opencode_text = concat!(
        "{\n  \"provider\": {\n    \"llama.cpp\": {\n",
        "      \"models\": { \"e2e\": { \"name\": \"e2e\" } }\n    }\n  }\n}\n"
    );
    std::fs::write(&opencode, opencode_text).expect("opencode.json");

    let pai = |args: &[&str]| {
        let argv: Vec<&str> = std::iter::once("pai").chain(args.iter().copied()).collect();
        let cli = crate::cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("pai {}: {e}", args.join(" ")));
        crate::cli::run(cli)
    };

    assert!(
        pai(&["preset", "delete", "no-such-preset"]).is_err(),
        "deleting a nonexistent preset must error, not report success"
    );
    assert!(
        presets::load_all().iter().any(|p| p.id == "e2e"),
        "nothing deleted"
    );

    pai(&["preset", "disable", "E2E"]).expect("preset disable");
    assert!(presets::load_all().iter().all(|p| p.id != "e2e"));
    assert!(presets::load_listed()
        .iter()
        .any(|l| l.preset.id == "e2e" && !l.enabled));
    assert_eq!(
        std::fs::read_to_string(&opencode).expect("opencode.json"),
        opencode_text,
        "the CLI never touches opencode.json: only the Integrations tab's Save does"
    );
    pai(&["preset", "disable", "e2e"]).expect("already off is not an error");
    pai(&["preset", "enable", "e2e"]).expect("preset enable");
    assert!(presets::load_all().iter().any(|p| p.id == "e2e"));
    assert!(
        pai(&["preset", "enable", "no-such-preset"]).is_err(),
        "an unknown id must error"
    );
}

/// `preset delete --delete-files` follows the Models tab's delete dialog: a
/// file another preset links is refused before anything changes, unless
/// `--even-if-shared`; the picked files go with the Hugging Face repo folders
/// they leave empty under ModelsDir; without the flag every file stays.
fn preset_delete_files_e2e() {
    use pai_app::presets::{self, Preset};
    let dir = tempfile::tempdir().expect("tempdir");
    paths::redirect_for_tests(dir.path());
    let models_dir = dir.path().join("models-root");
    let mut cfg = pai_app::server_cfg::load();
    cfg.models_dir = Some(models_dir.to_string_lossy().into_owned());
    pai_app::server_cfg::save(&cfg).expect("save server.ini");

    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let touch = |p: &std::path::Path, n: usize| {
        std::fs::create_dir_all(p.parent().expect("folder")).expect("folder");
        std::fs::write(p, vec![0u8; n]).expect("file");
    };
    let repo = |sub: &str, name: &str| models_dir.join(sub).join("org").join(name);
    let cli_model = repo("models", "Cli-GGUF").join("cli.gguf");
    let cli_draft = repo("mtps", "Cli-GGUF").join("cli-mtp.gguf");
    let other_model = repo("models", "Other-GGUF").join("other.gguf");
    touch(&cli_model, 2);
    touch(&cli_draft, 1);
    touch(&other_model, 3);
    presets::save(&Preset {
        id: "cli-del".into(),
        model: path(&cli_model),
        model_draft: path(&cli_draft),
        ..Preset::default()
    })
    .expect("save cli-del");
    presets::save(&Preset {
        id: "cli-other".into(),
        model: path(&other_model),
        model_draft: path(&cli_draft),
        ..Preset::default()
    })
    .expect("save cli-other");

    let pai = |args: &[&str]| {
        let argv: Vec<&str> = std::iter::once("pai").chain(args.iter().copied()).collect();
        let cli = crate::cli::Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("pai {}: {e}", args.join(" ")));
        crate::cli::run(cli)
    };
    let listed = |id: &str| presets::load_listed().iter().any(|l| l.preset.id == id);

    let err = pai(&["preset", "delete", "cli-del", "--delete-files", "all"])
        .expect_err("a shared drafter is refused");
    assert!(err.to_string().contains("[cli-other]"), "{err}");
    assert!(
        listed("cli-del") && cli_model.exists() && cli_draft.exists(),
        "a refusal changes nothing"
    );
    pai(&[
        "preset",
        "delete",
        "cli-del",
        "--delete-files",
        "all",
        "--even-if-shared",
    ])
    .expect("forced");
    assert!(!listed("cli-del") && !cli_model.exists() && !cli_draft.exists());
    assert!(!repo("models", "Cli-GGUF").exists() && !repo("mtps", "Cli-GGUF").exists());
    // Without the flag, every file stays.
    pai(&["preset", "delete", "cli-other"]).expect("plain delete");
    assert!(!listed("cli-other") && other_model.exists());
}
