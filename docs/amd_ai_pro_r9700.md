---
title: AMD Radeon AI PRO R9700
type: gpu
status: reference
gpus: [AMD Radeon AI PRO R9700]
models: []
platforms: [windows, linux]
llama-cpp: v0.5.0 to v0.8.0
updated: 2026-11-01
---

# AMD Radeon AI PRO R9700: configuration reference

How to run llama.cpp on the R9700 (RDNA4, `gfx1201`, 32 GB) through the ROCm (HIP) backend, on Windows
and on Linux. Figures come from llama.cpp v0.5.0 on *Windows* and v0.6.0 or v0.7.0 on *Linux*, mostly
with Qwen3.8-27B Q8_0; two cards are v0.8.0. Backend and driver versions differ between the two systems,
so a difference between them is not the OS alone. Model-specific detail is in the
[Qwen3.8-27B](qwen3.8_27b.md) and [Qwen3.8-Flash-Next](qwen3.8_flash_next.md) guides.

**In short.** Use the ROCm backend with `f16` KV at the context you need and `ubatch-size 1024`. For two
cards on Linux, use the tensor split with an even `tensor-split`: it decodes 48 to 65% faster than the
layer split for 16 to 22% less cold prefill.

## 1. Runtime prerequisites

| item | requirement | why |
| --- | --- | --- |
| ROCm | TheRock dist, newest stable: bundled by the installer | rocBLAS/Tensile kernels |
| HIP runtime | the dist's `amdhip64_7.dll`, `amd_comgr.dll`, `rocm_kpack.dll` next to `llama-server.exe` (bundled) | the runtime in the display driver aborts BF16/F16 GEMMs at the first prefill |
| GPU targets | `gfx1201` plus the arch of any visible Ryzen iGPU | one visible device without code objects makes every device fail |
| `LLVM_PATH`, `HIP_DEVICE_LIB_PATH` | never machine-wide | the driver's HIP runtime reads `LLVM_PATH`: devices report 0 MiB and children crash |
| `ROCBLAS_USE_HIPBLASLT` | unset | needed only without the staged runtime |
| llama.cpp | v0.4.1 or later | carries [upstream's gfx1201 FlashAttention tuning](https://github.com/ggml-org/llama.cpp/pull/28102): prefill 805 t/s at 93.6k, 442 without |
| driver | 26.9.2 or later, once released | from 26.5.1 on, a card driving no display has its VRAM purged after ~10 s idle, followed by a TDR |

## 2. Device facts

- **HIP device ids follow enumeration order**, and a visible Ryzen iGPU takes one. Select the card by
  name.
- **Usable VRAM is about 31.8 GiB on *Windows***, plus ~210 MiB of non-local memory no log line accounts
  for. On *Linux*, `mem_info_vram_total` reads 32624 MiB.
- **Prefill adds 85 to 170 MiB at run time** (*Linux*, ubatch `1024`), the same at 2.5k and 43.6k tokens.
- **The advertised PCIe link can mislead.** The card always reports Gen5 x16, the link to its own
  internal switch. Measure the real one with `tools\pcie-bench`. It matters for model load, snapshot
  restore and `split-mode tensor`.
- **A decode step is mostly kernels.** Tracing the 27B Q8_0 at 72k and 150k tokens, the Q8_0 GEMV takes 61
  to 73% of kernel time and FlashAttention the rest, growing with the context. Power stays at 212 to
  216 W of 300, so the headroom is in the kernels, not the power limit.

## 3. Settings

### Preset (presets.ini)

| key | value | evidence |
| --- | --- | --- |
| `load-mode` | `none`; `mmap`, set explicitly, on a MoE model with part of the experts on the card | `mmap` otherwise leaves ~1 GiB of weights crossing PCIe per batch: prefill 98 against 571 t/s at 43.7k |
| `flash-attn` | omit (`auto` resolves to on) | selects the FlashAttention kernel this card needs |
| `cache-type-k/-v` | `f16` at the context you need; `q8_0` only to fit a longer one | decode 35 t/s against 31.5 at 43.7k, prefill the same; `f16` fits ~66% of the `q8_0` context (campaign `27b-kv-f16-20261016`) |
| `ubatch-size` | `1024` | prefill 902 t/s at 115k, against 895 at `768`, 837 at `1536`, 809 at `2048`; free VRAM 1232, 659 and 75 MiB from `1024` up (campaign `coder390-ubatch-20261020`) |
| `split-mode` (two cards) | `tensor` on two R9700s on Linux, otherwise `layer` | [section 4](#4-multi-gpu) |
| `spec-draft-n-max` (DFlash2) | `7` | best at 2.6k and 93.6k; `1` is slower than no drafter |
| `spec-draft-type-k/-v` | `f16` | a DFlash draft KV is a small window (50 MiB in `f16`), no speed difference |

### Server (SDK Tuning, ROCm)

| key | value | evidence |
| --- | --- | --- |
| `HipFaBand` (env `GGML_HIP_FA_BAND_WMMA`) | default (`4`) | the framework's FlashAttention band (`patches\llama.cpp\0003`): attention 1.18x faster at 1 query and 1.6x at 3 to 4, on `f16` KV; end to end the 27B decodes 25.9 against 24.5 at 115k |
| `RocprofAttach` (env `ROCP_TOOL_ATTACH=1`) | off; on while diagnosing | lets `rocprofv3 --attach <pid>` trace the running server |

### Quantization on RDNA4

| quant | result against Q8_0, the fastest of the 27B files |
| --- | --- |
| Q5_K_M | ~70% of the size; prefill 10 to 13% slower, decode 7 to 15% slower. Worth it only where Q8_0 does not fit |
| Q6_K mixes | slower still: prefill a third below Q5_K_M |

## 4. Multi-GPU

### Two R9700s on Linux

One card in the CPU's Gen5 x16 slot (the x16 card), one behind the chipset at Gen4 x4 with no PCIe
AtomicOps (the x4 card). The Swift-1.5 fine-tune of the 27B Q8_0, `f16` KV, `parallel 2`, an n-gram stage then
MTP; [Qwen3.8-27B](qwen3.8_27b.md) has the detail and the full baseline (campaign
`swift27b-standard-1.0-2xr9700-linux-20261031`).

![Two R9700s: the x16 card in the CPU slot, the x4 card behind the chipset without AtomicOps; the tensor split stages its AllReduce through host memory](assets/diagrams/two-r9700-topology.svg)

| key | value | evidence |
| --- | --- | --- |
| `split-mode`, `tensor-split` | `tensor`, even | decode 59.6 against 40.4 at 43.7k (48% faster), 55.4 against 33.5 at 110k (65% faster); cold prefill 22% and 16% slower (campaign `swift27b-concurrency-2xr9700-linux-20261031`). An even split leaves 3624 / 2486 MiB free at 43.7k (campaign `swift27b-balance-mmproj-2xr9700-linux-20261031`) |
| `CudaAllreduce` | `internal` | RCCL needs AtomicOps on both slots. The internal one follows the x4 card's host link: prefill 785.6 against 1136.1 on a boot where the chipset uplink trained at x2 (campaign `swift27b-driver-check-2xr9700-linux-20261031`) |
| `parallel` | `2` | two decodes total 82.5 t/s against 59.6 for one (tensor, 43.7k). The context divides per slot: `425984` gives each 212992 (campaign `swift27b-ctx-tensor-2xr9700-linux-20261031`) |
| drafter | MTP behind an n-gram stage | MTP and DFlash2 are within noise: 30.9 and 28.8 at 43.7k; MTP takes ~1.3 GB more on the last card (campaign `swift27b-drafter-2xr9700-linux-20261030`; campaign `swift27b-drafter-code1536-2xr9700-linux-20261030`) |

Each card sits at its 300 W cap while prefilling and at 210 to 270 W while decoding (read by hand from
sysfs, not recorded). `power1_cap_max` equals the default, so the cap can only go down without the driver's
overdrive.

### Paired with a card of another backend

- **The last device in `device` receives the output head, the drafter's logits and the sampling**: on
  *Linux* this role on this card decodes 7% faster at 2.5k tokens and 4.5% at 43.6k, prefills 3%
  slower, and costs 1288 MiB of output head, 1024 of draft KV and 648 of draft compute at ctx 262144.
- **`split-mode tensor` does not pay**: with `none` AllReduce, prefill 599 against 1290 t/s and decode
  17.6 against 17.8 (*Linux*, v0.6.0).

On *Windows*, with no RCCL, a tensor split between same-backend cards goes through pinned host memory:
over a ~3 GB/s link that costs ~200 ms per 512-token ubatch and cancels the prefill gain. On *Linux*, RCCL
replaces that path when every device is a ROCm one and every slot has AtomicOps.

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| a low `spec-draft-n-max`, to stay on the FlashAttention vector kernel | `1` gives 9.8 t/s at 93.6k, against 13.0 with no drafter |
| Vulkan instead of ROCm | with Q8_0, prefill 25 to 27% and decode 15 to 18% slower; with Q5_K_M, prefill 22 to 23% and decode 30 to 41% slower. Its only edge is a compute buffer ~690 MiB smaller |

## 6. Checking a configuration

- *Windows*: `tools\report-memory.ps1` shows requested against actual VRAM per device and any spill into
  shared memory; a spill is real only above ~512 MiB beyond the pinned `*_Host` buffers.
- *Linux*: read `mem_info_vram_used` and `mem_info_gtt_used` after a prefill.
- `tools\pcie-bench\`: the card's real host link. On two cards, also read the boot log's link width.
- Benchmark tab or `llama-cpp-config bench run`: measure at 2.6k and ~90k context, with temp 0 and 3
  repetitions, or under the `standard` spec.

## 7. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `ROCBLAS_USE_HIPBLASLT` with the staged runtime | unset | is hipBLASLt (`1`) faster than Tensile (`0`) for BF16/F16 models? | TBD |
| `batch-size` | default (2048) | does a larger logical batch help prefill at the chosen `ubatch-size`? | TBD |
| `cache-ram` | 24576 | restore time of the prompt cache over the card's link | TBD |
| `spec-draft-p-min` | default | does a confidence floor lift acceptance on the reasoning phase? | TBD |
| power limit | stock (300 W) | prefill and decode per watt | partly: draw read by hand ([section 4](#4-multi-gpu)), no per-watt figure |
| `GGML_CUDA_AR_COPY_THRESHOLD` / `_CHUNK_BYTES` on two R9700 | default | the internal AllReduce's staging sizes (seeds: 1-2 MiB, 2 MiB) against the default | TBD |
| Linux: memory beyond the card | n/a | what moving buffers to GTT costs in speed | partly: a shortage fails the request; the speed under GTT is unmeasured |
| `GGML_HIP_FA_BAND_WMMA_SPLIT` | default (one block per CU) | 32, 48, 96 or 128 KV blocks against the default, at 65k and 131k cells | TBD |
| Q8_0 GEMV ceiling | n/a | `llama-bench` on this card alone at depth 0 and 131072, against the 58% of bandwidth seen in a trace | TBD |
