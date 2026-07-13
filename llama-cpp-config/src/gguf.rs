//! Describe a model in the GUI's "Model info" box: dense-vs-MoE (+ layer split),
//! layer count, trained context, GQA shape, quant, embedded MTP, the embedded
//! chat template (Jinja vs non-Jinja detection, with a raw-text preview), plus
//! the selected mmproj (clip) and draft/DFlash headers.
//!
//! Metadata is read with llama.cpp's OWN gguf reader — we runtime-load
//! `ggml-base.dll` (shipped next to `llama-cpp-config.exe` in `bin\`) and call
//! `gguf_init_from_file` with `no_alloc = true`, so only the header + tensor
//! infos are read, never the multi-GB weights. No GGUF parsing is reimplemented
//! here. If the DLL can't be loaded (e.g. a bare `cargo run` with no llama.cpp
//! alongside), reads return `None` and the box shows "unavailable" — and the
//! load is retried on the next read (only success is memoized; see `ffi::api`),
//! so a DLL that appears later (a finished `02-build.ps1`) is picked up live.
//!
//! Reads are synchronous and uncached — the header parse is fast enough to run on
//! the UI thread when the model/mmproj/draft selection changes.
//!
//! Key names and the `general.file_type` enum mirror the bundled llama.cpp
//! (`src/llama-arch.cpp`, `include/llama.h`, `tools/mtmd/clip-impl.h`); the GGUF
//! value-type enum mirrors `ggml/include/gguf.h`.

use std::path::Path;

use crate::model_scan;

/// Separator between the parts of a Model-info box line ("arch · quant · size").
/// Single source so every line joins on the exact same glyph + spacing.
const SEP: &str = "  ·  ";

/// Read a handful of typed metadata values by key. Abstracts over the live gguf
/// context (via `ggml-base.dll`) so the field-extraction logic stays testable
/// without the DLL (see the `tests` module's map-backed source).
trait KvSource {
    fn u32(&self, key: &str) -> Option<u32>;
    fn string(&self, key: &str) -> Option<String>;
    fn boolean(&self, key: &str) -> Option<bool>;
    /// A tensor's `(ggml_type, size_in_bytes)` by name — from the GGUF's tensor
    /// infos, not its KV block. `None` when the tensor is absent.
    fn tensor(&self, name: &str) -> Option<(u32, u64)>;
}

// ── Public descriptions ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub arch: String,
    pub size_label: String,
    /// Human quant name (e.g. "Q4_K_M"), or empty if `general.file_type` absent.
    pub quant: String,
    pub is_moe: bool,
    pub expert_count: u32,
    pub expert_used: u32,
    pub expert_shared_count: u32,
    /// Leading dense (non-MoE) blocks, e.g. DeepSeek-style hybrids. 0 = all MoE.
    pub leading_dense_block_count: u32,
    /// Some archs place an MoE block every N layers instead of a dense prefix.
    pub moe_every_n_layers: u32,
    pub n_layer: u32,
    pub n_ctx_train: u32,
    pub head_count: u32,
    pub head_count_kv: u32,
    /// `<arch>.nextn_predict_layers`; > 0 means the model embeds MTP heads.
    pub nextn_predict_layers: u32,
    /// `<arch>.block_size` — DFlash drafters' trained diffusion block; the
    /// `--spec-draft-n-max` ceiling is `block_size - 1`. 0 if absent.
    pub block_size: u32,
    /// Raw `tokenizer.chat_template` metadata (None when the GGUF carries
    /// none). `chat_template_line()` classifies it for the info box; the GUI's
    /// Preview button shows this raw text in a modal.
    pub chat_template: Option<String>,
    /// `token_embd.weight`'s `(ggml_type, bytes)`. Both halves drive a decision
    /// the rest of the box can't: whether an `override-tensor` rule pinning the
    /// embedding table to a GPU is a win or a catastrophe — see `embd_line`.
    /// None when the GGUF names its embedding tensor something else.
    pub embd: Option<(u32, u64)>,
}

#[derive(Debug, Clone)]
pub struct MmprojInfo {
    pub projector_type: String,
    pub has_vision: bool,
    pub has_audio: bool,
    pub vision_block_count: u32,
    pub image_size: u32,
    pub patch_size: u32,
}

/// External speculators found in the framework's `mtps\` / `dflashs\` folders,
/// cross-referenced by filename against the selected model.
#[derive(Debug, Clone, Default)]
pub struct ExternalDraft {
    pub mtp_count: usize,
    pub dflash_count: usize,
    /// Best filename-overlap match, if any: (label, "MTP" | "DFlash").
    pub best: Option<(String, &'static str)>,
}

// ── Field extraction (platform-independent, testable) ────────────────────

impl ModelInfo {
    fn from_kv<S: KvSource>(s: &S) -> Option<ModelInfo> {
        let arch = s.string("general.architecture")?;
        let a = |suffix: &str| s.u32(&format!("{arch}.{suffix}"));

        let expert_count = a("expert_count").unwrap_or(0);
        let quant = s
            .u32("general.file_type")
            .map(ftype_name)
            .unwrap_or_default();

        Some(ModelInfo {
            size_label: s.string("general.size_label").unwrap_or_default(),
            quant,
            is_moe: expert_count > 0,
            expert_count,
            expert_used: a("expert_used_count").unwrap_or(0),
            expert_shared_count: a("expert_shared_count").unwrap_or(0),
            leading_dense_block_count: a("leading_dense_block_count").unwrap_or(0),
            moe_every_n_layers: a("moe_every_n_layers").unwrap_or(0),
            n_layer: a("block_count").unwrap_or(0),
            n_ctx_train: a("context_length").unwrap_or(0),
            head_count: a("attention.head_count").unwrap_or(0),
            head_count_kv: a("attention.head_count_kv").unwrap_or(0),
            nextn_predict_layers: a("nextn_predict_layers").unwrap_or(0),
            block_size: a("block_size").unwrap_or(0),
            chat_template: s
                .string("tokenizer.chat_template")
                .filter(|t| !t.is_empty()),
            embd: s.tensor("token_embd.weight"),
            arch,
        })
    }

    // ── Display strings for the GUI ──────────────────────────────────────

    pub fn kind_line(&self) -> String {
        if !self.is_moe {
            return "Dense".to_string();
        }
        let mut parts = vec![format!("{} experts", self.expert_count)];
        if self.expert_used > 0 {
            parts.push(format!("{} active/tok", self.expert_used));
        }
        if self.expert_shared_count > 0 {
            parts.push(format!("{} shared", self.expert_shared_count));
        }
        format!("MoE — {}", parts.join(", "))
    }

    /// How many transformer layers actually carry MoE expert weights — the count
    /// that sizes the `--n-cpu-moe` lever (which keeps the first N layers' experts
    /// on CPU, trading VRAM for speed). Empty for dense models, where the lever is
    /// a no-op.
    pub fn moe_offload_line(&self) -> String {
        if !self.is_moe || self.n_layer == 0 {
            return String::new();
        }
        let desc = if self.leading_dense_block_count > 0
            && self.leading_dense_block_count < self.n_layer
        {
            format!(
                "{} of {} layers hold experts (first {} dense)",
                self.n_layer - self.leading_dense_block_count,
                self.n_layer,
                self.leading_dense_block_count
            )
        } else if self.moe_every_n_layers > 1 {
            format!(
                "experts every {} layers of {}",
                self.moe_every_n_layers, self.n_layer
            )
        } else {
            format!("all {} layers hold experts", self.n_layer)
        };
        format!("{desc}{SEP}saves VRAM (slower)")
    }

    pub fn arch_quant_line(&self) -> String {
        let mut parts: Vec<&str> = vec![&self.arch];
        if !self.quant.is_empty() {
            parts.push(&self.quant);
        }
        if !self.size_label.is_empty() {
            parts.push(&self.size_label);
        }
        parts.join(SEP)
    }

    /// The "Embeddings" row: `token_embd.weight`'s own quant and size. Facts only
    /// — the VERDICT they imply is `embd_pin_warning`, which the Tensor-placement
    /// table raises at the moment a rule actually pins the table. Both halves of
    /// this row exist because that verdict has two independent preconditions:
    ///
    /// Pinning the embedding table is the documented cure for the GBs of Windows
    /// "Shared GPU memory" a fully-offloaded model still shows (llama.cpp parks
    /// `token_embd` in a pinned host buffer even at `offloaded N/N layers`). But
    /// the cure has TWO preconditions, and violating either turns a win into a
    /// 10-15x loss — both measured on a 27B split across ROCm + CUDA:
    ///
    /// 1. **The type must have a GPU `get_rows` kernel.** ggml-cuda/ggml-hip
    ///    implement `GET_ROWS` for F32/F16/BF16/I32/Q1_0/Q4_0/Q4_1/Q5_0/Q5_1/Q8_0
    ///    ONLY (`ggml-cuda.cu`, `supports_op`) — every K-quant falls to `default:
    ///    return false`. Pin a K-quant embedding table and the tensor sits in VRAM
    ///    the GPU cannot read, so the lookup round-trips to the host EVERY token:
    ///    decode collapsed 25 t/s → 2.8 t/s (Q4_K embd) and → 1.9 t/s (Q5_K, whose
    ///    table is bigger, hence slower — the penalty tracks the table size).
    ///    Prefill is untouched (one round-trip per 2048-token batch amortizes it),
    ///    which is why this reads as "the GPU is idle" rather than "the GPU is slow".
    ///    The trap is that a file's NAME does not tell you: Unsloth's Q6_K_XL keeps
    ///    `token_embd` at Q8_0 (safe), while its Q4_K_XL/Q5_K_XL use Q4_K/Q5_K (not).
    /// 2. **The card must have room for it.** Pinning MOVES the table out of host
    ///    RAM and into VRAM, so it is a straight add to the device's footprint —
    ///    hence the size here. With the KV cache already filling the card, that
    ///    add is what tips it into paging (23 t/s → 13.7 t/s, a Q8_0 table on a
    ///    card holding a 256k f16 KV cache). Size is the number to budget against.
    pub fn embd_line(&self) -> String {
        match self.embd {
            Some((ty, bytes)) => {
                format!("{}{SEP}{}", ggml_type_name(ty), format_mib(bytes))
            }
            None => "n/a".to_string(),
        }
    }

    /// The warning the Tensor-placement table shows when this model's embedding
    /// table is pinned to a GPU it cannot be read from. Empty when the type is
    /// safe — and equally empty when it is UNKNOWN (no `token_embd.weight`, or no
    /// `ggml-base.dll` to read it with): a metadata read we couldn't do is not
    /// evidence of a problem, and crying wolf on every unreadable GGUF would train
    /// the warning away. So `!embd_pinnable()` alone must never gate this.
    pub fn embd_pin_warning(&self) -> String {
        match self.embd {
            Some((ty, _)) if !gpu_has_get_rows(ty) => format!(
                "This model stores token_embd as {} — CUDA/ROCm have no get_rows kernel for \
                 K-quants, so pinning it to a GPU sends the whole table back to host RAM on \
                 EVERY token (measured: 25 → 2.8 tok/s). Point the rule at CPU, or use a build \
                 whose embedding table is Q8_0/F16/BF16.",
                ggml_type_name(ty)
            ),
            _ => String::new(),
        }
    }

    pub fn layers_ctx_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.n_layer > 0 {
            parts.push(format!("{} layers", self.n_layer));
        }
        if self.n_ctx_train > 0 {
            parts.push(format!(
                "trained ctx {}",
                group_thousands(self.n_ctx_train as u64)
            ));
        }
        if parts.is_empty() {
            "n/a".to_string()
        } else {
            parts.join(SEP)
        }
    }

    pub fn attn_line(&self) -> String {
        if self.head_count == 0 {
            return "n/a".to_string();
        }
        if self.head_count_kv == 0 || self.head_count_kv == self.head_count {
            return format!("{} heads (MHA)", self.head_count);
        }
        if self.head_count.is_multiple_of(self.head_count_kv) {
            format!(
                "{} heads / {} KV (GQA {}×)",
                self.head_count,
                self.head_count_kv,
                self.head_count / self.head_count_kv
            )
        } else {
            format!(
                "{} heads / {} KV (GQA)",
                self.head_count, self.head_count_kv
            )
        }
    }

    /// One-line status for the Model info "Chat template" row: `Jinja
    /// (embedded)` when the GGUF ships a `tokenizer.chat_template` with Jinja
    /// `{%`/`{{}` markers, `embedded (non-Jinja)` for a marker-less (older /
    /// builtin-name) template, or `none` when the metadata is absent. The
    /// `{%`/`{{}` heuristic mirrors llama.cpp's own `common_chat_verify_template`.
    pub fn chat_template_line(&self) -> String {
        match &self.chat_template {
            None => "none".to_string(),
            Some(t) => {
                if t.contains("{%") || t.contains("{{") {
                    "Jinja (embedded)".to_string()
                } else {
                    "embedded (non-Jinja)".to_string()
                }
            }
        }
    }

    /// One-line description of a *draft* file's own header (arch, quant, layers,
    /// DFlash block_size → implied `--spec-draft-n-max` ceiling, embedded nextn).
    pub fn draft_file_line(&self) -> String {
        let mut parts = vec![self.arch.clone()];
        if !self.quant.is_empty() {
            parts.push(self.quant.clone());
        }
        parts.push(if self.n_layer > 0 {
            format!("{} layers", self.n_layer)
        } else {
            "0 layers (head)".to_string()
        });
        if self.block_size > 0 {
            parts.push(format!(
                "block_size {} (n-max ≤ {})",
                self.block_size,
                self.block_size.saturating_sub(1)
            ));
        }
        if self.nextn_predict_layers > 0 {
            parts.push(format!("nextn {}", self.nextn_predict_layers));
        }
        parts.join(SEP)
    }
}

impl MmprojInfo {
    fn from_kv<S: KvSource>(s: &S) -> Option<MmprojInfo> {
        let projector_type = s.string("clip.projector_type").unwrap_or_default();
        let has_vision = s.boolean("clip.has_vision_encoder").unwrap_or(false);
        let has_audio = s.boolean("clip.has_audio_encoder").unwrap_or(false);
        // Not an mmproj/clip file if none of these are present.
        if projector_type.is_empty() && !has_vision && !has_audio {
            return None;
        }
        Some(MmprojInfo {
            projector_type,
            has_vision,
            has_audio,
            vision_block_count: s.u32("clip.vision.block_count").unwrap_or(0),
            image_size: s.u32("clip.vision.image_size").unwrap_or(0),
            patch_size: s.u32("clip.vision.patch_size").unwrap_or(0),
        })
    }

    pub fn mmproj_line(&self) -> String {
        let modality = match (self.has_vision, self.has_audio) {
            (true, true) => "vision + audio",
            (true, false) => "vision",
            (false, true) => "audio",
            (false, false) => "projector",
        };
        let mut parts: Vec<String> = Vec::new();
        if !self.projector_type.is_empty() {
            parts.push(self.projector_type.clone());
        }
        parts.push(modality.to_string());
        if self.vision_block_count > 0 {
            parts.push(format!("{} enc. layers", self.vision_block_count));
        }
        if self.image_size > 0 {
            if self.patch_size > 0 {
                parts.push(format!("{}px / patch {}", self.image_size, self.patch_size));
            } else {
                parts.push(format!("{}px", self.image_size));
            }
        }
        parts.join(SEP)
    }
}

/// One-line summary of embedded MTP + any matching external drafter, for the
/// "Draft" row of the info box.
pub fn draft_line(info: &ModelInfo, ext: &ExternalDraft) -> String {
    let embedded = if info.nextn_predict_layers > 0 {
        let n = info.nextn_predict_layers;
        format!(
            "embedded MTP: yes ({n} nextn layer{})",
            if n == 1 { "" } else { "s" }
        )
    } else {
        "embedded MTP: no".to_string()
    };

    let external = if let Some((label, kind)) = &ext.best {
        format!("external: {kind} looks compatible ({label})")
    } else {
        let total = ext.mtp_count + ext.dflash_count;
        if total > 0 {
            format!(
                "external: none matched ({} MTP, {} DFlash in library)",
                ext.mtp_count, ext.dflash_count
            )
        } else {
            "external: none in library".to_string()
        }
    };

    format!("{embedded}{SEP}{external}")
}

// ── Reading via ggml-base.dll ────────────────────────────────────────────

/// Read a model's GGUF metadata for the info box. `None` if the file can't be
/// opened (e.g. `ggml-base.dll` isn't loadable, or it isn't a valid GGUF).
pub fn read_model_info(path: &Path) -> Option<ModelInfo> {
    let ctx = ffi::open(path)?;
    ModelInfo::from_kv(&ctx)
}

/// Read an mmproj/clip GGUF header. `None` if the file isn't a clip mmproj.
pub fn read_mmproj_info(path: &Path) -> Option<MmprojInfo> {
    let ctx = ffi::open(path)?;
    MmprojInfo::from_kv(&ctx)
}

// ── External drafter cross-reference ─────────────────────────────────────

/// Scan `mtps\` and `dflashs\` under `models_dir` and pick the drafter whose
/// filename shares the most "family" tokens with the model (a loose heuristic —
/// requires ≥2 shared distinctive tokens to claim a match).
pub fn external_drafters(models_dir: &str, model_path: &str) -> ExternalDraft {
    let model_stem = Path::new(model_path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let model_tokens = family_tokens(&model_stem);

    let mtp = model_scan::list(models_dir, model_scan::Category::Mtp.subdir());
    let dflash = model_scan::list(models_dir, model_scan::Category::Dflash.subdir());

    let mut out = ExternalDraft {
        mtp_count: mtp.len(),
        dflash_count: dflash.len(),
        best: None,
    };

    let mut best_score = 1usize; // require strictly > 1 shared tokens
    for (opt, kind) in mtp
        .iter()
        .map(|o| (o, "MTP"))
        .chain(dflash.iter().map(|o| (o, "DFlash")))
    {
        let score = token_overlap(&model_tokens, &family_tokens(&opt.label));
        if score > best_score {
            best_score = score;
            out.best = Some((opt.label.clone(), kind));
        }
    }
    out
}

/// Lowercased, alphanumeric "family" tokens of a filename, dropping quant/format
/// noise and pure-digit shard counters so matching keys on model identity
/// (e.g. `qwen3`, `30b`, `a3b`) rather than quant strings.
fn family_tokens(name: &str) -> Vec<String> {
    const NOISE: &[&str] = &[
        "gguf", "mtp", "dflash", "draft", "of", "the", "model", "instruct", "chat", "it", "base",
        "hf", "gs", "mostly", "f16", "f32", "bf16", "fp16", "fp32", "mxfp4", "nvfp4", "iq1", "iq2",
        "iq3", "iq4", "tq1", "tq2", "xxs", "xs", "km", "ks", "moe",
    ];
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 2)
        .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
        .filter(|t| !is_quant_token(t))
        .filter(|t| !NOISE.contains(t))
        .map(|t| t.to_string())
        .collect()
}

/// Match bare quant tokens like `q4`, `q5`, `q8`, and the lone `k`/`m`/`s`
/// suffixes that `Q4_K_M`-style names split into.
fn is_quant_token(t: &str) -> bool {
    if matches!(t, "k" | "m" | "s") {
        return true;
    }
    let b = t.as_bytes();
    b.len() == 2 && b[0] == b'q' && b[1].is_ascii_digit()
}

/// Number of DISTINCT tokens the two lists share. `family_tokens` doesn't
/// dedup, and HF-style names often repeat the family (e.g. a label carrying
/// the repo name twice) — counting duplicates would let a single shared token
/// clear the "> 1 shared tokens" match threshold.
fn token_overlap(a: &[String], b: &[String]) -> usize {
    let bs: std::collections::HashSet<&str> = b.iter().map(String::as_str).collect();
    a.iter()
        .map(String::as_str)
        .filter(|t| bs.contains(t))
        .collect::<std::collections::HashSet<&str>>()
        .len()
}

// ── Small helpers ────────────────────────────────────────────────────────

/// `262144` -> `"262,144"`. Locale-free thousands grouping for readability.
fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

/// `1_349_517_312` -> `"1.26 GiB"` / `715_128_832` -> `"682 MiB"`. Sizes the VRAM
/// an `override-tensor` pin would add to the target device.
fn format_mib(bytes: u64) -> String {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    if mib >= 1024.0 {
        format!("{:.2} GiB", mib / 1024.0)
    } else {
        format!("{mib:.0} MiB")
    }
}

/// Map a `ggml_type` (ggml/include/ggml.h) to its name. This is NOT the
/// `general.file_type` LLAMA_FTYPE enum that `ftype_name` maps — the two disagree
/// on nearly every value (e.g. 7 is Q8_0 as an ftype but Q5_1 as a ggml_type), so
/// crossing them would silently mislabel every tensor.
fn ggml_type_name(t: u32) -> String {
    let name = match t {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        6 => "Q5_0",
        7 => "Q5_1",
        8 => "Q8_0",
        9 => "Q8_1",
        10 => "Q2_K",
        11 => "Q3_K",
        12 => "Q4_K",
        13 => "Q5_K",
        14 => "Q6_K",
        15 => "Q8_K",
        16 => "IQ2_XXS",
        17 => "IQ2_XS",
        18 => "IQ3_XXS",
        19 => "IQ1_S",
        20 => "IQ4_NL",
        21 => "IQ3_S",
        22 => "IQ2_S",
        23 => "IQ4_XS",
        24 => "I8",
        25 => "I16",
        26 => "I32",
        27 => "I64",
        28 => "F64",
        29 => "IQ1_M",
        30 => "BF16",
        34 => "TQ1_0",
        35 => "TQ2_0",
        39 => "MXFP4",
        40 => "NVFP4",
        41 => "Q1_0",
        42 => "Q2_0",
        _ => return format!("ggml_type {t}"),
    };
    name.to_string()
}

/// Whether ggml-cuda / ggml-hip can run `GET_ROWS` (the embedding lookup) on a
/// tensor of this `ggml_type`. Mirrors the `case GGML_OP_GET_ROWS` arm of
/// `ggml_backend_cuda_device_supports_op` (`ggml/src/ggml-cuda/ggml-cuda.cu`),
/// which whitelists exactly these and returns false for everything else — every
/// K-quant and every IQ-quant included.
///
/// Keep this list in sync with that switch when bumping llama.cpp: a type that
/// GAINS a kernel upstream and is missing here only costs a needless warning, but
/// a type that is listed here WITHOUT a kernel silently green-lights the pin that
/// `embd_line` exists to prevent.
fn gpu_has_get_rows(ggml_type: u32) -> bool {
    matches!(
        ggml_type,
        0  // F32
        | 1  // F16
        | 2  // Q4_0
        | 3  // Q4_1
        | 6  // Q5_0
        | 7  // Q5_1
        | 8  // Q8_0
        | 26 // I32
        | 30 // BF16
        | 41 // Q1_0
    )
}

/// Map `general.file_type` (LLAMA_FTYPE) to its quant name. Falls back to
/// `"ftype N"` for values this build predates.
fn ftype_name(ft: u32) -> String {
    let name = match ft {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_XXS",
        24 => "IQ1_S",
        25 => "IQ4_NL",
        26 => "IQ3_S",
        27 => "IQ3_M",
        28 => "IQ2_S",
        29 => "IQ2_M",
        30 => "IQ4_XS",
        31 => "IQ1_M",
        32 => "BF16",
        36 => "TQ1_0",
        37 => "TQ2_0",
        38 => "MXFP4_MOE",
        39 => "NVFP4",
        40 => "Q1_0",
        _ => return format!("ftype {ft}"),
    };
    name.to_string()
}

// ── ggml-base.dll glue ───────────────────────────────────────────────────
// The GGUF reader (runtime-loaded `ggml-base.dll` on Windows; a `None` stub
// elsewhere) is split into src/gguf/ffi.rs so this module reads as pure
// metadata logic. Public surface: `ffi::open(path) -> Option<Ctx: KvSource>`.
mod ffi;

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A map-backed `KvSource` so field extraction is tested without the DLL.
    enum Tv {
        U(u32),
        S(&'static str),
        B(bool),
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
        fn tensor(&self, name: &str) -> Option<(u32, u64)> {
            self.tensors.get(name).copied()
        }
    }

    fn map(pairs: Vec<(&'static str, Tv)>) -> Map {
        Map {
            kv: pairs.into_iter().collect(),
            tensors: HashMap::new(),
        }
    }

    /// `map` plus the GGUF's tensor infos — only `token_embd.weight` is read.
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
        assert_eq!(
            info.kind_line(),
            "MoE — 160 experts, 6 active/tok, 2 shared"
        );
        assert_eq!(
            info.moe_offload_line(),
            "45 of 48 layers hold experts (first 3 dense)  ·  saves VRAM (slower)"
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
    fn family_tokens_drop_quant_noise() {
        let t = family_tokens("Qwen3-30B-A3B-Q4_K_M.gguf");
        assert!(t.contains(&"qwen3".to_string()));
        assert!(t.contains(&"30b".to_string()));
        assert!(t.contains(&"a3b".to_string()));
        assert!(!t
            .iter()
            .any(|x| x == "q4" || x == "k" || x == "m" || x == "gguf"));
    }

    // The "> 1 shared tokens" match threshold means DISTINCT tokens: a name
    // repeating its family token (HF repo + file name both carrying it) must
    // not clear the bar with a single real match.
    #[test]
    fn token_overlap_counts_distinct_tokens_only() {
        let model = family_tokens("GLM-4.5-Air-GLM-4.5-Air-Q4_K_M.gguf");
        let drafter = family_tokens("glm-dflash.gguf");
        assert_eq!(token_overlap(&model, &drafter), 1);
        let real = family_tokens("GLM-4.5-Air-DFlash.gguf");
        assert!(token_overlap(&model, &real) > 1);
    }

    #[test]
    fn draft_line_reports_embedded_and_external() {
        let m = map(vec![
            ("general.architecture", Tv::S("glm4moe")),
            ("glm4moe.expert_count", Tv::U(128)),
            ("glm4moe.nextn_predict_layers", Tv::U(1)),
        ]);
        let info = ModelInfo::from_kv(&m).unwrap();
        let ext = ExternalDraft {
            mtp_count: 0,
            dflash_count: 2,
            best: Some(("glm-dflash.gguf".into(), "DFlash")),
        };
        let line = draft_line(&info, &ext);
        assert!(line.contains("embedded MTP: yes (1 nextn layer)"));
        assert!(line.contains("DFlash looks compatible (glm-dflash.gguf)"));
    }

    /// The whole point of the Embeddings row: the FILE NAME does not tell you
    /// whether pinning `token_embd` is safe. These are the real headers of three
    /// Unsloth "XL" builds of the SAME model — the Q6_K_XL keeps its embedding
    /// table at Q8_0 (a type ggml-cuda/hip can `get_rows` on GPU), while the
    /// Q5/Q4 builds drop it to a K-quant, which has no GPU kernel. Pinning those
    /// two round-trips the table to the host every token: measured 2.8 t/s (Q4_K)
    /// and 1.9 t/s (Q5_K) against 21-52 t/s unpinned.
    #[test]
    fn embd_verdict_follows_the_tensor_type_not_the_file_name() {
        let base = || vec![("general.architecture", Tv::S("qwen35"))];

        // Qwen3.6-27B-UD-MTP-Q6_K_XL.gguf — embd is Q8_0: whitelisted.
        let info = ModelInfo::from_kv(&map_t(
            base(),
            vec![("token_embd.weight", (8, 1_351_614_464))],
        ))
        .unwrap();
        assert_eq!(info.embd_line(), "Q8_0  ·  1.26 GiB");
        assert!(info.embd_pin_warning().is_empty());

        // Qwen3.6-27B-UD-Q4_K_XL.gguf — embd is Q4_K: no GPU get_rows. The ROW is
        // unchanged in shape (facts only); the VERDICT lives in the warning.
        let info = ModelInfo::from_kv(&map_t(
            base(),
            vec![("token_embd.weight", (12, 715_128_832))],
        ))
        .unwrap();
        assert_eq!(info.embd_line(), "Q4_K  ·  682 MiB");
        assert!(info.embd_pin_warning().contains("Q4_K"));

        // Q5_K (13) is a K-quant too — the trap is not specific to Q4.
        let info = ModelInfo::from_kv(&map_t(
            base(),
            vec![("token_embd.weight", (13, 873_463_808))],
        ))
        .unwrap();
        assert_eq!(info.embd_line(), "Q5_K  ·  833 MiB");
        assert!(info.embd_pin_warning().contains("Q5_K"));

        // Unknown type (no `token_embd.weight`, or no DLL to read it with) must
        // NOT warn: a metadata read we couldn't do is not evidence of a problem.
        // This is also what keeps the e2e fixture's unreadable GGUF quiet.
        let info = ModelInfo::from_kv(&map(base())).unwrap();
        assert_eq!(info.embd_line(), "n/a");
        assert!(info.embd_pin_warning().is_empty());
    }

    /// `ggml_type` and `general.file_type` (LLAMA_FTYPE) are different enums that
    /// overlap numerically — 7 means Q8_0 as an ftype but Q5_1 as a ggml_type, and
    /// 14 means Q4_K_S vs Q6_K. Reading a tensor's type through `ftype_name` (or
    /// vice versa) would mislabel almost everything, so pin the divergence.
    #[test]
    fn ggml_type_and_ftype_enums_are_not_interchangeable() {
        assert_eq!(ggml_type_name(7), "Q5_1");
        assert_eq!(ftype_name(7), "Q8_0");
        assert_eq!(ggml_type_name(14), "Q6_K");
        assert_eq!(ftype_name(14), "Q4_K_S");
        // BF16 and Q8_0 are get_rows-capable; every K-quant and IQ-quant is not.
        assert!(gpu_has_get_rows(30) && gpu_has_get_rows(8));
        for k_quant in [10u32, 11, 12, 13, 14, 15, 23] {
            assert!(!gpu_has_get_rows(k_quant), "type {k_quant} must not be pinnable");
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
        assert_eq!(info.kind_line(), "MoE — 128 experts, 8 active/tok");
        assert_eq!(
            info.moe_offload_line(),
            "all 48 layers hold experts  ·  saves VRAM (slower)"
        );
    }
}
