//! Will this quantization run on THIS machine, and how: the placement
//! estimate the From Hugging Face dialog and `preset download --list` show
//! per quantization, the one quantization they recommend, and the preset keys
//! that placement becomes once the file is downloaded (`placement`).
//!
//! ## What is read, and how
//!
//! Before anything is downloaded, the first bytes of each candidate's GGUF
//! (every shard of a split one) are fetched with an HTTP range request
//! (`hf::Fetch::get_range`, the same stdin-configured curl as the listing, so
//! the token never reaches a command line) and parsed by `parse_header`, a
//! pure-Rust reader of the GGUF header: the key/values, then the tensor
//! infos (name, dims, type, offset). A tensor's SIZE is the gap to the next
//! tensor's offset (the last one's: the shard's size from the listing minus
//! its data start), so no table of ggml type sizes is mirrored and a new
//! quantization type needs no update here. `shape_of` folds the headers into
//! a `Shape`: per block, the bytes of the routed-expert tensors (the ones
//! llama.cpp's `--n-cpu-moe` moves: `blk.N.ffn_{up,down,gate,gate_up}_{,ch}exps`,
//! `LLM_FFN_EXPS_REGEX`) and of everything else; the input embedding tables
//! (`token_embd`, and `per_layer_token_embd` where a model has one: both are
//! `LLM_TENSOR_LAYER_INPUT` tensors, which stay in host memory even with
//! every layer offloaded; Qwen3.8-Flash-Next's is 27 GiB); the rest (the
//! output head and norms); and the KV cache. `local_shape` reads the same
//! headers from the downloaded files, for the preset.
//!
//! Why not the `gguf.rs` reader (ggml-base through FFI): it opens a FILE, so
//! the range would have to be written to a temporary file first; it is absent
//! from a bare `cargo test` and from a CLI run without the DLL beside it; and
//! a range that ends inside the header must read as "fetch more", not as a
//! failure. The header grammar is small and stable (v2 and v3), so the
//! mirror is cheaper than the plumbing.
//!
//! How much is fetched: the header holds the tokenizer (150k tokens and as
//! many merges for Qwen3: about 6 MiB), so the first request asks for
//! `INITIAL_FETCH`, or the largest header seen so far in this process plus a
//! margin (`HINT`), and a header still cut short asks again for four times
//! as much, up to `MAX_HEADER`. A shape is cached per (repo, commit, path) for
//! the life of the process (`CACHE`), so a quantization is fetched once,
//! however often its repo is listed. `fetch_shapes` runs the candidates on
//! `WORKERS` threads and hands each result over as it lands, which is what
//! lets the dialog fill its rows one by one without blocking the listing.
//!
//! ## The machine
//!
//! The GPUs are the device probe's (`devices::probed()` in the GUI,
//! `devices::list()` in the CLI), reduced by `gpu_pool` to the PHYSICAL
//! discrete cards: one card shows once per backend (CUDA0 and Vulkan1 are the
//! same RTX 4070 SUPER), so devices are grouped by name and counted as the
//! most any one backend sees; an integrated GPU (`is_integrated`: its
//! "memory" is system RAM) is left out whenever a discrete card is present.
//! Each card keeps the `--device` id it will be written as: the non-Vulkan
//! backend's when one sees it (CUDA on the RTX 4070 SUPER, ROCm on the R9700:
//! Vulkan is slower on both, `docs/amd_ai_pro_r9700.md` and
//! `docs/nvidia_rtx4070_super.md` "pin the CUDA id"), with that backend's
//! size. On Windows a Ryzen iGPU takes `ROCm0`, so the R9700 is `ROCm1`: the
//! ids come from the probe, never from a rule.
//!
//! Several cards are modelled ONE BY ONE, never as one pool: `Budget::gpus`
//! is in SPLIT ORDER (`split_order`), the order the guides measured best for
//! a CUDA + ROCm pair: on Windows the ROCm card first (`docs/qwen3.8_27b.md`
//! section 4: +9% prefill, +21% decode; `docs/qwen3.8_flash_next.md` section
//! 10 runs that order too), on Linux the CUDA card first
//! (`docs/qwen3.8_flash_next.md` section 2: +32% prefill, +13% decode; the
//! 27B puts the R9700 last there). Cards of one backend, or of a backend
//! the guides never measured, keep the probe's order. The basis line names
//! them in that order. No probe yet, or no GPU: every row says the estimate
//! is unavailable. System RAM is `hwmon::system_ram()` (one OS call, no new
//! dependency).
//!
//! ## The margin
//!
//! Per GPU, `GPU_RESERVE` (2 GiB) is kept free: llama.cpp's own `--fit`
//! leaves 1 GiB per device (`--fit-target` default, which a preset left to
//! the fit runs with), and the compute buffers plus the runtime's pools take
//! another 0.8 to 1.4 GiB at the default batch sizes (measured on the R9700
//! and the RTX 4070 SUPER, `docs/qwen3.8_flash_next.md`; the R9700 also wants
//! ~500 MB left after a real prefill). On top, the KV cache of a
//! `REF_CTX` (32k) context, or the model's own context when shorter, at f16,
//! from the header: `head_count_kv x (key_length + value_length) x 2 bytes`
//! per attention layer and token (a per-layer `head_count_kv` array is summed,
//! so recurrent layers count zero; `full_attention_interval` thins a hybrid;
//! an MLA model counts `kv_lora_rank + rope dims`). Sliding-window layers are
//! counted at full length, which over-reserves on Gemma-style models; an
//! unknown layout reserves `FALLBACK_KV`. RAM keeps `RAM_RESERVE` (4 GiB) for
//! the system and the desktop.
//!
//! ## The estimate (`estimate_with`): per device, as llama.cpp cuts the layers
//!
//! llama.cpp's layer split hands every device a CONSECUTIVE run of
//! `n_layer + 1` positions (each block, then the output layer), in `--device`
//! order (`gpu_split.rs` header). So the model is a row of position weights,
//! each block's non-expert bytes plus its share of the KV cache plus its
//! experts unless `--n-cpu-moe` keeps them in RAM, and the output head last;
//! the input embeddings never count (host memory). A layout is feasible when
//! the row can be cut into one run per card, in split order, each run within
//! that card's own usable memory (`pack`: each card takes as many of the next
//! positions as fit, which finds a cut whenever one exists). That is what one
//! added-up pool could not say: a block never straddles two cards, so two
//! cards hold less than their sum, and the small card's share is bounded by
//! the small card.
//!
//! 1. Everything fits: `Fit::Gpu`. On several cards, the one with the most
//!    usable memory alone when it holds the whole model (no cross-card hop on
//!    every token, and the other cards stay free), else every card.
//! 2. A mixture of experts that does not: the smallest `N` for which the row
//!    fits with the experts of blocks `0..N` in RAM, which is what
//!    `--n-cpu-moe N` does: the attention and shared weights of EVERY block,
//!    plus the experts of the last blocks, on the GPUs (`Fit::TooBig` when
//!    even every expert in RAM does not fit). The rest, plus the embeddings,
//!    is in system RAM: `Fit::Moe`.
//! 3. A dense model that does not fit: the most positions, counted from the
//!    output layer back, that fit, which is what `--n-gpu-layers` offloads
//!    (it takes positions off the FRONT), the rest in RAM: `Fit::Dense`
//!    (slow: every token then runs those layers on the CPU).
//!
//! The cut itself (`balanced`) is the feasible one that fills every card to
//! the same SHARE of its usable memory (the smallest common share for which
//! `pack` still fits), so the slack a fitting model leaves is spread in
//! proportion instead of all landing on the last card. It is recorded per
//! card (`Estimate::shares`: positions and bytes), and its position counts
//! ARE the `--tensor-split` vector: a vector summing to the positions assigns
//! exactly itself (`gpu_split::block_counts`).
//!
//! Whatever lands in RAM is compared with the RAM budget: a shortfall means
//! the remainder pages from disk through mmap on every token (`ram_short`).
//!
//! The KV cache is spread evenly over the blocks, which over-reserves on the
//! recurrent blocks of a hybrid and under-reserves on its attention blocks.
//!
//! ## The extras (`estimate_with`): the image encoder and a separate drafter
//!
//! The files a download brings beside the model (`Extras`, from the CURRENT
//! choices: the mmproj tick and the drafter pill in the dialog, `--no-mmproj`
//! and `--draft` in the CLI; `extras_of`) are counted on the card they land
//! on, before the model's cut, so a card hosting one leaves the model that
//! much less (the split shifts away from it, and a model that only fitted
//! without them spills or loses its recommendation):
//!
//! - The image encoder: its file size (the listing's) plus `MMPROJ_COMPUTE`
//!   (512 MiB) for encoding an image. Measured for Qwen3.8's encoder
//!   (`qwen3vl_merger`, 888 MiB of weights): 248 MiB of compute buffer at
//!   llama.cpp's warmup (1472x1472) and 418 MiB at a 3840x2160 image
//!   (`docs/nvidia_rtx4070_super.md`); its header is not read, since the
//!   compute buffer depends on the projector and the image, not on anything
//!   the header says. llama.cpp puts it on the FIRST `--device` entry
//!   (`common/arg.cpp`: `devices.front()` when no `--mmproj-device` is set),
//!   so the estimate tries that card first. When it does not fit there, it
//!   is MOVED rather than the model shrunk: to another card
//!   (`mmproj-device`, no cost per token), and only then to the CPU
//!   (`mmproj-offload = false`, seconds per image). The order for a model
//!   that fits on the GPUs: alone on the largest card with the encoder
//!   beside it, alone with the encoder on another card, split with the
//!   encoder on the first card, split with it on another card, then the same
//!   with the encoder on the CPU (a dense model only). An MoE that does not
//!   fit moves the experts of one more block to RAM before the encoder (one
//!   block of experts costs a few percent per token, the CPU encoder seconds
//!   per image): the encoder takes the card where `--n-cpu-moe` comes out
//!   smallest, the CPU only when no card takes it. A dense spill takes
//!   whichever spot leaves the most layers on the GPUs, a card on a tie.
//! - A separate drafter (an MTP, DFlash or DSpark file): its tensors but the
//!   input embeddings (`draft_shape_of`, its header read remotely like the
//!   model's: `fetch_draft_shape`, cached per file; unreadable, its file size
//!   alone), its KV cache at f16 for the MODEL's context (the draft context
//!   holds as many tokens as the target's, `common/speculative.cpp`), where a
//!   sliding-window layer holds its window plus a ubatch (2560 cells for a
//!   DFlash drafter, whose every layer is windowed: 50 MiB at f16, as
//!   measured), plus `DRAFT_COMPUTE` (768 MiB: the draft context's compute
//!   buffers, 218 to 243 MiB on the drafter's card and 536 MiB on the card
//!   with the target's output head, measured for a DFlash Q8_0 drafter of
//!   Qwen3.8-27B; all of it is counted on the drafter's card, which on a
//!   split is the output head's anyway). It goes with a model alone on its
//!   card (or on another card when that keeps the model alone), and on the
//!   LAST card of a split. Always a GPU: a drafter on the CPU is slower
//!   than no drafter.
//! - An MTP head EMBEDDED in the model file is part of the model's blocks
//!   (`shape_of` reads every tensor), so it is counted there and never
//!   again: only a separate file is an extra.
//!
//! Each counted extra shows in the row (`extras_label`) and in the note.
//!
//! ## The placement (`placement`): what the preset gets
//!
//! The estimate of the downloaded quantization becomes preset keys, the ones
//! the Models tab already edits (`gpu_split.rs` writes the same two strings):
//!
//! | estimate | written |
//! |---|---|
//! | no GPU, `TooBig`, a dense model with no block on the GPU | nothing (`fit` keeps today's defaults) |
//! | one card in the machine, all on GPU | nothing: the fit sizes the context to it |
//! | several cards, all on the largest one | `device` = that card; `fit` stays on and sizes the context to it |
//! | several cards, all on GPU across them | `device` + `tensor-split` + `ctx-size` |
//! | MoE spill | `n-cpu-moe` + `ctx-size` (+ `device` / `tensor-split` with several cards) |
//! | dense spill | `n-gpu-layers` + `ctx-size` (+ `device` / `tensor-split` with several cards) |
//!
//! `ctx-size` is the context the estimate reserved the KV cache for
//! (`Shape::ctx`: `REF_CTX`, or the model's own when shorter), since the cut
//! was sized for it. Writing it turns `fit` off (`form::fit_allowed`), which
//! is also what the placement needs: `common/fit.cpp` shrinks an unset
//! context against the TOTAL free memory before giving up on a
//! `tensor-split` or a tensor override set by hand, so with the context left
//! to it a card could be over-filled by a context sized for the sum.
//! `n-gpu-layers` set by hand stops it as well. A shortfall of RAM changes
//! nothing written: the GPU part is still the best there is, the RAM part
//! pages through mmap (the default load mode, which `none` would turn into a
//! failed allocation), and the note says so. A separate drafter is pinned
//! (`device-draft`) to the card it was counted on whenever the device list
//! would not put it there: on a multi-card split always (an empty
//! `device-draft` inherits the whole `device` list, and llama.cpp's
//! multi-device auto split crashes separate MTP heads, AGENTS.md "Draft
//! placement needs a draft FILE"; the last card is where the output head
//! and the sampling already are), and for a model alone on one card with
//! the drafter on another. The image encoder gets `mmproj-device` when it
//! was counted on a card other than the first device, `mmproj-offload =
//! false` when on the CPU. Every written placement carries a one-line note,
//! the section's `; placement =` comment (`presets::Preset::placement_note`),
//! which names the extras counted and where.
//!
//! ## The recommendation (`recommend`)
//!
//! Only a quantization that runs WELL is recommended, with the extras
//! chosen counted: tier 1, all on GPU, the extras too; tier 2, all on GPU
//! with the image encoder moved to the CPU; else tier 3, an MoE whose
//! attention and shared weights fit the GPU and whose experts fit in RAM
//! without paging. Within the best tier present, the
//! best quality (`quality`): nominal bits first, but 8 at most (Q8_0 is close
//! to lossless and BF16 costs twice the bandwidth for nothing a chat can see,
//! so a 16-bit file is picked only when it is the only one in the tier); at
//! equal bits a K-quant over an i-quant over a legacy one (an i-quant wins
//! only where it is the one that fits), then the larger variant (`_XL`, `_L`,
//! `_M`, `_S`, `_XS`, `_XXS` in that order), then the larger file. A dense model that
//! fits nowhere, an MoE that pages, or a tie of unknowns: no recommendation,
//! and the rows say why each one falls short.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::devices::DeviceOption;
use crate::hf::{self, Candidate, Fetch, HfError, Token};

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;

/// Kept free on every GPU: llama.cpp's `--fit-target` plus compute buffers.
pub const GPU_RESERVE: u64 = 2 * GIB;
/// Kept free in system RAM for the OS and the desktop.
pub const RAM_RESERVE: u64 = 4 * GIB;
/// The context the KV cache is reserved for (the model's own when shorter).
pub const REF_CTX: u64 = 32 * 1024;
/// The KV reserve when the header does not tell the attention layout.
const FALLBACK_KV: u64 = 2 * GIB;
/// The first range request of a header.
const INITIAL_FETCH: u64 = 8 * MIB;
/// Beyond this a header is refused: no real one comes close.
const MAX_HEADER: u64 = 128 * MIB;
/// Added to the largest header seen so far, for the next first request.
const HINT_MARGIN: u64 = 256 * 1024;
/// Header fetches in flight at once.
pub const WORKERS: usize = 4;

// ── GGUF header ──────────────────────────────────────────────────────────

/// One metadata value. Arrays of strings (the tokenizer) and long numeric
/// arrays are skipped, not kept: nothing here reads them.
#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Uint(u64),
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Arr(Vec<Kv>),
    Skipped,
}

impl Kv {
    fn as_u64(&self) -> Option<u64> {
        match self {
            Kv::Uint(v) => Some(*v),
            Kv::Int(v) => u64::try_from(*v).ok(),
            _ => None,
        }
    }
}

/// A parsed header: the key/values, each tensor's name and offset (relative
/// to `data_start`), and where the tensor data begins.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub kvs: HashMap<String, Kv>,
    pub tensors: Vec<(String, u64)>,
    pub data_start: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// The buffer ended inside the header: fetch more.
    Truncated,
    /// Not a GGUF header this reader takes.
    Invalid(String),
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: u64) -> Result<&'a [u8], ParseError> {
        let n = usize::try_from(n).map_err(|_| ParseError::Invalid("a length beyond memory".into()))?;
        let end = self.pos.checked_add(n).ok_or(ParseError::Truncated)?;
        if end > self.buf.len() {
            return Err(ParseError::Truncated);
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn skip(&mut self, n: u64) -> Result<(), ParseError> {
        self.take(n).map(|_| ())
    }

    fn u32(&mut self) -> Result<u32, ParseError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, ParseError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// A length-prefixed string; `MAX_STRING` bounds what a corrupt length
    /// can make this ask for.
    fn string_len(&mut self) -> Result<u64, ParseError> {
        const MAX_STRING: u64 = 64 * MIB;
        let n = self.u64()?;
        if n > MAX_STRING {
            return Err(ParseError::Invalid(format!("a string of {n} bytes")));
        }
        Ok(n)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let n = self.string_len()?;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    /// One value of type `t`.
    fn value(&mut self, t: u32, depth: u32) -> Result<Kv, ParseError> {
        Ok(match t {
            0 => Kv::Uint(u64::from(self.take(1)?[0])),
            1 => Kv::Int(i64::from(self.take(1)?[0] as i8)),
            2 => Kv::Uint(u64::from(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))),
            3 => Kv::Int(i64::from(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))),
            4 => Kv::Uint(u64::from(self.u32()?)),
            5 => Kv::Int(i64::from(self.u32()? as i32)),
            6 => Kv::Float(f64::from(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))),
            7 => Kv::Bool(self.take(1)?[0] != 0),
            8 => Kv::Str(self.string()?),
            9 => {
                if depth > 2 {
                    return Err(ParseError::Invalid("arrays nested too deep".into()));
                }
                let et = self.u32()?;
                let n = self.u64()?;
                if n > 1 << 28 {
                    return Err(ParseError::Invalid(format!("an array of {n} values")));
                }
                match (et, fixed_size(et)) {
                    // Small numeric arrays (per-layer head counts) are kept.
                    (_, Some(_)) if n <= 4096 => Kv::Arr((0..n).map(|_| self.value(et, depth + 1)).collect::<Result<_, _>>()?),
                    (_, Some(size)) => {
                        self.skip(n * size)?;
                        Kv::Skipped
                    }
                    (8, None) => {
                        for _ in 0..n {
                            let len = self.string_len()?;
                            self.skip(len)?;
                        }
                        Kv::Skipped
                    }
                    (9, None) => {
                        for _ in 0..n {
                            self.value(9, depth + 1)?;
                        }
                        Kv::Skipped
                    }
                    _ => return Err(ParseError::Invalid(format!("array element type {et}"))),
                }
            }
            10 => Kv::Uint(self.u64()?),
            11 => Kv::Int(self.u64()? as i64),
            12 => Kv::Float(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            _ => return Err(ParseError::Invalid(format!("value type {t}"))),
        })
    }
}

/// The byte size of a fixed-size GGUF value type.
fn fixed_size(t: u32) -> Option<u64> {
    match t {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    }
}

/// Parse a GGUF header (v2 or v3, little-endian) from the first bytes of a
/// file (pure). `Truncated` when `buf` ends before the tensor infos do.
pub fn parse_header(buf: &[u8]) -> Result<Header, ParseError> {
    let mut r = Reader { buf, pos: 0 };
    if r.take(4)? != b"GGUF" {
        return Err(ParseError::Invalid("no GGUF magic".into()));
    }
    let version = r.u32()?;
    if !matches!(version, 2 | 3) {
        return Err(ParseError::Invalid(format!("GGUF version {version}")));
    }
    let n_tensors = r.u64()?;
    let n_kv = r.u64()?;
    if n_tensors > 1 << 20 || n_kv > 1 << 20 {
        return Err(ParseError::Invalid(format!("{n_tensors} tensors, {n_kv} keys")));
    }
    let mut kvs = HashMap::new();
    for _ in 0..n_kv {
        let key = r.string()?;
        let t = r.u32()?;
        let v = r.value(t, 0)?;
        kvs.insert(key, v);
    }
    let mut tensors = Vec::with_capacity(n_tensors as usize);
    for _ in 0..n_tensors {
        let name = r.string()?;
        let n_dims = r.u32()?;
        if n_dims > 8 {
            return Err(ParseError::Invalid(format!("{name}: {n_dims} dimensions")));
        }
        r.skip(u64::from(n_dims) * 8)?;
        let _type = r.u32()?;
        let offset = r.u64()?;
        tensors.push((name, offset));
    }
    let align = kvs.get("general.alignment").and_then(Kv::as_u64).filter(|&a| a > 0).unwrap_or(32);
    let data_start = (r.pos as u64).div_ceil(align) * align;
    Ok(Header { kvs, tensors, data_start })
}

// ── Shape ────────────────────────────────────────────────────────────────

/// One block's weights, in bytes: the routed experts, and everything else.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Layer {
    pub experts: u64,
    pub other: u64,
}

/// A candidate's shape as a report holds it: read, or why not.
pub type ShapeResult = Result<Arc<Shape>, String>;

/// What the estimate needs of a model (`shape_of`).
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub arch: String,
    /// Routed-expert tensors are present.
    pub moe: bool,
    /// Every tensor, in bytes.
    pub total: u64,
    /// The input embedding tables (`is_input_embedding`): host memory.
    pub token_embd: u64,
    /// The tensors outside any block but the input embeddings (output head,
    /// norms): the output layer's position.
    pub globals: u64,
    pub layers: Vec<Layer>,
    /// The KV cache at `ctx` tokens, f16.
    pub kv: u64,
    pub ctx: u64,
}

/// `blk.N.` → N.
fn block_of(name: &str) -> Option<usize> {
    name.strip_prefix("blk.")?.split('.').next()?.parse().ok()
}

/// An input embedding table, which llama.cpp keeps in host memory whatever
/// the offload (`LLM_TENSOR_LAYER_INPUT`, `llama-arch.cpp`).
fn is_input_embedding(name: &str) -> bool {
    matches!(name, "token_embd.weight" | "per_layer_token_embd.weight")
}

/// A routed-expert tensor: what `--n-cpu-moe` moves (`LLM_FFN_EXPS_REGEX`,
/// `\.ffn_(up|down|gate|gate_up)_(ch|)exps`). A shared expert (`_shexp`) is not.
fn is_expert(name: &str) -> bool {
    ["up", "down", "gate", "gate_up"]
        .iter()
        .any(|k| name.contains(&format!(".ffn_{k}_exps")) || name.contains(&format!(".ffn_{k}_chexps")))
}

/// Fold the headers of a model's shards (first shard first, each with the
/// shard's file size) into a `Shape` (pure). The key/values come from the
/// shard that has `general.architecture` (the first, for gguf-split files).
pub fn shape_of(shards: &[(Header, u64)]) -> Result<Shape, String> {
    let kvs = shards
        .iter()
        .map(|(h, _)| &h.kvs)
        .find(|k| k.contains_key("general.architecture"))
        .ok_or("no general.architecture in the header")?;
    let arch = match kvs.get("general.architecture") {
        Some(Kv::Str(s)) => s.clone(),
        _ => return Err("general.architecture is not a string".into()),
    };
    let mut s = Shape {
        arch: arch.clone(),
        moe: false,
        total: 0,
        token_embd: 0,
        globals: 0,
        layers: Vec::new(),
        kv: 0,
        ctx: 0,
    };
    for (h, file_size) in shards {
        let mut offsets: Vec<(u64, &str)> = h.tensors.iter().map(|(n, o)| (*o, n.as_str())).collect();
        offsets.sort();
        let data = file_size
            .checked_sub(h.data_start)
            .ok_or_else(|| format!("a shard of {file_size} bytes whose header ends at {}", h.data_start))?;
        for (i, (off, name)) in offsets.iter().enumerate() {
            let end = offsets.get(i + 1).map_or(data, |n| n.0);
            let bytes = end.saturating_sub(*off);
            s.total += bytes;
            if is_input_embedding(name) {
                s.token_embd += bytes;
            } else if let Some(b) = block_of(name) {
                if b >= 4096 {
                    return Err(format!("block {b}"));
                }
                if s.layers.len() <= b {
                    s.layers.resize(b + 1, Layer::default());
                }
                if is_expert(name) {
                    s.layers[b].experts += bytes;
                    s.moe = true;
                } else {
                    s.layers[b].other += bytes;
                }
            } else {
                s.globals += bytes;
            }
        }
    }
    let key = |k: &str| kvs.get(&format!("{arch}.{k}"));
    let int = |k: &str| key(k).and_then(Kv::as_u64);
    s.ctx = int("context_length").filter(|&c| c > 0).map_or(REF_CTX, |c| c.min(REF_CTX));
    s.kv = kv_per_token(&key, s.layers.len() as u64).map_or(FALLBACK_KV, |b| b * s.ctx);
    Ok(s)
}

/// KV bytes per token (f16, all attention layers), from the attention keys;
/// `None` when the layout is not readable.
fn kv_per_token<'k>(key: &dyn Fn(&str) -> Option<&'k Kv>, blocks: u64) -> Option<u64> {
    let int = |k: &str| key(k).and_then(Kv::as_u64);
    let n_layer = int("block_count").unwrap_or(blocks);
    if let Some(rank) = int("attention.kv_lora_rank") {
        // MLA: the compressed latent plus the rope part, one cache.
        let rope = int("rope.dimension_count").unwrap_or(64);
        return Some((rank + rope) * 2 * n_layer);
    }
    let n_embd = int("embedding_length")?;
    let n_head = match key("attention.head_count")? {
        Kv::Arr(a) => a.iter().filter_map(Kv::as_u64).max()?,
        v => v.as_u64()?,
    };
    if n_head == 0 {
        return None;
    }
    let k_len = int("attention.key_length").unwrap_or(n_embd / n_head);
    let v_len = int("attention.value_length").unwrap_or(n_embd / n_head);
    let per_head = (k_len + v_len) * 2;
    match key("attention.head_count_kv") {
        Some(Kv::Arr(a)) => Some(a.iter().filter_map(Kv::as_u64).sum::<u64>() * per_head),
        other => {
            let hkv = other.and_then(Kv::as_u64).unwrap_or(n_head);
            let attn_layers = match int("full_attention_interval").filter(|&i| i > 1) {
                Some(i) => n_layer.div_ceil(i),
                None => n_layer,
            };
            Some(hkv * per_head * attn_layers)
        }
    }
}

// ── Remote headers ───────────────────────────────────────────────────────

/// Shapes already read, by `repo@commit/path` of the first shard.
static CACHE: Mutex<Option<HashMap<String, Arc<Shape>>>> = Mutex::new(None);
/// The largest header seen in this process (bytes), for the next first request.
static HINT: AtomicU64 = AtomicU64::new(0);

/// `<endpoint><repo>/resolve/<commit>/<path>`, each path segment
/// percent-encoded (the path is already `hf::safe_repo_path`).
fn resolve_url(endpoint: &str, repo: &str, commit: &str, path: &str) -> String {
    let path: Vec<String> = path.split('/').map(hf::percent_encode).collect();
    format!("{endpoint}{repo}/resolve/{commit}/{}", path.join("/"))
}

/// One shard's header over growing range requests, the first one sized by
/// `HINT`.
fn fetch_header(fetch: &dyn Fetch, url: &str, token: Option<&Token>, repo: &str, file_size: u64) -> Result<Header, HfError> {
    let hint = HINT.load(Ordering::Relaxed);
    let first = if hint > 0 { hint + HINT_MARGIN } else { INITIAL_FETCH };
    fetch_header_from(fetch, url, token, repo, file_size, first)
}

/// `fetch_header` with the first request's length given.
fn fetch_header_from(
    fetch: &dyn Fetch,
    url: &str,
    token: Option<&Token>,
    repo: &str,
    file_size: u64,
    first: u64,
) -> Result<Header, HfError> {
    let mut len = first;
    if file_size > 0 {
        len = len.min(file_size);
    }
    loop {
        let (status, body) = fetch.get_range(url, token, 0, len)?;
        if !matches!(status, 200 | 206) {
            let text = String::from_utf8_lossy(&body[..body.len().min(4096)]).into_owned();
            return Err(hf::api_error(status, &text, token.is_some(), repo));
        }
        match parse_header(&body) {
            Ok(h) => {
                HINT.fetch_max(h.data_start, Ordering::Relaxed);
                return Ok(h);
            }
            Err(ParseError::Truncated) if body.len() as u64 >= len && len < MAX_HEADER && (file_size == 0 || len < file_size) => {
                len = (len * 4).min(MAX_HEADER);
                if file_size > 0 {
                    len = len.min(file_size);
                }
            }
            Err(ParseError::Truncated) => {
                return Err(HfError::Io(format!("the GGUF header is cut short after {} bytes", body.len())));
            }
            Err(ParseError::Invalid(m)) => return Err(HfError::Io(format!("not a readable GGUF header: {m}"))),
        }
    }
}

/// The `Shape` of a candidate: every shard's header, read remotely (cached).
pub fn fetch_shape(
    fetch: &dyn Fetch,
    endpoint: &str,
    repo: &str,
    commit: &str,
    c: &Candidate,
    token: Option<&Token>,
) -> Result<Arc<Shape>, HfError> {
    let key = format!("{repo}@{commit}/{}", c.primary().path);
    if let Some(s) = CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)) {
        return Ok(s.clone());
    }
    let mut headers = Vec::with_capacity(c.shards.len());
    for f in &c.shards {
        if !hf::safe_repo_path(&f.path) {
            return Err(HfError::InvalidSpec(format!("refusing the repository file path {:?}", f.path)));
        }
        let url = resolve_url(endpoint, repo, commit, &f.path);
        headers.push((fetch_header(fetch, &url, token, repo, f.size)?, f.size));
    }
    let shape = Arc::new(shape_of(&headers).map_err(|m| HfError::Io(format!("{}: {m}", c.primary().name())))?);
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, shape.clone());
    Ok(shape)
}

/// `fetch_shape` for every candidate, `WORKERS` at a time (blocking until all
/// are done or `keep_going` turns false). `on_result(index, result)` is
/// called on a worker thread as each one lands.
#[allow(clippy::too_many_arguments)]
pub fn fetch_shapes(
    fetch: &(dyn Fetch + Sync),
    endpoint: &str,
    repo: &str,
    commit: &str,
    candidates: &[Candidate],
    token: Option<&Token>,
    keep_going: &(dyn Fn() -> bool + Sync),
    on_result: &(dyn Fn(usize, Result<Arc<Shape>, HfError>) + Sync),
) {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(candidates.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(c) = candidates.get(i) else {
                    break;
                };
                if !keep_going() {
                    break;
                }
                on_result(i, fetch_shape(fetch, endpoint, repo, commit, c, token));
            });
        }
    });
}

// ── Local headers ────────────────────────────────────────────────────────

/// One file's header from disk, over growing reads like `fetch_header_from`.
fn read_local_header(path: &Path, file_size: u64) -> Result<Header, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut len = INITIAL_FETCH.min(file_size);
    loop {
        let mut buf = Vec::new();
        f.seek(SeekFrom::Start(0))
            .and_then(|_| (&mut f).take(len).read_to_end(&mut buf))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        match parse_header(&buf) {
            Ok(h) => return Ok(h),
            Err(ParseError::Truncated) if len < MAX_HEADER && len < file_size => {
                len = (len * 4).min(MAX_HEADER).min(file_size);
            }
            Err(ParseError::Truncated) => return Err(format!("the GGUF header is cut short after {} bytes", buf.len())),
            Err(ParseError::Invalid(m)) => return Err(format!("not a readable GGUF header: {m}")),
        }
    }
}

/// The `Shape` of a model on disk: `first` and the rest of its shard set
/// (`linked_files::shard_set`), each header read locally. What the preset of a
/// download is placed from (`hf::preset_from_download`).
pub fn local_shape(first: &Path) -> Result<Shape, String> {
    let mut headers = Vec::new();
    for p in crate::linked_files::shard_set(first) {
        let size = std::fs::metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?.len();
        headers.push((read_local_header(&p, size)?, size));
    }
    shape_of(&headers)
}

// ── The machine ──────────────────────────────────────────────────────────

/// One physical GPU: the `--device` id it is written as, the name the probe
/// reports, its memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Gpu {
    pub id: String,
    pub name: String,
    pub total_mib: u64,
}

impl Gpu {
    /// What the estimate may fill: the card minus `GPU_RESERVE`.
    pub fn usable(&self) -> u64 {
        (self.total_mib * MIB).saturating_sub(GPU_RESERVE)
    }

    /// The name without the vendor words: `RTX 4070 SUPER`, `AI PRO R9700`.
    pub fn short_name(&self) -> String {
        const VENDOR: [&str; 6] = ["nvidia", "geforce", "amd", "radeon", "intel", "ati"];
        let words: Vec<&str> = self
            .name
            .split_whitespace()
            .filter(|w| {
                let bare = w.trim_end_matches("(TM)").trim_end_matches("(R)").to_ascii_lowercase();
                !VENDOR.contains(&bare.as_str())
            })
            .collect();
        if words.is_empty() { self.name.clone() } else { words.join(" ") }
    }
}

/// A GPU whose memory is system RAM: an APU's integrated graphics. By name,
/// since `--list-devices` does not say (AMD reports `AMD Radeon(TM)
/// Graphics`, or a `780M`/`890M`/`8060S` style model; Intel its UHD, Iris and
/// bare "Arc Graphics" iGPUs).
pub fn is_integrated(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.contains("radeon") {
        // `680M` / `780M` / `890M`, and Strix Halo's `8060S` / `8050S` /
        // `8040S`; a laptop dGPU's `RX 6600M` has four digits and an RX.
        let model = n.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| {
            let all_digits = |d: &str| d.chars().all(|c| c.is_ascii_digit());
            match (t.strip_suffix('m'), t.strip_suffix('s')) {
                (Some(d), _) => d.len() == 3 && all_digits(d),
                (_, Some(d)) => d.len() == 4 && d.starts_with("80") && all_digits(d),
                _ => false,
            }
        });
        return model || n.ends_with("radeon(tm) graphics") || n.ends_with("radeon graphics");
    }
    n.contains("intel")
        && (n.contains("uhd") || n.contains("iris") || n.contains("hd graphics") || n.ends_with("arc(tm) graphics") || n.ends_with("arc graphics"))
}

/// The physical discrete GPUs of a device probe (see the module header): CPU
/// entries and unsized devices dropped, one card counted once across
/// backends, integrated GPUs dropped when a discrete one exists. Each card
/// keeps the id and size of the backend that sees the most cards of its name,
/// a non-Vulkan one on a tie (CUDA0, not Vulkan1). Probe order.
pub fn gpu_pool(devs: &[DeviceOption]) -> Vec<Gpu> {
    // name -> backends in probe order, each with its devices of that name
    type Seen<'a> = Vec<(String, Vec<&'a DeviceOption>)>;
    let mut by_name: Vec<(String, Seen)> = Vec::new();
    for d in devs.iter().filter(|d| !d.is_cpu() && d.total_mib > 0) {
        let name = if d.name.is_empty() { d.id.clone() } else { d.name.trim().to_string() };
        let key = name.to_ascii_lowercase();
        let entry = match by_name.iter().position(|e| e.0.to_ascii_lowercase() == key) {
            Some(i) => &mut by_name[i],
            None => {
                by_name.push((name, Vec::new()));
                by_name.last_mut().unwrap()
            }
        };
        let b = backend_of(&d.id);
        match entry.1.iter_mut().find(|(k, _)| *k == b) {
            Some((_, list)) => list.push(d),
            None => entry.1.push((b, vec![d])),
        }
    }
    let mut gpus: Vec<Gpu> = Vec::new();
    for (name, backends) in by_name {
        let pick = backends.iter().max_by_key(|(b, list)| (list.len(), b != "vulkan"));
        if let Some((_, list)) = pick {
            gpus.extend(list.iter().map(|d| Gpu { id: d.id.clone(), name: name.clone(), total_mib: d.total_mib }));
        }
    }
    if gpus.iter().any(|g| !is_integrated(&g.name)) {
        gpus.retain(|g| !is_integrated(&g.name));
    }
    gpus
}

/// `cuda` for `CUDA0`: a device id's backend, lowercase.
fn backend_of(id: &str) -> String {
    id.trim_end_matches(|c: char| c.is_ascii_digit()).to_ascii_lowercase()
}

/// `gpus` in the order a split runs best on (pure; module header): on Windows
/// a ROCm card first and a CUDA one last, on Linux the reverse; the rest in
/// between, and the probe's order within a rank. Only a CUDA + ROCm machine
/// changes: the pair the guides measured.
pub fn split_order(mut gpus: Vec<Gpu>, windows: bool) -> Vec<Gpu> {
    let rank = |g: &Gpu| match (backend_of(&g.id).as_str(), windows) {
        ("rocm", true) | ("cuda", false) => 0,
        ("cuda", true) | ("rocm", false) => 2,
        _ => 1,
    };
    gpus.sort_by_key(rank);
    gpus
}

/// Total system RAM in bytes, `None` where it cannot be read.
pub fn system_ram_total() -> Option<u64> {
    #[cfg(any(target_os = "linux", windows))]
    {
        crate::hwmon::system_ram().map(|(total, _)| total)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// What an estimate is measured against.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// The cards, in split order (`split_order`).
    pub gpus: Vec<Gpu>,
    /// System RAM minus `RAM_RESERVE`; `None` when unknown.
    pub ram: Option<u64>,
    pub ram_total: Option<u64>,
}

impl Budget {
    /// Every card's usable memory added up: a ceiling, not a capacity (a
    /// block never straddles two cards).
    pub fn vram(&self) -> u64 {
        self.gpus.iter().map(Gpu::usable).sum()
    }
}

/// The budget of a machine; `None` without a GPU (no estimate then).
pub fn budget(devs: &[DeviceOption], ram_total: Option<u64>) -> Option<Budget> {
    budget_for(devs, ram_total, cfg!(windows))
}

/// `budget` with the OS of the split order given (pure).
pub fn budget_for(devs: &[DeviceOption], ram_total: Option<u64>, windows: bool) -> Option<Budget> {
    let gpus = gpu_pool(devs);
    if gpus.is_empty() {
        return None;
    }
    Some(Budget { gpus: split_order(gpus, windows), ram: ram_total.map(|r| r.saturating_sub(RAM_RESERVE)), ram_total })
}

// ── The extras: image encoder and separate drafter ───────────────────────

/// What an image encoder takes on top of its weights while it encodes an
/// image (module header, "The extras"): 248 MiB measured at llama.cpp's
/// warmup and 418 MiB at a 3840x2160 image for Qwen3.8's 888 MiB encoder.
pub const MMPROJ_COMPUTE: u64 = 512 * MIB;
/// A separate drafter's compute buffers (module header, "The extras"):
/// 218 to 243 MiB on the drafter's card plus 536 MiB on the card holding the
/// target's output head, measured for a DFlash Q8_0 drafter of Qwen3.8-27B.
pub const DRAFT_COMPUTE: u64 = 768 * MIB;
/// The cells a sliding-window layer holds beyond its window: the default
/// `--ubatch-size` (`llama_kv_cache_iswa`: 2048 + 512 = 2560 cells).
const SWA_EXTRA_CELLS: u64 = 512;

/// What a separate drafter file takes, from its header (`draft_shape_of`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DraftShape {
    /// Its tensors but the input embeddings (host memory, as the main
    /// model's), in bytes.
    pub weights: u64,
    /// KV bytes per token of its full-attention layers (f16).
    pub kv_full: u64,
    /// KV bytes per cell of its sliding-window layers (f16), which hold at
    /// most `swa_cells` cells whatever the context.
    pub kv_swa: u64,
    pub swa_cells: u64,
}

impl DraftShape {
    /// A drafter whose header could not be read: its file, nothing more.
    pub fn from_size(size: u64) -> DraftShape {
        DraftShape { weights: size, ..DraftShape::default() }
    }

    /// Its weights, its KV cache at `ctx` (the draft context holds as many
    /// tokens as the target's, `common/speculative.cpp`) and `DRAFT_COMPUTE`.
    pub fn bytes(&self, ctx: u64) -> u64 {
        self.weights + self.kv_full * ctx + self.kv_swa * ctx.min(self.swa_cells) + DRAFT_COMPUTE
    }
}

/// Fold a drafter's headers into a `DraftShape` (pure). Its sliding-window
/// layers (`attention.sliding_window` with a `sliding_window_pattern` array,
/// `true` for a windowed layer: every layer of a DFlash drafter) count the
/// window plus a ubatch, not the context; a layout the header does not tell
/// counts no KV, since a drafter's is small and `FALLBACK_KV` would dwarf it.
pub fn draft_shape_of(shards: &[(Header, u64)]) -> Result<DraftShape, String> {
    let s = shape_of(shards)?;
    let kvs = shards
        .iter()
        .map(|(h, _)| &h.kvs)
        .find(|k| k.contains_key("general.architecture"))
        .ok_or("no general.architecture in the header")?;
    let key = |k: &str| kvs.get(&format!("{}.{k}", s.arch));
    let int = |k: &str| key(k).and_then(Kv::as_u64);
    let per_token = kv_per_token(&key, s.layers.len() as u64).unwrap_or(0);
    let weights = s.total - s.token_embd;
    let n_layer = int("block_count").unwrap_or(s.layers.len() as u64);
    let window = int("attention.sliding_window").unwrap_or(0);
    let swa = match key("attention.sliding_window_pattern") {
        Some(Kv::Arr(a)) => (a.iter().filter(|v| matches!(v, Kv::Bool(true))).count() as u64).min(n_layer),
        _ => 0,
    };
    if window == 0 || swa == 0 {
        return Ok(DraftShape { weights, kv_full: per_token, ..DraftShape::default() });
    }
    let per_layer = per_token / n_layer;
    Ok(DraftShape { weights, kv_full: per_layer * (n_layer - swa), kv_swa: per_layer * swa, swa_cells: window + SWA_EXTRA_CELLS })
}

/// A drafter file's shape, or why its header could not be read.
pub type DraftResult = Result<Arc<DraftShape>, String>;

/// Drafter shapes already read, by `repo@commit/path`.
static DRAFT_CACHE: Mutex<Option<HashMap<String, Arc<DraftShape>>>> = Mutex::new(None);

/// The `DraftShape` of a repository's drafter file, its header read remotely
/// (cached, like `fetch_shape`).
pub fn fetch_draft_shape(
    fetch: &dyn Fetch,
    endpoint: &str,
    repo: &str,
    commit: &str,
    file: &hf::RepoFile,
    token: Option<&Token>,
) -> Result<Arc<DraftShape>, HfError> {
    let key = format!("{repo}@{commit}/{}", file.path);
    if let Some(s) = DRAFT_CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)) {
        return Ok(s.clone());
    }
    if !hf::safe_repo_path(&file.path) {
        return Err(HfError::InvalidSpec(format!("refusing the repository file path {:?}", file.path)));
    }
    let url = resolve_url(endpoint, repo, commit, &file.path);
    let header = fetch_header(fetch, &url, token, repo, file.size)?;
    let shape = Arc::new(draft_shape_of(&[(header, file.size)]).map_err(|m| HfError::Io(format!("{}: {m}", file.name())))?);
    DRAFT_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, shape.clone());
    Ok(shape)
}

/// The `DraftShape` of a drafter on disk.
pub fn local_draft_shape(path: &Path) -> Result<DraftShape, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    draft_shape_of(&[(read_local_header(path, size)?, size)])
}

/// The files a download brings beside the model, as the estimate counts
/// them (module header, "The extras").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extras {
    /// The image encoder's file size; 0 when none is taken.
    pub mmproj: u64,
    /// A separate drafter: what to call it (`DFlash drafter`) and its shape.
    pub drafter: Option<(String, DraftShape)>,
}

/// What `c` brings with the choices `mmproj` (take its mmproj when it has
/// one) and `drafter` (that sidecar, when it has it) (pure). `draft` answers
/// for a drafter file: `None` while its header is unknown (no extras yet: the
/// estimate waits), an error counts its file size alone.
pub fn extras_of(
    c: &Candidate,
    mmproj: bool,
    drafter: Option<hf::Sidecar>,
    draft: &dyn Fn(&hf::RepoFile) -> Option<DraftResult>,
) -> Option<Extras> {
    let mut x = Extras { mmproj: c.mmproj.as_ref().filter(|_| mmproj).map_or(0, |m| m.size), drafter: None };
    if let Some((kind, f)) = drafter.and_then(|k| Some((k, c.sidecar(k)?))) {
        let shape = match draft(f)? {
            Ok(s) => *s,
            Err(_) => DraftShape::from_size(f.size),
        };
        x.drafter = Some((format!("{} drafter", kind.label()), shape));
    }
    Some(x)
}

/// Where an extra file went.
#[derive(Debug, Clone, PartialEq)]
pub enum Spot {
    /// On this card. `pinned`: llama.cpp's default would put it elsewhere,
    /// so the placement writes its own key (`mmproj-device`, `device-draft`).
    Gpu { gpu: Gpu, pinned: bool },
    /// The image encoder on the CPU (`mmproj-offload = false`).
    Cpu,
}

/// An extra file as an estimate placed it: what it is, the bytes it takes
/// on its card (weights plus compute, plus a drafter's KV cache), where.
#[derive(Debug, Clone, PartialEq)]
pub struct Extra {
    pub what: String,
    pub bytes: u64,
    pub spot: Spot,
}

// ── The estimate ─────────────────────────────────────────────────────────

/// Where a model's weights go (see the module header).
#[derive(Debug, Clone, PartialEq)]
pub enum Fit {
    /// Everything on the GPUs: `need` of `budget` bytes.
    Gpu { need: u64, budget: u64 },
    /// The experts of `gpu_layers` of `moe_layers` MoE layers on the GPUs
    /// (`gpu_experts` bytes), `--n-cpu-moe n_cpu_moe`, `ram` bytes in RAM.
    Moe { gpu_layers: u32, moe_layers: u32, n_cpu_moe: u32, gpu_experts: u64, ram: u64 },
    /// A dense model split: `gpu_layers` of `layers` on the GPUs.
    Dense { gpu_layers: u32, layers: u32, ram: u64 },
    /// Even the attention and shared weights (with the KV cache) need more
    /// than the GPUs have.
    TooBig { base: u64, budget: u64 },
}

/// One card's part of a split: the card, how many consecutive positions it
/// holds (blocks, and the output layer on the last card that holds any), and
/// their bytes (weights plus KV cache).
#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    pub gpu: Gpu,
    pub positions: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub fit: Fit,
    /// How far the RAM part exceeds the RAM budget (0: it fits; the rest
    /// pages from disk through mmap otherwise); `None` when RAM is unknown.
    pub ram_short: Option<u64>,
    /// Where the GPU part goes, card by card in split order, on a machine
    /// with SEVERAL cards (a card given nothing is left out); empty on a
    /// one-card machine, where there is nothing to choose, and when nothing
    /// is on a GPU. The positions are the `--tensor-split` vector. A share's
    /// bytes are the model's alone; the extras are below.
    pub shares: Vec<Share>,
    /// The image encoder, when one is counted (`Extras::mmproj`).
    pub encoder: Option<Extra>,
    /// The separate drafter, when one is counted (always on a GPU).
    pub drafter: Option<Extra>,
}

impl Estimate {
    /// An estimate without extras.
    pub fn plain(fit: Fit, ram_short: Option<u64>, shares: Vec<Share>) -> Estimate {
        Estimate { fit, ram_short, shares, encoder: None, drafter: None }
    }
}

/// The `n_layer + 1` position weights of `s` (module header): block `i` its
/// non-expert bytes, its even share of the KV cache, and its experts unless
/// `i < n_cpu_moe`; then the output layer, with the KV the even spread left.
fn positions(s: &Shape, n_cpu_moe: usize) -> Vec<u64> {
    let n = s.layers.len() as u64;
    let kv_layer = s.kv.checked_div(n).unwrap_or(0);
    let mut w: Vec<u64> = s
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| l.other + kv_layer + if i >= n_cpu_moe { l.experts } else { 0 })
        .collect();
    w.push(s.globals + (s.kv - kv_layer * n));
    w
}

/// Cut `w` into one consecutive run per card in order, each within its cap
/// (pure): every card takes as many of the next positions as fit, which finds
/// a cut whenever one exists. The run lengths, or `None`.
fn pack(w: &[u64], caps: &[u64]) -> Option<Vec<u32>> {
    let mut at = 0;
    let mut counts = Vec::with_capacity(caps.len());
    for &cap in caps {
        let (start, mut used) = (at, 0u64);
        while at < w.len() && used + w[at] <= cap {
            used += w[at];
            at += 1;
        }
        counts.push((at - start) as u32);
    }
    (at == w.len()).then_some(counts)
}

/// The cut that fills every card to the same, smallest, share of its cap
/// (pure; module header): `pack` at the least fraction (in 1/65536ths) of
/// every cap that still fits.
fn balanced(w: &[u64], caps: &[u64]) -> Option<Vec<u32>> {
    const ONE: u64 = 1 << 16;
    let at = |f: u64| -> Vec<u64> { caps.iter().map(|&c| (u128::from(c) * u128::from(f) / u128::from(ONE)) as u64).collect() };
    pack(w, caps)?;
    let (mut lo, mut hi) = (0, ONE);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if pack(w, &at(mid)).is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    pack(w, &at(hi))
}

/// Where the image encoder is tried (module header, "The extras").
#[derive(Debug, Clone, Copy, PartialEq)]
enum Enc {
    /// No encoder is taken.
    Absent,
    /// On this card (index into `Budget::gpus`).
    Card(usize),
    Cpu,
}

/// What an image encoder is called in the rows and the note.
const ENCODER: &str = "image encoder";

/// Place `s` alone on the machine `b`, card by card (pure; see the module
/// header): `estimate_with` without extras, for the tests.
#[cfg(test)]
pub fn estimate(s: &Shape, b: &Budget) -> Estimate {
    estimate_with(s, b, &Extras::default())
}

/// Place `s` with the extras `x` on the machine `b`, card by card (pure; see
/// the module header, "The estimate" and "The extras").
pub fn estimate_with(s: &Shape, b: &Budget, x: &Extras) -> Estimate {
    let caps: Vec<u64> = b.gpus.iter().map(Gpu::usable).collect();
    let nc = caps.len();
    let n = s.layers.len();
    let short = |ram: u64| b.ram.map(|r| ram.saturating_sub(r));
    if nc == 0 {
        return Estimate::plain(Fit::Dense { gpu_layers: 0, layers: n as u32, ram: s.total }, short(s.total), Vec::new());
    }
    // The encoder's weights plus its compute allowance, the drafter's
    // weights plus its KV cache at the model's context and its compute.
    let m = if x.mmproj > 0 { x.mmproj + MMPROJ_COMPUTE } else { 0 };
    let d = x.drafter.as_ref().map_or(0, |(_, ds)| ds.bytes(s.ctx));
    // The caps left for the model with the encoder on `enc` and the drafter
    // on card `dc`; `None` when either does not fit its card.
    let room = |enc: Enc, dc: usize| -> Option<Vec<u64>> {
        let mut c = caps.clone();
        if let Enc::Card(i) = enc {
            c[i] = c[i].checked_sub(m)?;
        }
        if d > 0 {
            c[dc] = c[dc].checked_sub(d)?;
        }
        Some(c)
    };
    // The encoder's GPU spots in order of preference: `first` (where
    // llama.cpp puts it by default), then every other card in split order.
    let on_gpus = |first: usize| -> Vec<Enc> {
        if m == 0 {
            return vec![Enc::Absent];
        }
        std::iter::once(first).chain((0..nc).filter(|&i| i != first)).map(Enc::Card).collect()
    };
    let on_cpu: Vec<Enc> = if m > 0 { vec![Enc::Cpu] } else { Vec::new() };
    // The estimate of the cut `counts` over `w`, with `ram` bytes of the
    // model in RAM, the encoder on `enc` and the drafter on card `dc`.
    let finish = |fit: Fit, ram: u64, w: &[u64], counts: &[u32], enc: Enc, dc: usize| -> Estimate {
        let mut shares = Vec::new();
        if nc > 1 {
            let mut at = 0;
            for (g, &c) in b.gpus.iter().zip(counts) {
                let bytes = w[at..at + c as usize].iter().sum();
                at += c as usize;
                if c > 0 {
                    shares.push(Share { gpu: g.clone(), positions: c, bytes });
                }
            }
        }
        // The encoder follows the FIRST device, the drafter inherits the
        // whole device list: a key wherever that is not where it was counted.
        let first = shares.first().map(|f| f.gpu.id.clone());
        let encoder = match enc {
            Enc::Absent => None,
            Enc::Cpu => Some(Extra { what: ENCODER.into(), bytes: x.mmproj, spot: Spot::Cpu }),
            Enc::Card(i) => {
                let pinned = first.as_ref().is_some_and(|f| *f != b.gpus[i].id);
                Some(Extra { what: ENCODER.into(), bytes: m, spot: Spot::Gpu { gpu: b.gpus[i].clone(), pinned } })
            }
        };
        let drafter = x.drafter.as_ref().map(|(what, _)| {
            let gpu = b.gpus[dc].clone();
            let pinned = match shares.as_slice() {
                [] => false,
                [one] => one.gpu.id != gpu.id,
                _ => true,
            };
            Extra { what: what.clone(), bytes: d, spot: Spot::Gpu { gpu, pinned } }
        });
        let ram = ram + if enc == Enc::Cpu { x.mmproj } else { 0 };
        let ram_short = if ram == 0 { Some(0) } else { short(ram) };
        Estimate { fit, ram_short, shares, encoder, drafter }
    };
    let all = positions(s, 0);
    let need: u64 = all.iter().sum();
    // 1. All on GPU: the largest card alone when it holds everything (the
    // first of equals), else a cut over every card; the encoder on its
    // default card, then moved to another, then on the CPU. The drafter
    // goes with a model alone on its card, or to another card when that is
    // what lets the model stay alone; on a split, to the last card.
    let largest = (0..nc).rev().max_by_key(|&i| caps[i]).unwrap_or(0);
    let others: Vec<usize> = std::iter::once(largest).chain((0..nc).filter(|&i| i != largest)).collect();
    let drafter_cards: &[usize] = if d > 0 { &others } else { &others[..1] };
    let mut tries: Vec<(Enc, bool, usize)> = Vec::new();
    let mut add = |encs: &[Enc]| {
        for &e in encs {
            tries.extend(drafter_cards.iter().map(|&dc| (e, true, dc)));
            tries.push((e, false, nc - 1));
        }
    };
    add(&on_gpus(largest));
    // An MoE model rather moves one more block's experts to RAM (step 2)
    // than its encoder to the CPU: that costs little per token, while
    // encoding an image on the CPU takes seconds.
    if !s.moe {
        add(&on_cpu);
    }
    // The order: the model alone with the encoder on the same card, then on
    // another one (no cost per token), then a split with the encoder on its
    // default (first) card, then elsewhere, then the encoder on the CPU.
    tries.sort_by_key(|&(e, alone, _)| match (e, alone) {
        (Enc::Absent, true) => 0,
        (Enc::Card(i), true) if i == largest => 0,
        (Enc::Card(_), true) => 1,
        (Enc::Absent, false) | (Enc::Card(0), false) => 2,
        (Enc::Card(_), false) => 3,
        (Enc::Cpu, true) => 4,
        (Enc::Cpu, false) => 5,
    });
    for (enc, alone, dc) in tries {
        let Some(c) = room(enc, dc) else {
            continue;
        };
        let counts = if alone {
            (need <= c[largest]).then(|| {
                let mut counts = vec![0; nc];
                counts[largest] = all.len() as u32;
                counts
            })
        } else if nc > 1 {
            balanced(&all, &c)
        } else {
            None
        };
        if let Some(counts) = counts {
            let on_gpu = need + d + if matches!(enc, Enc::Card(_)) { m } else { 0 };
            return finish(Fit::Gpu { need: on_gpu, budget: b.vram() }, 0, &all, &counts, enc, dc);
        }
    }
    if s.moe {
        // 2. The fewest blocks with their experts in RAM, the encoder on the
        // card where that is fewest (its default first on a tie), and on the
        // CPU only when no card can take it at all.
        let fewest = |enc: Enc| -> Option<(usize, Vec<u64>)> {
            let c = room(enc, nc - 1)?;
            let k = (1..=n).find(|&k| pack(&positions(s, k), &c).is_some())?;
            Some((k, c))
        };
        let mut best: Option<(usize, Vec<u64>, Enc)> = None;
        for enc in on_gpus(0) {
            if let Some((k, c)) = fewest(enc) {
                if best.as_ref().is_none_or(|b| k < b.0) {
                    best = Some((k, c, enc));
                }
            }
        }
        if best.is_none() {
            best = on_cpu.iter().find_map(|&enc| fewest(enc).map(|(k, c)| (k, c, enc)));
        }
        let Some((k, c, enc)) = best else {
            let base = positions(s, n).iter().sum::<u64>() + d;
            return Estimate::plain(Fit::TooBig { base, budget: b.vram() }, short(s.total), Vec::new());
        };
        let w = positions(s, k);
        let counts = balanced(&w, &c).unwrap_or_default();
        let experts: u64 = s.layers.iter().map(|l| l.experts).sum();
        let gpu_experts: u64 = s.layers[k..].iter().map(|l| l.experts).sum();
        let ram = s.token_embd + experts - gpu_experts;
        let fit = Fit::Moe {
            gpu_layers: s.layers[k..].iter().filter(|l| l.experts > 0).count() as u32,
            moe_layers: s.layers.iter().filter(|l| l.experts > 0).count() as u32,
            n_cpu_moe: k as u32,
            gpu_experts,
            ram: ram + if enc == Enc::Cpu { x.mmproj } else { 0 },
        };
        return finish(fit, ram, &w, &counts, enc, nc - 1);
    }
    // 3. A dense model: the most positions from the output layer back, the
    // encoder wherever that is most (a card first on a tie, the CPU when it
    // frees a block: every block in RAM slows every token).
    let p = all.len();
    let most = |enc: Enc| -> Option<(usize, Vec<u64>)> {
        let c = room(enc, nc - 1)?;
        let l = (1..p).rev().find(|&l| pack(&all[p - l..], &c).is_some())?;
        Some((l, c))
    };
    let mut best: Option<(usize, Vec<u64>, Enc)> = None;
    for enc in on_gpus(0).into_iter().chain(on_cpu.iter().copied()) {
        if let Some((l, c)) = most(enc) {
            if best.as_ref().is_none_or(|b| l > b.0) {
                best = Some((l, c, enc));
            }
        }
    }
    let Some((l, c, enc)) = best else {
        return Estimate::plain(Fit::Dense { gpu_layers: 0, layers: n as u32, ram: s.total }, short(s.total), Vec::new());
    };
    let w = &all[p - l..];
    let counts = balanced(w, &c).unwrap_or_default();
    let blocks = l - 1;
    let on_gpu = s.globals + s.layers[n - blocks..].iter().map(|x| x.experts + x.other).sum::<u64>();
    let ram = s.total - on_gpu;
    let fit = Fit::Dense { gpu_layers: blocks as u32, layers: n as u32, ram: ram + if enc == Enc::Cpu { x.mmproj } else { 0 } };
    finish(fit, ram, w, &counts, enc, nc - 1)
}

fn gib(b: u64) -> String {
    format!("{:.1} GiB", b as f64 / GIB as f64)
}

/// `32k` for a context length.
fn ctx_label(ctx: u64) -> String {
    if ctx >= 1024 && ctx.is_multiple_of(1024) {
        format!("{}k", ctx / 1024)
    } else {
        ctx.to_string()
    }
}

/// What the RAM part means, after a split's figures.
fn ram_note(e: &Estimate) -> String {
    match e.ram_short {
        Some(0) => String::new(),
        Some(x) => format!(", {} over the RAM: the rest pages from disk", gib(x)),
        None => ", RAM size unknown".into(),
    }
}

/// `AI PRO R9700 (ROCm1) 20.2 GiB + RTX 4070 SUPER (CUDA0) 9.1 GiB,
/// tensor-split 26,23`: a split card by card, with the vector it is written as.
pub fn split_label(shares: &[Share]) -> String {
    let cards: Vec<String> = shares.iter().map(|s| format!("{} ({}) {}", s.gpu.short_name(), s.gpu.id, gib(s.bytes))).collect();
    let mut out = cards.join(" + ");
    if shares.len() > 1 {
        out.push_str(&format!(", tensor-split {}", tensor_split(shares)));
    }
    out
}

/// `26,23`: the `--tensor-split` of a split, its position counts.
fn tensor_split(shares: &[Share]) -> String {
    shares.iter().map(|s| s.positions.to_string()).collect::<Vec<_>>().join(",")
}

/// Where an extra went, for a label: `ROCm1`, `the CPU`.
fn spot_label(x: &Extra) -> String {
    match &x.spot {
        Spot::Gpu { gpu, .. } => gpu.id.clone(),
        Spot::Cpu => "the CPU".into(),
    }
}

/// `the image encoder (1.4 GiB) on ROCm1 and the DFlash drafter (2.7 GiB)
/// on CUDA0`: the extras an estimate counted; empty without any.
pub fn extras_label(e: &Estimate) -> String {
    let parts: Vec<String> =
        e.encoder.iter().chain(&e.drafter).map(|x| format!("the {} ({}) on {}", x.what, gib(x.bytes), spot_label(x))).collect();
    parts.join(" and ")
}

/// A candidate row's estimate line (pure). On a machine with several cards
/// it names the card or cards the GPU part goes to (`split_label`); the
/// extras counted follow (`extras_label`).
pub fn estimate_label(e: &Estimate, ctx: u64) -> String {
    let main = model_label(e, ctx);
    match extras_label(e) {
        x if x.is_empty() || matches!(e.fit, Fit::TooBig { .. }) => main,
        x => format!("{main}; with {x}"),
    }
}

/// `estimate_label` without the extras.
fn model_label(e: &Estimate, ctx: u64) -> String {
    let split = split_label(&e.shares);
    match &e.fit {
        Fit::Gpu { need, budget } if e.shares.is_empty() => format!(
            "all on GPU: {:.1} of {} with a {} context",
            *need as f64 / GIB as f64,
            gib(*budget),
            ctx_label(ctx)
        ),
        Fit::Gpu { .. } => format!("all on GPU with a {} context: {split}", ctx_label(ctx)),
        Fit::Moe { gpu_layers: 0, ram, .. } => format!(
            "MoE split: attention on GPU{}, every expert in RAM ({}), --n-cpu-moe all{}",
            if split.is_empty() { String::new() } else { format!(" ({split})") },
            gib(*ram),
            ram_note(e)
        ),
        Fit::Moe { gpu_layers, moe_layers, n_cpu_moe, gpu_experts, ram } => format!(
            "MoE split: experts of {gpu_layers}/{moe_layers} layers on GPU ({}), {}{} in RAM, --n-cpu-moe {n_cpu_moe}{}",
            gib(*gpu_experts),
            if split.is_empty() { String::new() } else { format!("{split}, ") },
            gib(*ram),
            ram_note(e)
        ),
        Fit::Dense { gpu_layers, layers, ram } => format!(
            "too big for the GPU: {gpu_layers}/{layers} layers on GPU{}, {} in RAM (slow){}",
            if split.is_empty() { String::new() } else { format!(" ({split})") },
            gib(*ram),
            ram_note(e)
        ),
        Fit::TooBig { base, budget } => format!(
            "too big: attention and shared weights alone need {}, the GPU budget is {}",
            gib(*base),
            gib(*budget)
        ),
    }
}

/// The line that says what the estimates are measured against (pure).
pub fn basis_label(b: &Budget) -> String {
    let gpus = if let [g] = b.gpus.as_slice() {
        format!("{} ({} usable)", g.name, gib(g.usable()))
    } else {
        let cards: Vec<String> = b.gpus.iter().map(|g| format!("{} ({}, {} usable)", g.name, g.id, gib(g.usable()))).collect();
        format!("{} GPUs in split order, {},", b.gpus.len(), cards.join(" then "))
    };
    let ram = match b.ram_total {
        Some(r) => format!("{} of RAM", gib(r)),
        None => "RAM size unknown".into(),
    };
    format!(
        "Estimated for {gpus} and {ram}, keeping {} free per GPU and {} of RAM, KV cache at f16",
        gib(GPU_RESERVE),
        gib(RAM_RESERVE)
    )
}

/// Why no estimate can be made without a GPU (after "estimate unavailable: ").
pub const NO_GPU: &str = "llama-server --list-devices reports no GPU";

/// A row's detail line: the estimate, led by the recommendation and its
/// reason when this row is the one (pure).
pub fn detail_line(estimate: &str, recommended: Option<&str>) -> String {
    match recommended {
        Some(reason) => format!("recommended, {reason}: {estimate}"),
        None => estimate.to_string(),
    }
}

// ── The placement ────────────────────────────────────────────────────────

/// The preset keys an estimate becomes (module header, "The placement"): an
/// empty string or `None` leaves the key at its default.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placement {
    pub device: String,
    pub tensor_split: String,
    pub n_cpu_moe: Option<i32>,
    pub n_gpu_layers: Option<i32>,
    pub ctx_size: Option<i32>,
    /// The card the separate drafter was counted on, when the device list
    /// alone would not put it there (a split, or a model alone on another
    /// card).
    pub device_draft: String,
    /// The card the image encoder was counted on, when it is not the first
    /// device (where llama.cpp puts it by default).
    pub mmproj_device: String,
    /// `Some(false)`: the image encoder was counted on the CPU.
    pub mmproj_offload: Option<bool>,
    /// One line for the section's `; placement =` comment.
    pub note: String,
}

/// The keys the estimate `e` of `s` becomes, `None` when it changes nothing
/// (pure; the table in the module header). The extras it counted keep
/// their cards: `mmproj-device`, `mmproj-offload = false`, `device-draft`
/// wherever llama.cpp's default would put them elsewhere.
pub fn placement(e: &Estimate, s: &Shape) -> Option<Placement> {
    let mut p = Placement::default();
    match e.fit {
        Fit::Gpu { .. } => {}
        Fit::Moe { n_cpu_moe, .. } => p.n_cpu_moe = Some(n_cpu_moe as i32),
        Fit::Dense { gpu_layers: 0, .. } | Fit::TooBig { .. } => return None,
        // The positions: the blocks plus the output layer.
        Fit::Dense { gpu_layers, .. } => p.n_gpu_layers = Some(gpu_layers as i32 + 1),
    }
    if !e.shares.is_empty() {
        p.device = e.shares.iter().map(|x| x.gpu.id.as_str()).collect::<Vec<_>>().join(",");
        if e.shares.len() > 1 {
            p.tensor_split = tensor_split(&e.shares);
        }
    }
    match e.encoder.as_ref().map(|x| &x.spot) {
        Some(Spot::Gpu { gpu, pinned: true }) => p.mmproj_device = gpu.id.clone(),
        Some(Spot::Cpu) => p.mmproj_offload = Some(false),
        _ => {}
    }
    if let Some(Spot::Gpu { gpu, pinned: true }) = e.drafter.as_ref().map(|x| &x.spot) {
        p.device_draft = gpu.id.clone();
    }
    let moved = !p.mmproj_device.is_empty() || p.mmproj_offload.is_some() || !p.device_draft.is_empty();
    if p.device.is_empty() && p.n_cpu_moe.is_none() && p.n_gpu_layers.is_none() && !moved {
        return None;
    }
    let sized = !p.tensor_split.is_empty() || p.n_cpu_moe.is_some() || p.n_gpu_layers.is_some();
    let mut note = if sized {
        p.ctx_size = Some(i32::try_from(s.ctx).unwrap_or(i32::MAX));
        format!("estimated at download for a {} context with {} kept free per GPU", ctx_label(s.ctx), gib(GPU_RESERVE))
    } else {
        let card = e.shares.first().map_or_else(|| "the GPU".to_string(), |x| x.gpu.short_name());
        format!("estimated at download: {card} holds the whole model, the fit sizes the context")
    };
    match e.ram_short {
        Some(0) => {}
        Some(x) => note.push_str(&format!(", {} over the RAM (the rest pages from disk)", gib(x))),
        None => note.push_str(", RAM size unknown"),
    }
    let extras = extras_label(e);
    if !extras.is_empty() {
        note.push_str(&format!("; counting {extras}"));
    }
    note.push_str("; a starting point to tune (optimize-preset)");
    p.note = note;
    Some(p)
}

// ── The recommendation ───────────────────────────────────────────────────

/// One row for `recommend`: the quant tag, the file size, and its estimate
/// (`None` while unknown or failed).
pub struct Row<'a> {
    pub tag: &'a str,
    pub size: u64,
    pub estimate: Option<&'a Estimate>,
}

/// The pick and why, in a few words.
#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub index: usize,
    pub reason: &'static str,
}

/// 1: all on GPU, the extras too; 2: all on GPU but the image encoder, on
/// the CPU; 3: an MoE with its experts in RAM without paging; `None`: does
/// not run well.
fn tier(e: &Estimate) -> Option<u8> {
    let encoder_on_cpu = matches!(e.encoder, Some(Extra { spot: Spot::Cpu, .. }));
    match e.fit {
        Fit::Gpu { .. } if encoder_on_cpu => Some(2),
        Fit::Gpu { .. } => Some(1),
        Fit::Moe { .. } if e.ram_short == Some(0) => Some(3),
        _ => None,
    }
}

/// The quality order of a quant tag (higher is better; see the module
/// header): at most 8 bits first, then bits, family, variant, file size.
pub fn quality(tag: &str, size: u64) -> (bool, i64, u8, u8, u64) {
    let t = tag.to_ascii_uppercase();
    let bits = hf::bits_of_tag(&t);
    let bits = if bits == i64::MAX { 0 } else { bits };
    let family = if t.starts_with("IQ") {
        1
    } else if t.contains("_K") {
        2
    } else {
        0
    };
    let variant = [("_XXS", 0), ("_XS", 1), ("_XL", 5), ("_S", 2), ("_M", 3), ("_L", 4)]
        .iter()
        .find(|(s, _)| t.ends_with(s))
        .map_or(3, |v| v.1);
    (bits > 0 && bits <= 8, bits, family, variant, size)
}

/// The quantization to recommend (pure; see the module header).
pub fn recommend(rows: &[Row]) -> Option<Recommendation> {
    let best_tier = rows.iter().filter_map(|r| r.estimate.and_then(tier)).min()?;
    let index = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.estimate.and_then(tier) == Some(best_tier))
        .max_by_key(|(_, r)| quality(r.tag, r.size))?
        .0;
    let reason = match best_tier {
        1 => "the best quality that fits in GPU memory",
        2 => "the best quality that fits in GPU memory with the image encoder on the CPU",
        _ => "the best quality whose experts fit in RAM without paging",
    };
    Some(Recommendation { index, reason })
}

/// Why nothing is recommended, once every row has its answer.
pub const NONE_RECOMMENDED: &str =
    "No quantization runs well here: none fits in GPU memory, and no MoE one keeps its experts in RAM without paging.";

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::hf::RepoFile;
    use std::sync::atomic::AtomicU32;

    // ── a GGUF writer for the fixtures (also `hf`'s and `save_flow`'s) ──

    pub(crate) enum W<'a> {
        U32(u32),
        U64(u64),
        Str(&'a str),
        ArrU32(Vec<u32>),
        ArrStr(Vec<&'a str>),
        ArrBool(Vec<bool>),
    }

    fn put_str(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    /// A GGUF v3 file's header for `kvs` and `tensors` (name, bytes), and the
    /// whole file's size (offsets laid out as llama.cpp does: each tensor
    /// aligned to 32).
    pub(crate) fn gguf(kvs: &[(&str, W)], tensors: &[(&str, u64)]) -> (Vec<u8>, u64) {
        let mut out = b"GGUF".to_vec();
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
        out.extend_from_slice(&(kvs.len() as u64).to_le_bytes());
        for (k, v) in kvs {
            put_str(&mut out, k);
            match v {
                W::U32(x) => {
                    out.extend_from_slice(&4u32.to_le_bytes());
                    out.extend_from_slice(&x.to_le_bytes());
                }
                W::U64(x) => {
                    out.extend_from_slice(&10u32.to_le_bytes());
                    out.extend_from_slice(&x.to_le_bytes());
                }
                W::Str(s) => {
                    out.extend_from_slice(&8u32.to_le_bytes());
                    put_str(&mut out, s);
                }
                W::ArrU32(a) => {
                    out.extend_from_slice(&9u32.to_le_bytes());
                    out.extend_from_slice(&4u32.to_le_bytes());
                    out.extend_from_slice(&(a.len() as u64).to_le_bytes());
                    for x in a {
                        out.extend_from_slice(&x.to_le_bytes());
                    }
                }
                W::ArrStr(a) => {
                    out.extend_from_slice(&9u32.to_le_bytes());
                    out.extend_from_slice(&8u32.to_le_bytes());
                    out.extend_from_slice(&(a.len() as u64).to_le_bytes());
                    for s in a {
                        put_str(&mut out, s);
                    }
                }
                W::ArrBool(a) => {
                    out.extend_from_slice(&9u32.to_le_bytes());
                    out.extend_from_slice(&7u32.to_le_bytes());
                    out.extend_from_slice(&(a.len() as u64).to_le_bytes());
                    out.extend(a.iter().map(|&b| u8::from(b)));
                }
            }
        }
        let mut off = 0u64;
        for (name, bytes) in tensors {
            put_str(&mut out, name);
            out.extend_from_slice(&2u32.to_le_bytes());
            out.extend_from_slice(&4096u64.to_le_bytes());
            out.extend_from_slice(&(bytes / 4096).max(1).to_le_bytes());
            out.extend_from_slice(&12u32.to_le_bytes()); // Q4_K
            out.extend_from_slice(&off.to_le_bytes());
            off += bytes.div_ceil(32) * 32;
        }
        let data_start = (out.len() as u64).div_ceil(32) * 32;
        (out, data_start + off)
    }

    /// A two-block Qwen3-30B-A3B lookalike (real key and tensor names, sizes
    /// scaled down): block 0 and 1 each with attention, norms, router and
    /// the three routed-expert tensors.
    fn moe_fixture() -> (Vec<u8>, u64) {
        let mut tensors: Vec<(String, u64)> = vec![("token_embd.weight".into(), 300 * MIB)];
        for b in 0..2 {
            for (t, size) in [
                ("attn_q.weight", 8 * MIB),
                ("attn_k.weight", MIB),
                ("attn_v.weight", MIB),
                ("attn_output.weight", 8 * MIB),
                ("attn_norm.weight", 4096),
                ("ffn_gate_inp.weight", 512 * 1024),
                ("ffn_gate_exps.weight", 400 * MIB),
                ("ffn_up_exps.weight", 400 * MIB),
                ("ffn_down_exps.weight", 500 * MIB),
            ] {
                tensors.push((format!("blk.{b}.{t}"), size));
            }
        }
        tensors.push(("output_norm.weight".into(), 4096));
        tensors.push(("output.weight".into(), 300 * MIB));
        let t: Vec<(&str, u64)> = tensors.iter().map(|(n, s)| (n.as_str(), *s)).collect();
        gguf(
            &[
                ("general.architecture", W::Str("qwen3moe")),
                ("general.name", W::Str("Qwen3-30B-A3B-Instruct-2507")),
                ("qwen3moe.block_count", W::U32(2)),
                ("qwen3moe.context_length", W::U32(262144)),
                ("qwen3moe.embedding_length", W::U32(2048)),
                ("qwen3moe.attention.head_count", W::U32(32)),
                ("qwen3moe.attention.head_count_kv", W::U32(4)),
                ("qwen3moe.attention.key_length", W::U32(128)),
                ("qwen3moe.attention.value_length", W::U32(128)),
                ("qwen3moe.expert_count", W::U32(128)),
                ("tokenizer.ggml.tokens", W::ArrStr(vec!["<|im_start|>", "hello", "world"])),
                ("general.file_type", W::U64(15)),
            ],
            &t,
        )
    }

    #[test]
    fn a_header_parses_keys_and_tensor_offsets() {
        let (h, size) = moe_fixture();
        let hdr = parse_header(&h).unwrap();
        assert_eq!(hdr.kvs["general.architecture"], Kv::Str("qwen3moe".into()));
        assert_eq!(hdr.kvs["qwen3moe.block_count"], Kv::Uint(2));
        assert_eq!(hdr.kvs["tokenizer.ggml.tokens"], Kv::Skipped, "string arrays are skipped");
        assert_eq!(hdr.tensors.len(), 21);
        assert_eq!(hdr.tensors[0], ("token_embd.weight".to_string(), 0));
        assert_eq!(hdr.data_start % 32, 0);
        assert!(hdr.data_start >= h.len() as u64 && hdr.data_start < size);
        // Followed by tensor data: a longer buffer parses the same.
        let mut longer = h.clone();
        longer.extend_from_slice(&[0u8; 4096]);
        assert_eq!(parse_header(&longer).unwrap(), hdr);
    }

    #[test]
    fn a_short_or_foreign_buffer_says_which() {
        let (h, _) = moe_fixture();
        for cut in [0, 3, 10, 30, h.len() / 2, h.len() - 1] {
            assert_eq!(parse_header(&h[..cut]), Err(ParseError::Truncated), "cut at {cut}");
        }
        assert!(matches!(parse_header(b"GGML\x03\0\0\0"), Err(ParseError::Invalid(_))));
        let mut v1 = h.clone();
        v1[4] = 1;
        assert!(matches!(parse_header(&v1), Err(ParseError::Invalid(_))));
        // A corrupt string length is refused, not fetched for.
        let mut huge = b"GGUF\x03\0\0\0".to_vec();
        huge.extend_from_slice(&0u64.to_le_bytes());
        huge.extend_from_slice(&1u64.to_le_bytes());
        huge.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(parse_header(&huge), Err(ParseError::Invalid(_))));
    }

    #[test]
    fn the_shape_splits_each_layer_into_experts_and_the_rest() {
        let (h, size) = moe_fixture();
        let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
        assert_eq!(s.arch, "qwen3moe");
        assert!(s.moe);
        assert_eq!(s.layers.len(), 2);
        assert_eq!(s.layers[0].experts, 1300 * MIB);
        // attention 18 MiB + norm 4 KiB + router 512 KiB, each padded to 32.
        assert_eq!(s.layers[1].other, 18 * MIB + 4096 + 512 * 1024);
        assert_eq!(s.token_embd, 300 * MIB);
        assert_eq!(s.globals, 300 * MIB + 4096);
        assert_eq!(s.total, size - parse_header(&h).unwrap().data_start);
        // 4 kv heads x (128 + 128) x 2 bytes x 2 layers, at 32k (the model's 256k capped).
        assert_eq!(s.ctx, REF_CTX);
        assert_eq!(s.kv, 4 * 256 * 2 * 2 * REF_CTX);
    }

    #[test]
    fn a_downloaded_model_is_read_from_disk_with_its_shards() {
        let dir = tempfile::tempdir().unwrap();
        let (h, size) = gguf(
            &[("general.architecture", W::Str("qwen3moe")), ("qwen3moe.block_count", W::U32(1))],
            &[("token_embd.weight", MIB), ("blk.0.ffn_up_exps.weight", 2 * MIB), ("output.weight", MIB)],
        );
        let one = dir.path().join("m.gguf");
        write_sparse(&one, &h, size);
        let s = local_shape(&one).unwrap();
        assert_eq!(s, shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap());
        // A split model: the first shard brings the rest.
        let (h1, s1) = gguf(&[("general.architecture", W::Str("qwen3"))], &[("blk.0.attn_q.weight", MIB), ("per_layer_token_embd.weight", 3 * MIB)]);
        let (h2, s2) = gguf(&[("split.no", W::U32(1))], &[("blk.1.attn_q.weight", MIB)]);
        write_sparse(&dir.path().join("x-00001-of-00002.gguf"), &h1, s1);
        write_sparse(&dir.path().join("x-00002-of-00002.gguf"), &h2, s2);
        let s = local_shape(&dir.path().join("x-00001-of-00002.gguf")).unwrap();
        assert_eq!(s.layers.len(), 2);
        assert_eq!(s.token_embd, 3 * MIB, "the per-layer embedding stays in host memory");
        assert!(local_shape(&dir.path().join("missing.gguf")).is_err());
    }

    /// `header` padded with a hole to `size` bytes.
    pub(crate) fn write_sparse(path: &Path, header: &[u8], size: u64) {
        std::fs::write(path, header).unwrap();
        std::fs::OpenOptions::new().write(true).open(path).unwrap().set_len(size).unwrap();
    }

    #[test]
    fn kv_layouts_per_layer_hybrid_and_mla() {
        let kv = |kvs: &[(&str, Kv)]| {
            let m: HashMap<String, Kv> = kvs.iter().map(|(k, v)| (format!("a.{k}"), v.clone())).collect();
            kv_per_token(&|k: &str| m.get(&format!("a.{k}")), 4)
        };
        let base = [
            ("embedding_length", Kv::Uint(4096)),
            ("attention.head_count", Kv::Uint(32)),
        ];
        // No head_count_kv: MHA, key/value length from the embedding.
        assert_eq!(kv(&base), Some(32 * 256 * 2 * 4));
        // A per-layer array: recurrent layers count zero.
        let mut arr = base.to_vec();
        arr.push(("attention.head_count_kv", Kv::Arr(vec![Kv::Uint(0), Kv::Uint(8), Kv::Uint(0), Kv::Uint(8)])));
        assert_eq!(kv(&arr), Some(16 * 256 * 2));
        // Every 4th layer attends.
        let mut hybrid = base.to_vec();
        hybrid.push(("attention.head_count_kv", Kv::Uint(2)));
        hybrid.push(("full_attention_interval", Kv::Uint(4)));
        assert_eq!(kv(&hybrid), Some(2 * 256 * 2));
        // MLA: latent plus rope dims.
        let mla = [("attention.kv_lora_rank", Kv::Uint(512)), ("rope.dimension_count", Kv::Uint(64))];
        assert_eq!(kv(&mla), Some(576 * 2 * 4));
        assert_eq!(kv(&[]), None, "unknown layout: the fallback reserve");
    }

    #[test]
    fn split_shards_add_up_with_the_keys_from_the_first() {
        let (h1, s1) = gguf(
            &[
                ("general.architecture", W::Str("qwen3")),
                ("qwen3.block_count", W::U32(2)),
                ("qwen3.embedding_length", W::U32(1024)),
                ("qwen3.attention.head_count", W::U32(8)),
                ("qwen3.attention.head_count_kv", W::ArrU32(vec![8, 8])),
            ],
            &[("token_embd.weight", 64 * MIB), ("blk.0.attn_q.weight", 32 * MIB)],
        );
        let (h2, s2) = gguf(&[("split.no", W::U32(1))], &[("blk.1.attn_q.weight", 32 * MIB), ("output.weight", 64 * MIB)]);
        let s = shape_of(&[(parse_header(&h1).unwrap(), s1), (parse_header(&h2).unwrap(), s2)]).unwrap();
        assert!(!s.moe);
        assert_eq!(s.layers, [Layer { experts: 0, other: 32 * MIB }; 2]);
        assert_eq!(s.globals, 64 * MIB);
        assert_eq!(s.kv, 16 * 256 * 2 * REF_CTX);
        assert!(shape_of(&[(parse_header(&h2).unwrap(), s2)]).is_err(), "no architecture anywhere");
    }

    // ── machine ──

    fn dev(id: &str, name: &str, total: u64) -> DeviceOption {
        DeviceOption { id: id.into(), label: String::new(), name: name.into(), total_mib: total, free_mib: total }
    }

    /// This repo's test machine, as `llama-server --list-devices` reports it.
    fn probe() -> Vec<DeviceOption> {
        vec![
            dev("CUDA0", "NVIDIA GeForce RTX 4070 SUPER", 12281),
            dev("ROCm0", "AMD Radeon(TM) Graphics", 37078),
            dev("ROCm1", "AMD Radeon AI PRO R9700", 32624),
            dev("Vulkan0", "AMD Radeon(TM) Graphics", 48754),
            dev("Vulkan1", "NVIDIA GeForce RTX 4070 SUPER", 11997),
            dev("Vulkan2", "AMD Radeon AI PRO R9700", 32624),
        ]
    }

    #[test]
    fn the_pool_counts_each_card_once_without_the_igpu() {
        let g = gpu_pool(&probe());
        assert_eq!(
            g,
            [
                Gpu { id: "CUDA0".into(), name: "NVIDIA GeForce RTX 4070 SUPER".into(), total_mib: 12281 },
                // ROCm1: the iGPU took ROCm0, and ROCm, not Vulkan2, names it.
                Gpu { id: "ROCm1".into(), name: "AMD Radeon AI PRO R9700".into(), total_mib: 32624 },
            ]
        );
        assert_eq!(g[0].short_name(), "RTX 4070 SUPER");
        assert_eq!(g[1].short_name(), "AI PRO R9700");
        // Two identical cards stay two.
        let twin = [dev("CUDA0", "RTX 3090", 24576), dev("CUDA1", "RTX 3090", 24576), dev("Vulkan0", "RTX 3090", 24000), dev("Vulkan1", "RTX 3090", 24000)];
        let ids: Vec<String> = gpu_pool(&twin).into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["CUDA0", "CUDA1"]);
        // A card only Vulkan sees keeps its Vulkan id.
        assert_eq!(gpu_pool(&[dev("Vulkan0", "Intel(R) Arc(TM) A770 Graphics", 16000)])[0].id, "Vulkan0");
        // An APU alone keeps its iGPU; the CPU never counts.
        let apu = [dev("Vulkan0", "AMD Radeon 8060S Graphics", 98304), dev("CPU", "CPU", 65536)];
        assert_eq!(gpu_pool(&apu).len(), 1);
        for n in ["AMD Radeon(TM) Graphics", "AMD Radeon 780M Graphics", "AMD Radeon(TM) 890M", "Intel(R) UHD Graphics 770", "Intel(R) Arc(TM) Graphics"] {
            assert!(is_integrated(n), "{n}");
        }
        for n in ["AMD Radeon AI PRO R9700", "AMD Radeon RX 7900 XTX", "Intel(R) Arc(TM) A770 Graphics", "NVIDIA GeForce RTX 4070 SUPER"] {
            assert!(!is_integrated(n), "{n}");
        }
    }

    #[test]
    fn the_budget_keeps_each_card_in_the_split_order_of_its_os() {
        let b = budget_for(&probe(), Some(64 * GIB), true).unwrap();
        let ids: Vec<&str> = b.gpus.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["ROCm1", "CUDA0"], "Windows: the ROCm card first");
        assert_eq!(b.gpus[1].usable(), 12281 * MIB - GPU_RESERVE);
        assert_eq!(b.vram(), (12281 + 32624) * MIB - 2 * GPU_RESERVE);
        assert_eq!(b.ram, Some(60 * GIB));
        let linux = budget_for(&probe(), Some(64 * GIB), false).unwrap();
        let ids: Vec<&str> = linux.gpus.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["CUDA0", "ROCm1"], "Linux: the CUDA card first");
        // One backend: the probe's order stands.
        let twin = [dev("ROCm0", "R9700", 32624), dev("ROCm1", "W7900", 49152)];
        let ids: Vec<String> = budget_for(&twin, None, true).unwrap().gpus.into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["ROCm0", "ROCm1"]);
        assert!(budget(&[dev("CPU", "CPU", 1000)], Some(GIB)).is_none(), "no GPU, no estimate");
        assert!(budget(&[], None).is_none());
        let basis = basis_label(&b);
        assert!(
            basis.starts_with(
                "Estimated for 2 GPUs in split order, AMD Radeon AI PRO R9700 (ROCm1, 29.9 GiB usable) then NVIDIA GeForce RTX 4070 SUPER (CUDA0, 10.0 GiB usable), and 64.0 GiB of RAM"
            ),
            "{basis}"
        );
    }

    // ── estimate ──

    /// A card with `usable_gib` to fill once the reserve is kept.
    fn card(id: &str, name: &str, usable_gib: u64) -> Gpu {
        Gpu { id: id.into(), name: name.into(), total_mib: usable_gib * 1024 + GPU_RESERVE / MIB }
    }

    fn budget_of(vram_gib: u64, ram_gib: Option<u64>) -> Budget {
        Budget { gpus: vec![card("CUDA0", "G", vram_gib)], ram: ram_gib.map(|r| r * GIB), ram_total: ram_gib.map(|r| r * GIB) }
    }

    /// This machine's two cards, Windows order: 29 and 10 GiB usable.
    fn pair(ram_gib: Option<u64>) -> Budget {
        Budget {
            gpus: vec![card("ROCm1", "AMD Radeon AI PRO R9700", 29), card("CUDA0", "NVIDIA GeForce RTX 4070 SUPER", 10)],
            ram: ram_gib.map(|r| r * GIB),
            ram_total: ram_gib.map(|r| r * GIB),
        }
    }

    /// Every share within its card, the positions all accounted for.
    fn check_shares(e: &Estimate, positions: u32) {
        for sh in &e.shares {
            assert!(sh.bytes <= sh.gpu.usable(), "{sh:?}");
        }
        assert_eq!(e.shares.iter().map(|s| s.positions).sum::<u32>(), positions, "{e:?}");
    }

    /// 48 MoE layers of 0.5 GiB experts + 32 MiB rest, 0.5 GiB embedding,
    /// 0.5 GiB output, 1 GiB KV (a 30B-A3B at about Q5): 26.5 GiB of weights,
    /// a 3 GiB base (all but the embedding and the experts, plus the KV).
    fn moe_shape() -> Shape {
        let layers = vec![Layer { experts: GIB / 2, other: 32 * MIB }; 48];
        let total = GIB / 2 + GIB / 2 + layers.iter().map(|l| l.experts + l.other).sum::<u64>();
        Shape { arch: "qwen3moe".into(), moe: true, total, token_embd: GIB / 2, globals: GIB / 2, layers, kv: GIB, ctx: REF_CTX }
    }

    fn dense_shape() -> Shape {
        let layers = vec![Layer { experts: 0, other: GIB / 2 }; 32];
        Shape { arch: "qwen3".into(), moe: false, total: 17 * GIB, token_embd: GIB / 2, globals: GIB / 2, layers, kv: 4 * GIB, ctx: REF_CTX }
    }

    #[test]
    fn a_dense_model_fits_or_splits_by_layers() {
        let s = dense_shape();
        let fits = estimate(&s, &budget_of(24, Some(32)));
        assert_eq!(fits, Estimate::plain(Fit::Gpu { need: 20 * GIB + GIB / 2, budget: 24 * GIB }, Some(0), vec![]));
        assert_eq!(estimate_label(&fits, REF_CTX), "all on GPU: 20.5 of 24.0 GiB with a 32k context");
        // 10 GiB: output 0.5, then layers of 0.5 + 0.125 KV: 15 of them.
        let split = estimate(&s, &budget_of(10, Some(32)));
        assert_eq!(split.fit, Fit::Dense { gpu_layers: 15, layers: 32, ram: 17 * GIB - GIB / 2 - 15 * GIB / 2 });
        assert_eq!(split.ram_short, Some(0));
        assert!(estimate_label(&split, REF_CTX).starts_with("too big for the GPU: 15/32 layers on GPU, 9.0 GiB in RAM (slow)"));
        // One card: nothing to write when it fits, the fit sizes the context.
        assert!(fits.shares.is_empty());
        assert_eq!(placement(&fits, &s), None);
        // Spilled: the positions (15 blocks and the output layer) at the
        // estimate's context, no device to name.
        let p = placement(&split, &s).unwrap();
        assert_eq!(
            p,
            Placement {
                n_gpu_layers: Some(16),
                ctx_size: Some(REF_CTX as i32),
                note: "estimated at download for a 32k context with 2.0 GiB kept free per GPU; a starting point to tune (optimize-preset)".into(),
                ..Placement::default()
            }
        );
        // Not even the output layer: left to the fit.
        let none = estimate(&s, &budget_of(0, Some(32)));
        assert_eq!(none.fit, Fit::Dense { gpu_layers: 0, layers: 32, ram: 17 * GIB });
        assert_eq!(placement(&none, &s), None);
    }

    #[test]
    fn two_unequal_cards_hold_a_dense_model_alone_or_cut_per_card() {
        let s = dense_shape();
        // 20.5 GiB: the R9700 alone, named so the 4070 stays free.
        let alone = estimate(&s, &pair(Some(64)));
        assert!(matches!(alone.fit, Fit::Gpu { .. }));
        check_shares(&alone, 33);
        assert_eq!(alone.shares[0].gpu.id, "ROCm1");
        assert_eq!(estimate_label(&alone, REF_CTX), "all on GPU with a 32k context: AI PRO R9700 (ROCm1) 20.5 GiB");
        let p = placement(&alone, &s).unwrap();
        assert_eq!((p.device.as_str(), p.tensor_split.as_str(), p.ctx_size, p.device_draft.as_str()), ("ROCm1", "", None, ""));
        assert!(p.note.starts_with("estimated at download: AI PRO R9700 holds the whole model, the fit sizes the context"), "{}", p.note);

        // 48 blocks of 0.875 GiB with their KV and a 1 GiB output layer: 43
        // GiB, past the 39 GiB of both cards. Per card the R9700 takes 33
        // blocks (28.9 of 29), the 4070 10 blocks and the output (9.75 of
        // 10): 44 positions; one more block fits neither.
        let big = Shape {
            arch: "qwen3".into(),
            moe: false,
            total: GIB / 2 + GIB + 48 * (3 * GIB / 4),
            token_embd: GIB / 2,
            globals: GIB,
            layers: vec![Layer { experts: 0, other: 3 * GIB / 4 }; 48],
            kv: 6 * GIB,
            ctx: REF_CTX,
        };
        let e = estimate(&big, &pair(Some(64)));
        assert_eq!(e.fit, Fit::Dense { gpu_layers: 43, layers: 48, ram: GIB / 2 + 5 * (3 * GIB / 4) });
        check_shares(&e, 44);
        assert_eq!(e.shares.iter().map(|s| s.positions).collect::<Vec<_>>(), [33, 11]);
        assert_eq!(
            estimate_label(&e, REF_CTX),
            "too big for the GPU: 43/48 layers on GPU (AI PRO R9700 (ROCm1) 28.9 GiB + RTX 4070 SUPER (CUDA0) 9.8 GiB, tensor-split 33,11), 4.2 GiB in RAM (slow)"
        );
        let p = placement(&e, &big).unwrap();
        assert_eq!(
            (p.device.as_str(), p.tensor_split.as_str(), p.n_gpu_layers, p.n_cpu_moe, p.ctx_size),
            ("ROCm1,CUDA0", "33,11", Some(44), None, Some(REF_CTX as i32))
        );
    }

    #[test]
    fn three_cards_share_a_dense_model_evenly() {
        let s = dense_shape();
        let b = Budget { gpus: vec![card("CUDA0", "A", 8), card("CUDA1", "A", 8), card("CUDA2", "A", 8)], ram: Some(60 * GIB), ram_total: Some(64 * GIB) };
        let e = estimate(&s, &b);
        assert!(matches!(e.fit, Fit::Gpu { .. }));
        check_shares(&e, 33);
        // 11 blocks each, the output layer on the last: 6.9, 6.9, 6.8 GiB.
        assert_eq!(e.shares.iter().map(|s| s.positions).collect::<Vec<_>>(), [11, 11, 11]);
        let p = placement(&e, &s).unwrap();
        assert_eq!(
            (p.device.as_str(), p.tensor_split.as_str(), p.ctx_size, p.device_draft.as_str()),
            ("CUDA0,CUDA1,CUDA2", "11,11,11", Some(REF_CTX as i32), "")
        );
        // A separate drafter: counted on the last card and pinned there.
        let e = estimate_with(&s, &b, &drafter(3 * GIB / 4));
        check_shares(&e, 33);
        let p = placement(&e, &s).unwrap();
        assert_eq!(p.device_draft, "CUDA2");
        assert!(p.note.contains("; counting the DFlash drafter (1.5 GiB) on CUDA2;"), "{}", p.note);
    }

    #[test]
    fn an_moe_puts_experts_of_the_last_layers_on_gpu() {
        let s = moe_shape();
        // A 3 GiB base in 8 GiB leaves 5 GiB: the experts of 10 layers.
        let e = estimate(&s, &budget_of(8, Some(32)));
        assert_eq!(
            e.fit,
            Fit::Moe { gpu_layers: 10, moe_layers: 48, n_cpu_moe: 38, gpu_experts: 5 * GIB, ram: GIB / 2 + 19 * GIB }
        );
        assert_eq!(e.ram_short, Some(0));
        assert_eq!(
            estimate_label(&e, REF_CTX),
            "MoE split: experts of 10/48 layers on GPU (5.0 GiB), 19.5 GiB in RAM, --n-cpu-moe 38"
        );
        // RAM short: the rest pages from disk.
        let short = estimate(&s, &budget_of(8, Some(16)));
        assert_eq!(short.ram_short, Some(3 * GIB + GIB / 2));
        assert!(estimate_label(&short, REF_CTX).ends_with(", 3.5 GiB over the RAM: the rest pages from disk"));
        // RAM unknown is said, not assumed.
        assert!(estimate_label(&estimate(&s, &budget_of(8, None)), REF_CTX).ends_with(", RAM size unknown"));
        // Not even the base fits.
        let tiny = estimate(&s, &budget_of(2, Some(64)));
        assert!(matches!(tiny.fit, Fit::TooBig { .. }));
        assert!(estimate_label(&tiny, REF_CTX).starts_with("too big: attention and shared weights alone need"));
        // Room for the base only: every expert in RAM.
        let none = estimate(&s, &budget_of(3, Some(64)));
        assert!(matches!(none.fit, Fit::Moe { gpu_layers: 0, n_cpu_moe: 48, .. }));
        assert!(estimate_label(&none, REF_CTX).starts_with("MoE split: attention on GPU, every expert in RAM"));
        // Everything fits: no split at all.
        assert!(matches!(estimate(&s, &budget_of(40, Some(64))).fit, Fit::Gpu { .. }));

        // The keys: n-cpu-moe at the estimate's context; a RAM shortfall is
        // written anyway (the GPU part is the best there is) and said.
        let p = placement(&e, &s).unwrap();
        assert_eq!((p.n_cpu_moe, p.n_gpu_layers, p.ctx_size, p.device.as_str()), (Some(38), None, Some(REF_CTX as i32), ""));
        let p = placement(&short, &s).unwrap();
        assert_eq!(p.n_cpu_moe, Some(38));
        assert!(p.note.contains(", 3.5 GiB over the RAM (the rest pages from disk);"), "{}", p.note);
        assert!(placement(&estimate(&s, &budget_of(8, None)), &s).unwrap().note.contains(", RAM size unknown;"));
        assert_eq!(placement(&none, &s).unwrap().n_cpu_moe, Some(48));
        assert_eq!(placement(&tiny, &s), None, "too big: left to the fit");
    }

    #[test]
    fn an_moe_on_two_cards_keeps_every_block_on_gpu_and_the_last_experts() {
        let s = moe_shape();
        // 27 GiB, past the 14 GiB of a 10 + 4 GiB pair.
        let b = Budget {
            gpus: vec![card("ROCm1", "AMD Radeon AI PRO R9700", 10), card("CUDA0", "NVIDIA GeForce RTX 4070 SUPER", 4)],
            ram: Some(60 * GIB),
            ram_total: Some(64 * GIB),
        };
        let e = estimate(&s, &b);
        let Fit::Moe { gpu_layers, n_cpu_moe, .. } = e.fit else { panic!("{e:?}") };
        assert_eq!(gpu_layers + n_cpu_moe, 48);
        check_shares(&e, 49);
        // The smallest n-cpu-moe whose row still cuts into the two cards.
        assert!(pack(&positions(&s, n_cpu_moe as usize), &[10 * GIB, 4 * GIB]).is_some());
        assert!(pack(&positions(&s, n_cpu_moe as usize - 1), &[10 * GIB, 4 * GIB]).is_none());
        // Pooled, 3 GiB of base in 14 left room for 22 layers of experts; the
        // cut costs a layer: a block never straddles the two cards.
        assert_eq!(gpu_layers, 21);
        let label = estimate_label(&e, REF_CTX);
        assert!(label.starts_with("MoE split: experts of 21/48 layers on GPU (10.5 GiB), AI PRO R9700 (ROCm1) "), "{label}");
        assert!(label.ends_with(&format!(" in RAM, --n-cpu-moe {n_cpu_moe}")), "{label}");
        let p = placement(&e, &s).unwrap();
        let split = tensor_split(&e.shares);
        assert_eq!(
            (p.device.as_str(), p.tensor_split.as_str(), p.n_cpu_moe, p.n_gpu_layers, p.device_draft.as_str()),
            ("ROCm1,CUDA0", split.as_str(), Some(n_cpu_moe as i32), None, "")
        );
    }

    #[test]
    fn a_cut_never_straddles_two_cards() {
        // Three blocks of 4 in 6 + 6: 12 in total, but two blocks per cut.
        assert_eq!(pack(&[4, 4, 4], &[6, 6]), None);
        assert_eq!(pack(&[4, 4], &[6, 6]), Some(vec![1, 1]));
        // Greedy fills the first card; balanced spreads the slack.
        assert_eq!(pack(&[1, 1, 1, 1], &[10, 10]), Some(vec![4, 0]));
        assert_eq!(balanced(&[1, 1, 1, 1], &[10, 10]), Some(vec![2, 2]));
        assert_eq!(balanced(&[1, 1, 1], &[20, 10]), Some(vec![2, 1]));
        assert_eq!(balanced(&[5], &[4]), None);
    }

    #[test]
    fn leading_dense_layers_do_not_move_the_n_cpu_moe_boundary() {
        let mut s = moe_shape();
        s.layers[0].experts = 0; // a DeepSeek-style dense first layer
        s.total -= GIB / 2;
        let e = estimate(&s, &budget_of(8, Some(64)));
        assert!(matches!(e.fit, Fit::Moe { gpu_layers: 10, moe_layers: 47, n_cpu_moe: 38, .. }), "{e:?}");
    }

    // ── extras ──

    /// A DFlash drafter of `weights` bytes and no KV cache.
    fn drafter(weights: u64) -> Extras {
        Extras { mmproj: 0, drafter: Some(("DFlash drafter".into(), DraftShape::from_size(weights))) }
    }

    /// An image encoder of `size` bytes.
    fn mmproj(size: u64) -> Extras {
        Extras { mmproj: size, drafter: None }
    }

    /// 44 dense blocks of 0.5 GiB plus 0.125 of KV and a 0.5 GiB output
    /// layer: 28 GiB on the GPU, which the R9700 (29 usable) holds alone.
    fn full_r9700() -> Shape {
        Shape {
            arch: "qwen3".into(),
            moe: false,
            total: GIB + 22 * GIB,
            token_embd: GIB / 2,
            globals: GIB / 2,
            layers: vec![Layer { experts: 0, other: GIB / 2 }; 44],
            kv: 44 * GIB / 8,
            ctx: REF_CTX,
        }
    }

    fn gpu_spot(x: &Option<Extra>) -> (&str, bool) {
        match x.as_ref().map(|x| &x.spot) {
            Some(Spot::Gpu { gpu, pinned }) => (gpu.id.as_str(), *pinned),
            other => panic!("not on a GPU: {other:?}"),
        }
    }

    #[test]
    fn the_image_encoder_counts_on_the_first_device() {
        let s = dense_shape();
        // 20.5 GiB plus 2.5 (2 of weights, 0.5 of compute) on the R9700 alone.
        let e = estimate_with(&s, &pair(Some(64)), &mmproj(2 * GIB));
        assert_eq!(e.fit, Fit::Gpu { need: 23 * GIB, budget: 39 * GIB });
        assert_eq!(gpu_spot(&e.encoder), ("ROCm1", false));
        assert_eq!(e.encoder.as_ref().unwrap().bytes, 2 * GIB + MMPROJ_COMPUTE);
        assert_eq!(
            estimate_label(&e, REF_CTX),
            "all on GPU with a 32k context: AI PRO R9700 (ROCm1) 20.5 GiB; with the image encoder (2.5 GiB) on ROCm1"
        );
        let p = placement(&e, &s).unwrap();
        assert_eq!((p.device.as_str(), p.mmproj_device.as_str(), p.mmproj_offload), ("ROCm1", "", None));
        assert!(p.note.contains("; counting the image encoder (2.5 GiB) on ROCm1;"), "{}", p.note);
        // One card: the encoder beside the model, nothing to write.
        let one = estimate_with(&s, &budget_of(24, Some(32)), &mmproj(2 * GIB));
        assert_eq!(gpu_spot(&one.encoder), ("CUDA0", false));
        assert_eq!(placement(&one, &s), None);
    }

    #[test]
    fn a_full_card_moves_the_image_encoder_rather_than_the_model() {
        let s = full_r9700();
        // 28 + 2.5 is past the R9700's 29: the model stays alone on it and
        // the encoder goes to the 4070, pinned there.
        let e = estimate_with(&s, &pair(Some(64)), &mmproj(2 * GIB));
        assert!(matches!(e.fit, Fit::Gpu { .. }), "{e:?}");
        assert_eq!(e.shares.len(), 1);
        assert_eq!(e.shares[0].gpu.id, "ROCm1");
        assert_eq!(gpu_spot(&e.encoder), ("CUDA0", true));
        let p = placement(&e, &s).unwrap();
        assert_eq!((p.device.as_str(), p.tensor_split.as_str(), p.mmproj_device.as_str()), ("ROCm1", "", "CUDA0"));
        // One card it no longer fits beside: the encoder on the CPU, the
        // model whole on the GPU; it costs RAM and says so.
        let one = estimate_with(&s, &budget_of(29, Some(32)), &mmproj(2 * GIB));
        assert!(matches!(one.fit, Fit::Gpu { need, .. } if need == 28 * GIB), "{one:?}");
        assert_eq!(one.encoder.as_ref().unwrap().spot, Spot::Cpu);
        assert!(estimate_label(&one, REF_CTX).ends_with("; with the image encoder (2.0 GiB) on the CPU"));
        let p = placement(&one, &s).unwrap();
        assert_eq!((p.device.as_str(), p.mmproj_offload, p.ctx_size), ("", Some(false), None));
        assert!(p.note.starts_with("estimated at download: the GPU holds the whole model"), "{}", p.note);
        // RAM too small for the encoder as well: said.
        let tight = estimate_with(&s, &budget_of(29, Some(1)), &mmproj(2 * GIB));
        assert_eq!(tight.ram_short, Some(GIB));
    }

    #[test]
    fn an_moe_keeps_its_encoder_on_the_gpu_at_the_cost_of_expert_layers() {
        let s = moe_shape();
        // 8 GiB: 38 layers of experts in RAM alone; 1.5 GiB of encoder takes
        // the room of three more.
        let e = estimate_with(&s, &budget_of(8, Some(32)), &mmproj(GIB));
        assert!(matches!(e.fit, Fit::Moe { n_cpu_moe: 41, .. }), "{e:?}");
        assert_eq!(gpu_spot(&e.encoder), ("CUDA0", false));
        assert_eq!(placement(&e, &s).unwrap().n_cpu_moe, Some(41));
        // Not even the base beside it: the CPU, and the base still on the GPU.
        let e = estimate_with(&s, &budget_of(4, Some(32)), &mmproj(GIB));
        assert!(matches!(e.fit, Fit::Moe { gpu_layers: 2, .. }), "{e:?}");
        assert_eq!(e.encoder.as_ref().unwrap().spot, Spot::Cpu);
        assert_eq!(placement(&e, &s).unwrap().mmproj_offload, Some(false));
    }

    #[test]
    fn a_drafter_counts_on_the_last_card_and_shifts_the_split() {
        // 52 blocks: 33 GiB, past the R9700 alone, so split over both.
        let s = Shape { layers: vec![Layer { experts: 0, other: GIB / 2 }; 52], kv: 52 * GIB / 8, ..full_r9700() };
        let plain = estimate(&s, &pair(Some(64)));
        let with = estimate_with(&s, &pair(Some(64)), &drafter(3 * GIB / 4));
        check_shares(&with, 53);
        assert_eq!(gpu_spot(&with.drafter), ("CUDA0", true));
        // The 4070 hosts 1.5 GiB of drafter: it takes fewer positions.
        assert!(with.shares[1].positions < plain.shares[1].positions, "{plain:?} {with:?}");
        assert!(with.shares[1].bytes + with.drafter.as_ref().unwrap().bytes <= with.shares[1].gpu.usable());
        let p = placement(&with, &s).unwrap();
        assert_eq!(p.device_draft, "CUDA0");
        assert_ne!(p.tensor_split, placement(&plain, &s).unwrap().tensor_split);
    }

    #[test]
    fn a_drafter_stays_beside_a_model_alone_or_goes_to_another_card() {
        let s = dense_shape();
        // One card: beside the model, nothing pinned.
        let one = estimate_with(&s, &budget_of(24, Some(32)), &drafter(3 * GIB / 4));
        assert_eq!(one.fit, Fit::Gpu { need: 22 * GIB, budget: 24 * GIB });
        assert_eq!(gpu_spot(&one.drafter), ("CUDA0", false));
        assert_eq!(placement(&one, &s), None);
        // The R9700 holds the model and the drafter: no pin.
        let both = estimate_with(&s, &pair(Some(64)), &drafter(3 * GIB / 4));
        assert_eq!(gpu_spot(&both.drafter), ("ROCm1", false));
        assert_eq!(placement(&both, &s).unwrap().device_draft, "");
        // A model that fills it: the drafter goes to the 4070, pinned.
        let s = full_r9700();
        let e = estimate_with(&s, &pair(Some(64)), &drafter(3 * GIB / 4));
        assert_eq!(e.shares.len(), 1, "{e:?}");
        assert_eq!(gpu_spot(&e.drafter), ("CUDA0", true));
        let p = placement(&e, &s).unwrap();
        assert_eq!((p.device.as_str(), p.device_draft.as_str()), ("ROCm1", "CUDA0"));
    }

    #[test]
    fn a_drafter_header_counts_its_window_not_the_context() {
        // A DFlash drafter as Qwen3.8-27B's: 5 blocks, 8 KV heads of 128,
        // every layer windowed at 2048.
        let (bytes, size) = gguf(
            &[
                ("general.architecture", W::Str("dflash")),
                ("dflash.block_count", W::U32(5)),
                ("dflash.embedding_length", W::U32(5120)),
                ("dflash.attention.head_count", W::U32(32)),
                ("dflash.attention.head_count_kv", W::U32(8)),
                ("dflash.attention.key_length", W::U32(128)),
                ("dflash.attention.value_length", W::U32(128)),
                ("dflash.attention.sliding_window", W::U32(2048)),
                ("dflash.attention.sliding_window_pattern", W::ArrBool(vec![true; 5])),
            ],
            &[("token_embd.weight", 4096 * 256), ("blk.0.attn_q.weight", 4096 * 512), ("output.weight", 4096 * 256)],
        );
        let h = parse_header(&bytes).unwrap();
        let d = draft_shape_of(&[(h.clone(), size)]).unwrap();
        assert_eq!((d.kv_full, d.kv_swa, d.swa_cells), (0, 5 * 8 * 256 * 2, 2560));
        // 50 MiB of KV at any context past the window, as measured at f16.
        assert_eq!(d.bytes(32768) - d.weights - DRAFT_COMPUTE, 50 * MIB);
        assert_eq!(d.bytes(32768), d.bytes(262144));
        // Its tensors but the input embeddings (host memory).
        let shape = shape_of(&[(h, size)]).unwrap();
        assert_eq!(d.weights, shape.total - shape.token_embd);
        assert!(shape.token_embd > 0);
        // Without the pattern: every layer at the full context.
        let (bytes, size) = gguf(
            &[
                ("general.architecture", W::Str("qwen3")),
                ("qwen3.block_count", W::U32(1)),
                ("qwen3.embedding_length", W::U32(256)),
                ("qwen3.attention.head_count", W::U32(4)),
                ("qwen3.attention.head_count_kv", W::U32(1)),
            ],
            &[("blk.0.attn_q.weight", 4096 * 16)],
        );
        let d = draft_shape_of(&[(parse_header(&bytes).unwrap(), size)]).unwrap();
        assert_eq!((d.kv_full, d.kv_swa), (2 * 64 * 2, 0));
        assert_eq!(d.bytes(1000), d.weights + 256 * 1000 + DRAFT_COMPUTE);
    }

    #[test]
    fn the_extras_follow_the_choices_and_an_embedded_mtp_counts_once() {
        let f = |p: &str, size: u64| hf::RepoFile { path: p.into(), size, oid: String::new() };
        let c = Candidate {
            tag: "Q4_K_M".into(),
            shards: vec![f("m.gguf", 10)],
            size: 10,
            mmproj: Some(f("mmproj.gguf", 7)),
            mtp: None,
            dflash: Some(f("dflash.gguf", 5)),
            dspark: None,
        };
        let known = DraftShape { weights: 4, kv_full: 1, ..DraftShape::default() };
        let read = |_: &hf::RepoFile| Some(Ok(Arc::new(known)));
        let x = extras_of(&c, true, Some(hf::Sidecar::Dflash), &read).unwrap();
        assert_eq!(x, Extras { mmproj: 7, drafter: Some(("DFlash drafter".into(), known)) });
        assert_eq!(extras_of(&c, false, None, &read).unwrap(), Extras::default());
        // The drafter's header not read yet: no estimate yet; unreadable: its size.
        assert_eq!(extras_of(&c, true, Some(hf::Sidecar::Dflash), &|_| None), None);
        let failed = extras_of(&c, true, Some(hf::Sidecar::Dflash), &|_| Some(Err("boom".into()))).unwrap();
        assert_eq!(failed.drafter.unwrap().1, DraftShape::from_size(5));
        // An MTP head inside the model file: no MTP FILE, so nothing extra;
        // its block is in the model's shape and counts there once.
        let x = extras_of(&c, false, Some(hf::Sidecar::Mtp), &read).unwrap();
        assert_eq!(x, Extras::default());
        let mut s = dense_shape();
        s.layers.push(Layer { experts: 0, other: GIB / 4 }); // the nextn block
        let b = budget_of(24, Some(32));
        assert_eq!(estimate_with(&s, &b, &x), estimate(&s, &b));
    }

    #[test]
    fn counting_the_extras_flips_the_recommendation() {
        let s = |gib: u64| Shape {
            arch: "qwen3".into(),
            moe: false,
            total: gib * GIB,
            token_embd: 0,
            globals: 0,
            layers: vec![Layer { experts: 0, other: gib * GIB / 4 }; 4],
            kv: GIB,
            ctx: REF_CTX,
        };
        let b = budget_of(12, Some(32));
        let x = Extras { mmproj: GIB, drafter: Some(("DFlash drafter".into(), DraftShape::from_size(GIB / 4))) };
        // Without extras the Q8_0 (10 GiB) fits; with 1.5 + 1 GiB of extras
        // it no longer does, and the Q4_K_M (6 GiB) is the one.
        let (q4, q8) = (estimate(&s(5), &b), estimate(&s(9), &b));
        let rows = [Row { tag: "Q4_K_M", size: 5, estimate: Some(&q4) }, Row { tag: "Q8_0", size: 9, estimate: Some(&q8) }];
        assert_eq!(recommend(&rows).unwrap().index, 1);
        let (q4, q8) = (estimate_with(&s(5), &b, &x), estimate_with(&s(9), &b, &x));
        assert_eq!(gpu_spot(&q4.encoder), ("CUDA0", false));
        // The Q8_0 keeps the drafter and moves the encoder to the CPU.
        assert_eq!(q8.encoder.as_ref().unwrap().spot, Spot::Cpu);
        let rows = [Row { tag: "Q4_K_M", size: 5, estimate: Some(&q4) }, Row { tag: "Q8_0", size: 9, estimate: Some(&q8) }];
        assert_eq!(recommend(&rows), Some(Recommendation { index: 0, reason: "the best quality that fits in GPU memory" }));
        // Only the Q8_0 listed: tier 2, said so.
        assert_eq!(
            recommend(&rows[1..]).unwrap().reason,
            "the best quality that fits in GPU memory with the image encoder on the CPU"
        );
    }

    // ── recommendation ──

    fn gpu() -> Estimate {
        Estimate::plain(Fit::Gpu { need: 1, budget: 2 }, Some(0), vec![])
    }

    fn moe_ram(short: u64) -> Estimate {
        Estimate::plain(Fit::Moe { gpu_layers: 1, moe_layers: 2, n_cpu_moe: 1, gpu_experts: 1, ram: 1 }, Some(short), vec![])
    }

    #[test]
    fn the_best_quality_on_gpu_is_recommended() {
        let (g, m) = (gpu(), moe_ram(0));
        let rows = [
            Row { tag: "Q4_K_M", size: 18, estimate: Some(&g) },
            Row { tag: "Q8_0", size: 32, estimate: Some(&m) },
            Row { tag: "Q5_K_S", size: 21, estimate: Some(&g) },
            Row { tag: "IQ4_XS", size: 16, estimate: Some(&g) },
        ];
        let r = recommend(&rows).unwrap();
        assert_eq!(r, Recommendation { index: 2, reason: "the best quality that fits in GPU memory" }, "GPU tier beats a bigger quant in RAM");
    }

    #[test]
    fn quality_orders_k_over_i_over_legacy_and_caps_at_eight_bits() {
        let q = |t: &str| quality(t, 1);
        assert!(q("Q4_K_M") > q("IQ4_XS"));
        assert!(q("IQ4_XS") > q("Q4_0"));
        assert!(q("Q4_K_M") > q("Q4_K_S"));
        assert!(q("Q4_K_XL") > q("Q4_K_L"));
        assert!(q("IQ2_XS") > q("IQ2_XXS"));
        assert!(q("Q8_0") > q("Q6_K"));
        assert!(q("Q8_0") > q("BF16"), "16 bits only when nothing else fits");
        assert!(q("IQ3_XXS") > q("Q2_K"), "more bits first");
        assert!(q("Q4_K_M") > q("model"));
        let g = gpu();
        let only16 = [Row { tag: "BF16", size: 2, estimate: Some(&g) }, Row { tag: "F32", size: 4, estimate: None }];
        assert_eq!(recommend(&only16).unwrap().index, 0);
    }

    #[test]
    fn an_moe_recommends_experts_in_ram_without_paging_or_nothing() {
        let (ok, pages) = (moe_ram(0), moe_ram(5));
        let dense_split = Estimate::plain(Fit::Dense { gpu_layers: 1, layers: 2, ram: 1 }, Some(0), vec![]);
        let rows = [
            Row { tag: "Q2_K", size: 1, estimate: Some(&ok) },
            Row { tag: "Q4_K_M", size: 2, estimate: Some(&ok) },
            Row { tag: "Q8_0", size: 3, estimate: Some(&pages) },
        ];
        assert_eq!(
            recommend(&rows),
            Some(Recommendation { index: 1, reason: "the best quality whose experts fit in RAM without paging" })
        );
        let none = [Row { tag: "Q8_0", size: 3, estimate: Some(&pages) }, Row { tag: "Q4_K_M", size: 1, estimate: Some(&dense_split) }];
        assert_eq!(recommend(&none), None);
        assert_eq!(recommend(&[Row { tag: "Q4_K_M", size: 1, estimate: None }]), None, "nothing known, nothing picked");
    }

    // ── remote ──

    /// Serves one file's bytes by range, counting the requests.
    struct Remote {
        file: Vec<u8>,
        calls: AtomicU32,
        lens: Mutex<Vec<u64>>,
        status: u16,
    }

    impl Fetch for Remote {
        fn get(&self, _url: &str, _token: Option<&Token>) -> Result<(u16, String), HfError> {
            unreachable!()
        }
        fn get_range(&self, url: &str, _token: Option<&Token>, start: u64, len: u64) -> Result<(u16, Vec<u8>), HfError> {
            assert!(url.starts_with("https://hf.test/o/r/resolve/"), "{url}");
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.lens.lock().unwrap().push(len);
            let end = ((start + len) as usize).min(self.file.len());
            Ok((self.status, self.file[start as usize..end].to_vec()))
        }
    }

    #[test]
    fn a_header_longer_than_the_first_range_is_fetched_again_larger() {
        // A tokenizer big enough to outgrow the first request.
        let tokens: Vec<String> = (0..700_000).map(|i| format!("tok{i}")).collect();
        let (mut h, size) = gguf(
            &[
                ("general.architecture", W::Str("qwen3")),
                ("tokenizer.ggml.tokens", W::ArrStr(tokens.iter().map(String::as_str).collect())),
            ],
            &[("blk.0.attn_q.weight", MIB)],
        );
        assert!(h.len() as u64 > INITIAL_FETCH);
        h.resize(size as usize, 0);
        let remote = Remote { file: h, calls: AtomicU32::new(0), lens: Mutex::new(Vec::new()), status: 206 };
        let url = "https://hf.test/o/r/resolve/c/m.gguf";
        let hdr = fetch_header_from(&remote, url, None, "o/r", size, INITIAL_FETCH).unwrap();
        assert_eq!(hdr.tensors.len(), 1);
        assert_eq!(*remote.lens.lock().unwrap(), [INITIAL_FETCH, (4 * INITIAL_FETCH).min(size)]);
        // A file shorter than its header claims is cut short, not fetched forever.
        let cut = Remote { file: remote.file[..1000].to_vec(), calls: AtomicU32::new(0), lens: Mutex::new(Vec::new()), status: 206 };
        assert!(matches!(fetch_header_from(&cut, url, None, "o/r", size, INITIAL_FETCH), Err(HfError::Io(_))));
        assert_eq!(cut.calls.load(Ordering::Relaxed), 1);
        // An error status is the API's error.
        let gated = Remote { file: b"{\"error\":\"gated\"}".to_vec(), calls: AtomicU32::new(0), lens: Mutex::new(Vec::new()), status: 403 };
        assert!(matches!(fetch_header_from(&gated, url, None, "o/r", 0, MIB), Err(HfError::Gated { .. })));
    }

    #[test]
    fn shapes_are_fetched_in_parallel_and_cached() {
        // The header alone (the fixture's data would be 3 GiB): it parses
        // from the first answer whatever its length.
        let (h, size) = moe_fixture();
        let remote = Remote { file: h, calls: AtomicU32::new(0), lens: Mutex::new(Vec::new()), status: 206 };
        let cands: Vec<Candidate> = (0..6)
            .map(|i| Candidate {
                tag: format!("Q{i}_K"),
                shards: vec![RepoFile { path: format!("dir/m Q{i}_K.gguf"), size, oid: String::new() }],
                size,
                mmproj: None,
                mtp: None,
                dflash: None,
                dspark: None,
            })
            .collect();
        let got = Mutex::new(Vec::new());
        let commit = "e".repeat(40);
        let run = || {
            fetch_shapes(&remote, "https://hf.test/", "o/r", &commit, &cands, None, &|| true, &|i, r| {
                got.lock().unwrap().push((i, r.map(|s| s.layers.len())));
            })
        };
        run();
        let mut g = got.lock().unwrap().clone();
        g.sort_by_key(|x| x.0);
        assert_eq!(g, (0..6).map(|i| (i, Ok(2))).collect::<Vec<_>>());
        assert_eq!(remote.calls.load(Ordering::Relaxed), 6);
        run();
        assert_eq!(remote.calls.load(Ordering::Relaxed), 6, "a second listing reads the cache");
        // A space in a path is encoded.
        assert_eq!(resolve_url("e/", "o/r", "c", "dir/m Q4.gguf"), "e/o/r/resolve/c/dir/m%20Q4.gguf");
        // A stop request ends the run early.
        let none = Mutex::new(0);
        let other: Vec<Candidate> = cands.iter().map(|c| Candidate { tag: "x".into(), ..c.clone() }).collect();
        fetch_shapes(&remote, "https://hf.test/", "o/r", "f", &other, None, &|| false, &|_, _| *none.lock().unwrap() += 1);
        assert_eq!(*none.lock().unwrap(), 0);
    }
}
