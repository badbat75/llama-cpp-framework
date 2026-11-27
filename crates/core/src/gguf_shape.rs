//! What a GGUF model file is made of, read from its header: the shape of the
//! weights (every tensor's bytes, the input embedding tables, each block's
//! routed experts and the rest) and the KV cache per token. `pai-app`'s
//! `hf_fit` places a model on the GPUs with it; the manager reads a model
//! profile's file with it to check the profile against the machine.
//!
//! The two ways to get a header: `parse_header` on a buffer already read, and
//! `local_shape` on a file on disk (a split model's first shard brings the
//! rest). A path a client wrote is opened as root, so `local_shape` follows a
//! symlink and reads a regular file only: no FIFO (opening one for reading
//! waits for a writer), no directory, no device, and the link's target is what
//! the check sees. The read of the header is capped at `MAX_HEADER`, which
//! only stops the reads from growing: a header that parses is returned, and a
//! file whose header is still cut short at the cap is refused, not read for.
//!
//! `GPU_RESERVE` is the memory kept free on every GPU, what an estimate may
//! not count.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::strings;

pub const GIB: u64 = 1 << 30;
pub const MIB: u64 = 1 << 20;

/// What the estimate keeps free on every card (module header).
pub const GPU_RESERVE: u64 = 2 * GIB;
/// The context the KV cache is costed at (module header).
pub const REF_CTX: u64 = 32 * 1024;
/// The KV reservation for a model whose KV layout this reader cannot read.
const FALLBACK_KV: u64 = 2 * GIB;

/// The most of a file this reader reads for a header: a larger read than this
/// is a corrupt entry count, not a big model.
pub const MAX_HEADER: u64 = 128 * MIB;
/// The largest string length a corrupt header can make this ask for.
const MAX_STRING: u64 = 64 * MIB;

// ── Header ───────────────────────────────────────────────────────────────

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
    pub fn as_u64(&self) -> Option<u64> {
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
    /// The buffer ended inside the header: read more.
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
        let n = usize::try_from(n)
            .map_err(|_| ParseError::Invalid(strings::t("gguf-length-beyond-memory")))?;
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
        let n = self.u64()?;
        if n > MAX_STRING {
            return Err(ParseError::Invalid(strings::fill(
                "gguf-string-too-long",
                &[&n],
            )));
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
            2 => Kv::Uint(u64::from(u16::from_le_bytes(
                self.take(2)?.try_into().unwrap(),
            ))),
            3 => Kv::Int(i64::from(i16::from_le_bytes(
                self.take(2)?.try_into().unwrap(),
            ))),
            4 => Kv::Uint(u64::from(self.u32()?)),
            5 => Kv::Int(i64::from(self.u32()? as i32)),
            6 => Kv::Float(f64::from(f32::from_le_bytes(
                self.take(4)?.try_into().unwrap(),
            ))),
            7 => Kv::Bool(self.take(1)?[0] != 0),
            8 => Kv::Str(self.string()?),
            9 => {
                if depth > 2 {
                    return Err(ParseError::Invalid(strings::t("gguf-arrays-too-deep")));
                }
                let et = self.u32()?;
                let n = self.u64()?;
                if n > 1 << 28 {
                    return Err(ParseError::Invalid(strings::fill(
                        "gguf-array-too-long",
                        &[&n],
                    )));
                }
                match (et, fixed_size(et)) {
                    // Small numeric arrays (per-layer head counts) are kept.
                    (_, Some(_)) if n <= 4096 => Kv::Arr(
                        (0..n)
                            .map(|_| self.value(et, depth + 1))
                            .collect::<Result<_, _>>()?,
                    ),
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
                    _ => {
                        return Err(ParseError::Invalid(strings::fill(
                            "gguf-array-element-type",
                            &[&et],
                        )))
                    }
                }
            }
            10 => Kv::Uint(self.u64()?),
            11 => Kv::Int(self.u64()? as i64),
            12 => Kv::Float(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            _ => return Err(ParseError::Invalid(strings::fill("gguf-value-type", &[&t]))),
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
        return Err(ParseError::Invalid(strings::t("gguf-no-magic")));
    }
    let version = r.u32()?;
    if !matches!(version, 2 | 3) {
        return Err(ParseError::Invalid(strings::fill(
            "gguf-version",
            &[&version],
        )));
    }
    let n_tensors = r.u64()?;
    let n_kv = r.u64()?;
    if n_tensors > 1 << 20 || n_kv > 1 << 20 {
        return Err(ParseError::Invalid(strings::fill(
            "gguf-too-many-entries",
            &[&n_tensors, &n_kv],
        )));
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
            return Err(ParseError::Invalid(strings::fill(
                "gguf-tensor-dimensions",
                &[&name, &n_dims],
            )));
        }
        r.skip(u64::from(n_dims) * 8)?;
        let _type = r.u32()?;
        let offset = r.u64()?;
        tensors.push((name, offset));
    }
    let align = kvs
        .get("general.alignment")
        .and_then(Kv::as_u64)
        .filter(|&a| a > 0)
        .unwrap_or(32);
    let data_start = (r.pos as u64).div_ceil(align) * align;
    Ok(Header {
        kvs,
        tensors,
        data_start,
    })
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
    /// The file has an `output.weight` tensor: the output head is its own
    /// tensor. A model without one outputs through its input embedding and
    /// llama.cpp loads a SECOND copy of that tensor for the output layer
    /// (`TENSOR_DUPLICATED`, `llama-model-loader.cpp`), which the GPUs carry:
    /// `full_gpu_bytes` subtracts `token_embd` only when this is true.
    pub output_head: bool,
    /// The tensors outside any block but the input embeddings (output head,
    /// norms): the output layer's position.
    pub globals: u64,
    pub layers: Vec<Layer>,
    /// The KV cache at `ctx` tokens, f16.
    pub kv: u64,
    pub ctx: u64,
    /// The context the model was trained on, uncapped (`ctx` carries the
    /// capped one): `<arch>.context_length`, 0 when the header has none.
    pub trained_ctx: u64,
    /// The f16 KV bytes of one token (`kv_per_token`); `None` when the layout
    /// cannot be read, the Shape does not decide what that costs.
    pub kv_f16_per_token: Option<u64>,
    /// The blocks of the model: `<arch>.block_count`, else the number of
    /// `blk.N` blocks the tensor names mention (the GUI's sliders run over it).
    pub block_count: u64,
    /// `<arch>.expert_count`, 0 when the header has none.
    pub expert_count: u64,
    /// `<arch>.expert_used_count`, 0 when the header has none.
    pub expert_used_count: u64,
}

/// `blk.N.` → N.
fn block_of(name: &str) -> Option<usize> {
    name.strip_prefix("blk.")?.split('.').next()?.parse().ok()
}

/// The number of `blk.N` blocks the tensor names of every shard mention: one
/// past the greatest `N` (the tensor names of a quantized model are a
/// contiguous run from `blk.0`).
fn named_block_count(shards: &[(Header, u64)]) -> u64 {
    shards
        .iter()
        .flat_map(|(h, _)| h.tensors.iter().map(|(n, _)| n.as_str()))
        .filter_map(block_of)
        .max()
        .map_or(0, |n| n as u64 + 1)
}

/// An input embedding table, which llama.cpp keeps in host memory whatever
/// the offload (`LLM_TENSOR_LAYER_INPUT`, `llama-arch.cpp`).
fn is_input_embedding(name: &str) -> bool {
    matches!(name, "token_embd.weight" | "per_layer_token_embd.weight")
}

/// A routed-expert tensor: what `--n-cpu-moe` moves (`LLM_FFN_EXPS_REGEX`,
/// `\.ffn_(up|down|gate|gate_up)_(ch|)exps`). A shared expert (`_shexp`) is not.
fn is_expert(name: &str) -> bool {
    ["up", "down", "gate", "gate_up"].iter().any(|k| {
        name.contains(&format!(".ffn_{k}_exps")) || name.contains(&format!(".ffn_{k}_chexps"))
    })
}

/// The output head tensor. A file without one outputs through its input
/// embedding, and llama.cpp then loads a second copy of that tensor for the
/// output layer (`TENSOR_DUPLICATED`, `llama-model-loader.cpp`), which goes
/// on the output layer's device.
fn is_output_head(name: &str) -> bool {
    name == "output.weight"
}

/// Fold the headers of a model's shards (first shard first, each with the
/// shard's file size) into a `Shape` (pure). The key/values come from the
/// shard that has `general.architecture` (the first, for gguf-split files).
/// The sums saturate: a header declaring more than a u64 of tensors is a
/// number at the ceiling, never a panic or a wrap.
pub fn shape_of(shards: &[(Header, u64)]) -> Result<Shape, String> {
    let kvs = shards
        .iter()
        .map(|(h, _)| &h.kvs)
        .find(|k| k.contains_key("general.architecture"))
        .ok_or_else(|| strings::t("gguf-no-arch"))?;
    let arch = match kvs.get("general.architecture") {
        Some(Kv::Str(s)) => s.clone(),
        _ => return Err(strings::t("gguf-arch-not-string")),
    };
    let mut s = Shape {
        arch: arch.clone(),
        moe: false,
        total: 0,
        token_embd: 0,
        output_head: false,
        globals: 0,
        layers: Vec::new(),
        kv: 0,
        ctx: 0,
        trained_ctx: 0,
        kv_f16_per_token: None,
        block_count: 0,
        expert_count: 0,
        expert_used_count: 0,
    };
    for (h, file_size) in shards {
        for (name, bytes) in tensor_bytes(h, *file_size)? {
            s.total = s.total.saturating_add(bytes);
            if is_input_embedding(name) {
                s.token_embd = s.token_embd.saturating_add(bytes);
            } else if let Some(b) = block_of(name) {
                if b >= 4096 {
                    return Err(strings::fill("block-one", &[&b]));
                }
                if s.layers.len() <= b {
                    s.layers.resize(b + 1, Layer::default());
                }
                if is_expert(name) {
                    s.layers[b].experts = s.layers[b].experts.saturating_add(bytes);
                    s.moe = true;
                } else {
                    s.layers[b].other = s.layers[b].other.saturating_add(bytes);
                }
            } else {
                if is_output_head(name) {
                    s.output_head = true;
                }
                s.globals = s.globals.saturating_add(bytes);
            }
        }
    }
    let key = |k: &str| kvs.get(&format!("{arch}.{k}"));
    let int = |k: &str| key(k).and_then(Kv::as_u64);
    s.trained_ctx = int("context_length").unwrap_or(0);
    s.block_count = int("block_count").unwrap_or_else(|| named_block_count(shards));
    s.expert_count = int("expert_count").unwrap_or(0);
    s.expert_used_count = int("expert_used_count").unwrap_or(0);
    s.ctx = int("context_length")
        .filter(|&c| c > 0)
        .map_or(REF_CTX, |c| c.min(REF_CTX));
    let per_token = kv_per_token(&key, s.layers.len() as u64);
    s.kv_f16_per_token = per_token;
    s.kv = per_token.map_or(FALLBACK_KV, |b| b.saturating_mul(s.ctx));
    Ok(s)
}

/// Each tensor of one shard with its bytes, in data order: from its offset to
/// the next tensor's, the last one to the end of the file.
pub fn tensor_bytes(h: &Header, file_size: u64) -> Result<Vec<(&str, u64)>, String> {
    let mut offsets: Vec<(u64, &str)> = h.tensors.iter().map(|(n, o)| (*o, n.as_str())).collect();
    offsets.sort();
    let data = file_size
        .checked_sub(h.data_start)
        .ok_or_else(|| strings::fill("gguf-shard-too-small", &[&file_size, &h.data_start]))?;
    Ok(offsets
        .iter()
        .enumerate()
        .map(|(i, (off, name))| {
            let end = offsets.get(i + 1).map_or(data, |n| n.0);
            (*name, end.saturating_sub(*off))
        })
        .collect())
}

/// The cap of a tensor list (J1 of "Layers, MoE and tensor placement on the
/// web model profile page", `ARCHITECTURE.md` 5): the web page places weights
/// from the names, and a model with more tensors is placed by its first ones.
pub const MAX_TENSOR_ROWS: usize = 20_000;

/// The byte bound of the same list (J1 of "Layers, MoE and tensor placement on
/// the web model profile page", `ARCHITECTURE.md` 5): the rows' JSON, each row
/// counted as `serde_json::to_string` spells a `{"name","bytes"}` row, plus the
/// `,` that follows it. A tensor name has no length bound in this reader and
/// JSON escaping grows it, and the pipe's frame is bounded
/// (`client::pipe::MAX_FRAME`), so the list is cut by bytes as well as rows.
pub const MAX_TENSOR_LIST_BYTES: usize = 917_504;

/// One row of a tensor list, counted the way the reply spells it: the JSON of a
/// `{name, bytes}` row (`client::pipe` writes exactly this), plus the `,` that
/// follows it. Serializing a name and a count cannot fail, so the text is empty
/// when it does, which costs one byte of the bound.
fn row_bytes(name: &str, bytes: u64) -> usize {
    let row = serde_json::to_string(&serde_json::json!({ "name": name, "bytes": bytes }))
        .unwrap_or_default();
    row.len() + 1
}

/// The tensor list of a shard set: every tensor's name (the GGUF header's own)
/// and bytes, each shard's tensors in data-offset order (`tensor_bytes`), shard
/// after shard. The list is cut at the FIRST row that would pass either bound,
/// [`MAX_TENSOR_ROWS`] rows or [`MAX_TENSOR_LIST_BYTES`] of the rows' JSON, and
/// the flag says the model has more tensors than the list carries. The row cap
/// is inclusive: a model of exactly that many tensors, within the bytes,
/// answers the whole list with the flag false.
pub fn tensor_rows(shards: &[(Header, u64)]) -> Result<(Vec<(String, u64)>, bool), String> {
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    let mut truncated = false;
    'shards: for (h, file_size) in shards {
        for (name, size) in tensor_bytes(h, *file_size)? {
            let row = row_bytes(name, size);
            if rows.len() >= MAX_TENSOR_ROWS || bytes + row > MAX_TENSOR_LIST_BYTES {
                truncated = true;
                break 'shards;
            }
            bytes += row;
            rows.push((name.to_string(), size));
        }
    }
    Ok((rows, truncated))
}

/// KV bytes per token (f16, all attention layers), from the attention keys;
/// `None` when the layout is not readable (no head count, or no key/value
/// length and no embedding length to take them from). The products saturate:
/// a header declaring a layout past a u64 counts gives `u64::MAX`, never a
/// panic.
pub fn kv_per_token<'k>(key: &dyn Fn(&str) -> Option<&'k Kv>, blocks: u64) -> Option<u64> {
    let int = |k: &str| key(k).and_then(Kv::as_u64);
    let n_layer = int("block_count").unwrap_or(blocks);
    if let Some(rank) = int("attention.kv_lora_rank") {
        // MLA: the compressed latent plus the rope part, one cache.
        let rope = int("rope.dimension_count").unwrap_or(64);
        return Some(
            rank.saturating_add(rope)
                .saturating_mul(2)
                .saturating_mul(n_layer),
        );
    }
    let n_embd = int("embedding_length");
    let n_head = match key("attention.head_count")? {
        Kv::Arr(a) => a.iter().filter_map(Kv::as_u64).max()?,
        v => v.as_u64()?,
    };
    if n_head == 0 {
        return None;
    }
    // The head's key/value length, the model width over the head count when
    // the header gives neither.
    let head_len = |k: &str| int(k).or_else(|| n_embd.map(|e| e / n_head));
    let k_len = head_len("attention.key_length")?;
    let v_len = head_len("attention.value_length")?;
    let per_head = k_len.saturating_add(v_len).saturating_mul(2);
    match key("attention.head_count_kv") {
        Some(Kv::Arr(a)) => Some(
            a.iter()
                .filter_map(Kv::as_u64)
                .fold(0u64, u64::saturating_add)
                .saturating_mul(per_head),
        ),
        other => {
            let hkv = other.and_then(Kv::as_u64).unwrap_or(n_head);
            let attn_layers = match int("full_attention_interval").filter(|&i| i > 1) {
                Some(i) => n_layer.div_ceil(i),
                None => n_layer,
            };
            Some(hkv.saturating_mul(per_head).saturating_mul(attn_layers))
        }
    }
}

/// What a KV cache type costs per 32 elements, in bytes (the block
/// `static_assert`s of `ggml/src/ggml-common.h`, and the type traits of
/// `ggml/src/ggml.c` for `f8_e4m3`). An unknown name costs f16: a preset's
/// cache types are content-checked against `checks::KV_CACHE_TYPES`, so a
/// name outside this list reaches here only from a caller that skipped it.
fn kv_type_bytes(t: &str) -> u64 {
    match t {
        "f32" => 128,
        "f16" | "bf16" => 64,
        "f8_e4m3" => 32,
        "q8_0" => 34,
        "q5_1" => 24,
        "q5_0" => 22,
        "q4_1" => 20,
        "q4_0" | "iq4_nl" => 18,
        _ => 64,
    }
}

/// The full-GPU estimate of a model (ARCHITECTURE.md (b) 2): what the preset
/// would put on the GPUs if every tensor llama.cpp can offload were there.
/// Every tensor of the model (`total`) less the input embedding tables
/// (`token_embd`, host memory whatever the placement) only when the model has
/// an `output.weight` of its own (`output_head`): a model without one outputs
/// through its embedding and llama.cpp loads a second copy of that tensor for
/// the output layer, so its whole `total` is the weights term. Plus the KV
/// cache of `ctx` tokens at `type_k` and `type_v`: `kv_f16_per_token` times
/// `ctx`, scaled by the mean of the two types' sizes relative to f16 (their
/// bytes per 32 elements over f16's). A layout `kv_per_token` could not read
/// costs `FALLBACK_KV`, `ctx` of 0 costs no KV, and the placement keys never
/// lower the result. `ctx` and a header's counts are numbers a client writes,
/// so the terms saturate: a product past a u64, divided last, is `u64::MAX`,
/// never a panic or a wrap.
pub fn full_gpu_bytes(s: &Shape, ctx: u64, type_k: &str, type_v: &str) -> u64 {
    let weights = if s.output_head {
        s.total.saturating_sub(s.token_embd)
    } else {
        s.total
    };
    let scale = kv_type_bytes(type_k) + kv_type_bytes(type_v);
    let kv = s.kv_f16_per_token.map_or(FALLBACK_KV, |per_token| {
        per_token
            .checked_mul(ctx)
            .and_then(|bytes| bytes.checked_mul(scale))
            .map_or(u64::MAX, |bytes| bytes / 128)
    });
    weights.saturating_add(kv)
}

// ── Shards ───────────────────────────────────────────────────────────────

/// A split model's file stem, `<base>-<counter>-of-<total>`, split into its
/// base and its counter: `m-Q4-00002-of-00003` gives `("m-Q4", "00002")`.
/// `None` for a name that is not one: the counter and the total are the five
/// digits llama.cpp writes, so a name with other digits is not a shard.
pub fn split_shard_suffix(stem: &str) -> Option<(&str, &str)> {
    let (head, total) = stem.rsplit_once("-of-")?;
    if total.len() != 5 || !total.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let idx = head.rfind('-')?;
    let counter = &head[idx + 1..];
    if counter.len() != 5 || !counter.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((&head[..idx], counter))
}

/// Every shard of the split model `path` belongs to, by name only (no disk
/// read): `m-00002-of-00003.gguf` gives `m-00001-of-00003.gguf` through
/// `m-00003-of-00003.gguf`, in the folder of `path` and with its extension's
/// spelling. Anything that is not a `-NNNNN-of-NNNNN` shard is a set of one.
pub fn shard_set(path: &Path) -> Vec<PathBuf> {
    let one = || vec![path.to_path_buf()];
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return one();
    };
    let Some(cut) = name.len().checked_sub(5) else {
        return one();
    };
    if !name.is_char_boundary(cut) || !name[cut..].eq_ignore_ascii_case(".gguf") {
        return one();
    }
    let (stem, ext) = (&name[..cut], &name[cut..]);
    let Some((base, _)) = split_shard_suffix(stem) else {
        return one();
    };
    // split_shard_suffix checked the 5 trailing digits.
    let total_text = &stem[stem.len() - 5..];
    match total_text.parse::<u32>() {
        Ok(total) if total > 0 => (1..=total)
            .map(|i| path.with_file_name(format!("{base}-{i:05}-of-{total_text}{ext}")))
            .collect(),
        _ => one(),
    }
}

// ── Local files ──────────────────────────────────────────────────────────

/// The first read of a file's header: enough for a normal header, small
/// enough to stay cheap on a slow disk. Each next read is four times as much,
/// up to `MAX_HEADER`.
const FIRST_READ: u64 = 8 * MIB;

/// `path` opened as a regular file, refusing what is not one BEFORE opening
/// it: a FIFO with no writer blocks an open forever, and a device returns
/// bytes no header check rejects (a block device reads its own raw contents,
/// a character device may block). A symlink is followed, so the check sees the
/// file the link names. A missing file keeps its own message, the one the file
/// pickers show. The open and its own check of the opened file are
/// `open_nonblocking_regular`.
fn open_regular(path: &Path) -> Result<(std::fs::File, u64), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.file_type().is_file() {
        return Err(format!(
            "{}: {}",
            path.display(),
            strings::t("gguf-not-file")
        ));
    }
    open_nonblocking_regular(path)
}

/// The open, and the check of what it opened: on unix the open carries
/// `O_NONBLOCK`, so no path can hold it, and the opened file is checked again,
/// because a path that was a regular file at an earlier stat can be a FIFO, a
/// folder or a device when it is opened. Its own length is what the read of a
/// header is bounded by: a file that shrank since that stat is read as it is
/// now. The refusal text is the one `open_regular` refuses with.
fn open_nonblocking_regular(path: &Path) -> Result<(std::fs::File, u64), String> {
    let refuse = || format!("{}: {}", path.display(), strings::t("gguf-not-file"));
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(
        &mut opts,
        nix::fcntl::OFlag::O_NONBLOCK.bits(),
    );
    let f = opts
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = f
        .metadata()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(refuse());
    }
    Ok((f, meta.len()))
}

/// One file's header, read in growing reads (`FIRST_READ`, then four times as
/// much, up to `MAX_HEADER`). The bytes a read asks for are bounded by the
/// length the OPENED file reports, not by `file_size` (the caller's earlier
/// `metadata`): a file that grew since that stat is read no further than it
/// had reached when it was opened.
///
/// A header is returned as soon as it parses. `MAX_HEADER` only stops the
/// reads from growing, so a header that is still cut short at the cap is
/// refused.
pub fn read_local_header(path: &Path, file_size: u64) -> Result<Header, String> {
    let (mut f, opened) = open_regular(path)?;
    let size = file_size.min(opened);
    let mut len = FIRST_READ.min(size);
    loop {
        let mut buf = Vec::new();
        f.seek(SeekFrom::Start(0))
            .and_then(|_| (&mut f).take(len).read_to_end(&mut buf))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        match parse_header(&buf) {
            Ok(h) => return Ok(h),
            Err(ParseError::Truncated) if len < MAX_HEADER && len < size => {
                len = (len * 4).min(MAX_HEADER).min(size);
            }
            Err(ParseError::Truncated) => {
                return Err(strings::fill("gguf-cut-short", &[&buf.len()]))
            }
            Err(ParseError::Invalid(m)) => return Err(strings::fill("gguf-unreadable", &[&m])),
        }
    }
}

/// The headers of a model's shard set, first shard first, each with the file
/// size the reader saw (`tensor_bytes` needs it for the last tensor's bytes).
pub fn read_shards(first: &Path) -> Result<Vec<(Header, u64)>, String> {
    let mut headers = Vec::new();
    for p in shard_set(first) {
        let size = std::fs::metadata(&p)
            .map_err(|e| format!("{}: {e}", p.display()))?
            .len();
        headers.push((read_local_header(&p, size)?, size));
    }
    Ok(headers)
}

/// The `Shape` of a model on disk: `first` and the rest of its shard set
/// (`shard_set`), each header read locally: every path is followed through a
/// symlink and must name a regular file. What the model profile of a
/// download is placed from (`hf::model_profile_from_download`).
pub fn local_shape(first: &Path) -> Result<Shape, String> {
    let mut headers = Vec::new();
    for p in shard_set(first) {
        let size = std::fs::metadata(&p)
            .map_err(|e| format!("{}: {e}", p.display()))?
            .len();
        headers.push((read_local_header(&p, size)?, size));
    }
    shape_of(&headers)
}

#[cfg(test)]
#[path = "gguf_shape_tests.rs"]
mod tests;
