# Qwen3.8-Flash-Next: configuration reference (draft)

Reference settings for Qwen3.8-Flash-Next-GSQ-RCO (architecture `qwen4exp`) on this
framework. Measured with llama.cpp v0.7.0 on Windows, on the AMD Radeon AI PRO R9700
(32 GB, ROCm backend, `ROCBLAS_USE_HIPBLASLT=0`) and, for the MoE cache, the RTX 4070
SUPER (12 GB, CUDA backend), with a 12-core AVX-512 host and 64 GB of RAM.
Decode figures are one run of 200 tokens after a 25-token prompt; the prompt figures are
too short to stand for prefill (section 2, prefill row).

**Linux rows** (marked *Linux*) come from the same host booted into Linux (kernel 7.0,
TheRock 10.1.0, CUDA 13.4.2) with BOTH cards in one model, layer split, the IQ3_S at ctx
131072, `q8_0` KV cache, ubatch 512, `op-offload false`, the experts of 14 layers in RAM
(`n-cpu-moe 14`) and `spec-type ngram-map-k`: three runs per value of 200 tokens after a
25-token prompt (72 with the chat template) unless stated, `Threads` = `ThreadsBatch` = 22 up
to the `threads` row and 12 from it on. llama.cpp v0.7.0 unless marked v0.6.0. This is the
layout the campaign started from; the recommended one (`f16`, `n-cpu-moe 16`) is in section 9.

## 1. Model facts

| item | value | source |
| --- | --- | --- |
| layers | 48; attention on every 4th (12 layers hold a KV cache), recurrent state on all 48 | GGUF header (`full_attention_interval = 4`); `llama_kv_cache: 12 layers` |
| Memory row (Model info) | hybrid (attention + recurrent) | `llm_arch_is_hybrid` |
| experts | 512 per layer, 10 routed per token, plus a shared expert | `expert_count`, `expert_used_count` |
| native context | 262144 | `context_length` |
| GGUF sampling defaults | temp 1.0, top-k 20, top-p 0.95 | `general.sampling.*` |
| chat template | reads `enable_thinking`, `preserve_thinking`, `reasoning_effort` | template source: `reasoning-preserve` and `reasoning-effort` are live levers |
| files (IQ3_S) | shard 1: 51.05 GiB of weights; shard 2: 26.82 GiB, the n-gram table `per_layer_token_embd` (27465 MiB) | file sizes; `add: tensor per_layer_token_embd.weight ... lazy read enabled` |
| non-expert weights on the GPU | 3983 MiB (every expert in RAM, `n-cpu-moe 48`) | `model buffer size` |
| fixed state at one slot | recurrent 112.57 MiB; KV 24 + 96 MiB per 4096 tokens of context (f16) | `RS buffer size`, `KV buffer size` at ctx 4096 and 32768 |
| `split-mode tensor` | not supported for `qwen4exp` (#28569) | `gguf::arch_supports_sm_tensor` |

## 2. Recommended settings

| key | value | evidence |
| --- | --- | --- |
| `lazy-mode` | `auto` (the default) | the 26.8 GiB n-gram table is above the 4 GiB threshold and stays mapped; no run read it into memory. *Linux*: `on`, `auto` and `off` decode 25.1 ±1.2, 25.4 ±0.4 and 25.9 ±0.4 t/s, noise; `on` and `auto` both print "lazy read enabled" and map the table (`CPU_Mapped`), `off` reads it into 27466 MiB of RAM for no gain |
| `n-cpu-moe` | the lowest that leaves ~500 MB free on each card (below that, hipBLASLt's workspace runs out on the first real prompt and the process hangs in the driver; a 5-token test does not show it). On a 32 GB card at ctx 32768, `26` (27170 MiB of weights on the card, 1017 MiB compute buffer, ~3 GiB left) | decode 17.8 t/s at `26` against 12.3 at `48` (both `mmap`). *Linux*, ubatch 512, the lowest with >= 512 MB free on the R9700 after a 2.9k-token prompt, decode on the 25-token prompt (two runs): see the table below |
| `moe-cache-mib` | `0` (off), with no condition, on llama.cpp v0.7.0 (`620d2225c`; the cache dates from #29887, 2026-10-07): a cache policy upstream changes would reopen it | R9700: a loss of 3-4x (14.1 to 3.0-4.0 t/s). RTX 4070 SUPER: at equal VRAM, experts kept on the card beat the cache at every split (25.6 t/s against 19.5-23.6), and with every expert in RAM it is noise (section 3). *Linux*, two cards, the cache on the R9700 in place of resident layers: 17.8 to 22.5 t/s against 27.4 (section 3) |
| `load-mode` | `none` whenever experts sit in RAM. On Windows/ROCm with experts on the GPU, `mmap`, set explicitly, until the abort of section 4 is fixed; on Linux `none` there too (no abort). `auto` resolves to `none` only when a device THE MODEL RUNS ON reports no mmap support (`llama-model.cpp`, `load_tensors`): a Ryzen iGPU visible to HIP counts only when `device` is left blank; with `device` pinned to the discrete cards `auto` is `mmap` | `n-cpu-moe 48`, decode with `none` against `mmap`: R9700 11.6 to 14.1 against 12.3; RTX 4070 SUPER 22.7 to 22.9 against 14.0 to 17.1. `none` loads slower (25 to 43 s against 10 to 11 s). `n-cpu-moe 26` with `none` aborts on the R9700 on Windows. *Linux*: `none` 25.6 ±0.1 t/s; `mmap` 23.9 ±2.6 (21.0 on the first run after the load, 24.9 and 25.9 after); `auto` resolved to `mmap` (log), 24.7 ±1.7. (That AMD advises against `mmap` on ROCm is a reported recommendation with no primary source linked here.) |
| `repack` | leave unset | it acts only with `no-host` (next row): without it the GPU's pinned host buffer type is picked before `CPU_REPACK`, so nothing is repacked: the MoE cache layout is identical with and without `--no-repack` (section 3). *Linux*, `n-cpu-moe 14`, `moe-cache-mib 512`: the same 4 groups and 508.50 MiB with and without `--no-repack`, no `CPU_REPACK` buffer in either log |
| `no-host` | `true` with `op-offload false` | weights kept in RAM skip the GPU's pinned host buffer (`ROCm_Host`), so the CPU repacks the types it has interleaved kernels for: here `ffn_down_exps` in IQ4_NL (4950 MiB of the 14997 MiB in RAM at `n-cpu-moe 16`; `gate`/`up` in IQ2_S/IQ3 stay as they are, 10047 MiB of ordinary RAM). The pinned buffer only speeds the per-batch copies of `op-offload true`, which `false` never makes. *Linux*, the recommended preset (section 9), 43.7k-token prompt: prefill 416 and 419 t/s against 383 and 382, decode 25.0 against 24.9, three prompts 27.3 both. With `op-offload true` (section 8) the copies would come from pageable memory: not measured |
| `spec-type` | `ngram-map-k`, `spec-ngram-map-k-size-m 64` (`size-n` left at 12) | no MTP head in the GGUF, so the n-gram drafters are the only speculators. *Linux*, v0.6.0, IQ3_XXS (section 7): code rewrite 61-65 t/s (`size-m` 48 and 64) against 28.6 with none; prose reasoning 40.0 ±1.6 against 29.4 ±1.3 |
| prefill | (measured) | *Linux*, ubatch 512: 368 and 372 t/s at 2.9k tokens, 364 and 370 at 43.7k, 340 and 341 at 87.4k; decode right after: 24.5, 20.5 and 17.5 t/s. v0.6.0 gave 342 and 338 at 43.7k |
| `threads` | one per physical core (12 on the 12-core host), `ThreadsBatch` the same | *Linux*, 43.7k-token prompt, two runs of 128 tokens: prefill 410 and 405 t/s at 12, 363 and 368 at 16, 371 and 377 at 22, 364 and 368 at 24; decode 21.8, 21.5, 21.1 and 17.6 |
| `ubatch-size` | 512 at the lowest `n-cpu-moe` that fits, with `op-offload false`; for long prompts, 8192 with `op-offload true` (section 8) | with `op-offload false` a larger ubatch is paid in experts moved to RAM (section 5). *Linux*: the R9700's compute buffer is 877 MiB at 512, 1496 at 1024 and 2125 at 2048, so at `n-cpu-moe 14` neither 1024 nor 2048 loads |
| `cache-type-k/-v` | `f16` | `q8_0` halves the KV buffers but raises the compute buffers, so it saves only one layer of experts at 131072 and two at 262144 (table below), and it costs decode as the context fills: *Linux*, 43.7k-token prompt, 21.8 t/s in `q8_0` at `n-cpu-moe 14` against 24.9 in `f16` at `16`, 20.8 against 23.4 at 262144 (section 9) |
| `reasoning-preserve` | leave unset (on since v0.4.0) | *Linux*, turn 2 of a chat that sends turn 1's thinking back: on, the history is replayed unchanged and only the new 19 tokens are processed (prompt 204); `false` strips the old thinking, so the prompt is shorter (118) but diverges inside turn 1 and 60 tokens are reprocessed from a checkpoint. `false` is worth it only when long thinking fills the context |
| `ctx-checkpoints` | leave unset (32, at least 8192 tokens apart) | the recurrent state cannot be rolled back, so a prompt that diverges from the cached one resumes from the last checkpoint before the divergence or from zero: *Linux*, the `reasoning-preserve false` turn above reprocesses 60 of 118 tokens with checkpoints and all 118 with `0`. A checkpoint is 112.6 MiB of host memory, up to 3.5 GiB at 32 |

*Linux* `n-cpu-moe` per context and KV type (`override-tensor = output\.weight=ROCm0` only;
VRAM free on the R9700 at that value; decode t/s):

| context, KV | both cards | R9700 alone (`device = ROCm0`) |
| --- | --- | --- |
| 131072, `q8_0` | `15`, split 40,9: 843 MB, 25.7 ±2.3 | `25`: 861 MB, 21.3 ±2.6 |
| 131072, `f16` | `16`, split 40,9: 645 MB, 26.9 ±0.2 | `26`: 545 MB, 21.8 ±1.5 |
| 262144, `q8_0` | `18`, split 41,8: 368 MB, 25.8 ±0.1 | `28`: 1318 MB, 21.1 ±1.5 |
| 262144, `f16` | `21`, split 41,8: 839 MB, 23.3 ±2.8 | `30`: 650 MB, 21.3 ±0.1 |

At 262144 the RTX 4070 SUPER's compute buffer
(1601 MiB with `q8_0`) no longer fits at split 40,9 however many experts go to RAM, since
`n-cpu-moe` frees the first layers, which sit on the R9700: the split must give the small
card a layer less. The `q8_0` KV cache halves the KV buffers but raises the compute
buffers (877 against 533 MiB on the R9700 at 131072), so it saves one layer at 131072 and
two at 262144. The second card is worth ten layers of experts and 20-25% of decode. The
`n-cpu-moe 14` of the *Linux* rows at 131072 `q8_0` comes from also moving
`blk\.38\.ffn_down_exps` to the RTX 4070 SUPER.

## 3. MoE cache and repack (what the init prints)

- **The cache groups layers by expert tensor types.** At `n-cpu-moe 48` the IQ3_S file
  gives seven groups (IQ4_NL `ffn_down_exps` on some layers, Q2_0 on the others), each at
  ~4.2% of its experts as slots in 2048 MiB, for 47962.50 MiB of host experts.
- **Repack happens only with `--no-host`** (the preset's `no-host`). With it,
  `CPU_REPACK` takes 17550 MiB of experts, those layers leave the cache, and 9 layers stay
  cacheable (7112.50 MiB of host experts, ~28.7% as slots in 2048 MiB). Without it, the
  default and `--no-repack` print the same 48-layer cache.
- **Each miss uploads the expert over the host link**: at 50% hits that is ~470 MiB per
  token, so the result follows the card's real link more than its compute.
- **On two cards the cache sits with the layers it serves.** *Linux*, `n-cpu-moe 14`,
  `moe-cache-mib 512`: the 14 host-expert layers belong to the R9700's share of the layer
  split, and the whole cache (508.50 MiB for 12650 MiB of host experts, 4 groups at
  3.7-4.0% slots) went to the R9700, none to the RTX 4070 SUPER. Size the cache against
  the free VRAM of the card holding the first layers, not by the split proportions.

Decode at `n-cpu-moe 48`, ctx 32768, `load-mode none`, 200 tokens (t/s; hit rate at
ubatch <= 8):

| `moe-cache-mib` | R9700 | RTX 4070 SUPER |
| --- | --- | --- |
| 0 | 14.1, 11.6 | 22.7, 22.9 |
| 2048 (~4.2% slots) | 3.0 (50%) | 23.2 (51%) |
| 4096 (~8.5% slots) | 4.0 (64%) | 23.9, 19.7 (62-64%) |
| 6144 (~12.8% slots) | n/a | 25.0 (71%), but 1.2 GiB over the free VRAM: the fit cannot correct it with `-ngl` set, so the driver spills to shared memory; not a usable setting |

With `mmap` the cache is a loss on both cards. RTX 4070 SUPER: 17.1 and
14.0 t/s without the cache, 13.4 (50%) at 2048 MiB, 10.4 (62%) at 4096. R9700: 12.3
without, 2.8 at 2048 MiB.

**The same VRAM as layers or as cache** (RTX 4070 SUPER, `load-mode none`, ctx 32768,
two runs each; a layer of experts is ~1.1 GiB on the card):

| configuration | projected VRAM | decode t/s | hit rate |
| --- | --- | --- | --- |
| `n-cpu-moe 45`, no cache | 9572 MiB | 25.8, 25.4 | n/a |
| `n-cpu-moe 46`, 1024 MiB | 9453 MiB | 19.7, 19.4 | 39% |
| `n-cpu-moe 47`, 2048 MiB | 9416 MiB | 21.8, 21.6 | 50% |
| `n-cpu-moe 48`, 3072 MiB | 9136 MiB | 23.8, 23.4 | 59% |

*Linux*, both cards, the preset of the *Linux* rows with 12 threads, the cache on the R9700
(three runs each):

| configuration | cache | decode t/s | hit rate |
| --- | --- | --- | --- |
| `n-cpu-moe 14`, no cache | n/a | 27.4 ±0.2 | n/a |
| `n-cpu-moe 15`, `moe-cache-mib 896` | 892.97 MiB for 13712.50 MiB of host experts | 17.8 ±0.0 | 45% |
| `n-cpu-moe 16`, `moe-cache-mib 1792` | 1789.53 MiB for 14675.00 MiB | 19.9 ±1.4 | 65% |
| `n-cpu-moe 17`, `moe-cache-mib 2688` | 2683.30 MiB for 15737.50 MiB | 22.5 ±0.1 | 74% |

The smallest cache is the worst, so no "one hot layer in cache" sweet spot exists here.

*Linux*, the same with `no-host` (section 2): the CPU repacks the IQ4_NL `ffn_down_exps`, so
only the five host layers with Q2_0 ones (1, 8, 10, 12, 13; 3787.50 MiB) stay cacheable,
and the cache spends far fewer synchronizations per token:

| configuration | decode t/s, three prompts | hit rate |
| --- | --- | --- |
| `n-cpu-moe 16`, no cache | 27.0 (26.7, 27.2, 27.2) | n/a |
| `n-cpu-moe 17`, `moe-cache-mib 1024` | 26.0 (25.9, 26.2, 25.8) | 80% |
| `n-cpu-moe 18`, `moe-cache-mib 2048` | 26.6 (26.8, 26.4, 26.5) | 93% |

Close to even, no longer a loss of 18-35%, but still no gain: at 93% hits the cache still trails
the two resident layers it replaced. A large cache does no better: without `no-host`, `n-cpu-moe 28` and
`moe-cache-mib 12000` (11993.90 MiB for 26875.00 MiB of host experts, every host layer
cacheable) decode 25.8 t/s on the three prompts (25.7, 26.6, 25.0) at 92% hits.
The cache's log has no per-layer statistics (`log_stats` prints two totals); the
aggregate is enough for the verdict, and the per-layer spread would only steer a manual
`override-tensor`, which is worth it only if an upstream fix still leaves the cache losing.

**The hit rate is high; serving the misses is what costs.** 4.2% of the experts as slots
gives 50% hits, 8.5% gives 62-64%: the routing is skewed. Each miss is served per layer
(`llama-moe-cache.cpp`, v0.7.0): the router's ids are read back with a
`ggml_backend_synchronize` (48 stalls per token), then one `ggml_backend_tensor_set_async`
per run of consecutive experts and per tensor, ~15 small transfers per layer and ~720 per
token at 0.3-1 MB each. On the R9700 the cache moves ~1.3 GB/s, far below any PCIe
link, so bandwidth is not the limit. (330 - 70) ms per token over ~720 transfers gives
~0.36 ms each, a fixed cost per transfer on the HIP path on Windows (WDDM submission);
on the RTX 4070 SUPER the same count fits within 44 ms per token. **That 0.36 ms is a
derived estimate, not a measurement**: the difference of two mean token times over an
estimated count of transfers, with no trace behind it (section 6).

Two changes that would target the cost, for an upstream issue:

- **Defer the router read by one layer.** Read layer l's ids while layer l+1 computes
  and start its uploads a layer late: one layer of latency on the first token, no
  synchronous stall per layer.
- **Stage the misses contiguously.** Copy a layer's missing experts host-side into one
  pinned staging area, then one `tensor_set_async` per tensor: 3 transfers per layer
  instead of ~15, which matters where the cost is per call.

## 4. Known failures

| configuration | result |
| --- | --- |
| `load-mode none` with experts on the GPU (`n-cpu-moe 26`) | aborts right after the weight upload: `ROCm error: unspecified launch failure` in `ggml_cuda_set_device`, at ctx 4096 and 32768, with and without the MoE cache (4 of 4 runs). It fails at load, not at decode, which points to a host buffer `none` leaves unpinned or an allocation it does not expect; worth a minimal upstream reproduction. Windows only: on *Linux* the same mode with the experts of 34 layers on the GPUs (R9700 and RTX 4070 SUPER in one model) loads and runs in every run since 2026-10-14, v0.6.0 and v0.7.0 |
| `mmap` with experts in RAM | llama.cpp warns "tensor overrides to CPU are used with mmap enabled - consider using --load-mode none"; at `n-cpu-moe 48` decode drops by up to 12.5% on the R9700 and by 25 to 39% on the RTX 4070 SUPER |

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| the MoE cache, to keep hot experts in VRAM | refuted: 3 to 4.6x slower decode on the R9700; on the RTX 4070 SUPER the same VRAM as resident layers is 8-31% faster, on Linux with two cards 22-54% faster, and with every expert in RAM the cache is noise (`none`) or a loss (`mmap`) (section 3) |
| `--no-repack`, to let repacked layers into the MoE cache | inert: nothing is repacked unless `--no-host` is set |
| a larger `ubatch-size`, paid for with experts in RAM, with `op-offload false` | refuted for prefill (with `op-offload true` it wins, section 8). *Linux*, `n-cpu-moe 26`, ctx 262144, `f16`, 43.7k-token prompt: 248 and 244 t/s at 512, 281 and 282 at 1024, 305 and 308 at 2048; with the preset's ctx 131072 and `q8_0` at 2048, 319 and 313; `n-cpu-moe 14` at 512 gives 364 and 370. `n-cpu-moe 26` holds more experts in RAM than 2048 needs at ctx 131072 (512 needs 15 there, section 2), so the true loss is smaller than this |

## 6. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| HIP upload cost per miss | n/a | a profile of the miss path (sync per layer, transfers per token) on ROCm against CUDA | TBD: needs a trace (rocprofv3, nsys), not a bench run |
| `load-mode none` abort | n/a | minimal reproduction for upstream; Windows only (not on Linux ROCm, section 4), but also CUDA/Vulkan on Windows? | TBD |

## 7. Two cards on Linux: quant and n-gram drafters (v0.6.0)

Both cards in one model as in the *Linux* rows above, each quant with the lowest
`n-cpu-moe` that fits (IQ3_S 14, IQ3_XXS 7; the drafter sweeps below ran at IQ3_XXS `n-cpu-moe` 7 and 8), the same other keys. The drafters change the
greedy path (the verify batches differ numerically), so two runs of one setting can
produce different texts: read the per-run figures, and compare settings on runs that
produced the same kind of output.

| | IQ3_XXS | IQ3_S |
| --- | --- | --- |
| prefill at 43.7k tokens | 478, 485 | 342, 338 |
| decode at 43.7k tokens | 24.0 ±0.2 | 20.9 ±0.1 |
| code rewrite (3.8k-token prompt, ngram-map-k) | 49.9, 56.6, 64.1 | 47.0 ±2.0 |
| prose reasoning (ngram-map-k) | 40.0 ±1.6 (at `n-cpu-moe 8`) | 31.8 ±1.0 |
| quality, model card (BF16 = 93.12) | 92.57 | 93.26 |

The prose gap is partly the drafter's: the IQ3_S text gave it 576
drafts against 1058.

N-gram drafters on the code-rewrite prompt (IQ3_XXS, 8000 tokens max, two runs; decode t/s):

| setting | decode | note |
| --- | --- | --- |
| none | 28.6 | |
| `ngram-mod` | 52.9 | the second run aborted: `CUDA error: out of memory` in `cudaGraphInstantiate` with ~400 MiB free on the 12 GB card |
| `ngram-simple` | 52.3 ±1.3 | |
| `ngram-map-k` | 61.0 ±2.7 | answer 124 t/s at 87-92% accepted, reasoning 35 t/s |
| `ngram-map-k`, `size-n` 4 / 8 / 12 / 16 | 35.0 / 51.5 / 61.3 / 56.0 | 12, the default, is best |
| `ngram-map-k`, `size-m` 16 / 32 / 48 / 64 | 48.2 / 49.7 / 59.8 / 63.6 | the answer (copied text) rises with the draft length: 97, 113, 132, 156 t/s |
| `min-hits` | inert | read only by `ngram-map-k4v` (`common/ngram-map.cpp`); `ngram-simple` and `ngram-map-k` ignore it |

## 8. Long prompts: experts in RAM, uploaded per ubatch (Strata-style)

The layout of Strata (https://github.com/niko1221/strata, which builds on ggml): experts in
RAM, the hot ones in VRAM, prompts read in ubatches of up to 8192 tokens.

*Linux*, both cards, 12 threads, ctx 131072, `q8_0`, `load-mode none`, `op-offload true`,
`cache-ram 2048` (the pinned experts leave little RAM), placement by `override-tensor`
(`blk\.(...)\.ffn_.*_exps=CPU`) rather than `n-cpu-moe`. With `op-offload true` each ubatch
uploads the host experts and the GPUs compute them, so a large ubatch amortizes the upload;
the compute buffer grows with it (4.8 GiB per card at 4096, 9.5 GiB at 8192). Prefill on the
43.7k-token prompt, decode right after it, and decode on three different prompts after a
warm-up (code, prose, math; 400 tokens each, temperature 0):

| layout | prefill t/s | decode at 43.7k | decode, three prompts |
| --- | --- | --- | --- |
| the *Linux* rows: `n-cpu-moe 14`, ubatch 512, `op-offload false` | 410, 405 | 21.8 | 27.4 |
| experts of layers 25-38 on the R9700, 34 in RAM, ubatch 8192 | 669, 674 | 17.4, 17.5 | 21.6 |
| layers 19-38 on the R9700, 28 in RAM, ubatch 4096 | 578, 581 | 18.4, 18.5 | 22.5 |
| layers 25-38 on the R9700, ubatch 4096, the same VRAM as `moe-cache-mib 8704` (75% hits) | 524, 586 | 15.5, 17.3 | 21.1 |
| every expert in RAM, `moe-cache-mib 28672`, ubatch 512, `op-offload false` | n/a | n/a | 24.8 |

A large ubatch buys 40-65% of prefill for 15-20% of decode: worth it where long prompts
dominate, not for chat. The VRAM a smaller ubatch frees is worth more as resident layers
than as MoE cache. The cache cannot reproduce Strata's decode: a miss is uploaded to the
GPU (section 3), where Strata computes the missing experts on the CPU alongside the GPU.
Every expert in RAM pins ~47 GiB (48284 MiB `ROCm_Host`) and, with a 2964 MiB host compute buffer at ubatch 8192,
ran a 59 GiB host into swap: keep part of the experts in VRAM.

## 9. Two cards on Linux: the preset (v0.7.0)

Both cards, 12 threads, ubatch 512, `op-offload false`, `ngram-map-k`; the 43.7k-token
prompt (two runs of 128 tokens) and three different prompts after a warm-up (code, prose,
math; 400 tokens, temperature 0):

| context, KV | `n-cpu-moe`, split | prefill t/s | decode at 43.7k | decode, three prompts |
| --- | --- | --- | --- | --- |
| 131072, `q8_0` | 14 (+ `blk\.38\.ffn_down_exps=CUDA0`), 40,9 | 397, 414 | 21.8 | 27.1 |
| **131072, `f16`** | **16, 40,9** | 381, 382 | **24.7, 25.1** | **27.4** |
| 262144, `q8_0` | 18, 41,8 | 349, 351 | 20.9, 20.7 | 26.0 |
| 262144, `f16` | 21, 41,8 | 311, 319 | 23.3, 23.4 | 25.2 |

The recommended preset is the
second row plus `no-host = true` (section 2: prefill 416 and 419 instead of 381 and 382, the
rest unchanged). Against the first row: 3% more prefill, the same decode on short prompts,
and 14% more deep in the context.
After a 2.9k-token prompt it leaves ~500 MB free on the R9700 (the drafter included); one
layer more in RAM if hipBLASLt runs out. 262144 costs 14-17% of prefill and 4-8% of
short-prompt decode against 131072 with the same KV type.

**Decode is mostly a fixed cost per token.** At 131072 `q8_0`, decode on the three prompts
with the experts of the first 15 layers on the CPU, of 16 layers with IQ2_S `gate`/`up`, of
14 layers with IQ3_XXS/IQ3_S `gate`/`up` (13.7 to 14.7 GiB each) and of the first 24 layers:
27.3, 27.3, 26.9 and 23.9 t/s. A layer of experts moved to the CPU costs ~1.3% of decode,
whichever quant type it holds, so one or two layers more or less do not show. During decode
the R9700 is ~75% busy and the RTX 4070 SUPER ~5% (`amd-smi`, `nvidia-smi`); the CPU
threads spin, so their load says nothing. Without the drafter, the preset decodes 27.5 t/s
on prose and math and `n-cpu-moe 24` 23.9 on all three, the same as with it: these
400-token reasoning openings give the n-gram drafter nothing to copy (its gain is on
repeated text, section 7).
