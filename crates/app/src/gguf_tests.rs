use super::*;
use std::collections::HashMap;

/// A map-backed `KvSource` so field extraction is tested without the DLL.
enum Tv {
    U(u32),
    S(&'static str),
    B(bool),
    F(f32),
    I(i32),
}
struct Map {
    kv: HashMap<&'static str, Tv>,
    /// name -> (ggml_type, bytes), mirroring the GGUF's tensor infos.
    tensors: HashMap<&'static str, (u32, u64)>,
}
impl KvSource for Map {
    fn u32(&self, key: &str) -> Option<u32> {
        match self.kv.get(key) {
            Some(Tv::U(n)) => Some(*n),
            _ => None,
        }
    }
    fn string(&self, key: &str) -> Option<String> {
        match self.kv.get(key) {
            Some(Tv::S(s)) => Some((*s).to_string()),
            _ => None,
        }
    }
    fn boolean(&self, key: &str) -> Option<bool> {
        match self.kv.get(key) {
            Some(Tv::B(b)) => Some(*b),
            _ => None,
        }
    }
    // Same widening as the ffi source: floats through `f32_decimal`,
    // integers of either sign as numbers.
    fn f64(&self, key: &str) -> Option<f64> {
        match self.kv.get(key) {
            Some(Tv::F(v)) => Some(f32_decimal(*v)),
            Some(Tv::I(v)) => Some(*v as f64),
            Some(Tv::U(v)) => Some(*v as f64),
            _ => None,
        }
    }
    fn i64(&self, key: &str) -> Option<i64> {
        match self.kv.get(key) {
            Some(Tv::I(v)) => Some(*v as i64),
            Some(Tv::U(v)) => Some(*v as i64),
            _ => None,
        }
    }
    fn tensor(&self, name: &str) -> Option<(u32, u64)> {
        self.tensors.get(name).copied()
    }
}

/// The `general.sampling.*` block: floats come back as the decimal the
/// converter wrote (not the float32's binary expansion), integers of any
/// width are read, and an absent key stays `None` so the model profile leaves
/// that sampler to llama.cpp.
#[test]
fn sampling_block_reads_the_converter_values() {
    let info = ModelInfo::from_kv(&map(vec![
        ("general.architecture", Tv::S("qwen3")),
        ("general.sampling.temp", Tv::F(0.6)),
        ("general.sampling.top_k", Tv::I(20)),
        ("general.sampling.top_p", Tv::F(0.95)),
        // A converter that wrote a float as an int still reaches us.
        ("general.sampling.penalty_repeat", Tv::U(1)),
    ]))
    .unwrap();
    assert_eq!(
        info.sampling,
        Sampling {
            temp: Some(0.6),
            top_k: Some(20),
            top_p: Some(0.95),
            min_p: None,
            penalty_repeat: Some(1.0),
        }
    );
    let bare = ModelInfo::from_kv(&map(vec![("general.architecture", Tv::S("llama"))])).unwrap();
    assert_eq!(bare.sampling, Sampling::default());
}

#[test]
fn f32_decimal_keeps_the_written_value() {
    assert_eq!(f32_decimal(0.95), 0.95);
    assert_eq!(f32_decimal(0.6), 0.6);
    assert_eq!(f32_decimal(1.0), 1.0);
}

fn map(pairs: Vec<(&'static str, Tv)>) -> Map {
    Map {
        kv: pairs.into_iter().collect(),
        tensors: HashMap::new(),
    }
}

/// `map` plus the GGUF's tensor infos: `token_embd.weight` for the
/// Embeddings row, the Markov / nextn heads for `spec_type_from_kv`.
fn map_t(pairs: Vec<(&'static str, Tv)>, tensors: Vec<(&'static str, (u32, u64))>) -> Map {
    Map {
        kv: pairs.into_iter().collect(),
        tensors: tensors.into_iter().collect(),
    }
}

#[test]
fn extracts_moe_model_with_layer_split() {
    let m = map(vec![
        ("general.architecture", Tv::S("deepseek2")),
        ("general.file_type", Tv::U(15)),
        ("general.size_label", Tv::S("236B-A21B")),
        ("deepseek2.block_count", Tv::U(48)),
        ("deepseek2.leading_dense_block_count", Tv::U(3)),
        ("deepseek2.context_length", Tv::U(163840)),
        ("deepseek2.expert_count", Tv::U(160)),
        ("deepseek2.expert_used_count", Tv::U(6)),
        ("deepseek2.expert_shared_count", Tv::U(2)),
        ("deepseek2.attention.head_count", Tv::U(128)),
        ("deepseek2.attention.head_count_kv", Tv::U(128)),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert!(info.is_moe);
    assert_eq!(info.quant, "Q4_K_M");
    assert_eq!(info.kind_line(), "MoE: 160 experts, 6 active/tok, 2 shared");
    assert_eq!(
        info.moe_offload_line(),
        "45 of 48 layers hold experts (first 3 dense)"
    );
    assert_eq!(info.arch_quant_line(), "deepseek2  ·  Q4_K_M  ·  236B-A21B");
    assert_eq!(info.layers_ctx_line(), "48 layers  ·  trained ctx 163,840");
    assert_eq!(info.attn_line(), "128 heads (MHA)");
}

#[test]
fn extracts_dense_model_and_gqa() {
    let m = map(vec![
        ("general.architecture", Tv::S("gemma3")),
        ("general.file_type", Tv::U(18)),
        ("gemma3.block_count", Tv::U(48)),
        ("gemma3.attention.head_count", Tv::U(32)),
        ("gemma3.attention.head_count_kv", Tv::U(8)),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert!(!info.is_moe);
    assert_eq!(info.kind_line(), "Dense");
    assert!(info.moe_offload_line().is_empty());
    assert_eq!(info.attn_line(), "32 heads / 8 KV (GQA 4×)");
    assert_eq!(info.quant, "Q6_K");
}

#[test]
fn chat_template_line_classifies_jinja_legacy_and_none() {
    // No template metadata → "none", and the raw value is None.
    let m = map(vec![("general.architecture", Tv::S("llama"))]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(info.chat_template_line(), "none");
    assert!(info.chat_template.is_none());

    // Jinja markers ({% / {{) → "Jinja (embedded)".
    let m = map(vec![
        ("general.architecture", Tv::S("llama")),
        (
            "tokenizer.chat_template",
            Tv::S("{% for message in messages %}{{ message.content }}{% endfor %}"),
        ),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(info.chat_template_line(), "Jinja (embedded)");
    assert!(info
        .chat_template
        .as_deref()
        .is_some_and(|t| t.contains("{%")));

    // Marker-less string (e.g. a builtin name) → "embedded (non-Jinja)".
    let m = map(vec![
        ("general.architecture", Tv::S("llama")),
        ("tokenizer.chat_template", Tv::S("chatml")),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(info.chat_template_line(), "embedded (non-Jinja)");
}

/// Build a model whose only interesting metadata is its chat template.
fn with_template(tmpl: &'static str) -> ModelInfo {
    ModelInfo::from_kv(&map(vec![
        ("general.architecture", Tv::S("qwen35")),
        ("tokenizer.chat_template", Tv::S(tmpl)),
    ]))
    .unwrap()
}

/// The Thinking / Past-thinking rows, against the shapes llama.cpp's own
/// bundled templates (`models/templates/`) actually use. The pairing is the
/// point: the two rows answer different questions, and a model can be a
/// thinker whose past thinking no flag can keep (`enable_thinking` with no
/// preserve variable: the Qwen3.5 form, which re-renders history WITHOUT the
/// trace as soon as a newer turn arrives).
#[test]
fn thinking_rows_read_the_levers_the_template_actually_exposes() {
    // Qwen3.5: switchable, but nothing gates the history → the flag is inert.
    let qwen35 = with_template(
        "{%- if enable_thinking is defined and enable_thinking is false %}{{- '<think>\\n\\n</think>\\n\\n' }}\
             {%- endif %}{%- if loop.index0 > ns.last_query_index %}{{- '<think>' + reasoning_content + '</think>' }}{%- endif %}",
    );
    assert_eq!(qwen35.thinking_line(), "yes");
    assert_eq!(qwen35.past_thinking_line(), "no");

    // Qwen3.6 adds the gate (README's "Keep past thinking" section quotes it).
    let qwen36 = with_template(
        "{%- if enable_thinking %}…{%- endif %}\
             {%- if preserve_thinking or loop.index0 > ns.last_query_index %}{{- '<think>' + reasoning_content + '</think>' }}{%- endif %}",
    );
    assert_eq!(qwen36.past_thinking_line(), "yes");

    // Same flag, a different one of its three variables (GLM-4.7 / Nemotron).
    let glm = with_template(
        "{%- if enable_thinking %}…{%- endif %}\
             {%- if ((clear_thinking is defined and not clear_thinking) or loop.index0 > ns.last_user_index) and reasoning_content -%}{{ '<think>' + reasoning_content + 'enddate' }}{%- endif -%}",
    );
    assert_eq!(glm.past_thinking_line(), "yes");
    let nemotron = with_template(
        "{%- if truncate_history_thinking %}{%- set content = content.split('enddate')[-1] %}{%- endif %}",
    );
    assert_eq!(nemotron.past_thinking_line(), "yes");

    // Ministral-3-Reasoning: [THINK] tags and a 'thinking' content block, no
    // enable_thinking anywhere; it always thinks and --reasoning can't stop it.
    let ministral = with_template(
        "{%- elif block['type'] == 'thinking' %}{{- '[THINK]' + block['thinking'] + '[/THINK]' }}",
    );
    assert_eq!(ministral.thinking_line(), "yes");
    assert_eq!(ministral.past_thinking_line(), "no");

    // gpt-oss: no on/off switch, a reasoning_effort level instead.
    let gpt_oss = with_template(
        "{%- if reasoning_effort is not defined %}{%- set reasoning_effort = \"medium\" %}{%- endif %}\
             {{- \"# Valid channels: analysis, commentary, final.\" }}{{- message.thinking }}",
    );
    assert_eq!(gpt_oss.thinking_line(), "yes");
    assert_eq!(gpt_oss.thinking(), Thinking::EffortOnly);

    // The SECOND name --reasoning-effort binds, and the reason EFFORT_VARS is
    // a list: `caps_apply_reasoning_effort` sets reasoning_effort AND
    // reasoning_strength from one value, so a template reading only the
    // latter takes the level just as much. Matching the first name alone
    // filed this under Always, i.e. "the flag has nothing to act on here",
    // which is the opposite answer.
    let strength_only = with_template(
        "{%- if reasoning_strength == 'high' %}{{- '<think>' }}{%- endif %}{{- message.thinking }}",
    );
    assert_eq!(strength_only.thinking_line(), "yes");
    assert_eq!(strength_only.thinking(), Thinking::EffortOnly);

    // A template with no thinking mechanics (Llama-3.1's), but MiMo-VL, which
    // DOES think, has one just as bare, so the verdict names the TEMPLATE and
    // never the model. What the silence proves is only that --reasoning and
    // --reasoning-preserve have nothing here to act on, which is exactly what
    // both rows are for.
    let llama = with_template(
        "{%- for message in messages %}{{- '<|start_header_id|>' + message['role'] + '<|end_header_id|>' + message['content'] }}{%- endfor %}",
    );
    assert_eq!(llama.thinking_line(), "no marker in the chat template");
    assert_eq!(llama.past_thinking_line(), "no");

    // No template at all: say so, rather than claiming the model can't think.
    let bare = ModelInfo::from_kv(&map(vec![("general.architecture", Tv::S("llama"))])).unwrap();
    assert_eq!(bare.thinking_line(), "unknown (no embedded chat template)");
    assert_eq!(
        bare.past_thinking_line(),
        "unknown (no embedded chat template)"
    );
}

#[test]
fn draft_file_line_surfaces_dflash_block_size() {
    let m = map(vec![
        ("general.architecture", Tv::S("dflash")),
        ("general.file_type", Tv::U(1)),
        ("dflash.block_count", Tv::U(12)),
        ("dflash.block_size", Tv::U(16)),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(
        info.draft_file_line(),
        "dflash  ·  F16  ·  12 layers  ·  block_size 16 (n-max ≤ 15)"
    );
}

#[test]
fn extracts_mmproj_vision() {
    let m = map(vec![
        ("clip.projector_type", Tv::S("gemma3")),
        ("clip.has_vision_encoder", Tv::B(true)),
        ("clip.vision.block_count", Tv::U(27)),
        ("clip.vision.image_size", Tv::U(896)),
        ("clip.vision.patch_size", Tv::U(14)),
    ]);
    let mp = MmprojInfo::from_kv(&m).unwrap();
    assert!(mp.has_vision);
    assert_eq!(
        mp.mmproj_line(),
        "gemma3  ·  vision  ·  27 enc. layers  ·  896px / patch 14"
    );
}

#[test]
fn non_clip_file_is_not_mmproj() {
    let m = map(vec![("general.architecture", Tv::S("llama"))]);
    assert!(MmprojInfo::from_kv(&m).is_none());
}

#[test]
fn draft_line_reports_the_embedded_mtp_heads() {
    let m = map(vec![
        ("general.architecture", Tv::S("glm4moe")),
        ("glm4moe.expert_count", Tv::U(128)),
        ("glm4moe.nextn_predict_layers", Tv::U(1)),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(draft_line(&info, ""), "MTP: 1 nextn layer");
}

/// The v1.11.2 report, in two halves. The row said the same thing whether a
/// drafter was picked or not (and after a restart with none picked), because
/// it described what a `mtps\` filename scan FOUND rather than what the
/// model profile loads. So: a pick is NAMED, and no pick leaves nothing behind but
/// what the model itself embeds. Nothing in this row may be a file the
/// model profile does not load.
#[test]
fn draft_line_names_the_picked_file_and_nothing_else() {
    let m = map(vec![
        ("general.architecture", Tv::S("qwen35")),
        ("qwen35.nextn_predict_layers", Tv::U(1)),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    let picked = draft_line(
        &info,
        &crate::paths::os_path(r"C:\models\mtps\qwen-mtp.gguf"),
    );
    assert!(picked.contains("MTP: 1 nextn layer"));
    assert!(picked.contains("draft file: qwen-mtp.gguf"));
    assert_eq!(
        draft_line(&info, ""),
        "MTP: 1 nextn layer",
        "with no draft picked the row must name no file at all"
    );
}

/// A model that embeds nothing, with nothing picked, must still say
/// something: an empty value reads as a broken row.
#[test]
fn draft_line_says_none_when_there_is_nothing_to_report() {
    let m = map(vec![("general.architecture", Tv::S("llama"))]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(draft_line(&info, ""), "none");
}

// ── Draft speculator classification ──────────────────────────────────
//
// The reason the picker cannot go on the folder alone: a DSpark drafter and
// a plain DFlash one share the arch, the file-name shape and the `dflashs\`
// folder they are dropped in. Only the Markov head separates them, and
// llama.cpp reads exactly this to answer the same question
// (`common_speculative_types_from_gguf`).

#[test]
fn spec_type_reads_dspark_from_the_markov_head() {
    let m = map_t(
        vec![("general.architecture", Tv::S("dflash"))],
        vec![("markov_w1.weight", (0, 4096))],
    );
    assert_eq!(spec_type_from_kv(&m), Some("draft-dspark"));
}

#[test]
fn spec_type_reads_plain_dflash_without_it() {
    let m = map(vec![("general.architecture", Tv::S("dflash"))]);
    assert_eq!(spec_type_from_kv(&m), Some("draft-dflash"));
}

// An MTP head announces itself on the LAST block, so the probe has to be
// aimed there: `block_count - 1`, not block 0 and not a scan.
#[test]
fn spec_type_reads_mtp_from_the_last_blocks_nextn_head() {
    let m = map_t(
        vec![
            ("general.architecture", Tv::S("qwen35moe")),
            ("qwen35moe.block_count", Tv::U(48)),
        ],
        vec![("blk.47.nextn.eh_proj.weight", (0, 4096))],
    );
    assert_eq!(spec_type_from_kv(&m), Some("draft-mtp"));

    // Same file, the head on any other block: not what llama.cpp looks for.
    let m = map_t(
        vec![
            ("general.architecture", Tv::S("qwen35moe")),
            ("qwen35moe.block_count", Tv::U(48)),
        ],
        vec![("blk.0.nextn.eh_proj.weight", (0, 4096))],
    );
    assert_eq!(spec_type_from_kv(&m), None);
}

// A head file with no blocks of its own (gemma4-assistant, block_count 0)
// matches nothing, upstream included. `None` is "the header did not say",
// which is why the caller falls back to the folder instead of clearing the
// key: that file IS a draft-mtp head.
#[test]
fn spec_type_is_silent_for_a_blockless_head_file() {
    let m = map_t(
        vec![
            ("general.architecture", Tv::S("gemma4-assistant")),
            ("gemma4-assistant.block_count", Tv::U(0)),
        ],
        vec![("token_embd.weight", (8, 4096))],
    );
    assert_eq!(spec_type_from_kv(&m), None);

    // …and with no block_count key at all, where upstream's own reader would
    // assert on the missing key rather than answer.
    let m = map(vec![("general.architecture", Tv::S("gemma4-assistant"))]);
    assert_eq!(spec_type_from_kv(&m), None);
}

/// The whole point of the Embeddings row: the FILE NAME does not tell you
/// which type `token_embd` is stored in. These are the real headers of three
/// Unsloth "XL" builds of the SAME model: the Q6_K_XL keeps its embedding
/// table at Q8_0 while the Q5/Q4 builds drop it to a K-quant. Up to b10088
/// that was the difference between a pin that worked and one that
/// round-tripped the table to the host every token, collapsing decode; since
/// b10089 (#25962)
/// the K-quants have a kernel and all three pin, so the verdict has to
/// follow the TYPE against the current whitelist, not a remembered rule.
#[test]
fn embd_verdict_follows_the_tensor_type_not_the_file_name() {
    let base = || vec![("general.architecture", Tv::S("qwen35"))];

    // Qwen3.6-27B-UD-MTP-Q6_K_XL.gguf, embd is Q8_0: whitelisted.
    let info = ModelInfo::from_kv(&map_t(
        base(),
        vec![("token_embd.weight", (8, 1_351_614_464))],
    ))
    .unwrap();
    assert_eq!(
        info.embd_line(),
        format!("{}  ·  1.26 GiB", ggml_type_name(8))
    );
    assert!(info.embd_pin_warning().is_empty());

    // Qwen3.6-27B-UD-Q4_K_XL.gguf, embd is Q4_K: a K-quant, pinnable since
    // b10089. The ROW is unchanged in shape (facts only); the VERDICT lives
    // in the warning, which must now stay quiet.
    let info = ModelInfo::from_kv(&map_t(
        base(),
        vec![("token_embd.weight", (12, 715_128_832))],
    ))
    .unwrap();
    assert_eq!(
        info.embd_line(),
        format!("{}  ·  682 MiB", ggml_type_name(12))
    );
    assert!(info.embd_pin_warning().is_empty());

    // Q5_K (13) likewise.
    let info = ModelInfo::from_kv(&map_t(
        base(),
        vec![("token_embd.weight", (13, 873_463_808))],
    ))
    .unwrap();
    assert_eq!(
        info.embd_line(),
        format!("{}  ·  833 MiB", ggml_type_name(13))
    );
    assert!(info.embd_pin_warning().is_empty());

    // A type with NO kernel still warns, naming the type: Q8_K (15) is the
    // one `default: return false` case a quantizer could plausibly emit.
    let info = ModelInfo::from_kv(&map_t(
        base(),
        vec![("token_embd.weight", (15, 873_463_808))],
    ))
    .unwrap();
    assert!(info.embd_pin_warning().contains(&ggml_type_name(15)));

    // Unknown type (no `token_embd.weight`, or no DLL to read it with) must
    // NOT warn: a metadata read we couldn't do is not evidence of a problem.
    // This is also what keeps the e2e fixture's unreadable GGUF quiet.
    let info = ModelInfo::from_kv(&map(base())).unwrap();
    assert_eq!(info.embd_line(), "n/a");
    assert!(info.embd_pin_warning().is_empty());
}

/// The two 32-value formats are conditional in upstream's switch: the
/// kernel iterates 256-value super-blocks, so the row length decides, and an
/// unknown width must land on the warning side.
#[test]
fn get_rows_width_condition_for_iq4_nl_and_mxfp4() {
    for ty in [20u32, 39] {
        assert!(gpu_has_get_rows(ty, 4096), "type {ty} at a 256-multiple");
        assert!(!gpu_has_get_rows(ty, 4000), "type {ty} off the grid");
        assert!(!gpu_has_get_rows(ty, 0), "type {ty} with unknown width");
    }
    // The width never rescues a type with no kernel at all.
    assert!(!gpu_has_get_rows(15, 4096));
}

/// `ggml_type` and `general.file_type` (LLAMA_FTYPE) are different enums that
/// overlap numerically: 7 means Q8_0 as an ftype but Q5_1 as a ggml_type, and
/// 14 means Q4_K_S vs Q6_K. Reading a tensor's type through `ftype_name` (or
/// vice versa) would mislabel almost everything, so pin the divergence.
#[test]
fn ggml_type_and_ftype_enums_are_not_interchangeable() {
    assert_ne!(ggml_type_name(7), ftype_name(7));
    assert_eq!(ftype_name(7), "Q8_0");
    assert_ne!(ggml_type_name(14), ftype_name(14));
    assert_eq!(ftype_name(14), "Q4_K_S");
    // ggml's own names, when this machine has the library to ask.
    if let Some(name) = ffi::type_name(7) {
        assert_eq!(name, "Q5_1");
        assert_eq!(ffi::type_name(14).as_deref(), Some("Q6_K"));
    }
    assert_eq!(ftype_name(41), "Q2_0");
    // The get_rows whitelist is keyed by ggml_type: BF16, Q8_0 and (since
    // b10089) the K-quants and IQ-quants are pinnable; Q8_K, the ternary
    // types and NVFP4 are the ones still without a kernel.
    assert!(gpu_has_get_rows(30, 0) && gpu_has_get_rows(8, 0));
    for quant in [10u32, 11, 12, 13, 14, 16, 17, 18, 19, 21, 22, 23, 29, 42] {
        assert!(gpu_has_get_rows(quant, 0), "type {quant} must be pinnable");
    }
    for no_kernel in [9u32, 15, 34, 35, 40] {
        assert!(
            !gpu_has_get_rows(no_kernel, 4096),
            "type {no_kernel} must not be pinnable"
        );
    }
}

/// The Memory row follows the tables generated from upstream's two switches,
/// and reads an unlisted arch as attention.
#[test]
fn memory_kind_follows_upstreams_recurrent_and_hybrid_lists() {
    assert!(!arch_tables::RECURRENT.is_empty() && !arch_tables::HYBRID.is_empty());
    for a in arch_tables::RECURRENT {
        assert_eq!(arch_memory(a), "recurrent", "{a}");
    }
    for a in arch_tables::HYBRID {
        assert_eq!(arch_memory(a), "hybrid (attention + recurrent)", "{a}");
    }
    assert_eq!(arch_memory("not-an-arch"), "attention");
    assert_eq!(arch_memory(""), "unknown");
}

/// `--split-mode tensor` is refused at load for the archs upstream's
/// `llm_arch_supports_sm_tensor` deny-lists (a throw in `llama_model_create`),
/// so the verdict is per-ARCH, spelled the way `LLM_ARCH_NAMES` spells it,
/// and silent for everything else.
#[test]
fn sm_tensor_verdict_follows_upstreams_arch_deny_list() {
    let info = |arch: &'static str| {
        ModelInfo::from_kv(&map(vec![("general.architecture", Tv::S(arch))])).unwrap()
    };
    assert!(info("not-an-arch").sm_tensor_warning().is_empty());
    assert!(!arch_tables::NO_SM_TENSOR.is_empty());
    for arch in arch_tables::NO_SM_TENSOR {
        assert!(info(arch).sm_tensor_warning().contains(arch), "{arch}");
    }
}

#[test]
fn moe_offload_line_pure_moe_covers_all_layers() {
    let m = map(vec![
        ("general.architecture", Tv::S("qwen3moe")),
        ("qwen3moe.block_count", Tv::U(48)),
        ("qwen3moe.expert_count", Tv::U(128)),
        ("qwen3moe.expert_used_count", Tv::U(8)),
    ]);
    let info = ModelInfo::from_kv(&m).unwrap();
    assert_eq!(info.kind_line(), "MoE: 128 experts, 8 active/tok");
    assert_eq!(info.moe_offload_line(), "all 48 layers hold experts");
}
