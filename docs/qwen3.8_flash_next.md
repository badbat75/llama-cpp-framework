# Qwen3.8-Flash-Next: configuration reference (draft)

Reference settings for Qwen3.8-Flash-Next-GSQ-RCO (architecture `qwen4exp`) on this
framework. Measured with llama.cpp v0.7.0 on Windows, on the AMD Radeon AI PRO R9700
(32 GB, ROCm backend, `ROCBLAS_USE_HIPBLASLT=0`) and, for the MoE cache, the RTX 4070
SUPER (12 GB, CUDA backend), with a 12-core AVX-512 host and 64 GB of RAM. Run logs: `bench\qwen-next-moe-20261016\` under `%LOCALAPPDATA%\llama.cpp\`.
Decode figures are one run of 200 tokens after a 25-token prompt; the prompt figures are
too short to stand for prefill (section 6).

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
| `lazy-mode` | `auto` (the default) | the 26.8 GiB n-gram table is above the 4 GiB threshold and stays mapped; no run read it into memory |
| `n-cpu-moe` | the lowest that fits; on a 32 GB card at ctx 32768, `26` (27170 MiB of weights on the card, 1017 MiB compute buffer, ~3 GiB left) | decode 17.8 t/s at `26` against 12.3 at `48` (both `mmap`) |
| `moe-cache-mib` | `0` (off): on ROCm in every measured case; on CUDA not advised with every expert in RAM; to re-measure with a partial `n-cpu-moe` (section 6) | R9700: a loss of 3-4x (14.1 to 3.0-4.0 t/s). RTX 4070 SUPER: noise with `load-mode none` (22.7 to 23.2-23.9), a loss with `mmap` (section 3) |
| `load-mode` | `none` with every expert in RAM; on ROCm with experts on the GPU, `auto` (mmap) until the abort of section 4 is fixed | `n-cpu-moe 48`, decode with `none` against `mmap`: R9700 11.6 to 14.1 against 12.3; RTX 4070 SUPER 22.7 to 22.9 against 14.0 to 17.1. `none` loads slower (25 to 43 s against 10 to 11 s). `n-cpu-moe 26` with `none` aborts on the R9700. (That AMD advises against `mmap` on ROCm is a reported recommendation with no primary source linked here.) |
| `repack` | leave unset | without `--no-host` the GPU's pinned host buffer type is picked before `CPU_REPACK`, so nothing is repacked: the MoE cache layout is identical with and without `--no-repack` (section 3) |
| `reasoning-preserve` | TBD | the template reads `preserve_thinking`; not yet measured |
| `ctx-checkpoints` | TBD | hybrid memory: a cache reuse past an edited turn needs a checkpoint |

## 3. MoE cache and repack (what the init prints)

- **The cache groups layers by expert tensor types.** At `n-cpu-moe 48` the IQ3_S file
  gives seven groups (IQ4_NL `ffn_down_exps` on some layers, Q2_0 on the others), each at
  ~4.2% of its experts as slots in 2048 MiB, for 47962.50 MiB of host experts.
- **Repack happens only with `--no-host`**, which the framework does not emit. With it,
  `CPU_REPACK` takes 17550 MiB of experts, those layers leave the cache, and 9 layers stay
  cacheable (7112.50 MiB of host experts, ~28.7% as slots in 2048 MiB). Without it, the
  default and `--no-repack` print the same 48-layer cache.
- **Each miss uploads the expert over the host link**: at 50% hits that is ~470 MiB per
  token, so the result follows the card's real link more than its compute.

Decode at `n-cpu-moe 48`, ctx 32768, `load-mode none`, 200 tokens (t/s; hit rate at
ubatch <= 8):

| `moe-cache-mib` | R9700 | RTX 4070 SUPER |
| --- | --- | --- |
| 0 | 14.1, 11.6 | 22.7, 22.9 |
| 2048 (~4.2% slots) | 3.0 (50%) | 23.2 (51%) |
| 4096 (~8.5% slots) | 4.0 (64%) | 23.9, 19.7 (62-64%) |
| 6144 (~12.8% slots) | n/a | 25.0 (71%), but 1.2 GiB over the free VRAM: the fit cannot correct it with `-ngl` set, so the driver spills to shared memory; not a usable setting |

With `mmap` (the default) the cache is a loss on both cards. RTX 4070 SUPER: 17.1 and
14.0 t/s without the cache, 13.4 (50%) at 2048 MiB, 10.4 (62%) at 4096. R9700: 12.3
without, 2.8 at 2048 MiB.

On the R9700 the cache moves ~1.3 GB/s at 2.8 t/s, on the RTX 4070 SUPER ~10.7 GB/s at
23 t/s. Two readings, neither verified yet (section 6):

- **Worst case by design.** At `n-cpu-moe 48` no expert sits in VRAM, so 2-4 GiB of slots
  must serve all 10 experts per token of every layer, out of 24576; with a working set
  that wide the hit rate stays low and each miss is a synchronous upload. The cache is
  meant for a partial `n-cpu-moe`, where it covers only the layers left in RAM: on ROCm
  that is the configuration blocked by the abort of section 4.
- **A backend-specific cost per miss.** The HIP upload path (the copy callback over the
  slot map, `ggml_backend_tensor_set_async` from pinned memory) may be far slower than
  CUDA's: a 3-4x loss against a tie at the same hit rate points to a per-miss cost of the
  backend, or of the card's host link (`tools\pcie-bench\`), more than to the hit rate.

## 4. Known failures

| configuration | result |
| --- | --- |
| `load-mode none` with experts on the GPU (`n-cpu-moe 26`) | aborts right after the weight upload: `ROCm error: unspecified launch failure` in `ggml_cuda_set_device`, at ctx 4096 and 32768, with and without the MoE cache (4 of 4 runs). It fails at load, not at decode, which points to a host buffer `none` leaves unpinned or an allocation it does not expect; worth a minimal upstream reproduction |
| `mmap` with experts in RAM | llama.cpp warns "tensor overrides to CPU are used with mmap enabled - consider using --load-mode none"; at `n-cpu-moe 48` decode drops by up to 12.5% on the R9700 and by 25 to 39% on the RTX 4070 SUPER |

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| the MoE cache, to keep hot experts in VRAM | refuted: 3 to 4.6x slower decode on the R9700, no gain beyond noise on the RTX 4070 SUPER with `load-mode none` and a loss with `mmap` (section 3) |
| `--no-repack`, to let repacked layers into the MoE cache | inert: nothing is repacked unless `--no-host` is set |

## 6. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| prefill | n/a | t/s at 2.6k and ~90k tokens at the chosen `n-cpu-moe` | TBD |
| `n-cpu-moe` | 26 | the lowest that fits at ctx 131072 and 262144 | TBD |
| `cache-type-k/-v` | f16 | does `q8_0` let `n-cpu-moe` drop, and at what decode cost? | TBD |
| `ubatch-size` | default (512) | prefill gain against the compute buffer it costs | TBD |
| `moe-cache-mib` with 3 repetitions | 0 | is the RTX 4070 SUPER's +3-5% at 2048-4096 MiB real? | TBD |
| `moe-cache-mib` with experts on the GPU | 0 | does the cache help once `n-cpu-moe` is below 48? (on ROCm, after the section 4 abort is fixed) | TBD |
| R9700 host link | n/a | measured bandwidth, against the 1.3 GB/s the cache moved | TBD |
| HIP upload cost per miss | n/a | the cache's upload time per expert on ROCm against CUDA, at the same hit rate | TBD |
| `moe-cache-mib` with `--no-host` | n/a | 9 cacheable layers at ~29% slots: decode against no cache | TBD |
| `lazy-mode on` / `off` | auto | decode and RAM use of reading the n-gram table into memory | TBD |
| `ctx-checkpoints` | default | restore cost after an edited turn on the hybrid memory | TBD |
| `threads` | 12 of 24 | does the logical-core count help the CPU experts? | TBD |
| IQ3_XXS against IQ3_S | IQ3_S | decode and quality at the `n-cpu-moe` each one allows | TBD |
| speculative decoding | none | any drafter for `qwen4exp` | TBD |
| `load-mode none` abort | n/a | minimal reproduction for upstream; ROCm only, or also CUDA/Vulkan? | TBD |
