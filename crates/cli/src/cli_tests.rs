use super::*;
use crate::server_cfg::ServerConfig;
use crate::tests::gate_lint;

// The only guard on `server set`'s schema mirror: every other server-field
// spot (server_cfg load/save, the form conversions) has a round-trip test,
// but `apply` is the CLI-only copy; an omitted field here is silent.

/// `pai model-profile download` and `pai model-profile delete --delete-files` resolve
/// ModelsDir through pai-app's resolver (`models_dir` in its server_cfg),
/// so under the Windows manager an unset one is the instance's folder (and the
/// download writes no `~\.llama.cpp` into the manager's file). Checked on
/// the text: the Windows branch only type-checks on Linux.
#[test]
fn the_cli_resolves_models_dir_for_the_backend_and_os() {
    let code = include_str!("cli.rs");
    let old = concat!("models_dir_or", "_default()");
    assert!(!code.contains(old), "cli.rs still calls {old}");
    assert!(
        code.matches(concat!("server_cfg::", "models_dir()"))
            .count()
            >= 2,
        "cli.rs does not resolve both the download and the delete through pai-app's resolver"
    );
}

/// Under the manager the instance saves the slot snapshots at its own
/// stop, so a stop from the command line saves none of its own: the
/// decision comes from the backend the command acts on, and the save (and
/// the progress line printed with it) sits behind it. Checked on the text:
/// a `Direct` stop in a test would send SIGTERM to a llama-server that is
/// really running. The local backend's save (macOS) stays.
#[test]
fn the_cli_saves_snapshots_only_on_the_local_backend() {
    let code = gate_lint::strip_comments(
        include_str!("cli.rs")
            .split("\n#[cfg(test)]\n")
            .next()
            .unwrap_or_default(),
    );
    const GATE: &str = "client_saves_snapshots(";
    let save = gate_lint::fn_body(&code, "snapshot_before_stop");
    assert!(
        save.contains(GATE),
        "snapshot_before_stop never asks who saves the snapshots"
    );
    for call in ["slot_state::save_all(", "snapshot-saving"] {
        assert!(
            gate_lint::call_behind_gate(&save, GATE, call),
            "the save's {call:?} is not behind the decision: {save}"
        );
    }
    assert!(
        save.contains("slot_state::save_all("),
        "the local backend must still save the snapshots"
    );
    let control = gate_lint::fn_body(&code, "run_control_with");
    assert!(
        control.contains("snapshot_before_stop(server)"),
        "the stop commands do not give the decision the backend they act on"
    );
}

/// The CLI calls itself `pai`, the executable's name: in `--version`,
/// `--help` and its errors.
#[test]
fn the_cli_is_named_pai() {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    assert_eq!(cmd.get_name(), "pai");
    assert_eq!(
        cmd.render_version(),
        format!("pai {}\n", env!("CARGO_PKG_VERSION"))
    );
    let help = cmd.render_help().to_string();
    assert!(!help.contains(concat!("llama.cpp", "-framework")), "{help}");
    assert!(!help.contains(concat!("llama-cpp", "-config")), "{help}");
}

// ── server boot ─────────────────────────────────────────────────────
// The start at boot of the instance, the service manager's own state
// read and changed through the manager (the GUI's Settings toggle).

fn boot_cmd(args: &[&str]) -> Result<Option<OnOff>, clap::Error> {
    let mut argv = vec!["pai", "server", "boot"];
    argv.extend_from_slice(args);
    match Cli::try_parse_from(argv)?.command {
        Command::Server(ServerCmd::Boot { state }) => Ok(state),
        other => panic!("not server boot: {other:?}"),
    }
}

#[test]
fn server_boot_takes_on_off_or_nothing() {
    assert_eq!(boot_cmd(&[]).expect("show"), None);
    assert_eq!(boot_cmd(&["on"]).expect("on"), Some(OnOff::On));
    assert_eq!(boot_cmd(&["off"]).expect("off"), Some(OnOff::Off));
}

#[test]
fn server_boot_refuses_anything_but_on_or_off() {
    for bad in [&["maybe"][..], &["yes"], &["1"], &["on", "off"]] {
        assert!(boot_cmd(bad).is_err(), "{bad:?}");
    }
}

/// The printed line by content: `strings::t` gives back a missing key.
fn boot_line(on: bool) -> String {
    let key = if on {
        "server-boot-on"
    } else {
        "server-boot-off"
    };
    let text = strings::t(key);
    assert!(
        !text.starts_with(key) && text.contains("when the system starts"),
        "{key}: {text:?}"
    );
    text
}

#[test]
fn server_boot_shows_the_managers_state_and_changes_nothing() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::InstanceState;
    for on in [true, false] {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        *c.boot.lock().unwrap() = on;
        let b = fake::backend(c);
        assert_eq!(run_server_boot_with(None, &b).expect("show"), boot_line(on));
        assert_eq!(log.lock().unwrap().clone(), ["boot_enabled default"]);
    }
}

#[test]
fn server_boot_on_or_off_sets_it_through_the_manager() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::InstanceState;
    for (state, on, word) in [(OnOff::On, true, "on"), (OnOff::Off, false, "off")] {
        let log = fake::log();
        let c = FakeClient::new(&log, InstanceState::Inactive);
        *c.boot.lock().unwrap() = !on;
        let boot = c.boot.clone();
        let b = fake::backend(c);
        assert_eq!(
            run_server_boot_with(Some(state), &b).expect("set"),
            boot_line(on)
        );
        assert_eq!(
            fake::actions(&log),
            [format!("set_boot_enabled default {word}")]
        );
        assert_eq!(*boot.lock().unwrap(), on);
    }
}

/// A user without `configure` is refused with the manager's reason, as
/// the GUI shows it; nothing changes.
#[test]
fn server_boot_refused_is_an_error_with_the_managers_reason() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::{ClientError, InstanceState};
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_set_boot = Some(ClientError::NotAuthorized);
    let boot = c.boot.clone();
    let b = fake::backend(c);
    let e = run_server_boot_with(Some(OnOff::On), &b).expect_err("refused");
    assert!(
        format!("{e:#}").contains(&ClientError::NotAuthorized.text()),
        "{e:#}"
    );
    assert!(!*boot.lock().unwrap(), "nothing changed");
}

#[test]
fn server_boot_with_no_manager_is_an_error_that_says_so() {
    use crate::server_control::fake::{self, FakeClient};
    use pai_core::client::{ClientError, InstanceState};
    let log = fake::log();
    let mut c = FakeClient::new(&log, InstanceState::Inactive);
    c.fail_boot = Some(ClientError::Unavailable);
    c.fail_set_boot = Some(ClientError::Unavailable);
    let b = fake::backend(c);
    for state in [None, Some(OnOff::On)] {
        let e = run_server_boot_with(state, &b).expect_err("no manager");
        assert!(
            format!("{e:#}").contains(&ClientError::Unavailable.text()),
            "{state:?}: {e:#}"
        );
    }
}

/// The executable is `pai`: the Cargo `[[bin]]` name, which is the
/// file every package installs.
#[test]
fn the_executable_is_named_pai() {
    let manifest = include_str!("../Cargo.toml");
    let bin = manifest.split("[[bin]]").nth(1).expect("a [[bin]] table");
    let name = bin
        .lines()
        .find_map(|l| l.trim().strip_prefix("name = "))
        .expect("a bin name");
    assert_eq!(name, "\"pai\"");
}

/// `model-profile delete` keeps every file unless asked, takes the files as a
/// comma list, and the shared override only beside a file list.
#[test]
fn model_profile_delete_parses_its_file_flags() {
    let parse = |args: &[&str]| {
        let mut argv = vec!["pai", "model-profile", "delete", "p"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).map(|c| match c.command {
            Command::ModelProfile(ModelProfileCmd::Delete {
                delete_files,
                even_if_shared,
                ..
            }) => (delete_files, even_if_shared),
            _ => panic!("not a preset delete"),
        })
    };
    assert_eq!(parse(&[]).unwrap(), (vec![], false), "keep is the default");
    assert_eq!(
        parse(&["--delete-files", "model,draft"]).unwrap(),
        (vec![FileKind::Model, FileKind::Draft], false)
    );
    assert_eq!(
        parse(&["--delete-files", "all", "--even-if-shared"]).unwrap(),
        (vec![FileKind::All], true)
    );
    assert!(
        parse(&["--even-if-shared"]).is_err(),
        "the override needs a file list"
    );
    assert!(
        parse(&["--delete-files", "weights"]).is_err(),
        "an unknown kind"
    );
}

#[test]
fn model_profile_delete_picks_by_kind_and_refuses_shared_files() {
    use crate::linked_files::{LinkedFile, Role};
    let lf = |role: Role, name: &str, used: &[&str]| LinkedFile {
        role,
        path: name.into(),
        files: vec![name.into()],
        size: 1,
        used_by: used.iter().map(|u| u.to_string()).collect(),
    };
    let linked = vec![
        lf(Role::Model, "m.gguf", &[]),
        lf(Role::Mmproj, "p.gguf", &["vision", "other"]),
        lf(Role::Draft, "d.gguf", &[]),
    ];
    let roles = |which: &[FileKind]| -> Vec<Role> {
        pick_files(&linked, which).iter().map(|f| f.role).collect()
    };
    assert!(roles(&[]).is_empty());
    assert_eq!(
        roles(&[FileKind::All]),
        [Role::Model, Role::Mmproj, Role::Draft]
    );
    assert_eq!(
        roles(&[FileKind::Draft, FileKind::Model]),
        [Role::Model, Role::Draft]
    );
    assert_eq!(
        shared_refusal(&pick_files(&linked, &[FileKind::Model, FileKind::Draft])),
        None
    );
    let refusal = shared_refusal(&pick_files(&linked, &[FileKind::All])).expect("refused");
    assert!(
        refusal.contains("p.gguf is used by [vision], [other]")
            && refusal.contains("--even-if-shared"),
        "{refusal}"
    );
}

/// `model-profile download` takes the spec positionally and every choice as a
/// flag, so a script can run it end to end without a prompt.
#[test]
fn model_profile_download_parses_its_flags() {
    let cli = Cli::try_parse_from([
        "pai",
        "model-profile",
        "download",
        "unsloth/Qwen3-8B-GGUF:Q4_K_M",
        "--no-mmproj",
        "--draft",
        "mtp",
        "--id",
        "q3",
    ])
    .unwrap();
    let Command::ModelProfile(ModelProfileCmd::Download(a)) = cli.command else {
        panic!("not a preset download");
    };
    assert_eq!(a.spec, "unsloth/Qwen3-8B-GGUF:Q4_K_M");
    assert!(a.no_mmproj && !a.list);
    assert_eq!(
        (a.draft.as_deref(), a.id.as_deref(), a.quant.as_deref()),
        (Some("mtp"), Some("q3"), None)
    );
    assert!(!a.search && a.limit == crate::hf::SEARCH_LIMIT);
    assert!(Cli::try_parse_from(["pai", "model-profile", "download"]).is_err());
    // Words search; --search forces it, --limit caps the rows.
    let cli = Cli::try_parse_from([
        "pai",
        "model-profile",
        "download",
        "unsloth/Qwen3",
        "--search",
        "--limit",
        "5",
    ])
    .unwrap();
    let Command::ModelProfile(ModelProfileCmd::Download(a)) = cli.command else {
        panic!("not a preset download");
    };
    assert!(a.search && a.limit == 5 && a.spec == "unsloth/Qwen3");
}

/// The search rows: aligned repo column, downloads, quant tags, sidecars,
/// access; a hint to go on, or one line when nothing matched.
#[test]
fn search_report_prints_repo_downloads_and_quants_per_row() {
    let hit = |repo: &str, downloads: u64, quants: &[&str], mmproj: bool, gated: bool| {
        crate::hf::SearchHit {
            repo: repo.into(),
            downloads,
            gated,
            private: false,
            quants: quants.iter().map(|q| q.to_string()).collect(),
            mmproj,
            drafters: Vec::new(),
            kind: crate::hf::ModelKind::Unknown,
            params: None,
        }
    };
    let lines = search_report(
        "gemma 3",
        &[
            hit("ggml-org/tinygemma3-GGUF", 73_988, &["Q8_0"], true, false),
            hit(
                "google/gemma-3-4b-it-qat-q4_0-gguf",
                3_330,
                &["Q4_0"],
                true,
                true,
            ),
            hit("o/r", 5, &[], false, false),
        ],
    );
    assert_eq!(lines.len(), 5);
    assert!(lines[0].contains("\"gemma 3\""));
    assert_eq!(
        lines[1],
        "  ggml-org/tinygemma3-GGUF              74.0k  Q8_0  [mmproj]"
    );
    assert_eq!(
        lines[2],
        "  google/gemma-3-4b-it-qat-q4_0-gguf     3.3k  Q4_0  [mmproj]  gated"
    );
    assert_eq!(
        lines[3],
        format!("  {:<34}  {:>7}  no GGUF model file", "o/r", "5")
    );
    assert!(lines[4].starts_with("Pick one: pai model-profile download <org/repo>"));
    assert_eq!(
        search_report("zzz", &[]),
        ["No GGUF repository matches \"zzz\"."]
    );

    // With the search's gguf summary: an aligned model column.
    let with_kind = |repo: &str, kind, params| crate::hf::SearchHit {
        kind,
        params,
        ..hit(repo, 10, &["Q4_K_M"], false, false)
    };
    let lines = search_report(
        "qwen3",
        &[
            with_kind(
                "o/Qwen3-30B-A3B-GGUF",
                crate::hf::ModelKind::Moe,
                Some(30_532_122_624),
            ),
            with_kind(
                "o/Qwen3-8B-GGUF",
                crate::hf::ModelKind::Dense,
                Some(8_190_735_360),
            ),
            with_kind(
                "o/Llama-GGUF",
                crate::hf::ModelKind::Unknown,
                Some(8_030_261_312),
            ),
            with_kind("o/x", crate::hf::ModelKind::Unknown, None),
        ],
    );
    assert_eq!(
        lines[1],
        "  o/Qwen3-30B-A3B-GGUF       10  MoE 30.5B                Q4_K_M"
    );
    assert_eq!(
        lines[2],
        "  o/Qwen3-8B-GGUF            10  dense 8.2B               Q4_K_M"
    );
    assert_eq!(
        lines[3],
        "  o/Llama-GGUF               10  8.0B, dense/MoE unknown  Q4_K_M"
    );
    assert_eq!(
        lines[4],
        "  o/x                        10                           Q4_K_M"
    );
}

/// `--list`: a placement line under every row, the basis, and the
/// recommendation; without an estimate, the plain rows.
#[test]
fn list_report_adds_the_estimate_and_the_recommendation() {
    use crate::hf::{Candidate, Listing, RepoFile};
    use crate::hf_fit::{Budget, Gpu, Layer, Shape};
    use std::sync::Arc;
    const GIB: u64 = 1 << 30;
    let cand = |tag: &str, size: u64| Candidate {
        tag: tag.into(),
        shards: vec![RepoFile {
            path: format!("m-{tag}.gguf"),
            size,
            oid: String::new(),
        }],
        size,
        mmproj: None,
        mtp: None,
        dflash: None,
        dspark: None,
    };
    let l = Listing {
        repo: "o/r".into(),
        commit: "c".repeat(40),
        project_url: String::new(),
        candidates: vec![
            cand("Q4_K_M", 5 * GIB),
            cand("Q8_0", 9 * GIB),
            cand("BF16", 17 * GIB),
        ],
        default: 0,
    };
    let shape = |gib: u64| {
        Arc::new(Shape {
            arch: "qwen3".into(),
            moe: false,
            total: gib * GIB,
            token_embd: 0,
            output_head: false,
            globals: 0,
            layers: vec![
                Layer {
                    experts: 0,
                    other: gib * GIB / 4
                };
                4
            ],
            kv: GIB,
            ctx: 32768,
            trained_ctx: 32768,
            kv_f16_per_token: Some(GIB / 32768),
            block_count: 4,
            expert_count: 0,
            expert_used_count: 0,
        })
    };
    let b = Budget {
        gpus: vec![Gpu {
            id: "CUDA0".into(),
            name: "G".into(),
            total_mib: 14 << 10,
        }],
        ram: Some(28 * GIB),
        ram_total: Some(32 * GIB),
    };
    let shapes = vec![
        Ok(shape(5)),
        Ok(shape(9)),
        Err("network error: boom".to_string()),
    ];
    let lines = list_report(&l, Some((&b, &shapes, &[])));
    assert_eq!(lines[0], format!("o/r @ {}", "c".repeat(12)));
    assert!(lines[1].starts_with("* Q4_K_M"));
    assert_eq!(
        lines[2],
        "    all on GPU: 6.0 of 12.0 GiB with a 32k context"
    );
    assert_eq!(lines[4], "    recommended, the best quality that fits in GPU memory: all on GPU: 10.0 of 12.0 GiB with a 32k context");
    assert_eq!(lines[6], "    estimate unavailable: network error: boom");
    assert!(
        lines[7].starts_with("Estimated for G (12.0 GiB usable) and 32.0 GiB of RAM"),
        "{}",
        lines[7]
    );
    assert_eq!(lines[8], "Recommended here: Q8_0 (the best quality that fits in GPU memory); download it with --quant Q8_0");
    assert_eq!(lines.len(), 9);
    // A 2 GiB image encoder taken with every row (2.5 GiB with its
    // compute): the Q8_0 no longer fits beside it, so its encoder goes to
    // the CPU and the recommendation falls back to the Q4_K_M, which
    // keeps it on the card.
    let mmproj = hf_fit_extras(2 * GIB);
    let lines = list_report(
        &l,
        Some((&b, &shapes, &[mmproj.clone(), mmproj.clone(), mmproj])),
    );
    assert_eq!(
        lines[2],
        "    recommended, the best quality that fits in GPU memory: all on GPU: 8.5 of 12.0 GiB with a 32k context; \
         with the image encoder (2.5 GiB) on CUDA0"
    );
    assert_eq!(lines[4], "    all on GPU: 10.0 of 12.0 GiB with a 32k context; with the image encoder (2.0 GiB) on the CPU");
    assert_eq!(lines[8], "Recommended here: Q4_K_M (the best quality that fits in GPU memory); download it with --quant Q4_K_M");
    // Two cards of 7 and 5 GiB usable: every row names the card or the
    // cut it would write. 6 GiB fits the first alone; the 9 GiB file (four
    // blocks of 2.25 GiB plus 0.25 of KV) fits neither, so it is cut 2 + 3
    // (the output layer on the last card).
    let pair = Budget {
        gpus: vec![
            Gpu {
                id: "ROCm1".into(),
                name: "AMD Radeon AI PRO R9700".into(),
                total_mib: 9 << 10,
            },
            Gpu {
                id: "CUDA0".into(),
                name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                total_mib: 7 << 10,
            },
        ],
        ram: Some(28 * GIB),
        ram_total: Some(32 * GIB),
    };
    let lines = list_report(&l, Some((&pair, &shapes, &[])));
    assert_eq!(
        lines[2],
        "    all on GPU with a 32k context: AI PRO R9700 (ROCm1) 6.0 GiB"
    );
    assert_eq!(
        lines[4],
        "    recommended, the best quality that fits in GPU memory: all on GPU with a 32k context: \
         AI PRO R9700 (ROCm1) 5.0 GiB + RTX 4070 SUPER (CUDA0) 5.0 GiB, tensor-split 2,3"
    );
    assert!(lines[7].starts_with("Estimated for 2 GPUs in split order, AMD Radeon AI PRO R9700 (ROCm1, 7.0 GiB usable) then "), "{}", lines[7]);

    // What a download prints once the model profile is saved.
    let mut p = model_profiles::ModelProfile::new_default("m".into(), "m.gguf".into());
    assert_eq!(placement_line(&p), None, "left to the fit: nothing to say");
    p.device = "ROCm1,CUDA0".into();
    p.tensor_split = "2,3".into();
    p.n_cpu_moe = Some(18);
    p.ctx_size = Some(32768);
    p.placement_note = "estimated at download".into();
    assert_eq!(
        placement_line(&p).unwrap(),
        "Placement: device = ROCm1,CUDA0, tensor-split = 2,3, n-cpu-moe = 18, ctx-size = 32768 (estimated at download)"
    );
    let plain = list_report(&l, None);
    assert_eq!(plain.len(), 4, "no estimate lines without --list");
    // The encoder moved by the placement shows too.
    p.mmproj = "mm.gguf".into();
    p.mmproj_device = "CUDA0".into();
    p.device_draft = "CUDA0".into();
    assert_eq!(
        placement_line(&p).unwrap(),
        "Placement: device = ROCm1,CUDA0, tensor-split = 2,3, n-cpu-moe = 18, ctx-size = 32768, device-draft = CUDA0, \
         mmproj-device = CUDA0 (estimated at download)"
    );
    p.mmproj_device.clear();
    p.mmproj_offload = Some(false);
    assert!(placement_line(&p)
        .unwrap()
        .contains(", device-draft = CUDA0, mmproj-offload = false ("));
}

/// The extras of an image encoder of `size` bytes alone.
fn hf_fit_extras(size: u64) -> crate::hf_fit::Extras {
    crate::hf_fit::Extras {
        mmproj: size,
        drafter: None,
    }
}

/// A `--id` is refused BEFORE the download when it is not a valid section
/// name or names an existing model profile in any case (headers match
/// case-insensitively): a multi-GB transfer must not end in a save error.
#[test]
fn download_id_is_checked_up_front() {
    let existing = vec!["qwen3-8b".to_string()];
    assert!(check_download_id("q3", &existing).is_ok());
    assert!(check_download_id("Qwen3-8B", &existing)
        .unwrap_err()
        .to_string()
        .contains("already exists"));
    assert!(check_download_id("bad id", &existing).is_err());
    assert!(check_download_id("", &existing).is_err());
}

/// `--prompt` and `--prompt-file` are two ways to set one field, so clap
/// has to refuse both at once rather than letting the later assignment win
/// silently: a sweep that measured a prompt the caller did not intend is
/// indistinguishable from one that measured the right one.
#[test]
fn prompt_and_prompt_file_are_mutually_exclusive() {
    let base = [
        "pai",
        "bench",
        "run",
        "--model-profile",
        "P",
        "--key",
        "spec-draft-n-max",
        "--values",
        "2,3",
    ];
    assert!(Cli::try_parse_from(base).is_ok());
    let mut with_both = base.to_vec();
    with_both.extend(["--prompt", "hi", "--prompt-file", "p.txt"]);
    assert!(Cli::try_parse_from(with_both).is_err());
    let mut with_file = base.to_vec();
    with_file.extend(["--prompt-file", "p.txt"]);
    assert!(Cli::try_parse_from(with_file).is_ok());
}

/// `--key` and `--values` come together or not at all: neither is the
/// model profile as it is, one without the other is a sweep with half its axis.
#[test]
fn key_and_values_come_together() {
    let base = ["pai", "bench", "run", "--model-profile", "P"];
    assert!(Cli::try_parse_from(base).is_ok());
    let mut key_only = base.to_vec();
    key_only.extend(["--key", "spec-draft-n-max"]);
    assert!(Cli::try_parse_from(key_only).is_err());
    let mut values_only = base.to_vec();
    values_only.extend(["--values", "2,3"]);
    assert!(Cli::try_parse_from(values_only).is_err());
    let mut restore_only = base.to_vec();
    restore_only.extend(["--restore-to", "3"]);
    assert!(Cli::try_parse_from(restore_only).is_err());
}

/// The profile `bench run` measures is named by `--model-profile`. The
/// flag's old name is not kept as a hidden alias, the way the retired
/// `pai preset` subcommand is not: clap's own refusal is what tells the
/// user the name changed, and it must name the flag it refused.
#[test]
fn bench_run_names_the_model_profile_flag() {
    let accepted = Cli::try_parse_from(["pai", "bench", "run", "--model-profile", "P"]);
    assert!(
        accepted.is_ok(),
        "--model-profile must parse: {:?}",
        accepted.err().map(|e| e.to_string())
    );
    let refused = Cli::try_parse_from(["pai", "bench", "run", "--preset", "P"])
        .expect_err("--preset is the retired name: clap must refuse it");
    let text = refused.to_string();
    assert!(
        text.contains("unexpected argument '--preset' found"),
        "the refusal must name the retired flag: {text}"
    );
}

/// The two refusals a model profile save can end on, byte for byte in the
/// file `pai` reads, next step included: the sentence names the keys of
/// `models.ini`, the file the command edits. And the text `main` prints when
/// a write is refused carries that next step (a test has no entry point to
/// pick `cli.toml`, so the printed text is the window's file: the next step
/// is the same step in both).
#[test]
fn the_two_resource_refusals_say_the_next_step_pai_prints() {
    use pai_core::client::ClientError;
    const CLI_OVER_MEMORY: &str = "`{0}` does not fit the GPU memory of instance `{1}`: it needs {2} MiB, the node has {3} MiB. To make it fit, lower its ctx-size, or pick a smaller cache-type-k or cache-type-v.";
    const CLI_MODEL_MISSING: &str = "`{0}` names a model file that is not there: `{2}` is missing for instance `{1}`. Put the file at that path on the server machine, or set the profile's model to another file.";
    let cli = pai_core::strings::parse(include_str!("../../core/strings/cli.toml"));
    assert_eq!(
        cli.get("model-profile-over-memory")
            .expect("model-profile-over-memory in cli.toml"),
        CLI_OVER_MEMORY,
        "cli.toml: model-profile-over-memory"
    );
    assert_eq!(
        cli.get("model-profile-model-missing")
            .expect("model-profile-model-missing in cli.toml"),
        CLI_MODEL_MISSING,
        "cli.toml: model-profile-model-missing"
    );
    let printed = |key: &'static str, values: &[&str]| {
        let r = pai_core::checks::Refusal {
            key,
            values: values.iter().map(|s| s.to_string()).collect(),
        };
        ClientError::Invalid(r).text()
    };
    let over = printed(
        "model-profile-over-memory",
        &["qwen", "default", "1446", "512"],
    );
    assert!(
        over.starts_with("`qwen` does not fit the GPU memory of instance `default`: it needs 1446 MiB, the node has 512 MiB. ")
            && over.contains("To make it fit, lower its"),
        "the refused save prints no next step: {over:?}"
    );
    let missing = printed(
        "model-profile-model-missing",
        &["qwen", "default", "/models/qwen.gguf"],
    );
    assert!(
        missing.starts_with(
            "`qwen` names a model file that is not there: `/models/qwen.gguf` is missing for instance `default`. "
        ) && missing.contains("Put the file at that path on the server machine"),
        "the refused save prints no next step: {missing:?}"
    );
}

#[test]
fn server_set_apply_copies_every_field() {
    let set = ServerSet {
        port: Some(9000),
        hostname: Some("0.0.0.0".into()),
        threads: Some(8),
        threads_batch: Some(16),
        models_max: Some(3),
        models_memory_margin: Some(1024),
        models_dir: Some("D:\\models".into()),
        // The tri-state arrives as its WORD (clap refuses anything else) and
        // must land as the matching Option<bool>.
        rocblas_use_hipblaslt: Some("off".into()),
        // The GTT switch arrives as its word as well: `on` must land as the
        // explicit true (the state an unset key already means at launch).
        rocm_host_memory_gtt: Some("on".into()),
        cuda_allreduce: Some("none".into()),
        cuda_disable_graphs: Some(true),
        attn_rot_disable: Some(true),
        rocprof_attach: Some(true),
        webui: Some(false),
        webui_mcp_proxy: Some(false),
        log_verbosity: Some(2),
        save_state_on_shutdown: Some(true),
        state_dir: Some(r"E:\llama-state".into()),
        opencode_base_url: Some("https://llm.example.com".into()),
        opencode_api_key: Some("sk-test-key".into()),
    };
    // The tags arrive from the file, not from a flag: `server set` has no
    // `--tags`, so `apply` must leave the list it was handed untouched.
    let mut cfg = ServerConfig {
        tags: vec!["edge".into(), "v2".into()],
        ..Default::default()
    };
    set.apply(&mut cfg);
    // Whole-struct equality against a second exhaustive literal: the
    // compiler forces a value for a new field in BOTH literals, and the
    // equality fails until `apply` actually copies it; an initialized-
    // but-never-copied field can't slip through.
    let expected = ServerConfig {
        port: Some(9000),
        hostname: Some("0.0.0.0".into()),
        threads: Some(8),
        threads_batch: Some(16),
        models_max: Some(3),
        models_memory_margin: Some(1024),
        // The GPUs: as with the tags below, no flag carries them, so
        // `apply` leaves the list the loaded config had (None here: every
        // GPU); `server_set_keeps_the_devices_the_file_has` pins a file
        // that names some.
        devices: None,
        models_dir: Some("D:\\models".into()),
        rocblas_use_hipblaslt: Some(false),
        // The GTT switch: the flag's word `on` lands as the explicit true.
        rocm_host_memory_gtt: Some(true),
        cuda_allreduce: Some("none".into()),
        cuda_disable_graphs: true,
        attn_rot_disable: true,
        rocprof_attach: true,
        webui: Some(false),
        webui_mcp_proxy: Some(false),
        log_verbosity: Some(2),
        save_state_on_shutdown: Some(true),
        state_dir: Some(r"E:\llama-state".into()),
        opencode_base_url: Some("https://llm.example.com".into()),
        opencode_api_key: Some("sk-test-key".into()),
        // No flag sets them: the list the config file had, kept.
        tags: vec!["edge".into(), "v2".into()],
    };
    assert_eq!(cfg, expected);
}

/// `pai server set` rewrites the WHOLE server profile from the config it
/// loaded, so a field no flag carries must survive the save. The GPUs of
/// R2, pinned from the file's text, as `load()` reads it, to the text
/// `save()` renders.
#[test]
fn server_set_keeps_the_devices_the_file_has() {
    let loaded = server_cfg::from_text("[Server]\nPort = 8580\nDevices = ROCm0,ROCm1\n");
    assert_eq!(
        loaded.devices.as_deref(),
        Some("ROCm0,ROCm1"),
        "the fixture must be a file that names GPUs"
    );
    let mut cfg = loaded.clone();
    ServerSet {
        port: Some(9000),
        ..ServerSet::default()
    }
    .apply(&mut cfg);
    assert_eq!(cfg.port, Some(9000), "the flag landed");
    assert_eq!(
        cfg.devices, loaded.devices,
        "server set erased the GPUs the file had"
    );
    let text = server_cfg::render(&cfg);
    assert!(
        text.contains("Devices = ROCm0,ROCm1"),
        "the written file has no Devices line:\n{text}"
    );
}

// The Show leg of the 3-spot CLI fan-out: every field set in a rich config
// must surface in `server show`'s output. Complements the `apply` test
// above so neither CLI leg can silently drop a new server field.
#[test]
fn show_lines_prints_every_field() {
    let cfg = ServerConfig {
        port: Some(9000),
        hostname: Some("0.0.0.0".into()),
        threads: Some(8),
        threads_batch: Some(16),
        models_max: Some(3),
        models_memory_margin: Some(1024),
        // The GPUs: `server show` has no row for them, so the fixture
        // keeps them unset (every GPU) and `needles` names nothing.
        devices: None,
        models_dir: Some(r"D:\models".into()),
        rocblas_use_hipblaslt: Some(false),
        // The GTT switch: on explicitly (unset is the same for the launch),
        // the state the row must print as `on`, not as a placeholder.
        rocm_host_memory_gtt: Some(true),
        cuda_allreduce: Some("none".into()),
        cuda_disable_graphs: true,
        attn_rot_disable: true,
        rocprof_attach: true,
        webui: Some(false),
        webui_mcp_proxy: Some(false),
        log_verbosity: Some(2),
        save_state_on_shutdown: Some(true),
        state_dir: Some(r"E:\llama-state".into()),
        opencode_base_url: Some("https://llm.example.com".into()),
        opencode_api_key: Some("sk-test-key".into()),
        // The profile's tags: a list the CLI reads and shows, never one it
        // passes to llama-server.
        tags: vec!["edge".into(), "v2".into()],
    };
    // The exhaustive destructure breaks compilation the moment a field is
    // added, until this test decides what to do with it, but the
    // assertions come from the hand-maintained `needles` array below:
    // bind the new field AND add its needle, or its Show row goes
    // unguarded (binding alone, or `field: _`, compiles fine).
    let ServerConfig {
        port,
        hostname,
        threads,
        threads_batch,
        models_max,
        models_memory_margin,
        devices: _, // No GPUs row in `server show`, so no needle to bind.
        models_dir,
        rocblas_use_hipblaslt,
        rocm_host_memory_gtt,
        cuda_allreduce,
        cuda_disable_graphs,
        attn_rot_disable,
        rocprof_attach,
        webui,
        webui_mcp_proxy,
        log_verbosity,
        save_state_on_shutdown,
        state_dir,
        opencode_base_url,
        opencode_api_key,
        tags,
    } = cfg.clone();
    let needles = [
        ("Port:", port.unwrap().to_string()),
        ("Hostname:", hostname.unwrap()),
        ("Threads:", threads.unwrap().to_string()),
        ("ThreadsBatch:", threads_batch.unwrap().to_string()),
        ("ModelsMax:", models_max.unwrap().to_string()),
        (
            "ModelsMemoryMargin:",
            format!("{} MiB", models_memory_margin.unwrap()),
        ),
        ("ModelsDir:", models_dir.unwrap()),
        // The row prints the tri-state's WORD, not the bool's `to_string()`:
        // "false" would read as a value the CLI can't even be given.
        (
            "RocblasUseHipblaslt:",
            if rocblas_use_hipblaslt.unwrap() {
                "on".to_string()
            } else {
                "off".to_string()
            },
        ),
        // The GTT row prints the state the launch acts on (unset reads as
        // on), so the needle comes from the fixture's own value: a row
        // printing a placeholder for `Some(true)` fails here.
        (
            "RocmHostMemoryGtt:",
            if rocm_host_memory_gtt.unwrap() {
                "on (GTT)".to_string()
            } else {
                "off (the runtime's defaults)".to_string()
            },
        ),
        ("CudaAllreduce:", cuda_allreduce.unwrap()),
        ("CudaDisableGraphs:", cuda_disable_graphs.to_string()),
        ("AttnRotDisable:", attn_rot_disable.to_string()),
        ("RocprofAttach:", rocprof_attach.to_string()),
        ("Webui:", webui.unwrap().to_string()),
        ("WebuiMcpProxy:", webui_mcp_proxy.unwrap().to_string()),
        ("LogVerbosity:", log_verbosity.unwrap().to_string()),
        (
            "SaveStateOnShutdown:",
            save_state_on_shutdown.unwrap().to_string(),
        ),
        ("StateDir:", state_dir.unwrap()),
        ("OpencodeBaseUrl:", opencode_base_url.unwrap()),
        ("OpencodeApiKey:", opencode_api_key.unwrap()),
        // The list prints with the file's own spelling (`Tags = a,b` in the
        // ini), the text the user reads back in the profile.
        ("Tags:", tags.join(",")),
    ];
    let out = show_lines_for(
        &cfg,
        server_control::Kind::Manager,
        server_control::Os::Linux,
        Path::new(LINUX_INSTANCE_MODELS),
        Path::new(LINUX_INSTANCE_STATE),
        false,
    );
    for (label, value) in needles {
        // Label and value must share a LINE: two separate contains() could
        // both pass off other fields (e.g. ModelsMax "3" matching inside
        // TensorSplit's "3,1") while this field's row prints a placeholder.
        assert!(
            out.lines().any(|l| l.contains(label) && l.contains(&value)),
            "no line pairs {label:?} with {value:?} in:\n{out}"
        );
    }
}

// ── server show: the folders an unset ModelsDir / StateDir mean ────────

use std::path::Path;

/// The instances' defaults as `paths::` gives them on each OS; passed
/// in, so no unit test reads `paths::`. The Windows ones split, so the
/// names lint does not read the engine's folder as the old user tree.
const LINUX_INSTANCE_MODELS: &str = "/var/lib/llama.cpp/models";
const LINUX_INSTANCE_STATE: &str = "/var/lib/private-ai/.local/share/private-ai/state";
const WINDOWS_INSTANCE_MODELS: &str = concat!(r"C:\ProgramData\llama", r".cpp\models");
const WINDOWS_INSTANCE_STATE: &str = concat!(r"C:\ProgramData\llama", r".cpp\state");

/// The value of `server show`'s row `label` (the text after the label).
fn row<'a>(out: &'a str, label: &str) -> &'a str {
    out.lines()
        .find_map(|l| l.trim_start().strip_prefix(label))
        .map(str::trim)
        .unwrap_or_else(|| panic!("no {label} row in:\n{out}"))
}

fn unset_dirs() -> [ServerConfig; 2] {
    [
        ServerConfig::default(),
        ServerConfig {
            models_dir: Some("  ".into()),
            state_dir: Some(String::new()),
            ..ServerConfig::default()
        },
    ]
}

/// Under the manager (Linux and Windows) an unset or blank ModelsDir or
/// StateDir is shown as the folder the instance uses (the one its launch
/// fills in), marked "(default)": not the user's `~/.llama.cpp` or data
/// root, which the instance's account never reads.
#[test]
fn server_show_names_the_instance_defaults_under_the_manager() {
    for (os, models, state) in [
        (
            server_control::Os::Linux,
            LINUX_INSTANCE_MODELS,
            LINUX_INSTANCE_STATE,
        ),
        (
            server_control::Os::Windows,
            WINDOWS_INSTANCE_MODELS,
            WINDOWS_INSTANCE_STATE,
        ),
    ] {
        for cfg in unset_dirs() {
            let out = show_lines_for(
                &cfg,
                server_control::Kind::Manager,
                os,
                Path::new(models),
                Path::new(state),
                false,
            );
            assert_eq!(
                row(&out, "ModelsDir:"),
                format!("{models} (default)"),
                "{os:?} {cfg:?}"
            );
            assert_eq!(
                row(&out, "StateDir:"),
                format!("{state} (default)"),
                "{os:?} {cfg:?}"
            );
        }
    }
}

/// A configured folder is shown as set, with no "(default)", under every
/// backend and OS.
#[test]
fn server_show_prints_configured_dirs_as_set() {
    let cfg = ServerConfig {
        models_dir: Some("/srv/llm".into()),
        state_dir: Some("/srv/llama-state".into()),
        ..ServerConfig::default()
    };
    for kind in [server_control::Kind::Manager, server_control::Kind::Direct] {
        for os in [
            server_control::Os::Linux,
            server_control::Os::Windows,
            server_control::Os::MacOs,
        ] {
            let out = show_lines_for(
                &cfg,
                kind,
                os,
                Path::new(LINUX_INSTANCE_MODELS),
                Path::new(LINUX_INSTANCE_STATE),
                false,
            );
            assert_eq!(row(&out, "ModelsDir:"), "/srv/llm", "{kind:?} {os:?}");
            assert_eq!(
                row(&out, "StateDir:"),
                "/srv/llama-state",
                "{kind:?} {os:?}"
            );
        }
    }
}

/// Elsewhere (macOS, a direct launch) neither row names the instance's
/// folders, and an unset StateDir stays the user's default marked
/// "(default)", as before.
#[test]
fn server_show_elsewhere_keeps_the_user_defaults() {
    for (kind, os) in [
        (server_control::Kind::Manager, server_control::Os::MacOs),
        (server_control::Kind::Direct, server_control::Os::Linux),
        (server_control::Kind::Direct, server_control::Os::Windows),
        (server_control::Kind::Direct, server_control::Os::MacOs),
    ] {
        for cfg in unset_dirs() {
            let out = show_lines_for(
                &cfg,
                kind,
                os,
                Path::new(LINUX_INSTANCE_MODELS),
                Path::new(LINUX_INSTANCE_STATE),
                false,
            );
            for label in ["ModelsDir:", "StateDir:"] {
                let value = row(&out, label);
                assert!(
                    !value.contains(LINUX_INSTANCE_MODELS) && !value.contains(LINUX_INSTANCE_STATE),
                    "{kind:?} {os:?} {label} {value}"
                );
            }
            let state = row(&out, "StateDir:");
            assert!(
                state.ends_with(" (default)") && state.len() > " (default)".len(),
                "{kind:?} {os:?}: {state}"
            );
        }
    }
}

/// A key the manager withheld (the caller may not configure) is shown
/// as set and withheld, never as "(none)": the file holds one. The row
/// is what scripts read, so its value is literal, like "(none)".
#[test]
fn server_show_says_a_withheld_key_is_set() {
    let withheld = ServerConfig::default();
    let out = show_lines_for(
        &withheld,
        server_control::Kind::Manager,
        server_control::Os::Linux,
        Path::new(LINUX_INSTANCE_MODELS),
        Path::new(LINUX_INSTANCE_STATE),
        true,
    );
    assert_eq!(row(&out, "OpencodeApiKey:"), "(set, withheld)");
    let out = show_lines_for(
        &withheld,
        server_control::Kind::Manager,
        server_control::Os::Linux,
        Path::new(LINUX_INSTANCE_MODELS),
        Path::new(LINUX_INSTANCE_STATE),
        false,
    );
    assert_eq!(row(&out, "OpencodeApiKey:"), "(none)", "nothing withheld");
}

/// `pai server show` prints through the pure rule above for the real
/// backend and OS and the instances' real folders. The Windows branch
/// only type-checks on Linux, hence a check on the text.
#[test]
fn server_show_resolves_the_dirs_for_the_backend_and_os() {
    let code = include_str!("cli.rs");
    let start = code.find("\nfn run_server(").expect("fn run_server");
    let rest = &code[start..];
    let body = &rest[..rest.find("\n}\n").expect("end of fn")];
    let show = &body[body.find("ServerCmd::Show").expect("Show arm")..];
    let mut show = show[..show.find("ServerCmd::Set").expect("Set arm")].to_string();
    // The arm may print through `show_lines(&cfg)`, which then resolves.
    if show.contains("show_lines(") {
        let start = code.find("\nfn show_lines(").expect("fn show_lines");
        let rest = &code[start..];
        show.push_str(&rest[..rest.find("\n}\n").expect("end of fn")]);
    }
    for want in [
        "show_lines_for(",
        ".kind()",
        "Os::current()",
        "instance_models_dir()",
        "instance_state_dir()",
        "api_key_withheld()",
    ] {
        assert!(show.contains(want), "server show has no {want}:\n{show}");
    }
    // The state folder shown as the default is the folder of the instance
    // the command drives, as the launch passes it: the engine's state root
    // joined with the instance's id, never the bare root (the root holds
    // the folders of every instance).
    let state = &show[show
        .find("instance_state_dir()")
        .expect("instance_state_dir()")..];
    let arg = &state[..state.find(',').expect("the end of the state-dir argument")];
    assert!(
        arg.contains(".join("),
        "the state default is the engine's root, not the instance's folder:\n{show}"
    );
}

// The tri-state's own clearing rule: `--rocblas-use-hipblaslt default` is the
// ONLY way back to unset from the CLI, and unset is what keeps the env var
// off the child entirely. A bool flag could not express it.
#[test]
fn server_set_rocblas_hipblaslt_takes_all_three_states() {
    for (word, expected) in [("on", Some(true)), ("off", Some(false)), ("default", None)] {
        let mut cfg = ServerConfig {
            rocblas_use_hipblaslt: Some(true),
            ..Default::default()
        };
        ServerSet {
            rocblas_use_hipblaslt: Some(word.into()),
            ..Default::default()
        }
        .apply(&mut cfg);
        assert_eq!(cfg.rocblas_use_hipblaslt, expected, "{word}");
    }
}

// GT3: the GTT switch has the same three CLI states as its neighbour,
// because the file's commented default (the on state) is a state the user
// reaches: `default` clears the key back to it, which a bool flag cannot
// express. The GUI and the web UI both write a bool; the CLI is the only
// way back to "no line".
#[test]
fn server_set_rocm_host_memory_gtt_takes_all_three_states() {
    for (word, expected) in [("on", Some(true)), ("off", Some(false)), ("default", None)] {
        let mut cfg = ServerConfig {
            rocm_host_memory_gtt: Some(false),
            ..Default::default()
        };
        ServerSet {
            rocm_host_memory_gtt: Some(word.into()),
            ..Default::default()
        }
        .apply(&mut cfg);
        assert_eq!(cfg.rocm_host_memory_gtt, expected, "{word}");
    }
}

#[test]
fn server_set_apply_leaves_unset_flags_untouched() {
    let before = ServerConfig {
        port: Some(1234),
        hostname: Some("localhost".into()),
        threads: Some(6),
        ..Default::default()
    };
    let mut cfg = before.clone();
    ServerSet::default().apply(&mut cfg); // all flags None
    assert_eq!(cfg, before);
}

#[test]
fn server_set_apply_clears_overrides_on_nonpositive_and_blank() {
    let mut cfg = ServerConfig {
        hostname: Some("0.0.0.0".into()),
        threads: Some(8),
        threads_batch: Some(4),
        models_dir: Some(r"D:\models".into()),
        ..Default::default()
    };
    let set = ServerSet {
        hostname: Some("  ".into()),
        threads: Some(0),
        threads_batch: Some(0),
        models_dir: Some(String::new()),
        ..Default::default()
    };
    set.apply(&mut cfg);
    assert_eq!(cfg.hostname, None);
    assert_eq!(cfg.threads, None);
    assert_eq!(cfg.threads_batch, None);
    assert_eq!(cfg.models_dir, None);
}
