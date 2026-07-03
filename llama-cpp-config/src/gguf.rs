// Describe a model in the GUI's "Model info" box: dense-vs-MoE (+ layer split),
// layer count, trained context, GQA shape, quant, embedded MTP, plus the
// selected mmproj (clip) and draft/DFlash headers.
//
// Metadata is read with llama.cpp's OWN gguf reader — we runtime-load
// `ggml-base.dll` (shipped next to `llama-cpp-config.exe` in `bin\`) and call
// `gguf_init_from_file` with `no_alloc = true`, so only the header + tensor
// infos are read, never the multi-GB weights. No GGUF parsing is reimplemented
// here. If the DLL can't be loaded (e.g. a bare `cargo run` with no llama.cpp
// alongside), reads return `None` and the box shows "unavailable".
//
// Reads are synchronous and uncached — the header parse is fast enough to run on
// the UI thread when the model/mmproj/draft selection changes.
//
// Key names and the `general.file_type` enum mirror the bundled llama.cpp
// (`src/llama-arch.cpp`, `include/llama.h`, `tools/mtmd/clip-impl.h`); the GGUF
// value-type enum mirrors `ggml/include/gguf.h`.

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

fn token_overlap(a: &[String], b: &[String]) -> usize {
    a.iter().filter(|t| b.contains(t)).count()
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
    struct Map(HashMap<&'static str, Tv>);
    impl KvSource for Map {
        fn u32(&self, key: &str) -> Option<u32> {
            match self.0.get(key) {
                Some(Tv::U(n)) => Some(*n),
                _ => None,
            }
        }
        fn string(&self, key: &str) -> Option<String> {
            match self.0.get(key) {
                Some(Tv::S(s)) => Some((*s).to_string()),
                _ => None,
            }
        }
        fn boolean(&self, key: &str) -> Option<bool> {
            match self.0.get(key) {
                Some(Tv::B(b)) => Some(*b),
                _ => None,
            }
        }
    }

    fn map(pairs: Vec<(&'static str, Tv)>) -> Map {
        Map(pairs.into_iter().collect())
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
