# Out-of-tree patches applied to the llama.cpp working tree

`02-build.ps1` applies every `*.patch` in this directory to `$cfg.LlamaCppDir`
(`build\windows\llama.cpp`) in file-name order, right after it detaches the clone onto
the newest `vX.Y.Z` release tag, and **reverts them again before the next
fetch/checkout** so the checkout always runs on a clean tree (a tag bump that
touches a patched file would otherwise abort with "local changes would be
overwritten"). Anything the reverts leave behind (an older revision of a
patch that has since changed, a hand edit) is **stashed** in the clone, never
discarded: `git stash list` there has it back. Both legs probe with `git apply --check` first, so re-running
the build is a no-op rather than an error, and a patch that no longer applies
**fails the build fast** instead of being skipped silently: an out-of-tree
change that quietly stops being applied is a binary that behaves differently
from the one the last build produced, with nothing in the log saying so.

Patch files are stored **verbatim** as published upstream, never reflowed or
"improved": a refresh is then a plain diff against the new upstream file, and
the checked-in sha256 below is the one the publisher serves. `.gitattributes`
pins `*.patch` to LF for the same reason (a CRLF checkout of the patch does not
apply to LF sources).

The llama.cpp clone lives under the gitignored `build\`, so nothing here is
committed into it; the patch is re-applied from this directory on every build.

## `0001-qwen35-fastmtp-d2t.patch`

Teaches the `qwen35` arch (Qwen3.5 / Qwen3.8) to read a **trimmed draft
vocabulary** from a standalone MTP head GGUF, which is what HauhauCS's
"FastMTP" sidecar ships. Source:

- <https://huggingface.co/HauhauCS/Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF/resolve/main/HauhauCS-FastMTP-llama.cpp.patch>
- sha256 `981285400b59dc45cf99936b6ff66d4b3aa0f1b532f85fa51418cb407e51d615`
- published against upstream `4df29be4f4c3673f428170fda944a5b19f743bb8`;
  verified to apply and compile clean on tag **b10488** (2026-08-20)
- **refreshed locally for v0.6.0** (2026-10-05), so the checked-in file is no
  longer the publisher's bytes: v0.6.0 inserted an optional `cls_out`
  projection between `output` and its NULL fallback in `load_arch_tensors`,
  which broke the first hunk's context. The publisher had not re-cut the patch
  (the URL above still served the sha256 above), upstream still has no `d2t`
  in `qwen35.cpp`, and `git apply --3way` re-applied it with every added and
  removed line unchanged; only offsets and context moved.
- **refreshed again for v0.7.0** (2026-10-16), and this time ONE added line
  changed: #30097 replaced the file's local `mtp_only` with the shared
  `nextn_flags()` helper, so the `d2t` gate reads
  `(nf.trunk & TENSOR_NOT_REQUIRED)` instead of `mtp_only`. Same condition
  (the helper probes the same `blk.0.attn_norm.weight` under the same
  `n_layer_nextn > 0`). The last hunk also lost its trailing
  `ggml_build_forward_expand(gf, cur)` context line: #30445 dropped that call
  from `graph_mtp` (the caller expands `t_logits`), which the patch never
  touched. Upstream still has no `d2t` in `qwen35.cpp`.
- **hardened locally** (2026-10-16): the publisher's patch trusts `d2t`, and
  `ggml_set_rows` writes wherever an index points, so a crafted sidecar could
  write past the logits tensor. The patch now refuses at load a `d2t` that is
  not I32/I64, has no entries or more than the vocabulary, and, through a
  `load_tensors` override (`models.h`) that runs once the data is in, any entry
  outside `[0, n_vocab)`. Checked-in sha256:
  `34db7d82eeb458a6c1e31ec80c2a83c047b484c7b6d2792be32cf75a109c5c91`

**What it changes.** A FastMTP sidecar is an MTP-only GGUF (no trunk blocks)
whose LM head covers **32,768** rows instead of the target's 248,320, plus a
`d2t` tensor (I64) mapping those rows onto target token ids. Stock
`src\models\qwen35.cpp` sizes `output.weight` as `{n_embd, n_vocab}` and so
rejects the file at load with `expected 5120, 248320, got 5120, 32768`. The
patch reads `d2t`'s length as `n_vocab_out`, sizes the head to it, and, in
`graph_mtp`, scatters the draft logits back onto the full vocabulary
(`ggml_fill` to `-INFINITY` then `ggml_set_rows` through `d2t`) so the sampler
and the verify step see ordinary full-width logits.

**Blast radius.** Both legs are gated on `model.d2t`, which is created only for
an `mtp_only` file that actually carries a `d2t` tensor. A normal target GGUF
with embedded MTP heads, and every other arch, take byte-identical paths. `d2t`
itself is not new: it is an existing `llama_model` member that `dflash.cpp` and
`eagle3.cpp` already load and scatter through in exactly this shape; this patch
is that same code on the third arch that needed it.

**The one divergence from the upstream copies is load-bearing, not sloppiness.**
`dflash.cpp` and `eagle3.cpp` both assert `model.d2t->type == GGML_TYPE_I64`
before the scatter; this patch accepts I32 as well, and it has to. The
FastMTP sidecar ships `d2t` as **I32** (it is the file's single `i32` tensor),
which `ggml_set_rows` accepts (`ggml.c`: `c->type == GGML_TYPE_I64 ||
c->type == GGML_TYPE_I32`) but the upstream assert would reject. So do not
"restore" that assert when refreshing the patch: it would turn the one file this
exists for into a hard failure. The load-time checks above (type, length, every
index in range) replace it.

**Confirming it is live** (rather than assuming): the loader logs
`QWEN35 MTP using d2t draft-vocab trim (n_vocab_out = 32768)`, at llama.cpp's
INFO level, which llama-server prints only from `-lv 4` up (`LogVerbosity` in
`server.ini`); at the usual 3 the line is absent and proves nothing. The failure
mode without the patch is a refused load
(`expected 5120, 248320, got 5120, 32768`), not a slow or silently
non-speculating server, so a server that starts at all with the sidecar attached
is already evidence.

Verified end to end on b10488 (2026-08-20) against
`Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q6_K_P.gguf` +
`...-FastMTP-32K.gguf`: the trim line appears, an 8-token completion comes out
coherent, and llama-server reports `draft acceptance = 0.57143 (4 accepted /
7 generated), mean len = 2.33`, i.e. the drafts are real and accepted rather
than garbage the target quietly rejects.

**Using it from llama-cpp-config**: a per-model preset with
`model-draft = <...FastMTP-32K.gguf>`, `spec-type = draft-mtp` and
`spec-draft-n-max = 3` (the depth the publisher qualified; the stock embedded
head runs at 2). Because the sidecar is a **separate draft file**, unlike the
embedded MTP heads, `device-draft` / `n-gpu-layers-draft` are honoured here and
should pin the sidecar to the same device as the model. `--spec-draft-p-min`
has no preset field; llama.cpp's default applies.

**When to drop it.** If upstream lands the same support, `git apply --check`
starts failing and the build stops with a message pointing here: delete the
file (git history keeps it) and rebuild. Re-verify at every llama.cpp tag bump
that touches `src\models\qwen35.cpp`; a refreshed patch from the publisher
replaces this one only with the `d2t` validation carried over (or present in
it), and the sha256 above changes with it.

## `0002-sampler-backend-probe-chain-width.patch`

Local patch (2026-10-20, written against v0.7.0, no upstream source), 114 lines in
`src/llama-sampler.cpp` only. Checked-in sha256: `644c45d800011f097ab0226dab016838476b9e2ff6a7008dababd6bfef03a6be`

**What it changes.** `llama_sampler_chain_backend_init` probes every sampler of the
chain on its own, on a fixed 131072-wide logits tensor, to decide whether it can run on
the backend. On the HIP backend, which has no CUB, `ARGSORT` over a row that wide is
unsupported (the bitonic kernel needs the padded row in LDS), so a `top-p` is refused
even when a `top-k 20` before it leaves 20 candidates, and the whole suffix of the chain
(`dist` included) falls back to the CPU: llama-server logs `does not have support for op
ARGSORT needed for sampler 'top-p'` on every request, the chain line reads
`+top-k -> -top-p -> dist`. The patch adds `llama_sampler_chain_probe_width`, which
applies the backend samplers before the one being probed to a probe graph and reads the
width they hand over, and `llama_sampler_backend_support` probes on that width
(`g_backend_probe_n_candidates`, thread-local, 131072 outside a chain). The probe starts
without `candidates`, as the chain's own probe does: a `top-p` after a `top-k` asserts on
2-D candidates. Upstream issue: <https://github.com/ggml-org/llama.cpp/issues/28513>.

**Measured (R9700 + RTX 4070 SUPER, Qwen3.8-27B-Coder390 Q8_0 MTP, 115k-token prompt,
temp 1.0, top-k 20, top-p 0.95, seed 42).** Chain on the backend end to end
(`+top-k -> +top-p -> +dist`), warning gone, identical output. Decode unchanged: 40.6-41.5
ms per token before, 40.7-40.8 after. Upstream `test-backend-sampler`
`test_multi_output_sampling_chain` (top-k -> top-p -> min-p -> temp -> dist against the
CPU reference) fails on ROCm without the patch and passes with it; `test_top_p` and
`test_penalties` (a top-p with no top-k before it: ARGSORT over the vocabulary) fail
either way and need hipCUB (<https://github.com/ggml-org/llama.cpp/pull/26592>). Kept for
the correctness, not for speed.

**When to drop it.** At the next llama.cpp release, if #28513 or #26592 is merged, or if
`test_multi_output_sampling_chain` passes on ROCm without it
(`LLAMACPP_TEST_MODELFILE=<gguf> test-backend-sampler --test=test_multi_output_sampling_chain`
with the server down). Note that upstream's own HIP skip list in that test is behind
`#ifdef GGML_USE_HIP`, which a `GGML_BACKEND_DL` build never defines for the test binary,
so the test does run on ROCm here.

## `0003-fa-band-wmma-rdna4.patch`

Local port (2026-10-20, against v0.7.0) of overdoingism's "FA band WMMA" patch for RDNA4
(<https://github.com/overdoingism/rdna4-fa-band-wmma>, commit `c59ecd60`, 2026-10-15; also
merged into <https://github.com/stew675/llama-cpp-rdna-boosts>). The original is written
against that fork and does not apply to upstream (its hunks lean on fork-only plumbing:
`kq_derived_t`, `fattn_kv_native_t`, `wmma_256`); this is the band mechanism alone, re-cut
on upstream's `fattn-mma-f16.cuh`, extended to **f16 K/V** (the original took q8_0 only),
plus its `test-backend-ops` cases. 375 lines: `ggml/src/ggml-cuda/fattn-common.cuh`,
`fattn-mma-f16.cuh`, `fattn.cu`, `tests/test-backend-ops.cpp`. Checked-in sha256:
`390f0ab2564b46c646cc53a14d456ef0a531cbf22d907e7b6e74c78f9989feb2`

**What it changes.** Upstream sends head-256 FlashAttention to the WMMA (matrix core)
kernel on RDNA4 only when `n_queries * gqa_ratio > 16` (#28102); below that, i.e. for
every single-token decode and every MTP verify batch of Qwen3.8-27B (GQA 6, 1 to 4
queries), it runs the scalar `flash_attn_tile` kernel, which reads each KV head up to
three times per step and reaches ~180 GB/s of the R9700's 640 (upstream issue
<https://github.com/ggml-org/llama.cpp/issues/28867>). The band runs those calls on the
MMA kernel with the KV sequence split round-robin over one block per CU and the GQA group
folded into the tile (`ncols2 = 8`), reading each K/V once; the partials are combined by
upstream's own stream-k fixup, untouched. **Opt-in through the environment**:
`GGML_HIP_FA_BAND_WMMA=4` (or `2`; unset or `0` = off), read by `getenv` at the first
attention call, so a change needs a server restart; `GGML_HIP_FA_BAND_WMMA_SPLIT=N`
overrides the number of blocks (default: the CU count). `llama-cpp-config` exports it from
`server.ini` (SDK Tuning). Gate (`ggml_cuda_fattn_band_wmma_applies`, one function for
kernel selection, dispatch and launch): RDNA4 with WMMA, K and V head 256, GQA in (4, 8],
at most 8 queries, one sequence, mask present, no softcap, no ALiBi, K/V both f16 or both
q8_0. Everything else, CUDA included (the files compile into `libggml-cuda.so` too, the
kernel branch sits under `AMD_WMMA_AVAILABLE`), takes the upstream path unchanged. The
original's launch gate enabled the 2-D grid for any WMMA launch with 8 or fewer queries
while the kernel handles it only for head 256 / `ncols2 8`; the port gates all three
places on the same predicate. On AMD `nstages` is 0 (no cp.async), which the round-robin
stride relies on: a `static_assert` pins it.

**Measured (R9700, gfx1201, ROCm 10.1.0, 2026-10-20).** `test-backend-ops perf
FLASH_ATTN_EXT` f16, head 256, 4 KV heads x 6, mask, us per call, tile vs band 4: kv
131072 at 1 query 1085 -> 922 (-15%), at 2: 1075 -> 924, at 3: 1494 -> 928, at 4:
1497 -> 930 (**-38%**); kv 65536 at 4 queries 752 -> 494. The band's cost is flat in the
query count (one KV read); band 2 helps at 1-2 queries only and falls back to the tile at
3-4, so **4 is the value to use**. q8_0 K/V stays far slower than f16 even with the band
(2388 vs 930 us at kv 131072, 4 queries): upstream converts quantized K/V to f16 on every
call of the MMA kernel, so quantized types gain nothing from this path. Server
(Qwen3.8-27B-Coder390 Q8_0 MTP, 115k-token prompt): decode 40.7 -> 38.7 ms per token at
temp 1.0 (-5%); at temp 0 without MTP, same 278 tokens, 71.7 -> 69.4 (-3%).
**Correctness**: `test-backend-ops test -o FLASH_ATTN_EXT -p 'nr23=\[6,1\]'` on ROCm0,
41/41 against the CPU reference with band 4 and with band 2 (kv 512 to 16384, 1 to 8
queries, f16 and q8_0; kv 512 covers blocks that own no KV iteration). **Greedy purity**:
at temp 0 the band's output with MTP, the band's without, and upstream's plain decode are
the same 278 tokens; upstream's tile WITH MTP diverges from all three after ~100 tokens
(its split and column count change with the query count), so the band also removes a
reproducibility defect of upstream between verify and decode.

**Confirming it is live.** Nothing is logged. With the variable set, a kernel trace
(`rocprofv3 --kernel-trace --stats`, see `ROCP_TOOL_ATTACH` in SDK Tuning) shows
`flash_attn_ext_f16` instances where an unpatched build shows `flash_attn_tile`; without a
profiler, `test-backend-ops perf` at the shape above with and without the variable differs
by the numbers above. A build without the patch ignores the variable silently.

**When to drop it.** When upstream routes small-batch head-256 FlashAttention to WMMA on
RDNA4 (#28867, or a PR that makes #28102's `> 16` gate go away), or when the fork's
version lands upstream: `git apply --check` then fails and the build stops with a message
pointing here. Re-verify at every tag bump that touches `fattn-mma-f16.cuh` or `fattn.cu`
with the `test` and `perf` commands above (GPU idle, server down).
