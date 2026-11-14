//! The Tensor placement table's rows: the `--override-tensor` grammar and
//! its edits live in `pai_app::tensor_override` (its module header says what
//! each kind is for); this is the conversion into the Slint
//! `TensorOverrideRow`s the table shows.

pub use pai_app::tensor_override::*;

use crate::devices::DeviceOption;
use crate::gui::TensorOverrideRow;
use crate::strings;

// ── Display ──────────────────────────────────────────────────────────────

/// The table rows, in rule order (which is the order llama.cpp applies them: the
/// FIRST matching rule wins, so the order is meaningful and the table never
/// re-sorts it).
pub fn build_rows(devices: &[DeviceOption], s: &str) -> Vec<TensorOverrideRow> {
    let (_, values) = device_options(devices, s);
    parse(s)
        .into_iter()
        .map(|rule| {
            let (kind, layers) = split_pattern(&rule.pattern);
            let device_index = values
                .iter()
                .position(|v| v.eq_ignore_ascii_case(&rule.device))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1);
            let detected = devices
                .iter()
                .any(|d| d.id.eq_ignore_ascii_case(&rule.device));
            TensorOverrideRow {
                kind_index: i32::try_from(kind).unwrap_or(0),
                custom: is_custom(&rule.pattern),
                per_layer: !KINDS[kind].block.is_empty(),
                layers: layers_text(layers.as_deref()).into(),
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
        return strings::t("override-empty-pattern");
    }
    if rule.device.is_empty() {
        return strings::t("pick-a-device");
    }
    if !detected {
        return strings::fill("device-not-detected", &[&rule.device]);
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices;

    // The same mixed box the gpu_split tests use, plus its CPU, which IS a
    // legal -ot target (and the only one gpu_split filters out).
    const SAMPLE: &str = "Available devices:\n  \
        ROCm0: AMD Radeon AI PRO R9700 (32624 MiB, 32462 MiB free)\n  \
        CUDA0: NVIDIA GeForce RTX 4070 SUPER (12281 MiB, 10844 MiB free)\n  \
        CPU: AMD Ryzen 9 9900X (63090 MiB, 48233 MiB free)\n";

    fn devs() -> Vec<DeviceOption> {
        devices::parse(SAMPLE)
    }

    const EMBD: &str = r"^token_embd\.weight";
    const EXPS: &str = r"\.ffn_(up|down|gate|gate_up)_(ch|)exps";

    fn rule(pattern: &str, device: &str) -> Rule {
        Rule {
            pattern: pattern.into(),
            device: device.into(),
        }
    }

    // A kind writes its regex over the row, and Custom's regex is the empty
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
        // until the user types, rather than silently saving a no-op rule.
        assert!(validate(&custom).is_err());
        assert_eq!(rows[0].problem, "empty pattern");

        assert_eq!(set_kind(&s, 0, "nonsense"), s, "unknown kind is a no-op");
        assert_eq!(set_kind(&s, 5, "exps"), s, "out-of-range index is a no-op");
    }

    // An unknown DEVICE is not a save-blocker (the probe is async, and a config
    // can legitimately name another machine's GPU); it's a row-level flag.
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
        assert_eq!(
            rows[0].device_index, 3,
            "the (custom) entry, after the 3 real ones"
        );
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
        assert_eq!(rows[1].kind_index as usize, idx("exps"), "MoE experts");
        assert_eq!(
            rows[1].device_index, 2,
            "CPU is offered, unlike in gpu_split"
        );
    }

    // A hand-written regex lands on the Custom row with its text intact: the
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

    fn idx(id: &str) -> usize {
        KINDS.iter().position(|k| k.id == id).unwrap()
    }

    #[test]
    fn layers_text_round_trips_through_the_pattern() {
        let s = format!(r"{}=ROCm0", KINDS[idx("ffn")].pattern);
        let s = set_layers(&s, 0, "10-12, 15");
        assert_eq!(
            parse(&s),
            [rule(r"blk\.(10|11|12|15)\.ffn_(up|down|gate)\.", "ROCm0")]
        );
        let rows = build_rows(&devs(), &s);
        assert_eq!(rows[0].kind_index as usize, idx("ffn"));
        assert!(rows[0].per_layer && !rows[0].custom);
        assert_eq!(rows[0].layers, "10-12, 15");

        let one = set_layers(&s, 0, "15");
        assert_eq!(parse(&one)[0].pattern, r"blk\.15\.ffn_(up|down|gate)\.");
        assert_eq!(build_rows(&devs(), &one)[0].layers, "15");

        let all = set_layers(&s, 0, "  ");
        assert_eq!(parse(&all)[0].pattern, KINDS[idx("ffn")].pattern);
        assert_eq!(build_rows(&devs(), &all)[0].layers, "");
    }
}
