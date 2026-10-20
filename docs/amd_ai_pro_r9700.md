# AMD Radeon AI PRO R9700: configuration reference

Reference settings for running llama.cpp on the R9700 (RDNA4, `gfx1201`, 32 GB) under
Windows, through the ROCm (HIP) backend. Values were measured on this framework with
llama.cpp v0.5.0. Section 7 holds what was measured again on Linux, with llama.cpp v0.6.0 unless a row says v0.7.0.

## 1. Runtime prerequisites

| item | requirement | why |
| --- | --- | --- |
| ROCm | TheRock dist, newest stable (`installer\dist-sources.psd1`): its runtime and kernels are bundled by the installer; a build from source reaches them through a machine-wide `HIP_PATH` | rocBLAS/Tensile kernels |
| HIP runtime | the dist's `amdhip64_7.dll`, `amd_comgr.dll`, `rocm_kpack.dll` next to `llama-server.exe` (bundled by the installer) | against the runtime bundled with the display driver, BF16/F16 GEMMs abort at the first prefill, and with a Ryzen iGPU visible no HIP device enumerates |
| GPU targets | `gfx1201` plus the arch of any visible Ryzen iGPU | one visible device without code objects makes every device fail with "device kernel image is invalid" |
| `LLVM_PATH`, `HIP_DEVICE_LIB_PATH` | never machine-wide | the driver's HIP runtime reads `LLVM_PATH`: devices report 0 MiB and children crash |
| `ROCBLAS_USE_HIPBLASLT` | unset (fallback `0`) | needed only without the staged runtime |
| llama.cpp | v0.4.1 or later, never with #28102 reverted | #28102 routes head-size-256 FlashAttention to the WMMA kernel: 93.6k prefill 805 t/s with it, 442 without |
| driver | 26.9.2 or later, once released | from 26.5.1 on, a card driving no display has its VRAM purged after ~10 s idle, followed by a TDR |

## 2. Device facts

- **HIP device ids follow enumeration order**, and a visible Ryzen iGPU takes one of them.
  Select the card by name, never by assuming an id.
- **Usable VRAM is about 31.8 GiB dedicated**: the total minus ~0.8 GiB. The HIP driver
  also keeps ~210 MiB of non-local memory that no log line accounts for.
- **The advertised PCIe link can be misleading.** The card always reports Gen5 x16,
  because that is the link to its own internal switch. The slot's real bandwidth to the
  CPU can be far lower; measure it with `tools\pcie-bench`. It matters for model load,
  snapshot restore and `split-mode tensor`, not for single-card or layer-split inference.

## 3. Settings

### Preset (presets.ini)

| key | value | evidence |
| --- | --- | --- |
| `load-mode` | `none`; on a MoE model with only part of the experts on the card (`n-cpu-moe` above 0 and below the layer count), `mmap`, set explicitly: `auto` resolves to `none` when any device of the model reports no mmap support, as a Ryzen iGPU visible to HIP does | `mmap` leaves ~1 GiB of weights `CPU_Mapped`, which cross PCIe on every batch: prefill 98 vs 571 t/s at 43.7k. With a partial `n-cpu-moe`, `none` aborts right after the weight upload (`ROCm error: unspecified launch failure` in `ggml_cuda_set_device`, 4 of 4 runs, llama.cpp v0.7.0); with every expert in RAM it loads |
| `flash-attn` | omit (`auto` resolves to on) | the #28102 WMMA path; a quantized KV cache needs FlashAttention |
| `cache-type-k/-v` | `f16` at the context you need; `q8_0` only to fit a longer one | *Linux* (section 7): `f16` decodes 11% faster deep in the context, and its compute buffer is smaller. `q5_1` saves ~30% of the KV against `q8_0` but decodes 6% slower; `iq4_nl` has no HIP FlashAttention kernel |
| `ubatch-size` | the largest that fits: `768` or `1024` | against `512`: `768` gives +5.8% prefill at 2.6k and nothing at 93.6k, `1024` +5% at both depths, `256` costs 20%. Decode is unaffected (identical per-step cost). Each ubatch token costs ~0.53 MiB of compute buffer per device |
| `split-mode` (multi-GPU) | `layer` | see section 4 |
| `spec-draft-n-max` (DFlash2) | `7` | best at 2.6k and 93.6k; `1` is slower than no drafter |
| `spec-draft-type-k/-v` | `f16` | a DFlash draft KV is a small window (50 MiB in f16, 27 MiB in q8_0). No measured speed or acceptance difference |

### Server (SDK Tuning, ROCm)

| key | value | evidence |
| --- | --- | --- |
| `HipFaBand` (env `GGML_HIP_FA_BAND_WMMA`) | default (`4`); `0` only to compare | the framework's FlashAttention band (`patches\llama.cpp\0003`): for head size 256, GQA 6 and up to 8 queries (an MTP verify batch), the KV is split across all CUs in one launch instead of the tile kernel upstream picks. `test-backend-ops perf`, f16 KV of 131072 cells, us per call (*Linux*, v0.7.0): 1 query 1085 -> 922 (1.18x), 3 to 4 queries 1494 -> 928 (1.6x); 65536 cells, 4 queries 752 -> 494. 41/41 correctness cases against the CPU reference. `2` helps only at 1 or 2 queries. A `q8_0` KV stays far slower (2895 -> 2388 against 930 in `f16`): the band is an `f16` win. End to end on the 27B: `qwen3.8_27b.md` section 2. Nothing in the log says it is active: a kernel trace shows `flash_attn_ext_f16` in place of `flash_attn_tile` |
| `RocprofAttach` (env `ROCP_TOOL_ATTACH=1`) | off; on while diagnosing | lets `rocprofv3 --attach <pid>` trace the running server (kernel names and durations per step); no measured cost when nothing is attached |

### Quantization on RDNA4

| quant | result |
| --- | --- |
| Q8_0 | fastest of the 27B files measured, prefill and decode |
| Q5_K_M | ~70% of the size; prefill -13% / -10% and decode -7% / -15% against Q8_0 (2.6k / 93.6k). Worth it only where Q8_0 does not fit |
| Q6_K (any Q6_K-based mix) | slow: prefill -33% against Q5_K_M |

## 4. Multi-GPU

- **The last device in `device` receives the output head, the drafter's logits and the
  sampling.** Put the card with the most free VRAM after its layers there.
- **`split-mode tensor`** reduces through the internal AllReduce on Windows (no RCCL),
  staged through pinned host memory, so the slowest card's PCIe link bounds it. Over a
  ~3 GB/s link that costs ~200 ms per 512-token ubatch, which cancels the prefill gain. It
  only has a fast path between two devices of the same backend. The Linux build links
  RCCL, which replaces that AllReduce when every device of the split is a ROCm one
  (Server tab, SDK Tuning, Tensor-split AllReduce `internal` switches back): not yet
  measured (section 8).
- **`split-mode tensor` with a card of the other backend** (measured on Linux, llama.cpp
  v0.6.0): the model aborts while loading unless Tensor-split AllReduce is `none`. With
  `none`, on a 27B Q8_0 at ctx 16384 with KV `f16` and a 3:1 ratio, 2.7k tokens, against
  the layer split: prefill 599 against 1290 t/s and decode 17.6 against 17.8 without a
  drafter, prefill 577 against 1232 and decode 44.7 against 45.0 with the embedded MTP
  head.

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| a low `spec-draft-n-max`, to stay on the FlashAttention vector kernel | refuted: `n-max 1` gives 9.8 t/s at 93.6k, against 13.0 with no drafter |
| Vulkan instead of ROCm | slower on both halves since #28102: with Q8_0, prefill -27% / -25% and decode -18% / -15% (2.6k / 93.6k), per-step cost +20-24% despite a slightly higher draft acceptance. With Q5_K_M the gap widens: prefill -22% / -23%, decode -41% / -30%, per-step cost +50-55%. Its only edge is a compute buffer ~690 MiB smaller. (On v0.4.0, before #28102, Vulkan had won both.) |

## 6. Checking a configuration

- `tools\report-memory.ps1`: requested vs actual VRAM per device, and spill into shared
  memory. A spill is real only above ~512 MiB beyond the pinned `*_Host` buffers.
- `tools\pcie-bench\`: the card's real host link.
- Benchmark tab / `llama-cpp-config bench sweep`: measure at 2.6k and ~90k context, with
  temp 0 and 3 repetitions.

## 7. Linux

Measured with llama.cpp v0.6.0 (v0.7.0 where a row says so) and the TheRock 10.1.0 dist on the `amdgpu` kernel
driver, with the desktop drawn by another adapter (nothing else on the card). llama.cpp
and ROCm are both newer than in the Windows campaign above: a figure that differs from
sections 2 to 5 is not the OS alone. Model: Qwen3.8-27B Q8_0 with its embedded MTP head,
KV `q8_0`, layer split with a CUDA card. Decode figures are 3 reps of 400 tokens at
temp 0.

| subject | result |
| --- | --- |
| VRAM | 32624 MiB total (`mem_info_vram_total` in the card's DRM sysfs directory). The figures below ran at 32442 MiB used, 182 MiB free, with `mem_info_gtt_used` at its idle 24 MiB throughout |
| run-time growth | 85 to 170 MiB over the idle figure at the first prefill (ubatch `1024`), the same at 2.5k and 43.6k tokens |
| `ubatch-size` | `1024` fits at ctx 262144. Model compute buffer on this card: 1176 MiB at `768` and ctx 196608; at `1024`, 1312 (196608), 1504 (229376) and 1696 to 1736 (262144): it grows with the context as well as with the ubatch |
| prefill, card first with most of the layers | 1122 t/s at 2.4k tokens, 1045 at 43.6k (ubatch `1024`, ctx 262144) |
| last-device role (MTP head, output head, sampling) on this card | costs 1288 MiB of output head, 1024 MiB of draft KV (f16) and 648 MiB of draft compute at ctx 262144. Against the same role on a CUDA card: decode 36.3 -> 38.9 t/s at 2.5k tokens (+7%) and 28.5 -> 29.8 at 43.6k (+4.5%), draft acceptance unchanged; prefill at 43.6k 1045 -> 1010 t/s (-3%) |
| `cache-type-k/-v` `f16` against `q8_0` (llama.cpp v0.7.0) | decode at a 43.7k-token prompt 35.2 and 34.5 t/s against 31.5 and 31.5 (+11%), prefill 1161 against 1164 t/s, short prompts within noise. The KV doubles (64 against 34 KiB per token over 16 layers), but the `q8_0` compute buffer grows with the context: on this card 1712 MiB at 262144 in `q8_0`, 456 MiB at 163840 and 620 at 172032 in `f16`. So `f16` fits ~66% of the context `q8_0` does, not half (campaign `27b-kv-f16-20261016`) |
| decode step anatomy (llama.cpp v0.7.0, kernel trace of the 27B Q8_0 at 72k and 150k tokens, `f16` KV) | the Q8_0 GEMV (`mul_mat_vec_q`) is 61 to 73% of the card's kernel time and reads the weights at ~372 GB/s, 58% of the card's 640 GB/s; FlashAttention is the rest and grows linearly with the context (19% at 72k, 33% at 150k on the tile kernel, before the band). Power 212 to 216 W of 300 with the memory clock at its maximum: the headroom is in the kernels, not the power limit |
| `ubatch-size` (llama.cpp v0.7.0, the 27B Q8_0 at a 115k-token prompt, `f16` KV, layer split with most blocks on this card) | `1024` stays the optimum: prefill 895 t/s at `768`, 902 at `1024`, 837 at `1536`, 809 at `2048`, decode unchanged within the draft's noise. Free VRAM on this card after the prefill: 1232 MiB at `1024`, 659 at `1536`, 75 at `2048` (campaign `coder390-ubatch-20261020`) |
| FlashAttention band, end to end | with the band (`HipFaBand` 4) the 27B at a 115k-token prompt decodes 25.9 against 24.5 t/s with its MTP drafter, prefill unchanged; the MTP verify batch and single-token decode then agree in greedy decoding, where the stock tile kernel diverges (`qwen3.8_27b.md` section 1) |
| checking a configuration | `tools\report-memory.ps1` is Windows-only: read `mem_info_vram_used` and `mem_info_gtt_used` after a prefill |

## 8. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `ROCBLAS_USE_HIPBLASLT` with the staged runtime | unset | is hipBLASLt (`1`) faster than Tensile (`0`) for BF16/F16 models? | TBD |
| `batch-size` | default (2048) | does a larger logical batch help prefill at the chosen `ubatch-size`? | TBD |
| `cache-ram` | 20480 | restore time of the prompt cache over the card's link | TBD |
| `parallel` > 1 | 1 | aggregate throughput versus the context divided per slot | TBD |
| `spec-draft-p-min` | default | does a confidence floor lift acceptance on the reasoning phase? | TBD |
| embedded MTP vs DFlash2 | DFlash2 | speed and memory of each on this card | TBD |
| power limit | stock (300 W) | prefill and decode per watt | TBD |
| `split-mode tensor` on two R9700 | n/a | real AllReduce cost; `GGML_CUDA_AR_COPY_THRESHOLD` / `_CHUNK_BYTES` (seeds: 1-2 MiB, 2 MiB) | TBD |
| Linux: `split-mode tensor` through RCCL | n/a | prefill and decode against `GGML_CUDA_ALLREDUCE=internal` and against the layer split | TBD |
| Linux: over-allocation | n/a | does it fail the load or move buffers to GTT, and at what cost? | TBD |
| `GGML_HIP_FA_BAND_WMMA_SPLIT` | default (one block per CU) | 32, 48, 96 or 128 KV blocks against the default, at 65k and 131k cells | TBD |
| Q8_0 GEMV ceiling | n/a | `llama-bench` on this card alone at depth 0 and 131072 against the 58% of bandwidth seen in the trace (section 7): how much the `mmvq` RDNA4 tuning leaves | TBD |
