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
| `split-mode tensor` | supported for `qwen4exp` since #28569 took it off llama.cpp's deny-list (v0.8.0 `llm_arch_supports_sm_tensor` does not name it); measured in section 11 | `src/llama-arch.cpp`, `gguf::arch_supports_sm_tensor` |

## 2. Recommended settings

| key | value | evidence |
| --- | --- | --- |
| `lazy-mode` | `auto` (the default) | the 26.8 GiB n-gram table is above the 4 GiB threshold and stays mapped; no run read it into memory. *Linux*: `on`, `auto` and `off` decode 25.1 ±1.2, 25.4 ±0.4 and 25.9 ±0.4 t/s, noise; `on` and `auto` both print "lazy read enabled" and map the table (`CPU_Mapped`), `off` reads it into 27466 MiB of RAM for no gain |
| `n-cpu-moe` | the lowest that leaves ~500 MB free on each card (below that, hipBLASLt's workspace runs out on the first real prompt and the process hangs in the driver; a 5-token test does not show it). On a 32 GB card at ctx 32768, `26` (27170 MiB of weights on the card, 1017 MiB compute buffer, ~3 GiB left) | decode 17.8 t/s at `26` against 12.3 at `48` (both `mmap`). *Linux*, ubatch 512, the lowest with >= 512 MB free on the R9700 after a 2.9k-token prompt, decode on the 25-token prompt (two runs): see the table below |
| `device`, `tensor-split` | the RTX 4070 SUPER FIRST, `CUDA0,ROCm0` at `23,26` with `n-cpu-moe 16` (the 4070 takes blocks 0-22, the 16 host-expert layers among them, the R9700 blocks 23-47 plus the output head and the sampling) | *Linux*, v0.7.0, same build and prompts as the row below it: against `ROCm0,CUDA0` at `40,9`, prefill 557.8 ±0.3 against 422.7 ±2.6 t/s at 43.7k (+32%), decode 28.2 ±0.6 against 25.0 (+13%), the three short prompts 31.0, 29.9 and 31.1 against 27.3 (+10 to 14%); free after the long prefill 708 MiB on the 4070 and 556 on the R9700 against 646-828 and 474. `24,25` does not load (`cudaMalloc` out of memory on the 4070's KV buffer): 23 blocks is the 4070's limit at this `n-cpu-moe`, and `n-cpu-moe 15` at `23,26` (layer 15's experts, 962 MiB, onto the 4070) does not load either: weights and KV fit, the 531 MiB compute buffer does not (`failed to allocate compute pp buffers`; campaign `flash-next-ncmoe15-linux-20261021`, no run since the leg failed). The split is sized from the per-layer weights (`llama-gguf <shard> r` lists every tensor's bytes: experts 738 to 1300 MiB per layer, the late layers heavier), not mirrored from `40,9` (campaign `flash-next-device-order-linux-20261021`). The IQ3_XXS preset (`q8_0` KV, `n-cpu-moe 7`) follows: `CUDA0,ROCm0` at `19,30` against `ROCm0,CUDA0` at `38,11`, prefill 776.7 ±15.8 against 580.8 ±10.6 t/s (+34%), decode 27.8 ±0.4 against 26.1 ±0.8 (+6.5%), free after the prefill 72 MiB on the 4070 and 386 on the R9700 against 44 and 434; `20,29` does not load. With `q8_0` KV the compute buffer is 874 MiB per card at `ubatch 512` instead of the 531 and 592 of `f16`, and the runtime pools take ~320 MiB on the 4070 and ~510 on the R9700 beyond the log's buffers: that is where the margin went (campaign `flash-next-xxs-device-order-linux-20261021`) |
| `ubatch-size` with the 4070 first | `512` | *Linux*, v0.7.0, `CUDA0,ROCm0` at `23,26`: `768` buys 3% of prefill at 43.7k (574.8 ±2.0 against 557.8 ±0.3 t/s) and loses decode (25.9 ±3.2 against 28.2 ±0.6: one repetition at 23.7), with the compute buffers at 795 and 885 MiB instead of 531 and 592, which leaves 440 MiB free on the 4070 and 252 on the R9700 after the prefill, half the margin the R9700 needs. The buffers grow ~1.0 MiB per ubatch token on the 4070 and ~1.15 on the R9700 with this model (campaign `flash-next-ubatch768-linux-20261021`) |
| `moe-cache-mib` | `0` (off), with no condition, on llama.cpp v0.7.0 (`620d2225c`; the cache dates from #29887, 2026-10-07): a cache policy upstream changes would reopen it | R9700: a loss of 3-4x (14.1 to 3.0-4.0 t/s). RTX 4070 SUPER: at equal VRAM, experts kept on the card beat the cache at every split (25.6 t/s against 19.5-23.6), and with every expert in RAM it is noise (section 3). *Linux*, two cards, the cache on the R9700 in place of resident layers: 17.8 to 22.5 t/s against 27.4 (section 3) |
| `load-mode` | `none` whenever experts sit in RAM. On Windows/ROCm with experts on the GPU, `mmap`, set explicitly, until the abort of section 4 is fixed; on Linux `none` there too (no abort). `auto` resolves to `none` only when a device THE MODEL RUNS ON reports no mmap support (`llama-model.cpp`, `load_tensors`): a Ryzen iGPU visible to HIP counts only when `device` is left blank; with `device` pinned to the discrete cards `auto` is `mmap` | `n-cpu-moe 48`, decode with `none` against `mmap`: R9700 11.6 to 14.1 against 12.3; RTX 4070 SUPER 22.7 to 22.9 against 14.0 to 17.1. `none` loads slower (25 to 43 s against 10 to 11 s). `n-cpu-moe 26` with `none` aborts on the R9700 on Windows. *Linux*: `none` 25.6 ±0.1 t/s; `mmap` 23.9 ±2.6 (21.0 on the first run after the load, 24.9 and 25.9 after); `auto` resolved to `mmap` (log), 24.7 ±1.7. (That AMD advises against `mmap` on ROCm is a reported recommendation with no primary source linked here.) |
| `repack` | leave unset | it acts only with `no-host` (next row): without it the GPU's pinned host buffer type is picked before `CPU_REPACK`, so nothing is repacked: the MoE cache layout is identical with and without `--no-repack` (section 3). *Linux*, `n-cpu-moe 14`, `moe-cache-mib 512`: the same 4 groups and 508.50 MiB with and without `--no-repack`, no `CPU_REPACK` buffer in either log |
| `no-host` | `true` with `op-offload false` | weights kept in RAM skip the GPU's pinned host buffer (`ROCm_Host`), so the CPU repacks the types it has interleaved kernels for: here `ffn_down_exps` in IQ4_NL (4950 MiB of the 14997 MiB in RAM at `n-cpu-moe 16`; `gate`/`up` in IQ2_S/IQ3 stay as they are, 10047 MiB of ordinary RAM). The pinned buffer only speeds the per-batch copies of `op-offload true`, which `false` never makes. *Linux*, the recommended preset (section 9), 43.7k-token prompt: prefill 416 and 419 t/s against 383 and 382, decode 25.0 against 24.9, three prompts 27.3 both. With `op-offload true` (section 8) the copies would come from pageable memory: not measured (campaign `flash-next-no-host-20261016`) |
| `spec-type` | `ngram-map-k`, `spec-ngram-map-k-size-m 64` (`size-n` left at 12) | no MTP head in the main GGUF; a separate MTP head file exists since 2026-10 and wins on two R9700s (section 11, MTP). *Linux*, v0.6.0, IQ3_XXS (section 7): code rewrite 61-65 t/s (`size-m` 48 and 64) against 28.6 with none; prose reasoning 40.0 ±1.6 against 29.4 ±1.3 |
| prefill | (measured) | *Linux*, ubatch 512: 368 and 372 t/s at 2.9k tokens, 364 and 370 at 43.7k, 340 and 341 at 87.4k; decode right after: 24.5, 20.5 and 17.5 t/s. v0.6.0 gave 342 and 338 at 43.7k |
| `threads` | one per physical core (12 on the 12-core host); `ThreadsBatch` the same on Linux, 24 (every logical core) on Windows | *Linux*, 43.7k-token prompt, two runs of 128 tokens: prefill 410 and 405 t/s at 12, 363 and 368 at 16, 371 and 377 at 22, 364 and 368 at 24; decode 21.8, 21.5, 21.1 and 17.6. *Windows*: prefill 275.7 and 279.0 at 24 against 250-262 at 12, decode unchanged (section 10, campaign `flash-next-cpu-prefill-windows-20261021`). Checked again on *Linux* with the RTX 4070 SUPER first (`23,26`, `no-host`): one 43.7k repetition at 24 gave 481.8 t/s prefill and 28.1 decode against 557.8 and 28.2 at 12 (run 47 of `flash-next-device-order-linux-20261021`), the sweep stopped there (not recorded): the loss holds on Linux whatever the placement |
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
cacheable) decode 25.8 t/s on the three prompts (25.7, 26.6, 25.0) at 92% hits (campaign `flash-next-moe-cache-20261016`).
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
| Vulkan (RADV) on the R9700 instead of ROCm, for the IQ expert kernels | refuted. *Linux*, the recommended preset with the R9700 as `Vulkan2` (the RTX 4070 SUPER stays on CUDA): prefill 330 and 336 t/s at 43.7k tokens against 416 and 419, decode right after 7.6 and 9.6 against 25.0, 5.1 on a short prompt |

## 6. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| HIP upload cost per miss | n/a | a profile of the miss path (sync per layer, transfers per token) on ROCm against CUDA | TBD: needs a trace (rocprofv3, nsys), not a bench run |
| `device` order on *Windows* | the R9700 first | does the RTX 4070 SUPER first pay there too (it did on *Linux*, section 2, and the 27B's order reverses between the two OSes, `qwen3.8_27b.md`)? | TBD |
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
rest unchanged) and, since 2026-10-21, the RTX 4070 SUPER first (`device CUDA0,ROCm0`,
`tensor-split 23,26`, section 2: prefill 557.8, decode 28.2 at 43.7k and 29.9 to 31.1 on the
short prompts, campaign `flash-next-device-order-linux-20261021`). Against the first row:
3% more prefill, the same decode on short prompts, and 14% more deep in the context, before
the device order added its 32% and 13%.
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

## 10. Windows: the band kernel on GQA 12, and the prefill gap (v0.7.0)

Measured on 2026-10-21 on the Windows machine of section 9's hardware (R9700 as `ROCm1`,
RTX 4070 SUPER as `CUDA0`, Ryzen 9 9900X, 12 threads), the preset of section 9 with
`device = ROCm1,CUDA0` and `override-tensor = output\.weight=ROCm1` (the device names
differ from Linux, nothing else), the build tree's `llama-server` launched exactly as the
configurator launches it (`--models-preset`, port 8599, `-t 12 --threads-batch 12`,
`GGML_HIP_FA_BAND_WMMA=4`, hipBLASLt off), temperature 0, `cache_prompt false`, 3 runs
per cell. Not in `bench.db`: the installed server was not the one under test.

**The band on GQA 12.** Patch 0003 (`patches\llama.cpp\README.md`) now sends GQA ratios
above 8 to the band WMMA kernel, so on the R9700 this model's 24-over-2 attention runs the
MMA kernel in two passes instead of the tile kernel. Cap 8 (the tile kernel, the behaviour
before) against cap 12 (the band, the default now), through `GGML_HIP_FA_BAND_WMMA_GQA_MAX`:

| prompt | tokens out | prefill t/s, cap 8 | cap 12 | decode t/s, cap 8 | cap 12 |
| --- | --- | --- | --- | --- | --- |
| 43,730 tokens | 128 | 258 ±3 | 256 ±6 | 20.1 ±0.2 | 20.5 ±0.5 |
| 2,620 tokens | 400 | 252 ±6 | 258 ±2 | 19.2 ±0.1 | 19.0 ±0.2 |

Noise both ways, and the output is byte-identical in every pair, with the drafter and
without it. That is what the kernel figures predict: with one query per step the band is
12% faster than the tile at 131k and 5% at 65k (`test-backend-ops perf`, the README),
and attention is ~5% of a decode step at 43.7k; its real gain, 55-59% at 65k-131k, is on
2 to 4 queries per step, which only an accepted draft produces, and the n-gram drafter
drafted nothing on these prompts. The small-context cost (+16 us per call at 4k, ~0.4% of a
step) does not show either. The band stays on for GQA 12 for the long-context,
accepted-draft case; nothing to tune in the preset. **Linux says the same** (campaign
`flash-next-patches-linux-20261021`, the installed server rebuilt with patches 0001-0004
against run 6 of `flash-next-no-host-20261016`, patch 0001 only, same preset and prompts):
43.7k-token prompt prefill 422.7 ±2.6 against 417.9 ±2.2 t/s, decode 25.0 against 25.0-25.1;
the code, prose and math prompts decode 27.3 both times. The band is live (the server logs
its enable line), `test-backend-ops` passes 135/135 GQA 12 cases on `ROCm0`, and patch
0004's pinned buffers survive the prompts of different shape that aborted its first version
on Windows. Free memory after the prefill: 474 MiB on the R9700, 828 on the RTX 4070 SUPER.

**Windows is 36% behind Linux on prefill, 20% on decode.** Same preset, same model files,
same llama.cpp (v0.7.0, patches 0001 and 0003/0004 make no difference here), same cards:

| system | prefill t/s at 43.7k | decode t/s, 128 tokens at 43.7k |
| --- | --- | --- |
| Linux (section 9, `bench.db` campaign 2) | 416, 419 | 25.0, 25.1 |
| Windows (this section, cap 12) | 250-262 | 20.4 ±0.5 |

The 27B, with every expert on the GPUs, loses 5% of its long-prompt prefill to Windows
(`docs\qwen3.8_27b.md` section 4, chipset link of the second card); this model loses 36%,
and what it has that the 27B has not is 16 layers of experts on the CPU (`n-cpu-moe 16`,
`op-offload false`): during prefill the experts of those layers run on the 12 CPU threads,
and during decode their weights stream from RAM. So the gap is the CPU side on Windows,
not the cards. The Windows build runs `ggml-cpu-zen4.dll` (AVX-512, REPACK, LLAMAFILE,
OpenMP through `libomp140`), the same feature set as Linux.

**The CPU launch levers** (same day, the tree build on port 8599 with the launch line above
plus the lever, one 43.7k run each, temperature 0; the one that pays confirmed on the
installed server in campaign `flash-next-cpu-prefill-windows-20261021`):

| lever | prefill t/s at 43.7k | decode t/s, 128 tokens |
| --- | --- | --- |
| none (above) | 250-262 | 20.4 ±0.5 |
| `--threads-batch 24` (the 24 logical cores for prefill, 12 for decode) | 273.2; installed server: 275.7, 279.0 | 20.6; 20.8, 20.8 |
| `--poll 0` | 266.0 | 21.1 |
| `--cpu-strict 1` | 268.7 | 21.2 |
| `--prio-batch 2` | 256.4 | 21.2 |
| the first three together | 274.6, 280.5 | 19.6, 20.8 |
| `--threads-batch 24 --cpu-mask 555555 --cpu-mask-batch FFFFFF --cpu-strict 1` (one decode thread per physical core, the batch on every logical one) | 278.5 | 20.7 |
| `--threads-batch 24` on a `ggml-cpu-zen4.dll` built with `GGML_OPENMP=OFF` (ggml's own threadpool) | 178.6 | 20.5 |

Only the batch threads pay, +6-9% prefill with decode unchanged, and the three together add
nothing over them: Windows is not losing the prefill to how it schedules the spinning
threads, and pinning one decode thread per physical core (without a mask, strict placement
puts thread i on logical CPU i, two per core on Windows) changes nothing either.
`ThreadsBatch = 24` is now the Windows `server.ini` value (server-wide; on Linux 12 measured
best, the settings table). The OpenMP runtime is not the culprit: without it prefill loses a
third, so the build keeps cmake's default `GGML_OPENMP=ON`. The gap to Linux stays at 33%.

**Every expert in RAM with the VRAM as MoE cache does not fit this machine.** `n-cpu-moe
48`, `moe-cache-mib 29000` (sized on the free VRAM after a load without the cache: 23.7 GB
on the R9700 for 39 GB of host experts, 5.3 GB on the RTX 4070 SUPER for 9 GB; `no-host`
with `repack false`, since the cache cannot hold repacked experts and a pinned buffer of
that size is impossible): the experts alone are 48.3 GB of RAM (`CPU model buffer size`)
on a 61.6 GB machine, free RAM fell to 0.6 GB and the 55-token warm-up ran at 10 t/s
prefill, 6 t/s decode, paging. Stopped before the long prompt; a speed figure would need
~128 GB of RAM.

**`op-offload true` at ubatch 512 is a loss on Windows**: the installed server, the
preset otherwise unchanged, prefilled the 43.7k prompt at 108 t/s (steady from 70% to
89% of the prompt, where the run was cancelled; not in `bench.db`), against 277 with
`op-offload false`. On Linux the same switch pays only with ubatch 4096-8192 and more
experts in RAM (section 8); on Windows the per-ubatch upload of the host experts costs
more than the CPU matmuls it replaces, so the large-ubatch variant was not tried.

Not pursued (the gap is too large for preset levers to close, and the model is not the
daily one on Windows): `llama-bench` on the CPU backend alone, Windows against Linux (is
the CPU backend itself a third slower here: the clang build of the zen4 variant, page
faults under `load-mode none`), which would decide whether the fix is in the build or in
the preset; which 16 layers to keep in RAM; a requant of the host layers to a repackable
type (Q4_0, Q8_0).

## 11. Two R9700s on Linux (v0.8.0)

The host now carries two R9700s and no RTX 4070 SUPER: one in the CPU's Gen5 x16 slot
(`ROCm0`), one behind the chipset at Gen4 x4 (`ROCm1`) with no PCIe AtomicOps, so RCCL does
not run; `split-mode tensor` needs the internal AllReduce (`CudaAllreduce = internal`), last table of this section. IQ3_S, `f16` KV, 43.7k-token
prompt with the prompt cache off, 128 tokens out, `Threads` 12. This replaces section 9 on
this hardware.

| key | value | evidence |
| --- | --- | --- |
| `n-cpu-moe` | unset: every expert in VRAM | prefill 1978.8 ±5.7 against 570 t/s at `n-cpu-moe 16`, decode 33.9 against 26.2 (campaign `flash-next-2xr9700-linux-ncm-20261028`) |
| `device`, `tensor-split` | the x16 card first, `1,1`; `24,25` with the MTP head (section 11, MTP) | the x16 card last: prefill 2046.6 ±23.7 (+3%), decode 33.3 (-2%); the decode wins (campaign `flash-next-2xr9700-linux-device-20261028`) |
| `ctx-size` | `262144` | at ubatch 512 it runs at the speed of 131072: prefill 1962.3 ±17.7, decode 34.0; free after the prefill 1872 and 772 MiB (campaign `flash-next-2xr9700-linux-ctx-20261028`) |
| `ubatch-size` | `512` at 262144 | `1024` prefills 2273.1 ±3.8 (+15%) and `2048` 2312.8 ±0.8 at 131072 (campaign `flash-next-2xr9700-linux-ubatch-20261028`), but at 262144 the larger compute buffer leaves no margin on the second card; moving `blk.25`'s `gate`/`up` experts to the first card to make room for `1024` gave 2044 / 33.0, not taken |
| `spec-type` | `ngram-map-k4v,draft-mtp` with the MTP head, `spec-draft-n-max 3`, `spec-ngram-map-k4v-size-m 64` | section 11, MTP. Before the MTP head, n-gram drafters alone, live on a coding agent's turns (server log, not in `bench.db`), decode t/s per 20k-token depth band, turns in brackets: `ngram-map-k` 30.5 (4) at 80k, 30.5 (6) at 100k, 31.0 (20) at 120k, 28.3 (18) at 140k; `ngram-map-k4v` 35.2 (19), 41.6 (3), 29.0 (12), 27.7 (9); `ngram-map-k4v,ngram-mod` 34.3 (16) at 100k, 28.8 (6) at 120k; no drafter 28.0 (12) at 140k |

Against the RTX 4070 SUPER + R9700 preset of section 9 (v0.7.0): prefill 558 to 1962 t/s
(x3.5), decode 28.2 to 34.0 (+21%).

**MTP head (separate file).** ggml-org publishes the MTP layer as its own GGUF
(`mtp-Qwen3.8-Flash-Next-Q4_0.gguf`, 2.05 GiB: one MoE layer plus its own `output.weight` and
`token_embd.weight`), loaded with `model-draft`, `spec-type draft-mtp` and `device-draft` on the
first card. Its context is the target's (`common/speculative.cpp`: the draft context holds as
many tokens per sequence as the target), so at 262144 it adds 1746 MiB of weights (the
`token_embd` stays in host RAM), 640 MiB of `f16` draft KV and a 566 MiB compute buffer on that card.

| configuration | result |
| --- | --- |
| `f16` KV, `1,1` | does not load: the draft's 512 MiB KV buffer fails on the first card (1924 MiB free before the draft) |
| `q8_0` KV, `1,1` | loads with 9 MiB free on the first card after an 8-token request: below hipBLASLt's floor. `q8_0` frees 1800 MiB of KV per card but the compute buffers grow from 867 / 942 to 1404 / 1479 MiB |
| `q8_0` KV, `24,25` (block 24, 1038 MiB, to the second card) | loads; after the 43.7k prefill 882 / 814 MiB free at `spec-draft-n-max 3` (background sampler minimum 852 / 732) |

At `q8_0`, `24,25`, 262144, 43.7k-token prompt, temperature 0:

| `spec-draft-n-max` | decode, HIP graphs on | decode, graphs off | accepted (on) |
| --- | --- | --- | --- |
| 1 | 41.5 +-0.7 | 38.1 +-0.6 | 79% |
| 2 | 45.3 +-1.4 | 43.7 +-0.2 | 61% |
| 3 | 54.9 +-0.4 | 50.1 +-1.0 | 59% |
| 4 | 45.1 +-1.2 | 46.1 +-1.0 | 44% |

Prefill 1514 to 1542 t/s on every leg (campaigns `flash-next-mtp-graphs-2xr9700-linux-20261030`
and, graphs off through `CudaDisableGraphs = true`, `flash-next-mtp-2xr9700-linux-20261030`).
Against the `f16`, `1,1`, n-gram preset above: decode 34.0 to 54.9 (+61%), prefill 1962 to 1520
(-23%). The prefill loss is the MTP head's, not the `q8_0` KV's: the same preset without
`model-draft` prefills 2020.4 +-2.9 (next table).

`spec-type` takes a list, and the n-gram drafters are always tried before the MTP head
whatever the order written (`common_speculative_init`): a step where the n-gram finds a match
drafts from the context (up to `size-m` tokens), the others fall back to the MTP head.
`ngram-map-k4v,draft-mtp` (`size-m 64`, `n-max 3`): on a code rewrite (rename one function in a
356-line Rust file, 4381-token prompt) 280.1 +-37.6 t/s against 95.3 +-0.2 for `draft-mtp` alone
(answer 374.7 against 96.2, reasoning 72.0 against 73.8); on the 43.7k prose prompt 56.3 +-1.4
against 54.9 +-0.4, no loss (campaign `flash-next-mtp-ngram-2xr9700-linux-20261030`).

Each drafter against none, same preset (`q8_0`, `24,25`, 262144, HIP graphs on; the two legs
without the MTP head also drop `model-draft`, or llama.cpp reads `draft-mtp` back from its
header), decode t/s:

| `spec-type` | 43.7k prose | code rewrite | prefill at 43.7k |
| --- | --- | --- | --- |
| none | 31.1 +-0.1 | 36.2 +-0.0 | 2020.4 +-2.9 |
| `ngram-map-k4v` | 31.2 +-0.0 | 274.5 +-16.4 (answer 348.1) | 2030.2 +-10.0 |
| `draft-mtp`, `n-max 3` | 54.9 +-0.4 | 95.3 +-0.2 (answer 96.2) | 1520.3 +-1.4 |
| `ngram-map-k4v,draft-mtp` | 56.3 +-1.4 | 280.1 +-37.6 (answer 374.7) | 1531.2 +-4.1 |

The two drafters cover each other's gap: the n-gram finds nothing to copy in prose (31.2
against 31.1) and the MTP head lifts it by 77%; on a rewrite the n-gram drafts whole runs of
the context and the MTP head adds little on top. The reasoning phase of the code prompt
(35.7 without the MTP head, 72.0 to 73.8 with it) is prose again. The chain costs 25% of
cold prefill (campaign `flash-next-mtp-base-2xr9700-linux-20261030`).

Where the decode time goes (live, a coding agent at ~75k tokens, 31.5 t/s): per decode pass
(~1.3 tokens with the drafter) ~41 ms, of which 12.5 ms of kernels on the first card and 12.9
on the second (`rocprofv3 --attach`); the host thread sits at 100% CPU, ~75% of it inside the
HSA runtime waiting on the GPUs (`perf`), under 0.5% in `libggml-hip`. The gaps are the hand-off
between the two cards of a layer split, not host work: no preset key reaches them. A
tool-call grammar (it disables backend sampling) costs nothing measurable: 37.5 / 37.9 / 38.0
t/s without tools against 37.7 / 36.8 / 37.9 with two tools, same short prompt, temperature 0.

`split-mode tensor` with the internal AllReduce (`CudaAllreduce = internal`, the second card has
no PCIe AtomicOps so RCCL is out), same preset otherwise (`ngram-map-k4v`, 262144, ubatch 512),
live on the same coding agent's turns (server log, not in `bench.db`):

| | layer split | `split-mode tensor` |
| --- | --- | --- |
| decode at 160k+ tokens | 28.1 t/s (42 turns; drafter 16% of the output) | 31.5 t/s (20 turns; drafter 11%) |
| prompt read | 1773 t/s re-reading 71k tokens | 933 t/s re-reading 166k tokens (not the same depth) |
| free VRAM | 1872 and 772 MiB after a 43.7k prefill | down to 9 MiB on the second card during the agent's turns |

Decode +12% at the same depth with a weaker drafter share; the prompt read drops (the 27B
lost 15% at 115k with the same AllReduce). Flash attention is forced on and backend sampling
falls back to the CPU (`backend sampling not supported with SPLIT_MODE_TENSOR`), which the
agent's tool grammar already did. The margin is the open problem: a smaller `ubatch-size`
or context is needed before this is a safe default.
