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

/// One metadata value written the way a GGUF file carries it.
pub fn put_kv(out: &mut Vec<u8>, v: &W) {
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
            put_str(out, s);
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
                put_str(out, s);
            }
        }
    }
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
        put_kv(&mut out, v);
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

/// A GGUF v3 header for `kvs` and tensors declared as
/// `(name, dims, offset)`: the dims are what the file DECLARES (a corrupt
/// header declares more than a u64 can count), the offset where the reader
/// looks for the tensor. No file size: `shape_of` counts a tensor's bytes
/// from the offsets and the size the caller passes, never from the dims.
pub fn gguf_declared(kvs: &[(&str, W)], tensors: &[(&str, &[u64], u64)]) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
    out.extend_from_slice(&(kvs.len() as u64).to_le_bytes());
    for (k, v) in kvs {
        put_str(&mut out, k);
        put_kv(&mut out, v);
    }
    for (name, dims, off) in tensors {
        put_str(&mut out, name);
        out.extend_from_slice(&(dims.len() as u32).to_le_bytes());
        for d in dims.iter() {
            out.extend_from_slice(&d.to_le_bytes());
        }
        out.extend_from_slice(&12u32.to_le_bytes()); // Q4_K
        out.extend_from_slice(&off.to_le_bytes());
    }
    out
}

// ── The tied-output fixtures: the same qwen2 lookalike with and without
// an `output.weight` tensor (slice 10b-1's tied-embedding pins) ──
//
// One block, an embedding of 100 MiB, attention of 12 MiB, the final norm:
// f16 KV of 1024 bytes per token (4 kv heads x (64 + 64) x 2 bytes x 1
// block), a trained context of 32k, so 32 MiB of KV at `REF_CTX`.

/// The attention keys both fixtures share.
fn qwen2_kvs() -> Vec<(&'static str, W<'static>)> {
    vec![
        ("general.architecture", W::Str("qwen2")),
        ("qwen2.block_count", W::U32(1)),
        ("qwen2.context_length", W::U32(REF_CTX as u32)),
        ("qwen2.embedding_length", W::U32(1024)),
        ("qwen2.attention.head_count", W::U32(16)),
        ("qwen2.attention.head_count_kv", W::U32(4)),
        ("qwen2.attention.key_length", W::U32(64)),
        ("qwen2.attention.value_length", W::U32(64)),
    ]
}

/// A model whose file has NO `output.weight`: it uses `token_embd` as its
/// output head (tied embeddings).
fn tied_fixture() -> (Vec<u8>, u64) {
    gguf(
        &qwen2_kvs(),
        &[
            ("token_embd.weight", 100 * MIB),
            ("blk.0.attn_q.weight", 8 * MIB),
            ("blk.0.attn_k.weight", 2 * MIB),
            ("blk.0.attn_v.weight", 2 * MIB),
            ("output_norm.weight", 4096),
        ],
    )
}

/// The same model with its own output head: `output.weight`, 100 MiB.
fn untied_fixture() -> (Vec<u8>, u64) {
    gguf(
        &qwen2_kvs(),
        &[
            ("token_embd.weight", 100 * MIB),
            ("blk.0.attn_q.weight", 8 * MIB),
            ("blk.0.attn_k.weight", 2 * MIB),
            ("blk.0.attn_v.weight", 2 * MIB),
            ("output_norm.weight", 4096),
            ("output.weight", 100 * MIB),
        ],
    )
}

/// A header whose attention keys are the largest a u64 can hold: every
/// product of `kv_per_token` overflows, and the shape's KV of any context
/// overflows with it.
fn huge_kv_fixture() -> (Vec<u8>, u64) {
    gguf(
        &[
            ("general.architecture", W::Str("qwen3")),
            ("qwen3.block_count", W::U32(4)),
            ("qwen3.embedding_length", W::U64(u64::MAX)),
            ("qwen3.attention.head_count", W::U64(1)),
            ("qwen3.attention.key_length", W::U64(u64::MAX)),
            ("qwen3.attention.value_length", W::U64(1)),
            ("qwen3.attention.head_count_kv", W::U64(2)),
        ],
        &[
            ("token_embd.weight", MIB),
            ("blk.0.attn_q.weight", 2 * MIB),
            ("output.weight", MIB),
        ],
    )
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

/// The block count the model profile page's sliders run over (L1 of
/// "Layers, MoE and tensor placement on the web model profile page",
/// ARCHITECTURE.md (1)): the header's `<arch>.block_count` when it has one,
/// the blocks the tensor names mention when it has none. The header wins:
/// a file that ships only some of its blocks' tensors (a partial offload
/// fixture, a split whose last shard is absent from the read) is the model
/// the header describes, and llama.cpp cuts `block_count + 1` positions by
/// the `tensor-split` weights.
#[test]
fn the_block_count_is_the_headers_key_else_the_blocks_the_tensors_name() {
    // The header says 40, the tensors name 3 of them.
    let (h, size) = gguf(
        &[
            ("general.architecture", W::Str("qwen3moe")),
            ("qwen3moe.block_count", W::U32(40)),
        ],
        &[
            ("token_embd.weight", MIB),
            ("blk.0.ffn_up_exps.weight", MIB),
            ("blk.1.attn_q.weight", MIB),
            ("blk.39.ffn_down_exps.weight", MIB),
            ("output.weight", MIB),
        ],
    );
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(
        s.block_count, 40,
        "the header's key wins over the blocks the tensors name"
    );
    assert_eq!(
        s.layers.len(),
        40,
        "the layers fold is unchanged by the count: blocks 0..=39"
    );

    // No key: the blocks the tensors name, the highest of them + 1.
    let (h, size) = gguf(
        &[("general.architecture", W::Str("llama"))],
        &[
            ("token_embd.weight", MIB),
            ("blk.0.attn_q.weight", MIB),
            ("blk.4.attn_q.weight", MIB),
            ("output.weight", MIB),
        ],
    );
    assert_eq!(
        shape_of(&[(parse_header(&h).unwrap(), size)])
            .unwrap()
            .block_count,
        5,
        "a header with no key is counted from its tensors"
    );

    // A split model: a block named only by the second shard counts.
    let (h1, s1) = gguf(
        &[("general.architecture", W::Str("llama"))],
        &[("blk.0.attn_q.weight", MIB)],
    );
    let (h2, s2) = gguf(&[("split.no", W::U32(1))], &[("blk.7.attn_q.weight", MIB)]);
    assert_eq!(
        shape_of(&[
            (parse_header(&h1).unwrap(), s1),
            (parse_header(&h2).unwrap(), s2)
        ])
        .unwrap()
        .block_count,
        8,
        "the count is over every shard, as the layers fold is"
    );
}

/// The expert counts of the same reply, read off the header as the GUI's
/// `ModelInfo` reads them: `<arch>.expert_count` and
/// `<arch>.expert_used_count`, 0 when the header has none (a dense model
/// has no experts to count, and the page's MoE slider stays hidden).
#[test]
fn the_expert_counts_are_the_headers_keys_and_zero_when_absent() {
    let (h, size) = gguf(
        &[
            ("general.architecture", W::Str("qwen3moe")),
            ("qwen3moe.block_count", W::U32(48)),
            ("qwen3moe.expert_count", W::U32(128)),
            ("qwen3moe.expert_used_count", W::U32(8)),
        ],
        &[
            ("token_embd.weight", MIB),
            ("blk.0.ffn_gate_exps.weight", MIB),
            ("output.weight", MIB),
        ],
    );
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!((s.expert_count, s.expert_used_count), (128, 8));
    assert!(s.moe, "the routed-expert tensors are what sets `moe`");

    // A dense model, and a MoE header that names only one of the two: the
    // count that is absent is 0, never the other one's value.
    let (h, size) = gguf(
        &[
            ("general.architecture", W::Str("qwen3moe")),
            ("qwen3moe.expert_count", W::U32(64)),
        ],
        &[("blk.0.ffn_gate_exps.weight", MIB)],
    );
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(s.expert_count, 64);
    assert_eq!(s.expert_used_count, 0, "no key is no experts used");
    let (h, size) = gguf(
        &[
            ("general.architecture", W::Str("llama")),
            ("llama.block_count", W::U32(2)),
        ],
        &[("blk.0.attn_q.weight", MIB)],
    );
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(
        (s.expert_count, s.expert_used_count, s.moe),
        (0, 0, false),
        "a dense model has nothing to count"
    );
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

// ── Slice 10b-1: the trained context and the f16 KV bytes per token on
// the Shape, and the full-GPU bytes of a shape for a context and two
// cache types (the manager's memory estimate, pure) ──

#[test]
fn the_shape_carries_the_trained_context_and_the_f16_kv_bytes_per_token() {
    let (h, size) = moe_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(
        s.trained_ctx, 262144,
        "the trained context, uncapped (`ctx` keeps the capped one)"
    );
    assert_eq!(
        s.kv_f16_per_token,
        Some(4096),
        "f16 bytes per token: 4 kv heads x (128 + 128) x 2 bytes x 2 layers"
    );
    // The download fixture (`a_downloaded_model_is_read_from_disk_with_its_
    // shards`): no context_length, and no attention keys the reader can
    // use.
    let (h2, s2) = gguf(
        &[
            ("general.architecture", W::Str("qwen3moe")),
            ("qwen3moe.block_count", W::U32(1)),
        ],
        &[
            ("token_embd.weight", MIB),
            ("blk.0.attn_q.weight", 2 * MIB),
            ("output.weight", MIB),
        ],
    );
    let s2 = shape_of(&[(parse_header(&h2).unwrap(), s2)]).unwrap();
    assert_eq!(
        s2.trained_ctx, 0,
        "no context_length in the header: 0, not the reference context"
    );
    assert_eq!(
        s2.kv_f16_per_token, None,
        "a layout kv_per_token cannot read: None, the Shape does not decide"
    );
}

#[test]
fn the_full_gpu_bytes_are_the_weights_less_the_input_embedding_plus_the_kv() {
    let (h, size) = moe_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    let weights = s.total - s.token_embd;
    // f16/f16 at the reference context: 4096 f16 bytes per token, so the
    // KV term is 128 MiB.
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "f16", "f16"),
        weights + 4096 * REF_CTX,
        "f16/f16: the KV term is 128 MiB at 32k"
    );
    // q8_0/q8_0: 34 bytes per 32 elements, relative to f16's 64: the mean
    // is 68/128, so 68 MiB at 32k.
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "q8_0", "q8_0"),
        weights + 4096 * REF_CTX * 68 / 128,
        "q8_0/q8_0: the KV term is 68 MiB at 32k"
    );
    // q4_0/f16: (18 + 64)/128, so 82 MiB at 32k.
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "q4_0", "f16"),
        weights + 4096 * REF_CTX * 82 / 128,
        "q4_0/f16: the KV term is 82 MiB at 32k"
    );
    // A context of 0: no KV at all, the weights alone.
    assert_eq!(full_gpu_bytes(&s, 0, "f16", "f16"), weights);
}

#[test]
fn each_cache_type_costs_its_ggml_block_size_per_32_elements() {
    // The sizes the design lists (ggml's block static asserts): at ctx 1
    // with one type paired with itself the KV term is 64 times the type's
    // bytes per 32 elements (4096 f16 bytes per token x 2s/128).
    let (h, size) = moe_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    let weights = s.total - s.token_embd;
    for (ty, bytes_per_32) in [
        ("f32", 128),
        ("f16", 64),
        ("bf16", 64),
        ("f8_e4m3", 32),
        ("q8_0", 34),
        ("q5_1", 24),
        ("q5_0", 22),
        ("q4_1", 20),
        ("q4_0", 18),
        ("iq4_nl", 18),
    ] {
        assert_eq!(
            full_gpu_bytes(&s, 1, ty, ty) - weights,
            64 * bytes_per_32,
            "{ty}: {bytes_per_32} bytes per 32 elements"
        );
    }
}

#[test]
fn an_unreadable_kv_layout_costs_the_fallback_reserve() {
    // The download fixture: kv_per_token reads no layout, so the KV term
    // is FALLBACK_KV whatever the context and the cache types.
    let (h, size) = gguf(
        &[
            ("general.architecture", W::Str("qwen3moe")),
            ("qwen3moe.block_count", W::U32(1)),
        ],
        &[
            ("token_embd.weight", MIB),
            ("blk.0.attn_q.weight", 2 * MIB),
            ("output.weight", MIB),
        ],
    );
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(s.kv_f16_per_token, None);
    let weights = s.total - s.token_embd;
    assert_eq!(
        weights,
        3 * MIB,
        "the fixture's weights, less the input embedding"
    );
    for ctx in [0, 8192] {
        assert_eq!(
            full_gpu_bytes(&s, ctx, "f16", "f16"),
            weights + FALLBACK_KV,
            "an unreadable layout costs FALLBACK_KV at ctx {ctx}"
        );
    }
    assert_eq!(
        full_gpu_bytes(&s, 8192, "q4_0", "q4_0"),
        weights + FALLBACK_KV,
        "and the cache types change nothing when the layout cannot be read"
    );
}

#[test]
fn an_unknown_cache_type_costs_f16() {
    // ASSUMPTION: the design says nothing about a name outside the list,
    // so this pins f16 for it. The values checked here are content-checked
    // against KV_CACHE_TYPES before any estimate runs (ARCHITECTURE.md
    // slice 10b's keys), so this is the defensive floor, not a live path.
    let (h, size) = moe_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "q4_0", "q8_nv3"),
        full_gpu_bytes(&s, REF_CTX, "q4_0", "f16"),
        "an unknown cache type name costs f16 (assumed: the design does not say)"
    );
}

// ── Slice 10b-1, review: the estimate is arithmetic over numbers a client
// writes (a profile's ctx-size, a header's counts), so it saturates instead
// of overflowing, and a model that uses its embedding as its output head
// keeps that embedding on the GPUs ──

#[test]
fn a_model_without_an_output_weight_keeps_its_embedding_on_the_gpu() {
    // A file with no `output.weight` uses `token_embd` as its output head,
    // and llama.cpp loads a SECOND copy of that tensor for the output
    // layer: `src/llama-model-loader.cpp` 1258-1263 turns a
    // `TENSOR_DUPLICATED` `token_embd` into `LLM_TENSOR_OUTPUT` (so the
    // copy goes on the output layer's device) and 1466-1481 adds its bytes
    // to the loaded size; `src/models/beam.cpp` 61-66 is one model that
    // does this. So the host-memory rule (`is_input_embedding`) does not
    // lower the estimate for such a model: the weights term is the whole
    // tensor total, `token_embd` subtracted only when the file has an
    // `output.weight`.
    let (h, size) = tied_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(s.token_embd, 100 * MIB, "the fixture has an embedding");
    assert_eq!(s.total, 112 * MIB + 4096, "the fixture's tensors");
    assert_eq!(
        full_gpu_bytes(&s, 0, "f16", "f16"),
        112 * MIB + 4096,
        "no output.weight, ctx 0 (no KV): the weights term is the whole total"
    );
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "f16", "f16"),
        144 * MIB + 4096,
        "no output.weight: the whole total plus the f16 KV of 32 MiB at 32k"
    );
}

#[test]
fn a_model_with_an_output_weight_keeps_its_embedding_in_host_memory() {
    // Guard: green today, the tied fix must keep it. The same model with
    // its own output head: the embedding stays in host memory, the output
    // head is the tensor that sits on the GPUs, so the estimate is the same
    // number as the tied model's: the duplicated embedding costs what a
    // head of its own costs.
    let (h, size) = untied_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(s.total, 212 * MIB + 4096, "the fixture's tensors");
    assert_eq!(s.token_embd, 100 * MIB, "the fixture's embedding");
    assert_eq!(
        full_gpu_bytes(&s, 0, "f16", "f16"),
        112 * MIB + 4096,
        "output.weight present, ctx 0: the total less the embedding"
    );
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "f16", "f16"),
        144 * MIB + 4096,
        "output.weight present: the same estimate as the tied model"
    );
}

#[test]
fn a_split_model_is_tied_only_when_no_shard_has_an_output_weight() {
    // The tensor list `shape_of` folds is the whole model's, so the output
    // head of a split model is seen wherever its shard is. Both rows put
    // the same 110 MiB of weights on the GPUs: one because the embedding IS
    // the output head, the other because the head is its own tensor and the
    // embedding is host memory. The first row is the guard (green today,
    // first so it is proven), the second the red.
    for (label, second, total, weights) in [
        (
            "the second shard declares output.weight",
            vec![
                ("blk.0.attn_v.weight", 2 * MIB),
                ("blk.0.attn_norm.weight", 4096),
                ("output.weight", 100 * MIB),
            ],
            210 * MIB + 4096,
            110 * MIB + 4096,
        ),
        (
            "no shard declares output.weight",
            vec![
                ("blk.0.attn_v.weight", 2 * MIB),
                ("blk.0.attn_norm.weight", 4096),
            ],
            110 * MIB + 4096,
            110 * MIB + 4096,
        ),
    ] {
        let (h1, s1) = gguf(
            &qwen2_kvs(),
            &[
                ("token_embd.weight", 100 * MIB),
                ("blk.0.attn_q.weight", 8 * MIB),
            ],
        );
        let t: Vec<(&str, u64)> = second.iter().map(|(n, b)| (*n, *b)).collect();
        let (h2, s2) = gguf(&[("split.no", W::U32(1))], &t);
        let s = shape_of(&[
            (parse_header(&h1).unwrap(), s1),
            (parse_header(&h2).unwrap(), s2),
        ])
        .unwrap();
        assert_eq!(s.total, total, "{label}: the shards add up");
        assert_eq!(
                full_gpu_bytes(&s, 0, "f16", "f16"),
                weights,
                "{label}, ctx 0: the second row's total is the first row's plus the 100 MiB embedding, so both rows put the same 110 MiB of weights on the GPUs"
            );
        assert_eq!(
            full_gpu_bytes(&s, REF_CTX, "f16", "f16"),
            weights + 32 * MIB,
            "{label}: that term plus the f16 KV of 32 MiB at 32k"
        );
    }
}

#[test]
fn the_estimate_saturates_when_the_context_overflows_the_kv_term() {
    // `ctx` is a model profile's ctx-size: a member of `pai` writes any
    // integer there and the manager does this arithmetic as root, so an
    // overflowing context is a number at the ceiling, never a panic (the
    // debug build checks the arithmetic) and never a wrap to something that
    // looks like it fits.
    let (h, size) = moe_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(s.kv_f16_per_token, Some(4096), "the fixture's real KV");
    // Guard, first so it is proven: green today, the saturation must not
    // blunt an answer that fits.
    assert_eq!(
        full_gpu_bytes(&s, 1 << 40, "f16", "f16"),
        s.total - s.token_embd + 4096 * (1 << 40),
        "a context that does not overflow keeps its exact answer"
    );
    for ctx in [u64::MAX, u64::MAX / 2, 1 << 50] {
        assert_eq!(
            full_gpu_bytes(&s, ctx, "f32", "f32"),
            u64::MAX,
            "ctx {ctx}: the KV term alone is past a u64, the estimate saturates"
        );
    }
    assert_eq!(
        full_gpu_bytes(&s, u64::MAX, "q4_0", "q4_0"),
        u64::MAX,
        "the cheapest cache type does not save an overflowing context"
    );
}

#[test]
fn the_estimate_saturates_when_the_kv_bytes_per_token_overflow_them() {
    // The other side of the same arithmetic: a header that declares a KV
    // layout bigger than a u64 (a corrupt or hostile file), read by the
    // manager as root.
    let (h, size) = huge_kv_fixture();
    let s = shape_of(&[(parse_header(&h).unwrap(), size)]).unwrap();
    assert_eq!(
        s.kv_f16_per_token,
        Some(u64::MAX),
        "a layout past a u64: the per-token count saturates, no panic"
    );
    assert_eq!(
        s.kv,
        u64::MAX,
        "the shape's KV at the reference context saturates with it"
    );
    for (ctx, k, v) in [
        (1u64, "f16", "f16"),
        (REF_CTX, "f16", "f16"),
        (REF_CTX, "q4_0", "q4_0"),
    ] {
        assert_eq!(
            full_gpu_bytes(&s, ctx, k, v),
            u64::MAX,
            "{k}/{v} at ctx {ctx}: the KV term is past a u64"
        );
    }
}

#[test]
fn kv_per_token_saturates_on_a_header_with_huge_head_counts() {
    let kv = |kvs: &[(&str, Kv)]| {
        let m: HashMap<String, Kv> = kvs
            .iter()
            .map(|(k, v)| (format!("a.{k}"), v.clone()))
            .collect();
        kv_per_token(&|k: &str| m.get(&format!("a.{k}")), 4)
    };
    let big = u64::MAX;
    let heads = u64::from(u32::MAX);
    // Guard, first so it is proven: green today, the saturation must not
    // turn an unreadable layout into a number.
    assert_eq!(
        kv(&[
            ("embedding_length", Kv::Uint(4096)),
            ("attention.head_count", Kv::Uint(0)),
        ]),
        None,
        "0 heads is an unreadable layout, not a division by zero"
    );
    // MLA: the latent size alone is past a u64.
    assert_eq!(
        kv(&[
            ("attention.kv_lora_rank", Kv::Uint(big)),
            ("rope.dimension_count", Kv::Uint(64)),
            ("block_count", Kv::Uint(4)),
        ]),
        Some(u64::MAX),
        "MLA with a huge latent: saturated, not a panic"
    );
    // The head count as a per-layer array, the largest u32 in every layer,
    // and the key and value lengths the embedding divided by it.
    assert_eq!(
        kv(&[
            ("embedding_length", Kv::Uint(big)),
            ("attention.head_count", Kv::Arr(vec![Kv::Uint(heads); 4])),
            ("block_count", Kv::Uint(4)),
        ]),
        Some(u64::MAX),
        "the head count times the default key length times the layers"
    );
    // head_count_kv as an array, the sum times a huge key length.
    assert_eq!(
        kv(&[
            ("attention.head_count", Kv::Uint(1)),
            ("attention.key_length", Kv::Uint(big)),
            ("attention.value_length", Kv::Uint(1)),
            ("attention.head_count_kv", Kv::Arr(vec![Kv::Uint(heads); 4])),
            ("block_count", Kv::Uint(4)),
        ]),
        Some(u64::MAX),
        "a huge key length times the kv-head array"
    );
    // Guard: green today, the saturation must not turn this into a number.
    assert_eq!(
        kv(&[
            ("embedding_length", Kv::Uint(4096)),
            ("attention.head_count", Kv::Uint(0)),
        ]),
        None,
        "0 heads is an unreadable layout, not a division by zero"
    );
}

#[test]
fn shape_of_saturates_a_header_declaring_more_than_a_u64_can_count() {
    // The dims are skipped, so the bytes come from the offsets and the size
    // the caller passes: two shards each declaring a file of `u64::MAX`
    // bytes is more than a u64 counts.
    let h = gguf_declared(
        &[
            ("general.architecture", W::Str("qwen3")),
            ("qwen3.block_count", W::U32(1)),
        ],
        &[
            ("token_embd.weight", &[u64::MAX, u64::MAX], 0),
            ("blk.0.attn_q.weight", &[u64::MAX, u64::MAX], 4096),
        ],
    );
    let hdr = parse_header(&h).expect("the reader skips the dims, the header parses");
    let s = shape_of(&[(hdr.clone(), u64::MAX), (hdr, u64::MAX)]).expect("no panic");
    assert_eq!(
        s.token_embd,
        2 * 4096,
        "the embedding is the small tensor at offset 0, in both shards"
    );
    assert_eq!(s.total, u64::MAX, "two shards of a u64::MAX: saturated");
    assert_eq!(s.layers[0].other, u64::MAX, "the block's bytes, saturated");
    assert_eq!(s.globals, 0, "nothing outside the blocks");
    assert_eq!(s.kv, FALLBACK_KV, "no attention keys: the fallback reserve");
    assert_eq!(
        full_gpu_bytes(&s, REF_CTX, "f16", "f16"),
        u64::MAX,
        "the estimate of a saturated shape saturates, it does not overflow"
    );
}
