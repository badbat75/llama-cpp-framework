---
title: AMD Radeon AI PRO R9700
type: gpu
status: reference
gpus: [AMD Radeon AI PRO R9700]
models: []
platforms: [windows, linux]
llama-cpp: v0.5.0 to v0.8.0
updated: 2026-11-04
---

# AMD Radeon AI PRO R9700: configuration reference

How to run llama.cpp on the R9700 (RDNA4, `gfx1201`, 32 GB) through the ROCm (HIP) backend, on Windows
and on Linux. Figures come from llama.cpp v0.5.0 on *Windows* and v0.6.0 or v0.7.0 on *Linux*, mostly
with Qwen3.8-27B Q8_0; two cards are v0.8.0 on both. Backend and driver versions differ between the two systems,
so a difference between them is not the OS alone. Model-specific detail is in the
[Qwen3.8-27B](qwen3.8_27b.md) and [Qwen3.8-Flash-Next](qwen3.8_flash_next.md) guides.

**In short.** Use the ROCm backend with `f16` KV at the context you need and `ubatch-size 1024`. For two
R9700s, use the tensor split with an even `tensor-split`: on Linux it decodes 48 to 65% faster than the
layer split for 16 to 22% less cold prefill. Windows runs the same preset with one request's decode level
with Linux and prefill 11 to 18% slower; the gain over the layer split is unmeasured there.

**The build behind these figures.** The framework bundles llama.cpp v1.0.0, and no figure here was measured on it.
Every figure from v0.8.0, and every recorded one from v0.7.0 except campaign `27b-kv-f16-20261016`, comes from a
build carrying patches the bundled build does not have: the FlashAttention band on RDNA4 and the backend sampler probe, and on all but the earliest v0.7.0 campaigns host buffers pinned for every runtime. v0.5.0 and v0.6.0 figures predate them; a figure read by hand on v0.7.0 does not record its build, so it may
or may not have carried them. The band is the one
that moved speed: on `f16` KV it ran attention 1.18x faster at 1 query and 1.6x at 3 to 4, and the 27B decoded 25.9
against 24.5 t/s without it at 115k, prefill unchanged (not recorded). The bundled build runs this card's `f16` decode
steps on upstream's tile kernel. How far the figures move is open ([section 7](#7-not-yet-measured-placeholders)).

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
- **Usable VRAM is about 31.8 GiB on *Windows***, and the driver keeps ~210 MiB more, which no log line
  accounts for. On *Linux*, `mem_info_vram_total` reads 32624 MiB.
- **Prefill adds 85 to 170 MiB at run time** (*Linux*, ubatch `1024`), the same at 2.5k and 43.6k tokens.
- **The advertised PCIe link can mislead.** The card always reports Gen5 x16, the link to its own
  internal switch. Measure the real one with `tools\pcie-bench`. It matters for model load, snapshot
  restore and `split-mode tensor`.
- **A decode step is mostly kernels** (*Linux*, v0.7.0, on the tile kernel). Tracing the 27B Q8_0 at 72k and 150k tokens, the Q8_0 GEMV takes 61
  to 73% of kernel time and FlashAttention the rest, growing with the context. Power stays at 212 to
  216 W of 300, so the headroom is in the kernels, not the power limit.
- **With `backend-sampling`, a `top-p` falls to the CPU, with the samplers after it.** ROCm has no CUB, so the backend
  probe refuses `ARGSORT` on a vocabulary-wide row even when a `top-k` before it leaves 20 candidates. llama-server logs
  `does not have support for op ARGSORT needed for sampler 'top-p'` on every request. It costs no speed:
  [Qwen3.8-27B, section 8](qwen3.8_27b.md#8-choices-that-look-better-than-they-measure).

## 3. Settings

### Preset (presets.ini)

| key | value | evidence |
| --- | --- | --- |
| `load-mode` | `none`; on *Windows*, `mmap`, set explicitly, on a MoE model with part of the experts on the card | `mmap` otherwise leaves ~1 GiB of weights crossing PCIe per batch: prefill 98 against 571 t/s at 43.7k |
| `flash-attn` | omit (`auto` resolves to on) | selects the FlashAttention kernel this card needs |
| `cache-type-k/-v` | `f16` at the context you need; `q8_0` only to fit a longer one | *Linux*, layer split across two backends: decode 35 t/s against 31.5 at 43.7k, prefill the same; `f16` fits ~66% of the `q8_0` context (campaign `27b-kv-f16-20261016`) |
| `ubatch-size` | `1024` | *Linux*, the same split: prefill 902 t/s at 115k, against 895 at `768`, 837 at `1536`, 809 at `2048`; free VRAM 1232, 659 and 75 MiB from `1024` up (campaign `coder390-ubatch-20261020`) |
| `split-mode` (two cards) | `tensor` on two R9700s, `layer` with a card of another backend | [section 4](#4-multi-gpu); `tensor` against `layer` on two R9700s is measured on *Linux* only |
| `spec-draft-n-max` (DFlash2) | `7` on *Windows*, `5` on two R9700s on *Linux* | *Windows*, v0.5.0: `7` best at 2.6k and 93.6k, `1` slower than no drafter. Two R9700s, *Linux*, v0.8.0, 43.7k: 34.1 ±2.0 at `5` against 32.2 ±1.4 at `7` (campaign `qwen27b-dflash-2xr9700-linux-draft-n-max-20261028`) |
| `spec-draft-type-k/-v` | `f16` | a DFlash draft KV is a small window (50 MiB in `f16`), no speed difference |

On *Windows*, with part of a MoE model's experts on the card, `load-mode none` aborts right after the weight
upload, and `auto` resolves to `none` when a device of the model reports no mmap support, as a Ryzen iGPU
visible to HIP does with `device` left blank: hence `mmap`, set explicitly. *Linux* runs `none` there.

`q5_1` saves ~30% of the KV against `q8_0` but decodes 6% slower; `iq4_nl` has no HIP FlashAttention kernel.

### Server (SDK tuning, ROCm)

| key | value | evidence |
| --- | --- | --- |
| `RocprofAttach` (env `ROCP_TOOL_ATTACH=1`) | off; on while diagnosing | lets `rocprofv3 --attach <pid>` trace the running server |

### Quantization on RDNA4

| quant | result against Q8_0, the fastest of the 27B files |
| --- | --- |
| Q5_K_M | ~70% of the size; prefill 10 to 13% slower, decode 7 to 15% slower. Worth it only where Q8_0 does not fit |
| Q6_K mixes | slower still: prefill a third below Q5_K_M |

## 4. Multi-GPU

### Two R9700s

One card in the CPU's Gen5 x16 slot (the x16 card), one behind the chipset at Gen4 x4 with no PCIe
AtomicOps (the x4 card). The Swift-1.5 fine-tune of the 27B Q8_0, `parallel 2`, an n-gram stage then
MTP; [Qwen3.8-27B](qwen3.8_27b.md) has the detail and the full baseline
(campaign `swift27b-standard-1.0-2xr9700-linux-20261031`). The table is measured on *Linux*.

![Two R9700s: the x16 card in the CPU slot, the x4 card behind the chipset without AtomicOps; the tensor split stages its AllReduce through host memory](assets/diagrams/two-r9700-topology.svg)

| key | value | evidence |
| --- | --- | --- |
| `split-mode`, `tensor-split` | `tensor`, even | decode 59.6 against 40.4 at 43.7k (48% faster), 55.4 against 33.5 at 115k (65% faster); cold prefill 22% and 16% slower (campaign `swift27b-concurrency-2xr9700-linux-20261031`). An even split leaves 3624 / 2486 MiB free at 43.7k, an uneven one left the x16 card at 0 MiB (campaign `swift27b-balance-mmproj-2xr9700-linux-20261031`) |
| `CudaAllreduce` | `internal` | RCCL needs AtomicOps on both slots, and exists on *Linux* only. The internal one follows the x4 card's host link: prefill 785.6 against 1136.1 on a boot where the chipset uplink trained at x2 (campaign `swift27b-driver-check-2xr9700-linux-20261031`) |
| `parallel` | `2` | two decodes total 82.5 t/s against 59.6 for one (tensor, 43.7k). The context divides per slot: `425984` gives each 212992 (campaign `swift27b-ctx-tensor-2xr9700-linux-20261031`) |
| drafter | MTP behind an n-gram stage | MTP and DFlash2 are within noise on prose and in the chains: 30.9 and 28.8 at 43.7k. Alone on code DFlash2 leads, 58.2 against 49.8. MTP takes ~1.3 GB more on the last card (campaign `swift27b-drafter-2xr9700-linux-20261030`; campaign `swift27b-drafter-code1536-2xr9700-linux-20261030`) |

The first three rows ran with `f16` KV at temperature 0, one request unless the row says two, the speed
figures on a slightly uneven split that matches the even one at 43.7k. The drafter row ran with `q8_0` KV
at 514288, temperature 1, 5 repetitions, layer split. Two cold prefills run one after the other, so their
total is one request's rate, and a request decoding beside a prefill drops to 37.1 t/s under the tensor
split and 20.2 under the layer split.

**On *Windows* the same tensor preset runs at the same decode for one request and a slower prefill**: 57.7
against 59.3 at 43.7k, 53.9 against 59.2 at 115k, and prefill 952.1 against 1125.4 and 823.3 against 960.1
(15% and 14% slower) (campaign `swift27b-standard-1.1-2xr9700-windows-20261101`). The cause of that prefill gap is
unmeasured. One-repetition runs at 43.7k (`spec = custom`, derived from `standard-1.1`) find no change from
`ThreadsBatch 12` against `24` or from PCIe ASPM off, and 2 to 3% from Memory Integrity off:
[Qwen3.8-27B, section 6](qwen3.8_27b.md#6-linux-against-windows). No run of this model uses the layer split on two
R9700s on *Windows*, so the tensor split's decode gain and prefill cost against it are unmeasured there.

The one-repetition runs read 4829 / 3390 MiB free (x16 / x4 card) after the first 43.7k prefill with the speculator off and
2758 / 1166 with it on, system-wide from the Windows GPU counters (campaign `swift27b-memory-integrity-windows-20261102`);
the standard-spec run holds none. By hand, with two 115k contexts at once at `ctx-size 425984`, there is no spill to
shared memory and the cards peak at 31304 and 29712 MiB of 32624 (Windows performance counters, not recorded; the
fuller one is probably the x4 card).

**The tensor split's prefill follows the x4 card's link on either system**, since the internal AllReduce stages
through pinned host memory. On *Linux*, RCCL replaces that path when every device is a ROCm one and every slot has
AtomicOps; *Windows* has no RCCL. A healthy Gen4 x4 link measures about 6.8 GB/s, and a boot can train the chipset
uplink at half that. On *Windows*, `tools\pcie-bench` read 3.45 / 3.52 GB/s of DMA (device to host / host to
device, 32 MiB) on the x4 card on one boot and 6.75 / 6.77 after a power cycle, the x16 card ~45 to 49; prefill at
43.7k read 674 on the slow boot against 945 after the power cycle, one request each with the drafter on (read by
hand, not recorded). Check before a tensor-split benchmark, and power cycle if the x4 card reads about half.

Each card sits at its 300 W cap while prefilling and at 210 to 270 W while decoding (read by hand from
sysfs, not recorded). `power1_cap_max` equals the default, so the cap can only go down without the driver's
overdrive.

An example `presets.ini` section, the preset of the
[baseline under the standard spec](qwen3.8_27b.md#7-baseline-under-the-standard-spec-two-r9700s): set
`model` and `mmproj` to your own paths, and `device` and `mmproj-device` to your own ids, which
`llama-server --list-devices` prints (they can change between boots, and both cards print the same name,
so the list does not say which one is the x16 card). On *Windows* the integrated GPU takes `ROCm0`, so the same
preset reads `device = ROCm2,ROCm1` with `mmproj-device = ROCm1` on the machine measured. There, on this
same-card tensor pair, either order prefills within 1%, one repetition per pass at 43.7k (campaign `swift27b-device-order-windows-20261102`),
and the x16 card stays first; a decode difference stays open in
[Qwen3.8-27B, section 9](qwen3.8_27b.md#9-not-yet-measured-placeholders). The order that reverses on *Windows*, below,
concerns a card of another backend.

<details>
<summary>presets.ini: Qwen3.8-27B on two R9700s</summary>

```ini
[Swift-1.5-Qwen3.8-27B]
; url = https://huggingface.co/ukisai/Swift-1.5-Qwen3.8-27B-GGUF
model = Swift-1.5-Qwen3.8-27B-Q8_0.gguf
mmproj = Qwen3.8-27B-UD-mmproj-BF16.gguf
mmproj-offload = true
; the x4 card
mmproj-device = ROCm1

spec-type = ngram-map-k4v,draft-mtp
spec-draft-n-max = 2
spec-draft-type-k = f16
spec-draft-type-v = f16
spec-ngram-map-k4v-size-m = 32

ctx-size = 425984
; the x16 card first
device = ROCm0,ROCm1
; needs CudaAllreduce = internal in server.ini
split-mode = tensor
tensor-split = 1,1
parallel = 2
kv-unified = false
ubatch-size = 1024

cache-type-k = f16
cache-type-v = f16
cache-ram = 24576
cache-reuse = 256
ctx-checkpoints = 12

jinja = true
reasoning = auto
reasoning-format = auto
reasoning-budget-message = Reasoning budget reached. Stop analysing and write the final answer now.

load-mode = none
fit = off

temp = 1
top-k = 20
top-p = 0.95
min-p = 0
repeat-penalty = 1
presence-penalty = 0
backend-sampling = true
```

</details>

### Paired with a card of another backend

- **The last device in `device` receives the output head, the drafter's logits and the sampling**, so
  put the card with the most free VRAM after its layers there. On *Linux* this role on this card decodes
  7% faster at 2.5k tokens and 4.5% at 43.6k than the same role on the other card, prefills 3% slower,
  and costs 1288 MiB of output head, 1024 of draft KV and 648 of draft compute at ctx 262144. On *Windows*
  the sign reverses: [Qwen3.8-27B, section 6](qwen3.8_27b.md#6-linux-against-windows).
- **`split-mode tensor` does not pay**: it aborts at load unless AllReduce is `none`, and with `none`, prefill 599 against 1290 t/s and decode
  17.6 against 17.8 (*Linux*, v0.6.0).

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| a low `spec-draft-n-max`, to stay on the FlashAttention vector kernel | `1` gives 9.8 t/s at 93.6k, against 13.0 with no drafter |
| Vulkan instead of ROCm | with Q8_0, prefill 25 to 27% and decode 15 to 18% slower; with Q5_K_M, prefill 22 to 23% and decode 30 to 41% slower. Its only edge is a compute buffer ~690 MiB smaller |
| Memory Integrity off on *Windows*, two cards | prefill 2 to 3% faster, decode unchanged within noise: a small part of the gap to *Linux* at most, and the setting stays on ([Qwen3.8-27B, section 6](qwen3.8_27b.md#6-linux-against-windows)) |
| `ThreadsBatch 12` against `24`, or PCIe ASPM off, on *Windows*, two cards with every weight on them | no change: prefill 0.3% slower each ([Qwen3.8-27B, section 6](qwen3.8_27b.md#6-linux-against-windows)) |

## 6. Checking a configuration

- *Windows*: `tools\report-memory.ps1` shows requested against actual VRAM per device and any spill into
  shared memory; a spill is real only above ~512 MiB beyond the pinned `*_Host` buffers. A benchmark run records the
  memory free after its first prefill per adapter, under its PCI address (for example `pci-14:00.0`): system-wide,
  read from the Windows GPU counters. Use this script where a run holds no such row.
- *Linux*: read `mem_info_vram_used` and `mem_info_gtt_used` after a prefill.
- `tools\pcie-bench\`: the card's real host link. On two cards on *Linux*, also read the boot log's link width;
  *Windows* shows no width on the bridges, so there a card reading about half its bandwidth is the sign.
- Benchmark tab or `pai bench run`: measure at 2.6k and ~90k context, with temp 0 and 3
  repetitions, or under the `standard` spec.

## 7. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `ROCBLAS_USE_HIPBLASLT` with the staged runtime | unset | is hipBLASLt (`1`) faster than Tensile (`0`) for BF16/F16 models? | TBD |
| `batch-size` | default (2048) | does a larger logical batch help prefill at the chosen `ubatch-size`? | TBD |
| `cache-ram` | 24576 | restore time of the prompt cache over the card's link | TBD |
| `spec-draft-p-min` | default | does a confidence floor lift acceptance on the reasoning phase? | TBD |
| power limit | stock (300 W) | prefill and decode per watt | partly: draw read by hand ([section 4](#4-multi-gpu)), no per-watt figure |
| `split-mode` on two R9700s on *Windows* | `tensor` | what do the tensor split's decode gain and prefill cost come to against `layer` there? | TBD |
| the prefill gap to *Linux* on two R9700s on *Windows* | n/a | what causes it: the driver and HIP runtime, transfers and submissions under `split-mode tensor`, the hypervisor? | partly: Memory Integrity gives 2 to 3%, `ThreadsBatch` and PCIe ASPM nothing ([section 5](#5-choices-that-look-better-than-they-measure)); the candidates are unmeasured |
| `GGML_CUDA_AR_COPY_THRESHOLD` / `_CHUNK_BYTES` on two R9700 | default | the internal AllReduce's staging sizes (seeds: 1-2 MiB, 2 MiB) against the default | TBD |
| Linux: memory beyond the card | n/a | what moving buffers to GTT costs in speed | partly: with two 115k contexts on a full card, requests ran out of memory in the HIP graph update, once after 14 GB had moved to GTT (campaign `swift27b-concurrency-2xr9700-linux-20261031`); the speed under GTT is unmeasured |
| the settings on the bundled llama.cpp v1.0.0 | as in [section 3](#3-settings) | how far do these figures move without the band and the other patches they ran with? | TBD |
| Q8_0 GEMV ceiling | n/a | `llama-bench` on this card alone at depth 0 and 131072, against the 58% of bandwidth seen in a trace | TBD |
