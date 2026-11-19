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
    ["up", "down", "gate", "gate_up"].iter().any(|k| {
        name.contains(&format!(".ffn_{k}_exps")) || name.contains(&format!(".ffn_{k}_chexps"))
    })
}

/// Fold the headers of a model's shards (first shard first, each with the
/// shard's file size) into a `Shape` (pure). The key/values come from the
/// shard that has `general.architecture` (the first, for gguf-split files).
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
        globals: 0,
        layers: Vec::new(),
        kv: 0,
        ctx: 0,
    };
    for (h, file_size) in shards {
        for (name, bytes) in tensor_bytes(h, *file_size)? {
            s.total += bytes;
            if is_input_embedding(name) {
                s.token_embd += bytes;
            } else if let Some(b) = block_of(name) {
                if b >= 4096 {
                    return Err(strings::fill("block-one", &[&b]));
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
    s.ctx = int("context_length")
        .filter(|&c| c > 0)
        .map_or(REF_CTX, |c| c.min(REF_CTX));
    s.kv = kv_per_token(&key, s.layers.len() as u64).map_or(FALLBACK_KV, |b| b * s.ctx);
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

/// KV bytes per token (f16, all attention layers), from the attention keys;
/// `None` when the layout is not readable.
pub fn kv_per_token<'k>(key: &dyn Fn(&str) -> Option<&'k Kv>, blocks: u64) -> Option<u64> {
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
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    // ── a GGUF writer for the fixtures ──
    //
    // A copy of `pai-app`'s `hf_fit::tests::gguf`: a test module imports no
    // other test module, and the shapes both crates check are the same bytes.

    pub enum W<'a> {
        U32(u32),
        U64(u64),
        Str(&'a str),
        ArrU32(Vec<u32>),
        ArrStr(Vec<&'a str>),
    }

    fn put_str(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    /// A GGUF v3 file's header for `kvs` and `tensors` (name, bytes), and the
    /// whole file's size (offsets laid out as llama.cpp does: each tensor
    /// aligned to 32).
    pub fn gguf(kvs: &[(&str, W)], tensors: &[(&str, u64)]) -> (Vec<u8>, u64) {
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
                (
                    "tokenizer.ggml.tokens",
                    W::ArrStr(vec!["<|im_start|>", "hello", "world"]),
                ),
                ("general.file_type", W::U64(15)),
            ],
            &t,
        )
    }

    /// `header` padded with a hole to `size` bytes.
    pub fn write_sparse(path: &Path, header: &[u8], size: u64) {
        std::fs::write(path, header).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_len(size)
            .unwrap();
    }

    #[test]
    fn a_header_parses_keys_and_tensor_offsets() {
        let (h, size) = moe_fixture();
        let hdr = parse_header(&h).unwrap();
        assert_eq!(hdr.kvs["general.architecture"], Kv::Str("qwen3moe".into()));
        assert_eq!(hdr.kvs["qwen3moe.block_count"], Kv::Uint(2));
        assert_eq!(
            hdr.kvs["tokenizer.ggml.tokens"],
            Kv::Skipped,
            "string arrays are skipped"
        );
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
            assert_eq!(
                parse_header(&h[..cut]),
                Err(ParseError::Truncated),
                "cut at {cut}"
            );
        }
        assert!(matches!(
            parse_header(b"GGML\x03\0\0\0"),
            Err(ParseError::Invalid(_))
        ));
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
            &[
                ("general.architecture", W::Str("qwen3moe")),
                ("qwen3moe.block_count", W::U32(1)),
            ],
            &[
                ("token_embd.weight", MIB),
                ("blk.0.ffn_up_exps.weight", 2 * MIB),
                ("output.weight", MIB),
            ],
        );
        let one = dir.path().join("m.gguf");
        write_sparse(&one, &h, size);
        let s = local_shape(&one).unwrap();
        assert_eq!(s, shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap());
        // A split model: the first shard brings the rest.
        let (h1, s1) = gguf(
            &[("general.architecture", W::Str("qwen3"))],
            &[
                ("blk.0.attn_q.weight", MIB),
                ("per_layer_token_embd.weight", 3 * MIB),
            ],
        );
        let (h2, s2) = gguf(&[("split.no", W::U32(1))], &[("blk.1.attn_q.weight", MIB)]);
        write_sparse(&dir.path().join("x-00001-of-00002.gguf"), &h1, s1);
        write_sparse(&dir.path().join("x-00002-of-00002.gguf"), &h2, s2);
        let s = local_shape(&dir.path().join("x-00001-of-00002.gguf")).unwrap();
        assert_eq!(s.layers.len(), 2);
        assert_eq!(
            s.token_embd,
            3 * MIB,
            "the per-layer embedding stays in host memory"
        );
        assert!(local_shape(&dir.path().join("missing.gguf")).is_err());
    }

    /// The bytes of a header declaring one string key per entry of `sizes`,
    /// each key's descriptor written where the reader will be once the previous
    /// string's hole lies behind it: the declared sizes are holes, so the
    /// header's own bytes are a few hundred of them, spread over its whole
    /// span. Every name here is 12 bytes, so a descriptor is 32 (8 of string
    /// length, 12 of name, 4 of type, 8 of declared size) and the header ends
    /// `24 + sum(32 + size)` bytes into the file.
    fn header_of_chained_strings(sizes: &[u64]) -> Vec<u8> {
        let mut h = b"GGUF\x03\0\0\0".to_vec();
        h.extend_from_slice(&0u64.to_le_bytes());
        h.extend_from_slice(&(sizes.len() as u64).to_le_bytes());
        for (i, size) in sizes.iter().enumerate() {
            let name = format!("general.key{i}");
            let at = 24 + 32 * i as u64 + sizes[..i].iter().sum::<u64>();
            h.resize(usize::try_from(at).unwrap(), 0);
            put_str(&mut h, &name);
            h.extend_from_slice(&8u32.to_le_bytes());
            h.extend_from_slice(&size.to_le_bytes());
        }
        h
    }

    /// How long a refusal may take to come back: a call that blocks (a path
    /// opened for reading with no writer to answer it) is a timeout here, never
    /// a hung suite. Every caller is a unix test (a FIFO), so the constant is
    /// unix too: the Windows type check would call it dead code.
    #[cfg(unix)]
    const REFUSAL_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

    /// `what` on a thread: the message its `Err` carries, answered within
    /// `bound`. Panics when it blocks, when it accepts, when it panics. Unix
    /// only, like every caller.
    #[cfg(unix)]
    fn refused_within_bound(
        what: impl FnOnce() -> Result<(), String> + Send + 'static,
        bound: std::time::Duration,
    ) -> String {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(what().err());
        });
        match rx.recv_timeout(bound) {
            Ok(Some(m)) => m,
            Ok(None) => panic!("the path was accepted as a model file"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("the call blocked: no answer in {} s", bound.as_secs())
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("the call panicked")
            }
        }
    }

    #[test]
    fn a_header_beyond_max_header_is_refused() {
        // The cap the read of a file's header stops at: no real header comes
        // close, so a file whose header runs past it is refused, not read for.
        // Two strings of the largest size the reader takes, the second ending
        // 88 bytes past the cap.
        assert_eq!(MAX_HEADER, 128 * MIB);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("long.gguf");
        let h = header_of_chained_strings(&[super::MAX_STRING, super::MAX_STRING]);
        write_sparse(&p, &h, MAX_HEADER + 4 * MIB);
        let e = local_shape(&p).expect_err("a header past MAX_HEADER is refused");
        assert!(
            e.contains("cut short"),
            "the refusal says the header was cut short, not that the file is gone: {e}"
        );
    }

    #[test]
    fn a_header_that_ends_at_the_cap_is_read() {
        // The read grows 8, 32 and 128 MiB. A header that ends inside the last
        // of those reads is a whole header: it is read, not refused as cut
        // short. Only a header running past `MAX_HEADER` is (the test above).
        // Each 32 bytes of descriptor sit after the hole before it, so the two
        // strings end 120 bytes under the cap, and exactly at it.
        for (label, sizes, data_start) in [
            (
                "under the cap",
                [super::MAX_STRING, super::MAX_STRING - 200],
                MAX_HEADER - 96,
            ),
            (
                "at the cap",
                [super::MAX_STRING, super::MAX_STRING - 88],
                MAX_HEADER,
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("fills-the-read.gguf");
            let size = MAX_HEADER + 4 * MIB;
            write_sparse(&p, &header_of_chained_strings(&sizes), size);
            let hdr = read_local_header(&p, size).unwrap_or_else(|e| {
                panic!("{label}: a header ending inside the cap is a whole header, not a cut short one: {e}")
            });
            assert_eq!(
                hdr.kvs.len(),
                2,
                "{label}: both keys of the header are there"
            );
            assert_eq!(
                hdr.data_start, data_start,
                "{label}: the tensor data starts where the header ends"
            );
            match &hdr.kvs["general.key1"] {
                Kv::Str(s) => assert_eq!(
                    s.len(),
                    usize::try_from(sizes[1]).unwrap(),
                    "{label}: the second key sits where the reader looks for it"
                ),
                v => panic!("{label}: the second key is a string, not {v:?}"),
            }
        }
    }

    #[test]
    fn local_shape_refuses_a_path_that_is_not_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        // A folder is not a model file, and reading one is not a thing.
        let e = local_shape(dir.path()).expect_err("a directory is refused");
        assert!(
            e.contains(&dir.path().display().to_string()),
            "the refusal names the path it refused: {e}"
        );
        // A FIFO: opening it for reading blocks until a writer comes, so the
        // path is refused BEFORE it is opened. Bounded: a blocked read is a
        // timeout here, never a hung suite.
        #[cfg(unix)]
        {
            let fifo = dir.path().join("pipe.gguf");
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .expect("mkfifo");
            let (tx, rx) = std::sync::mpsc::channel();
            let target = fifo.clone();
            std::thread::spawn(move || {
                let _ = tx.send(local_shape(&target).err());
            });
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(Some(m)) => assert!(
                    m.contains(&fifo.display().to_string()),
                    "the refusal names the path it refused: {m}"
                ),
                Ok(None) => panic!("the FIFO was accepted as a model file"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("local_shape opened the FIFO and blocked reading it")
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("local_shape panicked on the FIFO")
                }
            }
        }
    }

    /// A symlink is followed: the Hugging Face cache keeps a model as a symlink
    /// to a blob, so the link is the model file. `std::os::unix::fs::symlink`
    /// has no Windows twin, so Windows runs none of these three.
    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_model_file_is_read_as_that_file() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let (h, size) = gguf(
            &[
                ("general.architecture", W::Str("qwen3")),
                ("qwen3.block_count", W::U32(1)),
            ],
            &[("token_embd.weight", MIB), ("output.weight", MIB)],
        );
        let blob = dir.path().join("blob.gguf");
        write_sparse(&blob, &h, size);
        let link = dir.path().join("model.gguf");
        symlink(&blob, &link).expect("symlink");
        let s = local_shape(&link).expect("a symlink to a model file is that file");
        assert_eq!(
            s,
            shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap(),
            "the link reads like the file it points at"
        );
    }

    /// A link is followed, and what it leads to is checked: a folder is not a
    /// model file. Guard: green today, the fix must keep it.
    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_folder_is_refused() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("folder.gguf");
        symlink(dir.path(), &link).expect("symlink");
        let want = link.clone();
        let m = refused_within_bound(move || local_shape(&link).map(|_| ()), REFUSAL_BOUND);
        assert!(
            m.contains(&want.display().to_string()),
            "the refusal names the path it refused: {m}"
        );
    }

    /// A symlink to a FIFO: the link is refused, naming its own path, and
    /// never opened for reading (a blocked open is a timeout, not a hung
    /// suite). Guard: green today, the fix must keep it.
    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_fifo_is_refused_without_opening_it() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe.blob");
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        let link = dir.path().join("pipe.gguf");
        symlink(&fifo, &link).expect("symlink");
        let want = link.clone();
        let m = refused_within_bound(move || local_shape(&link).map(|_| ()), REFUSAL_BOUND);
        assert!(
            m.contains(&want.display().to_string()),
            "the refusal names the path it refused: {m}"
        );
    }

    /// The open must refuse a FIFO by itself, not only the type check before
    /// it: a path that was a regular file at the stat can be a FIFO when it is
    /// opened. Pinned on the open step directly, the swap the test cannot make
    /// happen. Guard: green today, and the fix must keep it.
    #[cfg(unix)]
    #[test]
    fn the_open_refuses_a_fifo_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe.gguf");
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        let want = fifo.clone();
        let m = refused_within_bound(move || open_regular(&fifo).map(|_| ()), REFUSAL_BOUND);
        assert!(
            m.contains(&want.display().to_string()),
            "the open refused the FIFO naming its path: {m}"
        );
    }

    /// The open step alone, with no stat in front of it: `open_regular` refuses
    /// a FIFO before it opens, so a FIFO of `local_shape` says nothing about
    /// what the open itself does. Here the refusal can only come from the open:
    /// with `O_NONBLOCK` the open of a FIFO with no writer answers at once and
    /// the opened file is not a regular file; without the flag the open never
    /// answers, and the bound reads the block as a failure. Bounded, so the
    /// missing flag is a red, never a hung suite.
    #[cfg(unix)]
    #[test]
    fn the_open_itself_refuses_a_fifo_that_the_nonblocking_open_answers() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe.gguf");
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        let want = fifo.clone();
        let m = refused_within_bound(
            move || open_nonblocking_regular(&fifo).map(|_| ()),
            REFUSAL_BOUND,
        );
        assert!(
            m.contains(&want.display().to_string()),
            "the open refused the FIFO naming its path: {m}"
        );
    }

    #[test]
    fn kv_layouts_per_layer_hybrid_and_mla() {
        let kv = |kvs: &[(&str, Kv)]| {
            let m: HashMap<String, Kv> = kvs
                .iter()
                .map(|(k, v)| (format!("a.{k}"), v.clone()))
                .collect();
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
        arr.push((
            "attention.head_count_kv",
            Kv::Arr(vec![Kv::Uint(0), Kv::Uint(8), Kv::Uint(0), Kv::Uint(8)]),
        ));
        assert_eq!(kv(&arr), Some(16 * 256 * 2));
        // Every 4th layer attends.
        let mut hybrid = base.to_vec();
        hybrid.push(("attention.head_count_kv", Kv::Uint(2)));
        hybrid.push(("full_attention_interval", Kv::Uint(4)));
        assert_eq!(kv(&hybrid), Some(2 * 256 * 2));
        // MLA: latent plus rope dims.
        let mla = [
            ("attention.kv_lora_rank", Kv::Uint(512)),
            ("rope.dimension_count", Kv::Uint(64)),
        ];
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
            &[
                ("token_embd.weight", 64 * MIB),
                ("blk.0.attn_q.weight", 32 * MIB),
            ],
        );
        let (h2, s2) = gguf(
            &[("split.no", W::U32(1))],
            &[
                ("blk.1.attn_q.weight", 32 * MIB),
                ("output.weight", 64 * MIB),
            ],
        );
        let s = shape_of(&[
            (parse_header(&h1).unwrap(), s1),
            (parse_header(&h2).unwrap(), s2),
        ])
        .unwrap();
        assert!(!s.moe);
        assert_eq!(
            s.layers,
            [Layer {
                experts: 0,
                other: 32 * MIB
            }; 2]
        );
        assert_eq!(s.globals, 64 * MIB);
        assert_eq!(s.kv, 16 * 256 * 2 * REF_CTX);
        assert!(
            shape_of(&[(parse_header(&h2).unwrap(), s2)]).is_err(),
            "no architecture anywhere"
        );
    }

    #[test]
    fn the_gpu_reserve_is_what_an_estimate_leaves_free() {
        // The estimate may not count this on any card; a check built on it
        // (the manager's, and `hf_fit`'s) agrees on one number.
        assert_eq!(GPU_RESERVE, 2 * GIB);
    }
}
