# Candidate optimizations: what was checked

Reference for the out-of-tree optimizations assessed for porting: what was measured, what was found
superseded or not applicable, and what to watch. The open work they lead to is in
[PLAN.md](../../PLAN.md), under "Next steps" and "Later, not planned yet".

Candidates taken from <https://github.com/overdoingism/llama-cpp-rdna-boosts> (MIT), assessed
on 2026-10-21 against the tree of that day (llama.cpp v0.7.0 with patches 0001 to 0004). The
tree is v0.9.0 with `patches\llama.cpp\0001`, `0005` and `0006`: 0002, 0003 and 0004 are removed,
and the items below say where that changes their conclusion. That
repository is a 16-block `git am` series against upstream `097cf33d9` (2026-10-12, an
ancestor of v0.7.0, 98 commits behind it), validated on Linux with two or three R9700 in
`split-mode tensor` over RCCL; nothing there was run on NVIDIA hardware, on a mixed
CUDA+HIP process, or on Windows by the author (Windows figures come from issue reporters).
Our setup is the opposite on every axis: one R9700 plus one 4070 SUPER, layer split, Windows
and Linux. So the headline numbers of that repo (prefill +8 % to +70 %, decode +7-8 %, 2x
R9700, Qwen3.8-27B Q8_0, measured in August against a master that predates upstream's RDNA4
WMMA FlashAttention #28102) do not transfer; the items below are the ones whose mechanism
applies to our cards, ranked by expected gain over porting cost. Cherry-picking means
re-cutting hunks onto the current tag by hand, as the removed 0003 was (the blocks depend on each other and on
fork-only plumbing), plus one ~35 minute rebuild per attempt.

## Checks done before ranking (Windows, 2026-10-21)

Hand runs on the server of that day (v0.7.0 with patches 0001 to 0004), preset `Qwen3.8-27B-Coder390`,
device `ROCm1,CUDA0`, split `52,14`, `draft-mtp`, temperature 0, one repetition each, not
recorded in `benchmarks\bench.db` (diagnostics, not a campaign).

**HIP/CUDA graphs already replay in MTP decode.** The repository's block 11 ("skip CUDA
graphs for multi-token prefill", +16-19 % decode on a reporter's Windows/ROCm 10 gfx1201)
fixes a base where decode never replayed a HIP graph. Upstream v0.7.0 keys its graphs per
first node and llama's graph reuse keeps the uid stable, so with `GGML_CUDA_GRAPH_DEBUG=1`
200 decoded tokens show 270 "Graph id reused" lines against 3 warmups and 3 resets, and the
HIP API log (`AMD_LOG_LEVEL=3`) shows, per MTP decode step on the R9700, 0 `hipLaunchKernel`,
2 `hipGraphLaunch` (verify and draft) and 20 `hipMemcpyAsync`, with 2 `hipGraphInstantiate`
and 2 `hipGraphExecUpdate` in the whole run (so the ROCm <= 10.0 `hipGraphExecUpdate`
kernarg leak the block works around, rocm-systems#10713, is moot here). What graphs are
worth today, `GGML_CUDA_DISABLE_GRAPHS=1` against the default:

| leg | prefill t/s, 1,064 tokens | decode t/s, 200 tokens at 1k | prefill t/s, 43,730 tokens | decode t/s, 128 tokens at 43.7k |
| --- | --- | --- | --- | --- |
| graphs on (default) | 837 | 39.4 | 1131 | 33.7 |
| graphs off | 857 | 38.7 | 1119 | 33.1 |

Block 11 is therefore superseded: nothing to port.

**Band WMMA for GQA ratios above 8: was ported as patch 0003, removed with it.** The build and the
configurator have no band: the R9700's small decode batches run upstream's tile kernel (`fattn.cu`,
v0.9.0: WMMA only above 16 queries times the GQA fold). What the port measured on v0.7.0:
`test-backend-ops` 135/135 at `nr23 {12,1}`; the band was 12 % faster than the tile kernel at 131k
with one query per step and 55-59 % with 2-4 queries at 65k-131k, 16 us slower per call at 4k. On
the Flash-Next preset at 43.7k and 2.6k (3 runs each, temperature 0) prefill and decode were within
noise and the output byte-identical, because the n-gram drafter drafted nothing on those prompts and
attention is ~5 % of a decode step: `docs\qwen3.8_flash_next.md` section 7. What its absence costs
with accepted drafts is the open measurement in [PLAN.md](../../PLAN.md), "Re-measure the presets on v0.9.0".

**Upstream's chunked GDN prefill kernel excludes RDNA4.** `gated-delta-net-mma.cu` (the
MMA chunked kernel for the gated delta net layers, 36 of the 48 layers of Qwen3.8-27B and of
Qwen3.8-Flash-Next) compiles its HIP arm only `#if defined(RDNA3)` and dispatches it only
for `GGML_CUDA_CC_IS_RDNA3_5` at `n_tokens >= 2048`; its HIP tile types
(`DATA_LAYOUT_I_MAJOR_MIRRORED`, `tile<16, 16, float, DATA_LAYOUT_J_MAJOR>`) and the bf16
`mma` they use exist in `mma.cuh` for RDNA3 only. gfx1201 runs the sequential
`launch_gated_delta_net` path on every prefill ubatch. Opening the gate is not a flag flip:
it needs a bf16 16x16x16 WMMA tile for gfx12 in `mma.cuh` and the matching loads.

**Ported to RDNA4 and measured: the chunked kernel loses to the sequential path (Linux,
R9700, 2026-10-21).** The gfx12 pieces are in fact already in v0.7.0's `mma.cuh` (the
`tile<16, 8, nv_bfloat162, DATA_LAYOUT_I_MAJOR>` operand and the
`__builtin_amdgcn_wmma_f32_16x16x16_bf16_w32_gfx12` product), so the port is nine lines of
`gated-delta-net-mma.cu`: the compile gate opened to `AMD_WMMA_AVAILABLE`, the operand
layout picked by `get_input_data_layout()`, the dispatch opened to `GGML_CUDA_CC_IS_RDNA4`.
`test-backend-ops test -o GATED_DELTA_NET` on `ROCm0`: 43/43, the 16-K-head cases at 64 to
2048 tokens on the new path. `perf` at the shape llama builds for Qwen3.8-27B and
Flash-Next (16 K heads x 48 V heads, d 128; cases added locally as
`test_gated_delta_net(GGML_TYPE_F32, 16, 128, n, 1, 3)`), us per call at 64 / 256 / 512 /
1024 / 2048 / 4096 tokens: sequential 69 / 233 / 449 / 925 / 1902 / 3949; chunked with
upstream's HIP schedule (`slice64`, two blocks per head) 95 / 337 / 665 / 1371 / 2755 /
5468, +48 %; with the CUDA schedule (`slice32`, one or two blocks per SM, same numbers
either way) 89 / 304 / 600 / 1222 / 2426 / 4861, +33 %. Slower at every size, so no
threshold saves it. The HIP arm of the kernel reads every operand element out of shared
memory with a scalar load and a float round trip (the CUDA arm uses `ldmatrix`) and issues
three WMMA per k-step for the bf16 hi/lo split; making it fast on RDNA4 is kernel work,
the fork's 1015-line gfx12 file. The RTX 4070 SUPER runs the same cases on the chunked
kernel at 33 / 115 / 225 / 492 / 966 / 1917. The prize was bounded anyway: 36 layers x
449 us per 512-token ubatch is 32 us per token, 1.3 % of Flash-Next's 2.4 ms prefill step
at 43.7k, and 36 x 925 us per 1024 tokens is 3.5 % of the 27B's 0.9 ms. Reverted. Revisit only if upstream ships an RDNA4 arm of its own
(check `gated-delta-net-mma.cu` at every tag bump).

**KV-split width determinism (`upstream/UPSTREAM-PR-fa-kv-split-width.patch`): open again, it
depended on the band (measured on Linux, 2026-10-21, on v0.7.0 with patches 0001 to 0004).** The patch
pins the KV split of `launch_fattn`'s non-stream-k branch for `Q->ne[1] <= 8`, so a verify batch reduces
the KV like plain decode. That branch is the tile and vec kernels. With the band of 0003 no decode path
of ours took it, so the patch was found not applicable. Without the band the R9700's decode takes it, so
that finding no longer holds on the R9700; whether the patch applies to v0.9.0 is unchecked. The RTX
4070 SUPER still runs the stream-k MMA kernel, where `switch_ncols1` picks a different kernel instance
per verify width (1, 4 or 8 query columns, each with its own KV batch and rounding order). The same run,
band on, Coder390 preset as configured (`CUDA0,ROCm0`, 16 layers on the 4070), 43.7k-token prompt,
temperature 0, seed 42, 300 tokens: `spec-draft-n-max` 2, 4 and no drafter give three different texts
(first flips at token ~120 and ~250), each run byte-identical when repeated, so that dependence is
deterministic and sits in the paths the patch leaves alone. Width purity on that placement would need the
MMA kernel pinned to one `ncols1` for every small width (a kernel change, not this patch).

## Watch items (no action unless the symptom appears)

- `UPSTREAM-PR-per-device-host-buffers` (`-sm layer -ncmoe` sends every host-expert op to
  device 0: 10.3 to 58.6 t/s on 2x R9700): on our Flash-Next preset the 16 host layers
  belong to the R9700, the first device, so there is no cross-device hop to remove. Relevant
  only if a preset ever puts host experts on the 4070's layers.
- `UPSTREAM-PR-sched-cross-device-input-order`: a layer-split race with host experts on
  the second device (correctness, with a mixed-backend `event_wait` caveat). Same trigger.
- `UPSTREAM-PR-fa-decode-verify-kernel-family` (VEC vs TILE band split, -0.5 % decode):
  only for quantized KV, which none of the presets uses now.

## Not applicable or superseded

- Block 11 HIP graphs on verify batches: superseded by upstream's keyed graphs (measured above).
- Blocks 06 and 13 (MoE expert cache, slab allocator, staging ring, MoE MMQ fusions,
  737 KB): built for `-ncmoe` under tensor split; `GGML_CUDA_ASYNC_INPUT` (a 16 MiB pinned
  ring per device) cannot be extracted from the block.
- Block 10 k-quant VDR: the dense kernels keep upstream's VDR (only the MoE expert kernel
  changes), so the Q8_0 dense 27B gains nothing.
- Block 15 derived KQ mask (`LLAMA_KQ_MASK_DERIVED`): saves mask memory but costs -1.6 %
  prefill at 98k on gfx1201 with the 27B in a 2-GPU layer split; our mask copy cost went
  away with the R9700 first in `device`.
- Block 04 RDNA prefill tuning: the WMMA FlashAttention it tunes is upstream since #28102.
- Block 14 qwen4exp fusions (QSA sparse FA, HC fusions, +2.4 % decode): Flash-Next only,
  very large; revisit only if Flash-Next becomes the daily model.
- Runtime knobs: `ROCBLAS_USE_HIPBLASLT=0` is already exported by the configurator;
  `GGML_CUDA_FA_WMMA_256=0` is harmful (their own charts: 3x slower deep prefill).

