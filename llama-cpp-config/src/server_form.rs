//! Conversion between the Slint server form (`ServerForm`) and the
//! `server_cfg::ServerConfig` schema: the server-side mirror of `form.rs`.
//! Kept out of `gui.rs` (which only shuttles the whole `ServerForm`, never
//! per-field) so adding a server field touches this file plus `ServerForm`
//! (ui/types.slint), the widget (ui/server_page.slint), `ServerConfig`
//! (server_cfg.rs), and the CLI (three spots in cli.rs; the full checklist
//! lives at the top of server_cfg.rs), not the GUI wiring.

use std::ops::RangeInclusive;

use slint::SharedString;

use crate::gui::ServerForm;
use crate::ini;
use crate::server_cfg;

/// An optional int as the text its `DefaultLineEdit` shows, falling back to
/// `hint` when the key is unset, the server-side twin of `form::itxt` (and its
/// doc explains why these numerics are text and not a SpinBox `value`: Slint's
/// SpinBox edits itself on a stray scroll of the page).
fn itxt(v: Option<i32>, hint: i32) -> SharedString {
    v.unwrap_or(hint).to_string().into()
}

/// The `-lv` threshold's whole domain, as the dropdown shows it. llama.cpp defines
/// exactly these six and prints a message when its level is `<=` the threshold
/// (`common/arg.cpp`'s `--verbosity` help, `common/log.h`'s `LOG_LEVEL_*`), so a
/// free-number field could only ever offer levels that do not exist.
///
/// Mirrors `Options.log_levels` in ui/components.slint: same strings, same order.
/// The e2e (`src/tests/ui_bindings.rs`) asserts the two lists are equal, because a
/// label here that the dropdown does not contain would leave the combo showing its
/// first entry (OUTPUT:0) and quietly SAVE that on the next write.
pub(crate) const LOG_LEVELS: [(&str, i32); 6] = [
    ("OUTPUT:0", 0),
    ("ERROR:1", 1),
    ("WARN:2", 2),
    ("INFO:3", 3),
    ("TRACE:4", 4),
    ("DEBUG:5", 5),
];

/// An `-lv` value as its dropdown label. Out-of-range values are CLAMPED into the
/// six, which is behaviour-preserving where it matters: a hand-edited `LogVerbosity
/// = 7` prints exactly what `5` prints (nothing in llama.cpp logs above DEBUG), so
/// it shows (and re-saves) as DEBUG:5 rather than as a level that does not exist.
fn log_level_label(v: i32) -> SharedString {
    let clamped = v.clamp(LOG_LEVELS[0].1, LOG_LEVELS[LOG_LEVELS.len() - 1].1);
    LOG_LEVELS
        .iter()
        .find(|(_, n)| *n == clamped)
        .map(|(label, _)| SharedString::from(*label))
        .unwrap_or_else(|| SharedString::from(LOG_LEVELS[4].0))
}

/// The dropdown label back to the `-lv` number, the inverse of `log_level_label`.
/// An unknown label (only a hand-edited form could produce one) falls back to the
/// framework default rather than to 0; silence is the one outcome nobody asked for.
fn parse_log_level(label: &str) -> i32 {
    LOG_LEVELS
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, n)| *n)
        .unwrap_or_else(|| server_cfg::ServerConfig::default().log_verbosity_or_default())
}

/// A `HipFaBand` value as its dropdown label: `off` for 0, the number itself
/// otherwise. Mapped over `server_cfg::HIP_FA_BANDS`, it is `Options.hip_fa_bands`
/// in ui/components.slint after its leading "default" (the unset state), which
/// the e2e (`src/tests/ui_bindings.rs`) asserts.
pub(crate) fn hip_fa_band_label(v: i32) -> SharedString {
    if v == 0 {
        "off".into()
    } else {
        v.to_string().into()
    }
}

/// The dropdown label back to the `HipFaBand` value, the inverse of
/// `hip_fa_band_label`. "default" (and a label the dropdown cannot produce) is
/// None, the unset state, which still exports the tool's default (4).
fn parse_hip_fa_band(label: &str) -> Option<i32> {
    if label == "off" {
        return Some(0);
    }
    server_cfg::normalize_hip_fa_band(label)
}

/// The ports a listener can bind.
const PORT: RangeInclusive<i32> = 1..=65535;

/// Every INTEGER text field of the server form, as (server.ini key, its "default"
/// box, its text, the values it takes): the server twin of `form::int_fields`,
/// with the ranges `form_to_config` reads the same fields with (the
/// thread counts are sliders, and LogVerbosity a dropdown, so neither is here).
fn int_fields(f: &ServerForm) -> [(&'static str, bool, &str, RangeInclusive<i32>); 3] {
    [
        ("Port", f.port_default, f.port.as_str(), PORT),
        (
            "ModelsMax",
            f.models_max_default,
            f.models_max.as_str(),
            ini::INT_ANY,
        ),
        (
            "ModelsMemoryMargin",
            f.models_memory_margin_default,
            f.models_memory_margin.as_str(),
            ini::INT_NON_NEGATIVE,
        ),
    ]
}

/// The integer fields whose "default" box is unticked but whose text is not a
/// number the key takes, one status-line phrase each; empty when the form can
/// be saved. The server twin of `form::invalid_numbers`: `form_to_config` would
/// write each as unset, i.e. hand the key back to llama.cpp's default unasked.
pub fn invalid_numbers(f: &ServerForm) -> Vec<String> {
    int_fields(f)
        .into_iter()
        .filter(|(_, default, ..)| !default)
        .filter_map(|(key, _, text, range)| ini::int_problem(key, text, &range))
        .collect()
}

/// `ServerConfig` → the editable form. Materializes the display defaults the UI
/// needs for always-present controls (the models dir, and the split-mode combo's
/// "default" sentinel); the port / models-max carry a `*_default`
/// checkbox (checked = omit the flag → fall back to llama.cpp's own default), with
/// the text seeded from that same default so the disabled field shows a hint; the
/// thread counts map `None` → the slider's "auto" flag. `form_to_config` reverses
/// each of these.
pub fn config_to_form(cfg: &server_cfg::ServerConfig) -> ServerForm {
    ServerForm {
        port: itxt(cfg.port, server_cfg::DEFAULT_PORT),
        port_default: cfg.port.is_none(),
        hostname: cfg.hostname_or_default().into(),
        // Thread counts are auto-flagged sliders: unset ⇒ "auto" (omit the flag).
        threads: cfg.threads.unwrap_or(0),
        threads_auto: cfg.threads.is_none(),
        threads_batch: cfg.threads_batch.unwrap_or(0),
        threads_batch_auto: cfg.threads_batch.is_none(),
        // Seed the disabled field with llama.cpp's own default (4) so the
        // "default" hint the user sees matches what omitting the flag yields.
        models_max: itxt(cfg.models_max, 4),
        models_max_default: cfg.models_max.is_none(),
        // Seeded with llama.cpp's default (0 = off), like models_max above.
        models_memory_margin: itxt(cfg.models_memory_margin, 0),
        models_memory_margin_default: cfg.models_memory_margin.is_none(),
        // Same "blank ⇒ default dir" rule as save()/start(), so a hand-edited
        // blank ModelsDir shows the default it will actually resolve to.
        models_dir: cfg.models_dir_or_default().into(),
        // Driven by the tensor-placement table, which reads and rewrites this one
        // string; no second copy of the rules exists (see src/tensor_override.rs).
        // The SDK Tuning card's ROCm knob: a TRI-state through the same helper
        // pair as the preset tab's flash-attn, because "never export
        // ROCBLAS_USE_HIPBLASLT" is a third instruction and the framework's
        // default. Shown as a dropdown (Options.tri_states).
        rocblas_use_hipblaslt: crate::form::tri_state(cfg.rocblas_use_hipblaslt),
        // "default" is the dropdown's entry for "never export
        // GGML_CUDA_ALLREDUCE"; it two-way-binds to this, so the sentinel is
        // stored rather than "".
        cuda_allreduce: cfg
            .cuda_allreduce
            .as_deref()
            .and_then(server_cfg::normalize_allreduce)
            .unwrap_or("default")
            .into(),
        cuda_disable_graphs: cfg.cuda_disable_graphs,
        attn_rot_disable: cfg.attn_rot_disable,
        // A dropdown carrying the LABEL ("off", "2", "4"); "default" is its
        // entry for unset (the tool's default, 4, still exported), stored as
        // the sentinel like `cuda_allreduce`'s.
        hip_fa_band: cfg
            .hip_fa_band
            .map_or_else(|| "default".into(), hip_fa_band_label),
        rocprof_attach: cfg.rocprof_attach,
        // Plain bool toggles (framework defaults materialized when unset), same
        // shape as `mlock`. The proxy keeps its own value with the UI off: the
        // widget is greyed out, never cleared (server_page.slint).
        webui: cfg.webui_or_default(),
        webui_mcp_proxy: cfg.webui_mcp_proxy_or_default(),
        // Always has a value (framework default 4 when unset); a dropdown, no
        // "default" checkbox: the launch always passes -lv. The form carries the
        // LABEL ("TRACE:4"), which is what the EnumComboBox matches on.
        log_verbosity: log_level_label(cfg.log_verbosity_or_default()),
        // A plain bool toggle, and a path that materializes its default the same
        // way ModelsDir does: the field is always populated, so the user can see
        // where 8 GiB-per-model dumps would land before turning the toggle on.
        save_state_on_shutdown: cfg.save_state_on_shutdown_or_default(),
        state_dir: cfg.state_dir_or_default().into(),
        // Base URL: when unset, show auto-derived and check the default box.
        opencode_base_url: cfg
            .opencode_base_url
            .clone()
            .unwrap_or_else(|| cfg.opencode_base_url_or_default())
            .into(),
        base_url_default: cfg.opencode_base_url.is_none(),
        // API Key: when unset, show empty and check the nokey box.
        opencode_api_key: cfg.opencode_api_key.clone().unwrap_or_default().into(),
        api_key_nokey: cfg.opencode_api_key.is_none(),
    }
}

/// The editable form → `ServerConfig`. The `*_default` checkbox decides
/// None/Some for port / models-max (checked ⇒ None); the thread
/// counts use their existing `_auto` flags; blank / sentinel string fields
/// collapse back to `None` (server.ini's `save` renders those as commented
/// hint lines).
pub fn form_to_config(f: &ServerForm) -> server_cfg::ServerConfig {
    server_cfg::ServerConfig {
        // The numerics are TEXT on the form (see `itxt`), so each is re-parsed
        // inside the range that was the SpinBox's `minimum`/`maximum`; text
        // outside it reads as unset (⇒ llama.cpp's own default). The Save button
        // asks `invalid_numbers` first and refuses instead, so that reading never
        // reaches server.ini from the GUI. Same ranges as `int_fields`.
        port: if f.port_default {
            None
        } else {
            ini::parse_int_in(f.port.as_str(), &PORT)
        },
        // Blank collapses to None like every optional string, matching what
        // the same input produces via `server set` / `load()`: a `Some("")`
        // here would only diverge the in-memory config, since every consumer
        // re-blanks it through `hostname_or_default`.
        hostname: server_cfg::opt_nonblank(Some(f.hostname.to_string())),
        // "auto" ⇒ omit the flag; otherwise the slider's value.
        threads: if f.threads_auto {
            None
        } else {
            Some(f.threads)
        },
        threads_batch: if f.threads_batch_auto {
            None
        } else {
            Some(f.threads_batch)
        },
        // 0 is a real value here (= unlimited), so no `> 0` filter.
        models_max: if f.models_max_default {
            None
        } else {
            ini::parse_int_in(f.models_max.as_str(), &ini::INT_ANY)
        },
        models_memory_margin: if f.models_memory_margin_default {
            None
        } else {
            ini::parse_int_in(f.models_memory_margin.as_str(), &ini::INT_NON_NEGATIVE)
        },
        // Blank ⇒ None (fall back to the default dir), same rule as hostname.
        models_dir: server_cfg::opt_nonblank(Some(f.models_dir.to_string())),
        // "default" (and anything unrecognised) collapses to None: the natural
        // `Some(s == "on")` would turn "leave the variable alone" into an
        // explicit `ROCBLAS_USE_HIPBLASLT=0`, i.e. into the Tensile workaround
        // for everyone who never asked for it.
        rocblas_use_hipblaslt: crate::form::tri_bool(f.rocblas_use_hipblaslt.as_str()),
        // "default" (and anything that is not a mode) is None: exporting an
        // unknown word would run every tensor split as `none`.
        cuda_allreduce: server_cfg::normalize_allreduce(f.cuda_allreduce.as_str())
            .map(str::to_string),
        cuda_disable_graphs: f.cuda_disable_graphs,
        attn_rot_disable: f.attn_rot_disable,
        // "default" is None: the default stays in the code, not in server.ini.
        hip_fa_band: parse_hip_fa_band(f.hip_fa_band.as_str()),
        rocprof_attach: f.rocprof_attach,
        webui: Some(f.webui),
        webui_mcp_proxy: Some(f.webui_mcp_proxy),
        // The one field with no "unset" state (the launch always passes -lv): the
        // form carries the dropdown's label, and `parse_log_level` maps it back.
        log_verbosity: Some(parse_log_level(f.log_verbosity.as_str())),
        save_state_on_shutdown: Some(f.save_state_on_shutdown),
        // Blank ⇒ None (fall back to the default dir), the same rule ModelsDir
        // and hostname follow.
        state_dir: server_cfg::opt_nonblank(Some(f.state_dir.to_string())),
        // Default checked = None (auto-derived); unchecked = save the value.
        // Strip trailing slashes then /v1 so the convention "store without
        // suffix" holds even if the user typed it (e.g. https://gw.example.com/v1/).
        opencode_base_url: if f.base_url_default {
            None
        } else {
            let url = f.opencode_base_url.trim().trim_end_matches('/');
            let cleaned = url.strip_suffix("/v1").unwrap_or(url).trim_end_matches('/');
            server_cfg::opt_nonblank(Some(cleaned.to_string()))
        },
        // No key checked = None; unchecked = save the value.
        opencode_api_key: if f.api_key_nokey {
            None
        } else {
            server_cfg::opt_nonblank(Some(f.opencode_api_key.to_string()))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_cfg::ServerConfig;

    // A fully-populated (all-`Some`, non-blank) config survives config → form →
    // config unchanged. The guard for the server-field fan-out: a field wired
    // into one conversion but not the other drops out here. (Configs with `None`
    // port / models_dir are NOT fixed points, since the form materializes their
    // display defaults, which is why this uses an all-populated config.)
    #[test]
    fn rich_server_config_round_trips() {
        let cfg = ServerConfig {
            port: Some(9090),
            hostname: Some("0.0.0.0".into()),
            threads: Some(8),
            threads_batch: Some(12),
            models_max: Some(2),
            models_memory_margin: Some(2048),
            models_dir: Some(r"E:\models".into()),
            // Non-default (None is the framework default), so the tri-state's
            // round-trip through the form's string spelling isn't vacuous.
            rocblas_use_hipblaslt: Some(false),
            cuda_allreduce: Some("none".into()),
            cuda_disable_graphs: true,
            attn_rot_disable: true,
            hip_fa_band: Some(2),
            rocprof_attach: true,
            webui: Some(false),
            webui_mcp_proxy: Some(false),
            log_verbosity: Some(2),
            // Non-default toggle, and a StateDir off the system drive: the path
            // is only overridable because 8 GiB dumps do not belong on C:.
            save_state_on_shutdown: Some(true),
            state_dir: Some(r"E:\llama-state".into()),
            opencode_base_url: Some("https://proxy.example.com".into()),
            opencode_api_key: Some("sk-test-key".into()),
        };
        assert_eq!(form_to_config(&config_to_form(&cfg)), cfg);
    }

    // The Save button's refusal and `form_to_config` agree on each integer field:
    // refused exactly when the conversion would read the text as unset. Also
    // pins the three ranges, `0` being a real ModelsMax (unlimited) and
    // ModelsMemoryMargin (off) and not a real Port, and `-1` no margin.
    #[test]
    fn invalid_numbers_match_what_the_conversion_drops() {
        for text in [
            "",
            "x",
            "0",
            "-1",
            "1",
            "8080",
            "65536",
            "1.5",
            "99999999999",
        ] {
            let form = ServerForm {
                port: text.into(),
                models_max: text.into(),
                models_memory_margin: text.into(),
                models_dir: r"E:\models".into(),
                ..Default::default()
            };
            let cfg = form_to_config(&form);
            let dropped: Vec<&str> = [
                ("Port", cfg.port),
                ("ModelsMax", cfg.models_max),
                ("ModelsMemoryMargin", cfg.models_memory_margin),
            ]
            .into_iter()
            .filter(|(_, v)| v.is_none())
            .map(|(key, _)| key)
            .collect();
            let refused: Vec<String> = invalid_numbers(&form);
            assert_eq!(refused.len(), dropped.len(), "text {text:?}: {refused:?}");
            for key in dropped {
                assert!(
                    refused.iter().any(|r| r.starts_with(key)),
                    "text {text:?}: {key} dropped but not refused ({refused:?})"
                );
            }
        }
        let ticked = ServerForm {
            port_default: true,
            models_max_default: true,
            models_memory_margin_default: true,
            ..Default::default()
        };
        assert!(
            invalid_numbers(&ticked).is_empty(),
            "a ticked box is a choice"
        );
        let zero = ServerForm {
            port: "8080".into(),
            models_max: "0".into(),
            models_memory_margin: "0".into(),
            ..Default::default()
        };
        assert!(
            invalid_numbers(&zero).is_empty(),
            "ModelsMax 0 = unlimited, ModelsMemoryMargin 0 = off"
        );
    }

    /// The log level is the one field the form carries as a LABEL rather than as
    /// its value, so the two halves of that mapping must compose back to identity
    /// for every level llama.cpp has, and the out-of-domain cases must land
    /// somewhere defensible rather than on OUTPUT:0 (which would silence the log).
    #[test]
    fn log_level_labels_round_trip_and_clamp() {
        for (label, n) in LOG_LEVELS {
            assert_eq!(log_level_label(n).as_str(), label);
            assert_eq!(parse_log_level(label), n);
        }
        // Above DEBUG there is nothing to print, so a hand-edited 7 IS DEBUG.
        assert_eq!(log_level_label(7).as_str(), "DEBUG:5");
        assert_eq!(log_level_label(-2).as_str(), "OUTPUT:0");
        // A label the dropdown can't produce falls back to the framework default,
        // never to 0: losing the log entirely is the one outcome nobody wants.
        assert_eq!(parse_log_level("nonsense"), 4);
    }

    /// The FlashAttention band dropdown, every entry in both directions: 0 must
    /// come back as an explicit `Some(0)` (off), and "default" as `None`, never
    /// as an explicit 4 (the default belongs to the code, not to server.ini).
    #[test]
    fn hip_fa_band_labels_round_trip() {
        let states = server_cfg::HIP_FA_BANDS.map(Some);
        for band in states.into_iter().chain([None]) {
            let form = config_to_form(&ServerConfig {
                hip_fa_band: band,
                models_dir: Some(r"E:\models".into()),
                ..Default::default()
            });
            assert_eq!(form_to_config(&form).hip_fa_band, band);
        }
        assert_eq!(
            config_to_form(&ServerConfig {
                models_dir: Some(r"E:\models".into()),
                ..Default::default()
            })
            .hip_fa_band
            .as_str(),
            "default"
        );
        assert_eq!(hip_fa_band_label(0).as_str(), "off");
        assert_eq!(parse_hip_fa_band("nonsense"), None);
    }

    /// The SDK Tuning tri-state, all three states in both directions. "default"
    /// must come back as `None` and NOT as an explicit off: the difference is
    /// whether `ROCBLAS_USE_HIPBLASLT=0` reaches the child at all, i.e. whether
    /// every machine silently gets the Tensile workaround.
    #[test]
    fn rocblas_hipblaslt_tri_state_round_trips() {
        for (state, word) in [(Some(true), "on"), (Some(false), "off"), (None, "default")] {
            let form = config_to_form(&ServerConfig {
                rocblas_use_hipblaslt: state,
                // Explicit, like the sibling tests below: an unset ModelsDir
                // would send config_to_form through `paths::` (mod.rs: unit
                // tests must stay clear of it).
                models_dir: Some(r"E:\models".into()),
                ..Default::default()
            });
            assert_eq!(form.rocblas_use_hipblaslt.as_str(), word, "{state:?}");
            assert_eq!(form_to_config(&form).rocblas_use_hipblaslt, state, "{word}");
        }
    }

    /// The AllReduce dropdown, every entry in both directions. "default" must
    /// come back as `None`: the difference is whether `GGML_CUDA_ALLREDUCE`
    /// reaches the child at all.
    #[test]
    fn cuda_allreduce_round_trips_with_default_as_unset() {
        let states = [None, Some("nccl"), Some("internal"), Some("none")];
        for state in states {
            let form = config_to_form(&ServerConfig {
                cuda_allreduce: state.map(str::to_string),
                models_dir: Some(r"E:\models".into()),
                ..Default::default()
            });
            assert_eq!(form.cuda_allreduce.as_str(), state.unwrap_or("default"));
            assert_eq!(form_to_config(&form).cuda_allreduce.as_deref(), state);
        }
    }

    // Unset thread counts ⇄ the slider's "auto" flag: `None` becomes `*_auto` in
    // the form and collapses back to `None` (the slider value is ignored).
    #[test]
    fn threads_auto_round_trips() {
        let form = config_to_form(&ServerConfig {
            port: Some(8080),
            models_dir: Some(r"E:\models".into()),
            ..Default::default()
        });
        assert!(form.threads_auto && form.threads_batch_auto);
        let cfg = form_to_config(&form);
        assert_eq!(cfg.threads, None);
        assert_eq!(cfg.threads_batch, None);
    }

    /// form_to_config strips trailing /v1 and / from the Base URL so the
    /// convention "store without suffix" holds even when the user typed it.
    #[test]
    fn base_url_strips_trailing_v1_and_slash() {
        let cases = [
            ("https://gw.example.com/v1/", "https://gw.example.com"),
            ("https://gw.example.com/v1", "https://gw.example.com"),
            ("https://gw.example.com/", "https://gw.example.com"),
            ("https://gw.example.com", "https://gw.example.com"),
            ("https://gw.example.com/v1//", "https://gw.example.com"),
        ];
        for (input, expected) in cases {
            let form = ServerForm {
                opencode_base_url: input.into(),
                base_url_default: false,
                ..Default::default()
            };
            let cfg = form_to_config(&form);
            assert_eq!(
                cfg.opencode_base_url.as_deref(),
                Some(expected),
                "input: {input}"
            );
        }
    }
}
