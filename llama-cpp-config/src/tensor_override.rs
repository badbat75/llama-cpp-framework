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
//! one its layer would have used. The buffer type is a DEVICE name: llama.cpp
//! builds the lookup from `ggml_backend_dev_buffer_type` over every backend, so
//! the legal values are exactly the ids `--list-devices` prints, plus `CPU`
//! (`common/arg.cpp`, `parse_tensor_buffer_overrides`: an unknown one is a
//! hard `throw`, i.e. the model fails to load).
//!
//! ## Why a table and not a text field
//! The grammar is positional and unforgiving in two ways that a free-text field
//! quietly walks into, because llama.cpp splits BEFORE it parses:
//!   - rules are split on `,`, so a `{1,2}` quantifier inside a regex tears the
//!     rule in half, and the half without an `=` is a fatal "invalid value";
//!   - a rule is split at its FIRST `=`, so an `=` inside the regex silently
//!     eats part of the pattern.
//!
//! Neither is escapable. So the pattern is a field the user can't type freely,
//! which is exactly what `sanitize_pattern` enforces (below), and the device is
//! a dropdown of real device ids, not a string to spell.
//!
//! ## The canned patterns
//! `KINDS` is the discoverable part: every tensor family worth placing, named,
//! and for the ones inside the blocks a Layers field (`10-12, 15`) that limits
//! the rule to those blocks by writing `blk\.(10|11|12|15)` in front of the
//! family's regex, so a placement by layer needs no regex either. `Custom` is
//! the escape hatch for the rest. Two of them
//! are llama.cpp's own regexes verbatim (`LLM_FFN_EXPS_REGEX` and, since v0.4.0,
//! `LLM_FFN_DENSE_REGEX` in common/common.h): a rule with one of them pointed at
//! CPU is what `--cpu-moe` / `--n-cpu-moe` / `--n-cpu-ffn` expand into, so the
//! table can spell the same placement for EVERY layer where the sliders on the
//! Hardware Config card take the first N.
//!
//! `Embedding table` is the one that pays for the table's existence. llama.cpp
//! leaves `token_embd.weight` in HOST memory even when it reports
//! `offloaded 33/33 layers to GPU`: the embedding lookup is a `get_rows` over a
//! handful of tokens, cheap on CPU and worth ~1-2 GiB of VRAM. But with a GPU
//! backend active that host buffer is PINNED (`ROCm_Host` / `CUDA_Host`), and
//! Windows counts pinned host memory as **Shared GPU memory**, so a model that
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

/// A named pattern for the "Tensor" dropdown. The last entry is always `CUSTOM`:
/// the free-text escape hatch, which carries no canned pattern of its own.
pub struct Kind {
    /// Stable id, round-tripped through the Slint combo's `values` model.
    pub id: &'static str,
    pub label: &'static str,
    /// The regex this kind writes into the rule over ALL layers. Empty for
    /// `CUSTOM`.
    pub pattern: &'static str,
    /// For a kind that lives inside the blocks (`blk.<n>.…`), what follows the
    /// block number: a rule limited to some layers is `blk\.<n><block>` or
    /// `blk\.(<n>|<m>|…)<block>`. Empty for a tensor outside the blocks
    /// (embeddings, output head), which has no layer to pick.
    pub block: &'static str,
}

/// The `Custom regex…` kind's id: the one that keeps the row's existing pattern
/// and lets the user edit it.
pub const CUSTOM: &str = "custom";

/// The canned patterns, in dropdown order: every tensor family llama.cpp's
/// loaders name (`llama-arch.cpp`), outside the blocks first, then inside them.
/// None of them carries a `,` or an `=` (see the module header).
///
/// The tensors outside the blocks are anchored with `^`: regex_search matches
/// anywhere in the name, and an unanchored `output\.weight` also takes every
/// `blk.<n>.attn_output.weight`, as `token_embd\.weight` takes Gemma 3n's
/// `per_layer_token_embd.weight`.
///
/// `exps` and `ffn` are llama.cpp's own regexes verbatim (`LLM_FFN_EXPS_REGEX`
/// and `LLM_FFN_DENSE_REGEX` in `common/common.h`), the ones `--cpu-moe` /
/// `--n-cpu-ffn` install, so "MoE experts → CPU" here is exactly `-cmoe`.
pub const KINDS: &[Kind] = &[
    Kind {
        id: "embd",
        label: "Embedding table (token_embd)",
        pattern: r"^token_embd\.weight",
        block: "",
    },
    Kind {
        id: "ple",
        label: "Per-layer embeddings (per_layer_token_embd)",
        pattern: r"^per_layer_token_embd\.weight",
        block: "",
    },
    Kind {
        id: "output",
        label: "Output head (output)",
        pattern: r"^output\.weight",
        block: "",
    },
    Kind {
        id: "layer",
        label: "Whole layer (blk.N.*)",
        pattern: r"^blk\.\d+\.",
        block: r"\.",
    },
    // `\.attn_`, so the dot keeps the hyper-connection `hc_attn_*` out.
    Kind {
        id: "attn",
        label: "Attention (attn_*)",
        pattern: r"\.attn_",
        block: r"\.attn_",
    },
    Kind {
        id: "ssm",
        label: "Linear attention / SSM (ssm_*)",
        pattern: r"\.ssm_",
        block: r"\.ssm_",
    },
    // `LLM_FFN_DENSE_REGEX` verbatim (v0.4.0), what `--n-cpu-ffn` expands into
    // per layer. The trailing `\.` is load-bearing: it is what keeps
    // `ffn_up_exps` / `ffn_up_shexp` OUT, so on a MoE model this reaches only
    // the dense layers it may carry. The three single projections below keep it.
    Kind {
        id: "ffn",
        label: "Dense FFN (ffn_up/down/gate)",
        pattern: r"\.ffn_(up|down|gate)\.",
        block: r"\.ffn_(up|down|gate)\.",
    },
    Kind {
        id: "ffn_up",
        label: "Dense FFN up (ffn_up)",
        pattern: r"\.ffn_up\.",
        block: r"\.ffn_up\.",
    },
    Kind {
        id: "ffn_gate",
        label: "Dense FFN gate (ffn_gate)",
        pattern: r"\.ffn_gate\.",
        block: r"\.ffn_gate\.",
    },
    Kind {
        id: "ffn_down",
        label: "Dense FFN down (ffn_down)",
        pattern: r"\.ffn_down\.",
        block: r"\.ffn_down\.",
    },
    Kind {
        id: "exps",
        label: "MoE experts (ffn_*_exps)",
        pattern: r"\.ffn_(up|down|gate|gate_up)_(ch|)exps",
        block: r"\.ffn_(up|down|gate|gate_up)_(ch|)exps",
    },
    // `gate_` + `(ch|)exps` does not reach the fused `ffn_gate_up_exps`, which
    // only the all-experts kind above names.
    Kind {
        id: "exps_up",
        label: "MoE experts up (ffn_up_exps)",
        pattern: r"\.ffn_up_(ch|)exps",
        block: r"\.ffn_up_(ch|)exps",
    },
    Kind {
        id: "exps_gate",
        label: "MoE experts gate (ffn_gate_exps)",
        pattern: r"\.ffn_gate_(ch|)exps",
        block: r"\.ffn_gate_(ch|)exps",
    },
    Kind {
        id: "exps_down",
        label: "MoE experts down (ffn_down_exps)",
        pattern: r"\.ffn_down_(ch|)exps",
        block: r"\.ffn_down_(ch|)exps",
    },
    Kind {
        id: "shexp",
        label: "MoE shared expert (ffn_*_shexp)",
        pattern: r"\.ffn_(up|down|gate)_shexp",
        block: r"\.ffn_(up|down|gate)_shexp",
    },
    // `ffn_gate_inp` and, on models with a gated shared expert, its
    // `ffn_gate_inp_shexp`: both are the router's small matrices.
    Kind {
        id: "router",
        label: "MoE router (ffn_gate_inp)",
        pattern: r"\.ffn_gate_inp",
        block: r"\.ffn_gate_inp",
    },
    Kind {
        id: CUSTOM,
        label: "Custom regex…",
        pattern: "",
        block: "",
    },
];

/// Patterns this table once wrote, unanchored, read as the anchored kind they
/// meant (and written back anchored on the next save). Only where the two match
/// the same tensors on every model the old one was offered for:
/// `token_embd\.weight` differs from `^token_embd\.weight` only on a model with
/// per-layer embeddings, where the anchored one is what "Embedding table" means.
const ALIASES: &[(&str, &str)] = &[(r"token_embd\.weight", r"^token_embd\.weight")];

/// The largest layer number a range may name, and the most layers one rule may
/// list: each becomes an alternative of the regex, and no model has more.
const MAX_LAYER: u32 = 999;
const MAX_LAYERS: usize = 256;

/// A pattern taken apart: its kind and, for a rule limited to some layers, the
/// layer numbers (ascending, no repeats). `None` = every layer (or a kind with
/// none). An unrecognised pattern is `CUSTOM` with no layers.
fn split_pattern(pattern: &str) -> (usize, Option<Vec<u32>>) {
    let custom = KINDS.len() - 1;
    if let Some(i) = KINDS
        .iter()
        .position(|k| k.id != CUSTOM && k.pattern == pattern)
    {
        return (i, None);
    }
    let Some(rest) = pattern.strip_prefix(r"blk\.") else {
        return (custom, None);
    };
    let (nums, block) = if let Some(inner) = rest.strip_prefix('(') {
        let Some((list, block)) = inner.split_once(')') else {
            return (custom, None);
        };
        (list, block)
    } else {
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        (&rest[..end], &rest[end..])
    };
    let Some(kind) = KINDS
        .iter()
        .position(|k| !k.block.is_empty() && k.block == block)
    else {
        return (custom, None);
    };
    let mut layers = Vec::new();
    for n in nums.split('|') {
        let ok = !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit());
        match n.parse::<u32>() {
            Ok(v) if ok => layers.push(v),
            _ => return (custom, None),
        }
    }
    // Only the form `compose` writes reads back as a kind: ascending, no repeats,
    // no leading zeros (`blk\.07\.` matches no tensor). Anything else stays the
    // hand-written regex it is.
    let canonical = layers
        .iter()
        .zip(nums.split('|'))
        .all(|(v, n)| v.to_string() == n);
    if !canonical || layers.windows(2).any(|w| w[0] >= w[1]) {
        return (custom, None);
    }
    (kind, Some(layers))
}

/// The pattern for `kind` limited to `layers` (`None` = every layer).
pub(crate) fn compose(kind: &Kind, layers: Option<&[u32]>) -> String {
    match layers {
        Some(l) if !kind.block.is_empty() && !l.is_empty() => {
            let nums: Vec<String> = l.iter().map(u32::to_string).collect();
            if nums.len() == 1 {
                format!(r"blk\.{}{}", nums[0], kind.block)
            } else {
                format!(r"blk\.({}){}", nums.join("|"), kind.block)
            }
        }
        _ => kind.pattern.to_string(),
    }
}

/// The Layers field's text: `10-12, 15`; empty = every layer.
fn layers_text(layers: Option<&[u32]>) -> String {
    let Some(l) = layers else {
        return String::new();
    };
    let mut parts = Vec::new();
    let mut i = 0;
    while i < l.len() {
        let mut j = i;
        while j + 1 < l.len() && l[j + 1] == l[j] + 1 {
            j += 1;
        }
        parts.push(if j == i {
            l[i].to_string()
        } else {
            format!("{}-{}", l[i], l[j])
        });
        i = j + 1;
    }
    parts.join(", ")
}

/// The Layers field → layer numbers. `10-12, 15` (commas or spaces) gives
/// `[10, 11, 12, 15]`; blank gives `None` (every layer). `Err` for anything
/// else, a range out of order or past `MAX_LAYER`, or more than `MAX_LAYERS`.
fn parse_layers(text: &str) -> Result<Option<Vec<u32>>, ()> {
    let mut set = std::collections::BTreeSet::new();
    for part in text.split([',', ' ']).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: u32 = a.parse().map_err(|_| ())?;
        let b: u32 = b.parse().map_err(|_| ())?;
        if a > b || b > MAX_LAYER || (b - a) as usize >= MAX_LAYERS {
            return Err(());
        }
        set.extend(a..=b);
        if set.len() > MAX_LAYERS {
            return Err(());
        }
    }
    Ok(if set.is_empty() {
        None
    } else {
        Some(set.into_iter().collect())
    })
}

/// The kind a pattern belongs to: the canned entry it matches (over every layer
/// or some), else `CUSTOM` (always the last index, so an unrecognised, i.e.
/// hand-written, pattern lands on the free-text row).
fn kind_index(pattern: &str) -> usize {
    split_pattern(pattern).0
}

fn is_custom(pattern: &str) -> bool {
    kind_index(pattern) == KINDS.len() - 1
}

// ── String ↔ rules ───────────────────────────────────────────────────────

/// `"token_embd\.weight=ROCm0, x=CPU"` → two rules. Tolerant on purpose: a piece
/// with no `=` (only reachable from a hand-edited INI: the table can't produce
/// one) keeps its text as the pattern and an EMPTY device, so the row survives,
/// shows its problem, and can be fixed instead of vanishing on the next save.
pub fn parse(s: &str) -> Vec<Rule> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|piece| match piece.split_once('=') {
            Some((pattern, device)) => Rule {
                pattern: unalias(pattern.trim()),
                device: device.trim().to_string(),
            },
            None => Rule {
                pattern: unalias(piece),
                device: String::new(),
            },
        })
        .collect()
}

/// A pattern as the table reads it: an `ALIASES` entry becomes its kind's.
fn unalias(pattern: &str) -> String {
    ALIASES
        .iter()
        .find(|(old, _)| *old == pattern)
        .map_or(pattern, |(_, new)| new)
        .to_string()
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

/// Append a rule. Seeded with the Embedding table, the reason this table exists
/// (module header), on the first real GPU the probe found, so the common case is
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
/// whatever was there, keeping the row's layers when both kinds have them, and
/// `CUSTOM` (whose canned regex is the empty string)
/// therefore CLEARS it, handing the user an empty field to type into.
///
/// That is not a choice so much as a consequence: a row's kind is DERIVED from
/// its pattern (`kind_index`), with no hidden "is custom" bit anywhere, because a
/// second source of truth is a second thing to keep in sync with a hand-edited
/// INI. So a row still holding `\.ffn_…_exps` necessarily reads as the MoE kind:
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
    let (_, layers) = split_pattern(&rule.pattern);
    rule.pattern = compose(kind, layers.as_deref());
    render(&rules)
}

/// Limit a row to some layers (the Layers field; see `parse_layers`), or blank
/// for every layer. A no-op on a kind without layers, and on text that is not a
/// layer list yet (a half-typed `10-`): the INI keeps the last good value, which
/// the summary line under the table shows.
pub fn set_layers(s: &str, index: usize, text: &str) -> String {
    let Ok(layers) = parse_layers(text) else {
        return s.to_string();
    };
    let mut rules = parse(s);
    let Some(rule) = rules.get_mut(index) else {
        return s.to_string();
    };
    let kind = &KINDS[kind_index(&rule.pattern)];
    if kind.block.is_empty() {
        return s.to_string();
    }
    rule.pattern = compose(kind, layers.as_deref());
    render(&rules)
}

/// Set a row's regex (the Custom row's text field). Sanitized; see
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
/// (the table sanitizes), but that is exactly the case worth catching, because
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
                "tensor override {n} (`{}`) names no device: a rule is `<pattern>=<device>`, \
                 and llama.cpp splits it at the first `=`",
                rule.pattern
            ));
        }
    }
    Ok(())
}

/// True when the rules pin the embedding table to a GPU, the one placement whose
/// payoff depends on the MODEL, not on the rule: `token_embd` must be stored in a
/// type ggml-cuda/ggml-hip can `get_rows` on-device (see `gguf::ModelInfo::
/// embd_pin_warning`). `CPU` targets are not a pin (leaving the table in host
/// memory is exactly what the rule would be undoing), so they don't count, and an
/// empty device is `validate`'s business, not ours.
pub fn pins_embeddings_to_gpu(s: &str) -> bool {
    parse(s).iter().any(|r| {
        r.pattern.contains("token_embd")
            && !r.pattern.contains("per_layer_token_embd")
            && !r.device.is_empty()
            && !r.device.eq_ignore_ascii_case("CPU")
    })
}

// ── Display ──────────────────────────────────────────────────────────────

/// The dropdown behind every row's Device cell: every probed device INCLUDING
/// the CPU (unlike the GPU distribution table, which filters it out: `-ot`'s
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
        return "(no overrides: every tensor goes wherever its layer went)".into();
    }
    format!("--override-tensor {}", render(&parse(s)))
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

    // ── The grammar ───────────────────────────────────────────────────────

    #[test]
    fn parse_and_render_round_trip_a_multi_rule_value() {
        let s = format!("{EMBD}=ROCm0,{EXPS}=CPU");
        assert_eq!(
            parse(&s),
            [rule(EMBD, "ROCm0"), rule(EXPS, "CPU")],
            "the experts regex carries `|` and `(`; neither is a separator"
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
    // pattern can never own one, and a device that itself contained `=` would
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
    // table, on a real GPU, never a blank device (which cannot be saved).
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

    // Only a GPU target is a "pin": `=CPU` leaves the table in host memory, which
    // is the state the rule would otherwise be undoing, so it must NOT trip the
    // model-dependent warning. Neither may an experts-only rule.
    #[test]
    fn only_a_gpu_target_counts_as_pinning_the_embeddings() {
        assert!(pins_embeddings_to_gpu(&format!("{EMBD}=ROCm0")));
        assert!(pins_embeddings_to_gpu(&format!("{EMBD}=CUDA0")));
        assert!(!pins_embeddings_to_gpu(&format!("{EMBD}=CPU")));
        assert!(!pins_embeddings_to_gpu(&format!("{EMBD}=cpu")));
        assert!(!pins_embeddings_to_gpu(r"\.ffn_up_exps=CPU"));
        assert!(!pins_embeddings_to_gpu(""));
        // A rule with no device is validate()'s complaint, not this one's.
        assert!(!pins_embeddings_to_gpu(EMBD));
        // Found among several rules, not just alone.
        assert!(pins_embeddings_to_gpu(&format!(
            r"\.ffn_up_exps=CPU,{EMBD}=ROCm0"
        )));
    }

    // The canned experts and dense-FFN regexes must stay byte-identical to
    // llama.cpp's LLM_FFN_EXPS_REGEX / LLM_FFN_DENSE_REGEX (common/common.h),
    // the ones --cpu-moe / --n-cpu-ffn install. If upstream renames its FFN
    // tensors, these are the lines that have to move. Checked against v0.4.0.
    #[test]
    fn the_ffn_kinds_are_llama_cpps_own_regexes() {
        let exps = &KINDS[idx("exps")];
        let ffn = &KINDS[idx("ffn")];
        assert_eq!(exps.pattern, r"\.ffn_(up|down|gate|gate_up)_(ch|)exps");
        assert_eq!(ffn.pattern, r"\.ffn_(up|down|gate)\.");
        assert_eq!(KINDS.last().unwrap().id, CUSTOM);
        assert!(KINDS.last().unwrap().pattern.is_empty());
        // The dense regex must not swallow the expert or shared-expert tensors
        // (`ffn_up_exps`, `ffn_up_shexp`): the trailing `\.` is the whole
        // difference between the two kinds, so pin that character (this crate
        // carries no regex engine to match against).
        assert!(ffn.pattern.ends_with(r"\."));
        assert!(!ffn.pattern.contains("exps"));
    }

    fn idx(id: &str) -> usize {
        KINDS.iter().position(|k| k.id == id).unwrap()
    }

    // ── Layers ────────────────────────────────────────────────────────────

    // No canned pattern may hold what llama.cpp splits on, and a per-layer
    // kind's block part must be what follows the number in its all-layers form
    // (or a ranged rule would match other tensors than the unranged one).
    #[test]
    fn every_kind_is_splittable_and_its_block_agrees_with_its_pattern() {
        for k in KINDS {
            assert!(!k.pattern.contains([',', '=']), "{}", k.id);
            if !k.block.is_empty() && k.id != "layer" {
                assert_eq!(k.pattern, k.block, "{}", k.id);
            }
            if k.id != CUSTOM {
                assert_eq!(split_pattern(k.pattern), (idx(k.id), None), "{}", k.id);
            }
        }
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

    #[test]
    fn half_typed_or_bad_layers_leave_the_value_alone() {
        let s = format!(r"{}=CPU", KINDS[idx("exps")].pattern);
        for bad in ["10-", "x", "5-3", "1000", "0-999"] {
            assert_eq!(set_layers(&s, 0, bad), s, "{bad:?}");
        }
        let embd = format!("{EMBD}=ROCm0");
        assert_eq!(
            set_layers(&embd, 0, "3"),
            embd,
            "no layers outside the blocks"
        );
    }

    #[test]
    fn a_new_kind_keeps_the_rows_layers() {
        let s = set_layers(&format!(r"{}=CPU", KINDS[idx("exps")].pattern), 0, "0-2");
        let s = set_kind(&s, 0, "exps_down");
        assert_eq!(parse(&s)[0].pattern, r"blk\.(0|1|2)\.ffn_down_(ch|)exps");
        let s = set_kind(&s, 0, "layer");
        assert_eq!(parse(&s)[0].pattern, r"blk\.(0|1|2)\.");
        let s = set_kind(&s, 0, "output");
        assert_eq!(parse(&s)[0].pattern, r"^output\.weight");
    }

    // A block list in any other shape than the one `compose` writes is a
    // hand-written regex, kept verbatim on the Custom row.
    #[test]
    fn only_the_composed_layer_form_reads_back_as_a_kind() {
        for p in [
            r"blk\.(3|1)\.ffn_up\.",
            r"blk\.(1|1)\.ffn_up\.",
            r"blk\.07\.ffn_up\.",
            r"blk\.15\.ffn_(up|down)\.weight",
            r"blk\.1[0-9]\.ffn_up\.",
        ] {
            assert_eq!(kind_index(p), KINDS.len() - 1, "{p}");
        }
    }

    // The unanchored embedding regex this table used to write also took Gemma
    // 3n's per_layer_token_embd; it reads as the anchored kind it meant.
    #[test]
    fn the_old_unanchored_embedding_pattern_reads_as_the_kind() {
        assert_eq!(parse(r"token_embd\.weight=CUDA0"), [rule(EMBD, "CUDA0")]);
    }
}
