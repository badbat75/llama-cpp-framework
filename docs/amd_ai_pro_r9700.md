# AMD Radeon AI PRO R9700: configuration reference

Reference settings for running llama.cpp on the R9700 (RDNA4, `gfx1201`, 32 GB) under
Windows, through the ROCm (HIP) backend. Values were measured on this framework with
llama.cpp v0.5.0.

## 1. Runtime prerequisites

| item | requirement | why |
| --- | --- | --- |
| ROCm | TheRock dist (`installer\dist-pins.psd1`), `HIP_PATH` machine-wide | rocBLAS/Tensile kernels |
| HIP runtime | the dist's `amdhip64_7.dll`, `amd_comgr.dll`, `rocm_kpack.dll` next to `llama-server.exe` (the installer's hidden `-StageHipRuntime` step) | against the runtime bundled with the display driver, BF16/F16 GEMMs abort at the first prefill, and with a Ryzen iGPU visible no HIP device enumerates |
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

### server.ini

| key | value | evidence |
| --- | --- | --- |
| `LoadMode` | `none` | `mmap` leaves ~1 GiB of weights `CPU_Mapped`, which cross PCIe on every batch: prefill 98 vs 571 t/s at 43.7k |

### Preset (presets.ini)

| key | value | evidence |
| --- | --- | --- |
| `flash-attn` | omit (`auto` resolves to on) | the #28102 WMMA path; a quantized KV cache needs FlashAttention |
| `cache-type-k/-v` | `q8_0` | `q5_1` saves ~30% of the KV but decodes 6% slower; `iq4_nl` has no HIP FlashAttention kernel |
| `ubatch-size` | the largest that fits: `768` or `1024` | against `512`: `768` gives +5.8% prefill at 2.6k and nothing at 93.6k, `1024` +5% at both depths, `256` costs 20%. Decode is unaffected (identical per-step cost). Each ubatch token costs ~0.53 MiB of compute buffer per device |
| `split-mode` (multi-GPU) | `layer` | see section 4 |
| `spec-draft-n-max` (DFlash2) | `7` | best at 2.6k and 93.6k; `1` is slower than no drafter |
| `spec-draft-type-k/-v` | `f16` | a DFlash draft KV is a small window (50 MiB in f16, 27 MiB in q8_0). No measured speed or acceptance difference |

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
  only has a fast path between two devices of the same backend.

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

## 7. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `ROCBLAS_USE_HIPBLASLT` with the staged runtime | unset | is hipBLASLt (`1`) faster than Tensile (`0`) for BF16/F16 models? | TBD |
| `batch-size` | default (2048) | does a larger logical batch help prefill at the chosen `ubatch-size`? | TBD |
| `cache-type-k/-v` `f16` | `q8_0` | speed gained versus twice the KV memory | TBD |
| `cache-ram` | 20480 | restore time of the prompt cache over the card's link | TBD |
| `parallel` > 1 | 1 | aggregate throughput versus the context divided per slot | TBD |
| `spec-draft-p-min` | default | does a confidence floor lift acceptance on the reasoning phase? | TBD |
| embedded MTP vs DFlash2 | DFlash2 | speed and memory of each on this card | TBD |
| power limit | stock (300 W) | prefill and decode per watt | TBD |
| `split-mode tensor` on two R9700 | n/a | real AllReduce cost; `GGML_CUDA_AR_COPY_THRESHOLD` / `_CHUNK_BYTES` (seeds: 1-2 MiB, 2 MiB) | TBD |
