//! The tensor-placement table: which of a model's tensors llama.cpp must put on
//! a device OTHER than the one its layer landed on (`--override-tensor` / `-ot`).
//! Pure logic (no IO, no Slint state) so every rule below is unit tested;
//! `gui::refresh_tensor_rows` is the thin shell that pushes the rows into
//! `AppState` and writes the result back into the form.
//!
//! ## What the flag actually is
//! `-ot` is a list of `<regex>=<buffer type>` rules. llama.cpp matches the regex
//! against every tensor NAME with `std::regex_search` (`llama-model-loader.cpp`)
//! and, on a hit, allocates that tensor from the named buffer type instead of the
//! one its layer would have used. The buffer type is a DEVICE name — llama.cpp
//! builds the lookup from `ggml_backend_dev_buffer_type` over every backend, so
//! the legal values are exactly the ids `--list-devices` prints, plus `CPU`
//! (`common/arg.cpp`, `parse_tensor_buffer_overrides` — an unknown one is a
//! hard `throw`, i.e. the model fails to load).
//!
//! ## Why a table and not a text field
//! The grammar is positional and unforgiving in two ways that a free-text field
//! quietly walks into, because llama.cpp splits BEFORE it parses:
//!   - rules are split on `,` — so a `{1,2}` quantifier inside a regex tears the
//!     rule in half, and the half without an `=` is a fatal "invalid value";
//!   - a rule is split at its FIRST `=` — so an `=` inside the regex silently
//!     eats part of the pattern.
//!
//! Neither is escapable. So the pattern is a field the user can't type freely,
//! which is exactly what `sanitize_pattern` enforces (below) — and the device is
//! a dropdown of real device ids, not a string to spell.
//!
//! ## The three canned patterns
//! `KINDS` is the discoverable part: the regexes worth knowing, named. `Custom`
//! is the escape hatch and keeps whatever pattern is already there, so switching
//! a row to Custom never wipes the text you were about to edit.
//!
//! `Embedding table` is the one that pays for the table's existence. llama.cpp
//! leaves `token_embd.weight` in HOST memory even when it reports
//! `offloaded 33/33 layers to GPU` — the embedding lookup is a `get_rows` over a
//! handful of tokens, cheap on CPU and worth ~1-2 GiB of VRAM. But with a GPU
//! backend active that host buffer is PINNED (`ROCm_Host` / `CUDA_Host`), and
//! Windows counts pinned host memory as **Shared GPU memory** — so a model that
//! fits in VRAM with room to spare still shows GBs of shared allocation, and the
//! graph carries a CPU split it didn't need. On a big-vocab BF16 model the table
//! is huge (`n_vocab 248320 × n_embd 4096 × 2 B` = 1940 MiB), which is when
//! moving it onto a GPU is worth the VRAM.

use crate::devices::DeviceOption;
use crate::gui::TensorOverrideRow;

/// One `<pattern>=<buffer type>` rule. `device` is a buffer-type name, i.e. a
/// device id (`ROCm0`) or `CPU`; empty means a hand-edited INI left the `=<dev>`
/// off, which `validate` refuses at save.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rule {
    pub pattern: String,
    pub device: String,
}

/// A named pattern for the "Tensor" dropdown. The last entry is always `CUSTOM`
/// — the free-text escape hatch, which carries no canned pattern of its own.
pub struct Kind {
    /// Stable id, round-tripped through the Slint combo's `values` model.
    pub id: &'static str,
    pub label: &'static str,
    /// The regex this kind writes into the rule. Empty for `CUSTOM`.
    pub pattern: &'static str,
}

/// The `Custom regex…` kind's id — the one that keeps the row's existing pattern
/// and lets the user edit it.
pub const CUSTOM: &str = "custom";

/// The canned patterns, in dropdown order. `exps` is llama.cpp's own expert
/// regex verbatim (`LLM_FFN_EXPS_REGEX` in `common/common.h`) — the one
/// `--cpu-moe` installs — so "MoE experts → CPU" here is exactly `-cmoe`.
pub const KINDS: &[Kind] = &[
    Kind {
        id: "embd",
        label: "Embedding table (token_embd)",
        pattern: r"token_embd\.weight",
    },
    Kind {
        id: "output",
        label: "Output head (output)",
        pattern: r"^output\.weight",
    },
    Kind {
        id: "exps",
        label: "MoE experts, all layers (ffn_*_exps)",
        pattern: r"\.ffn_(up|down|gate|gate_up)_(ch|)exps",
    },
    Kind {
        id: CUSTOM,
        label: "Custom regex…",
        pattern: "",
    },
];

/// The kind a pattern belongs to: the canned entry it matches verbatim, else
/// `CUSTOM` (always the last index, so an unrecognised — i.e. hand-written —
/// pattern lands on the free-text row).
fn kind_index(pattern: &str) -> usize {
    KINDS
        .iter()
        .position(|k| k.id != CUSTOM && k.pattern == pattern)
        .unwrap_or(KINDS.len() - 1)
}

fn is_custom(pattern: &str) -> bool {
    kind_index(pattern) == KINDS.len() - 1
}

// ── String ↔ rules ───────────────────────────────────────────────────────

/// `"token_embd\.weight=ROCm0, x=CPU"` → two rules. Tolerant on purpose: a piece
/// with no `=` (only reachable from a hand-edited INI — the table can't produce
/// one) keeps its text as the pattern and an EMPTY device, so the row survives,
/// shows its problem, and can be fixed instead of vanishing on the next save.
pub fn parse(s: &str) -> Vec<Rule> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|piece| match piece.split_once('=') {
            Some((pattern, device)) => Rule {
                pattern: pattern.trim().to_string(),
                device: device.trim().to_string(),
            },
            None => Rule {
                pattern: piece.to_string(),
                device: String::new(),
            },
        })
        .collect()
}

/// Rules → the INI value. The inverse of `parse` for every rule the table can
/// build (patterns are sanitized, so nothing here needs escaping).
pub fn render(rules: &[Rule]) -> String {
    rules
        .iter()
        .map(|r| format!("{}={}", r.pattern, r.device))
        .collect::<Vec<_>>()
        .join(",")
}

/// Strip what llama.cpp's own splitting would destroy: `,` ends the rule and the
/// first `=` ends the pattern, and neither can be escaped (see the module
/// header). Enforced at the ONE place a pattern is typed, so the form string is
/// always a value llama.cpp reads back as the same rules.
fn sanitize_pattern(p: &str) -> String {
    p.replace([',', '='], "").trim().to_string()
}

// ── Edits (each returns the new INI value; the caller writes it to the form) ──

/// Append a rule. Seeded with the Embedding table — the reason this table exists
/// (module header) — on the first real GPU the probe found, so the common case is
/// one click. Falls back to CPU when no GPU is known yet (an un-probed GUI): a
/// device is never left blank, because a blank one can't be saved.
pub fn add(s: &str, devices: &[DeviceOption]) -> String {
    let mut rules = parse(s);
    let device = devices
        .iter()
        .find(|d| !d.is_cpu())
        .map_or_else(|| "CPU".to_string(), |d| d.id.clone());
    rules.push(Rule {
        pattern: KINDS[0].pattern.to_string(),
        device,
    });
    render(&rules)
}

/// Drop one rule. Rows are addressed by INDEX, not by a key: the same pattern may
/// legitimately appear twice (two devices, two halves of a model), so there is no
/// id to name them by.
pub fn remove(s: &str, index: usize) -> String {
    let mut rules = parse(s);
    if index >= rules.len() {
        return s.to_string();
    }
    rules.remove(index);
    render(&rules)
}

/// Switch a row to another named pattern: the kind writes its canned regex over
/// whatever was there, and `CUSTOM` — whose canned regex is the empty string —
/// therefore CLEARS it, handing the user an empty field to type into.
///
/// That is not a choice so much as a consequence: a row's kind is DERIVED from
/// its pattern (`kind_index`), with no hidden "is custom" bit anywhere, because a
/// second source of truth is a second thing to keep in sync with a hand-edited
/// INI. So a row still holding `\.ffn_…_exps` necessarily reads as the MoE kind —
/// "Custom, but the text happens to equal a canned regex" is not a state this can
/// represent, and pretending otherwise would leave the user picking Custom and
/// watching nothing happen.
pub fn set_kind(s: &str, index: usize, kind_id: &str) -> String {
    let Some(kind) = KINDS.iter().find(|k| k.id == kind_id) else {
        return s.to_string();
    };
    let mut rules = parse(s);
    let Some(rule) = rules.get_mut(index) else {
        return s.to_string();
    };
    rule.pattern = kind.pattern.to_string();
    render(&rules)
}

/// Set a row's regex (the Custom row's text field). Sanitized — see
/// `sanitize_pattern`.
pub fn set_pattern(s: &str, index: usize, pattern: &str) -> String {
    let mut rules = parse(s);
    let Some(rule) = rules.get_mut(index) else {
        return s.to_string();
    };
    rule.pattern = sanitize_pattern(pattern);
    render(&rules)
}

/// Set a row's target buffer type (a device id, or `CPU`).
pub fn set_device(s: &str, index: usize, device: &str) -> String {
    let mut rules = parse(s);
    let Some(rule) = rules.get_mut(index) else {
        return s.to_string();
    };
    rule.device = device.trim().to_string();
    render(&rules)
}

// ── Validation ───────────────────────────────────────────────────────────

/// Refuse a value llama.cpp would choke on. Only a hand-edited INI can get here
/// — the table sanitizes — but that is exactly the case worth catching, because
/// llama.cpp's failure mode is a `throw` during arg parsing: the model does not
/// load, and the reason is buried in a child process's log.
///
/// Not checked: whether the device EXISTS. The probe is async and a config may
/// name another machine's GPU; `build_rows` flags that as an undetected row
/// instead, the same way the GPU distribution table does.
pub fn validate(s: &str) -> Result<(), String> {
    for (i, rule) in parse(s).iter().enumerate() {
        let n = i + 1;
        if rule.pattern.is_empty() {
            return Err(format!("tensor override {n} has an empty pattern"));
        }
        if rule.device.is_empty() {
            return Err(format!(
                "tensor override {n} (`{}`) names no device — a rule is `<pattern>=<device>`, \
                 and llama.cpp splits it at the first `=`",
                rule.pattern
            ));
        }
    }
    Ok(())
}

// ── Display ──────────────────────────────────────────────────────────────

/// The dropdown behind every row's Device cell: every probed device INCLUDING
/// the CPU (unlike the GPU distribution table, which filters it out — `-ot`'s
/// whole point is that CPU is a legal target), plus any device a rule already
/// names that the probe doesn't know, kept as a `(custom)` entry so a stale or
/// another-machine id is never silently rewritten by the next save.
pub fn device_options(devices: &[DeviceOption], s: &str) -> (Vec<String>, Vec<String>) {
    let mut labels: Vec<String> = devices.iter().map(|d| d.label.clone()).collect();
    let mut values: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
    for rule in parse(s) {
        if rule.device.is_empty() || values.iter().any(|v| v.eq_ignore_ascii_case(&rule.device)) {
            continue;
        }
        labels.push(format!("(custom) {}", rule.device));
        values.push(rule.device);
    }
    (labels, values)
}

/// The table rows, in rule order (which is the order llama.cpp applies them: the
/// FIRST matching rule wins, so the order is meaningful and the table never
/// re-sorts it).
pub fn build_rows(devices: &[DeviceOption], s: &str) -> Vec<TensorOverrideRow> {
    let (_, values) = device_options(devices, s);
    parse(s)
        .into_iter()
        .map(|rule| {
            let device_index = values
                .iter()
                .position(|v| v.eq_ignore_ascii_case(&rule.device))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1);
            let detected = devices
                .iter()
                .any(|d| d.id.eq_ignore_ascii_case(&rule.device));
            TensorOverrideRow {
                kind_index: i32::try_from(kind_index(&rule.pattern)).unwrap_or(0),
                custom: is_custom(&rule.pattern),
                problem: row_problem(&rule, detected).into(),
                device_index,
                detected,
                device: rule.device.into(),
                pattern: rule.pattern.into(),
            }
        })
        .collect()
}

/// A row's own complaint, or empty when it is fine.
fn row_problem(rule: &Rule, detected: bool) -> String {
    if rule.pattern.is_empty() {
        return "empty pattern".into();
    }
    if rule.device.is_empty() {
        return "pick a device".into();
    }
    if !detected {
        return format!("{} was not detected", rule.device);
    }
    String::new()
}

/// The line under the table: the llama-server flag these rules produce.
pub fn summary(s: &str) -> String {
    if parse(s).is_empty() {
        return "(no overrides — every tensor goes wherever its layer went)".into();
    }
    format!("--override-tensor {}", render(&parse(s)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices;

    // The same mixed box the gpu_split tests use, plus its CPU — which IS a
    // legal -ot target (and the only one gpu_split filters out).
    const SAMPLE: &str = "Available devices:\n  \
        ROCm0: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
        CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
        CPU: AMD Ryzen 9 9900X (63090 MiB, 48233 MiB free)\n";

    fn devs() -> Vec<DeviceOption> {
        devices::parse(SAMPLE)
    }

    const EMBD: &str = r"token_embd\.weight";
    const EXPS: &str = r"\.ffn_(up|down|gate|gate_up)_(ch|)exps";

    fn rule(pattern: &str, device: &str) -> Rule {
        Rule {
            pattern: pattern.into(),
            device: device.into(),
        }
    }

    // ── The grammar ───────────────────────────────────────────────────────

    #[test]
    fn parse_and_render_round_trip_a_multi_rule_value() {
        let s = format!("{EMBD}=ROCm0,{EXPS}=CPU");
        assert_eq!(
            parse(&s),
            [rule(EMBD, "ROCm0"), rule(EXPS, "CPU")],
            "the experts regex carries `|` and `(` — neither is a separator"
        );
        assert_eq!(render(&parse(&s)), s);
    }

    #[test]
    fn parse_tolerates_whitespace_and_never_invents_a_rule() {
        assert_eq!(parse(""), []);
        assert_eq!(parse("  ,  "), []);
        assert_eq!(parse(" a = CPU , "), [rule("a", "CPU")]);
    }

    // A rule is split at its FIRST `=` (llama.cpp's `override.find('=')`), so a
    // pattern can never own one — and a device that itself contained `=` would
    // be a nonsense buffer type anyway.
    #[test]
    fn a_rule_splits_at_the_first_equals_like_llama_cpp_does() {
        assert_eq!(parse("a=b=c"), [rule("a", "b=c")]);
    }

    // The half-rule a hand-edited INI can leave behind: keep it (so it can be
    // fixed) rather than dropping it (so the next save can't silently eat it).
    #[test]
    fn a_piece_without_a_device_survives_as_a_rule_with_none() {
        assert_eq!(parse("token_embd"), [rule("token_embd", "")]);
        assert!(validate("token_embd").is_err());
    }

    // ── Sanitizing: the two characters a pattern cannot hold ──────────────

    // `,` would tear the rule in half (the half without an `=` is a fatal
    // "invalid value" in llama.cpp) and `=` would eat the pattern's tail. There
    // is no escape for either, so the field simply refuses them.
    #[test]
    fn set_pattern_strips_the_separators_llama_cpp_would_split_on() {
        let s = set_pattern(&format!("{EMBD}=ROCm0"), 0, r"blk\.{1,2}\.attn=x");
        assert_eq!(parse(&s), [rule(r"blk\.{12}\.attnx", "ROCm0")]);
        // …and what comes out is still ONE rule, which is the whole point.
        assert_eq!(parse(&s).len(), 1);
        assert!(validate(&s).is_ok());
    }

    #[test]
    fn set_pattern_trims_and_leaves_the_device_alone() {
        let s = set_pattern(&format!("{EMBD}=CUDA0"), 0, "  output  ");
        assert_eq!(parse(&s), [rule("output", "CUDA0")]);
    }

    // ── Edits ─────────────────────────────────────────────────────────────

    // Adding a row is one click for the case the table exists for: the embedding
    // table, on a real GPU — never a blank device (which cannot be saved).
    #[test]
    fn add_seeds_the_embedding_table_on_the_first_gpu() {
        let s = add("", &devs());
        assert_eq!(parse(&s), [rule(EMBD, "ROCm0")]);
        assert!(validate(&s).is_ok());
    }

    #[test]
    fn add_falls_back_to_cpu_before_the_probe_lands() {
        assert_eq!(parse(&add("", &[])), [rule(EMBD, "CPU")]);
    }

    #[test]
    fn add_appends_and_remove_takes_one_row_out() {
        let s = add(&add("", &devs()), &devs());
        assert_eq!(parse(&s).len(), 2);
        let s = set_kind(&s, 1, "exps");
        let s = set_device(&s, 1, "CPU");
        assert_eq!(parse(&s), [rule(EMBD, "ROCm0"), rule(EXPS, "CPU")]);

        assert_eq!(parse(&remove(&s, 0)), [rule(EXPS, "CPU")]);
        assert_eq!(remove(&s, 9), s, "an out-of-range index is a no-op");
    }

    // A kind writes its regex over the row — and Custom's regex is the empty
    // string, so picking it hands over an empty field. It CANNOT keep the pattern
    // it was switched from: the kind is derived from the pattern, so a row still
    // holding `\.ffn_…_exps` would just read back as the MoE kind and the text
    // field would never appear (the shape this shipped with, briefly).
    #[test]
    fn every_kind_overwrites_the_pattern_and_custom_clears_it() {
        let s = format!("{EXPS}=ROCm0");
        assert_eq!(parse(&set_kind(&s, 0, "embd")), [rule(EMBD, "ROCm0")]);

        let custom = set_kind(&s, 0, CUSTOM);
        assert_eq!(parse(&custom), [rule("", "ROCm0")], "the device stays put");
        let rows = build_rows(&devs(), &custom);
        assert!(rows[0].custom, "…and the row now offers its regex field");
        assert_eq!(rows[0].kind_index as usize, KINDS.len() - 1);
        // An empty pattern is a real rule llama.cpp would not want, so it says so
        // until the user types — rather than silently saving a no-op rule.
        assert!(validate(&custom).is_err());
        assert_eq!(rows[0].problem, "empty pattern");

        assert_eq!(set_kind(&s, 0, "nonsense"), s, "unknown kind is a no-op");
        assert_eq!(set_kind(&s, 5, "exps"), s, "out-of-range index is a no-op");
    }

    // The rule ORDER is llama.cpp's match order (first hit wins), so no edit may
    // quietly re-sort it.
    #[test]
    fn edits_never_reorder_the_rules() {
        let s = format!("{EXPS}=CPU,{EMBD}=ROCm0");
        assert_eq!(parse(&set_device(&s, 1, "CUDA0"))[0].pattern, EXPS);
        assert_eq!(parse(&set_pattern(&s, 0, "x"))[1].pattern, EMBD);
    }

    // ── Validation ────────────────────────────────────────────────────────

    #[test]
    fn validate_accepts_an_empty_value_and_a_well_formed_one() {
        assert!(validate("").is_ok());
        assert!(validate(&format!("{EMBD}=ROCm0,{EXPS}=CPU")).is_ok());
    }

    #[test]
    fn validate_names_the_offending_rule() {
        let err = validate(&format!("{EMBD}=ROCm0,dangling")).expect_err("no device");
        assert!(err.contains('2'), "names the rule's position: {err}");
        assert!(err.contains("dangling"), "quotes the pattern: {err}");
        assert!(validate("=CPU").is_err(), "empty pattern");
    }

    // An unknown DEVICE is not a save-blocker (the probe is async, and a config
    // can legitimately name another machine's GPU) — it's a row-level flag.
    #[test]
    fn an_unknown_device_is_flagged_on_its_row_not_refused_at_save() {
        let s = format!("{EMBD}=SYCL3");
        assert!(validate(&s).is_ok());
        let rows = build_rows(&devs(), &s);
        assert!(!rows[0].detected);
        assert!(rows[0].problem.contains("SYCL3"));
        // …and it stays selectable, so the next save can't rewrite it away.
        let (labels, values) = device_options(&devs(), &s);
        assert_eq!(values.last().unwrap(), "SYCL3");
        assert!(labels.last().unwrap().starts_with("(custom)"));
        assert_eq!(rows[0].device_index, 3, "the (custom) entry, after the 3 real ones");
    }

    // ── Rows ──────────────────────────────────────────────────────────────

    #[test]
    fn rows_carry_the_kind_the_device_and_no_problem_when_clean() {
        let rows = build_rows(&devs(), &format!("{EMBD}=ROCm0,{EXPS}=CPU"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind_index, 0, "Embedding table");
        assert!(!rows[0].custom);
        assert_eq!(rows[0].device, "ROCm0");
        assert_eq!(rows[0].device_index, 0);
        assert!(rows[0].detected);
        assert_eq!(rows[0].problem, "");
        assert_eq!(rows[1].kind_index, 2, "MoE experts");
        assert_eq!(rows[1].device_index, 2, "CPU is offered, unlike in gpu_split");
    }

    // A hand-written regex lands on the Custom row with its text intact — the
    // only way an unrecognised pattern stays editable.
    #[test]
    fn an_unrecognised_pattern_becomes_the_custom_row() {
        let rows = build_rows(&devs(), r"blk\.1[0-9]\..*=CUDA0");
        assert_eq!(rows[0].kind_index as usize, KINDS.len() - 1);
        assert!(rows[0].custom);
        assert_eq!(rows[0].pattern, r"blk\.1[0-9]\..*");
    }

    #[test]
    fn no_rules_means_no_rows_and_a_summary_that_says_so() {
        assert!(build_rows(&devs(), "").is_empty());
        assert!(summary("").starts_with("(no overrides"));
        assert_eq!(
            summary(&format!("{EMBD}=ROCm0")),
            format!("--override-tensor {EMBD}=ROCm0")
        );
    }

    // The canned experts regex must stay byte-identical to llama.cpp's
    // LLM_FFN_EXPS_REGEX (common/common.h) — the one --cpu-moe installs. If
    // upstream renames its expert tensors, this is the line that has to move.
    #[test]
    fn the_experts_kind_is_llama_cpps_own_regex() {
        assert_eq!(KINDS[2].pattern, r"\.ffn_(up|down|gate|gate_up)_(ch|)exps");
        assert_eq!(KINDS.last().unwrap().id, CUSTOM);
        assert!(KINDS.last().unwrap().pattern.is_empty());
    }
}
