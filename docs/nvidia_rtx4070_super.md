---
title: NVIDIA GeForce RTX 4070 SUPER
type: gpu
status: reference
gpus: [NVIDIA GeForce RTX 4070 SUPER]
models: []
platforms: [windows, linux]
llama-cpp: v0.5.0 to v0.7.0
updated: 2026-11-01
---

# NVIDIA GeForce RTX 4070 Super: configuration reference

How to run llama.cpp on the RTX 4070 Super (Ada, `sm_89`, 12 GB) through the CUDA backend, on Windows and
on Linux. Figures come from llama.cpp v0.5.0 on *Windows* and v0.6.0 or v0.7.0 on *Linux*, with Qwen3.8-27B
Q8_0 and its embedded MTP head in a layer split with a HIP card. Backend and driver versions differ between
the two systems, so a difference between them is not the OS alone.

**In short.** Use the CUDA backend with `f16` KV. The embedded MTP head and the sampling run best on this
card on Windows and on the other card on Linux. Read `memory.free`, and run a real prefill before trusting
a configuration: a clean load proves nothing.

## 1. Runtime prerequisites

| item | requirement | why |
| --- | --- | --- |
| cuBLAS | CUDA 13 `cublas64_13.dll` + `cublasLt64_13.dll` next to `llama-server.exe` (pinned: 13.8.0.4) | without them the CUDA backend is skipped silently and the card falls back to Vulkan |
| cudart | linked statically | nothing to install |
| CUDA toolkit (build) | newest installed version, picked by `xtask build` | |
| `GGML_CUDA_FA_QUANTS` (build) | q5_0/q5_1 pairs added (`xtask/cmake/common.toml`) | without them a q5 KV cache converts to f16 on every short decode step |

## 2. Device facts

- **The card is enumerated twice**, by CUDA and by Vulkan. Pin the CUDA id. An unselected device still
  holds ~160 MiB per llama-server child.
- **Usable VRAM is about 11.9 GiB on *Windows*** and 11876 MiB on *Linux*, where `nvidia-smi` counts 406 MiB
  of `memory.reserved` inside its 12282 MiB total. A process holds ~270 MiB beyond the buffers its load log
  lists.
- **On *Linux*, over-allocation fails, never spills.** It fails at load or at the first prefill, when the
  CUDA pool takes ~40 MiB more, the same at 2.5k and 43.6k tokens: a load with ~20 MiB free aborted there
  (`cuMemCreate`). On *Windows* the driver keeps 290 to 360 MiB of
  shared memory in every configuration; a spill is real only above ~512 MiB beyond the pinned buffers.

## 3. Settings

### Preset (presets.ini)

| key | value | evidence |
| --- | --- | --- |
| `mmproj-device` | off this card when it is the VRAM constraint | the image encoder takes ~1.14 GiB and computes only on image requests; two more blocks in its place give 2.4% more decode at 93.6k |
| `flash-attn` | omit (`auto` resolves to on) | a quantized KV cache needs FlashAttention |
| `cache-type-k/-v` | `f16` at the context you need; `q8_0` only to fit a longer one | decode 35 t/s against 31.5 at 43.7k, prefill the same; `f16` fits ~66% of the `q8_0` context (campaign `27b-kv-f16-20261016`). `q5_1` decodes 6% slower than `q8_0` |
| `override-tensor` | no `token_embd` rule on *Linux*: the table stays in host RAM | *Linux*: same speed either way (31.2 t/s at 43.7k, 29.3 at 115k), and ~1.26 GiB free on this card (campaign `coder390-placement-20261020`). *Windows*, v0.5.0: the table on this card gives +1-2% decode |
| `backend-sampling` | `true` when this card holds the logits | the sampler runs on the GPU instead of copying the logits to the host every token |
| `ubatch-size` | the largest that fits: `1024` on *Linux*, `768` on *Windows* | *Linux*, 115k: prefill 902 against 895 at `768`, decode 29.3 against 28.5 (campaign `coder390-ubatch-20261020`). *Windows*: `768` prefills 5.8% faster than `512` at 2.6k, nothing at 93.6k; `1024` does not fit beside a 27B at ctx 196608 |

Compute buffers grow with both the ubatch and the context: ~0.53 MiB per ubatch token, and 1206 MiB at `768`
and ctx 196608 against 1696 to 1736 MiB at `1024` and ctx 262144. `iq4_nl` has no CUDA FlashAttention kernel.

### Server (SDK Tuning, CUDA)

| key | value | evidence |
| --- | --- | --- |
| `CudaDisableGraphs` (env `GGML_CUDA_DISABLE_GRAPHS`) | off (graphs on) | decode 39.4 t/s against 38.7 at 1k tokens and 33.7 against 33.1 at 43.7k, prefill within 3% (`GGML_CUDA_DISABLE_GRAPHS` slightly faster at 1k): *Windows*, v0.7.0, this card last with `draft-mtp`, temperature 0, one repetition, not recorded |

### Embedded MTP head (runs entirely on the last device)

With a layer split the head is the model's last block, so its weights, its draft KV and its compute all
land on the last device. *Windows* unless a row says otherwise, this card last, 3 repetitions, temperature 0:

| key | value | evidence |
| --- | --- | --- |
| `spec-draft-n-max` | `4` as a compromise, `3` at long context | decode at 2.6k with 2/3/4: 38.0 / 45.9 / **50.6**; at 93.6k: 23.9 / **30.1** / 29.3 |
| `spec-draft-type-k/-v` | `f16` on *Linux*, `q8_0` on *Windows* | *Linux*, v0.6.0: `q8_0` halves the draft KV (544 MiB at 262144 cells) but grows the draft compute from 648 to 1696 MiB. *Windows*, v0.5.0: 416 against 832 MiB at 212k cells, `f16` +1% decode at 93.6k |
| memory | draft KV plus draft compute on this card: 390 to 648 MiB on *Linux*, ~1 GiB on *Windows* | draft KV in `f16`: 768 / 896 / 1024 MiB at 196608 / 229376 / 262144 cells; budget about two fewer blocks of a 27B Q8_0 |

## 4. Multi-GPU

- **As the last device, the card takes the output head, a drafter's logits and compute, and the sampling.**
  Budget for them before giving it layers: with a 27B Q8_0, a DFlash2 drafter and ctx 196608, ~1 GiB of model
  compute, ~0.5 GiB of drafter compute and the KV of its own layers.
- **Which card should be last depends on the OS.** On *Windows*, with the embedded MTP head, a HIP card last
  decodes 23% slower at 2.6k and 15% at 93.6k than this card last: the MTP draft loop is latency-bound and
  runs faster here. On *Linux* the sign reverses: a HIP card last decodes 7% faster at 2.5k and 4.5% at
  43.6k, and prefills 3% slower.
- **`split-mode tensor`** needs NCCL (*Linux* only) or two devices of the same backend. **With a card of
  another backend it does not pay**: it aborts at load unless AllReduce is `none`, and with `none`, prefill 599
  against 1290 t/s and decode 17.6 against 17.8 (*Linux*, v0.6.0).

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| this card first in a split, on *Windows* | llama-bench prefill improves (up to 1500 t/s) while live decode falls by 22%: a llama-bench placement win does not carry over to live |
| the last-device role on a HIP card to free this one, on *Windows* | decode 23% and 15% slower ([section 4](#4-multi-gpu)); host sampling is worse still, so the sampler is not the cause |

## 6. Checking a configuration

- *Windows*: `tools\report-memory.ps1` shows requested against actual VRAM and any spill into shared memory.
- *Linux*: read `nvidia-smi --query-gpu=memory.used,memory.free,memory.reserved` after a prefill.
- `tools\pcie-bench\`: the card's real host link.
- Benchmark tab or `llama-cpp-config bench run`: measure at 2.6k and ~90k context, with temp 0 and 3
  repetitions.

## 7. Not yet measured (placeholders)

Each row needs this card in the test machine.

| parameter | current value | question | result |
| --- | --- | --- | --- |
| Vulkan instead of CUDA | CUDA | does the Vulkan backend lose anything on this card? | TBD |
| `backend-sampling` on vs off | on | the gain at 2.6k and 93.6k with this card holding the logits | TBD |
| `GGML_CUDA_FORCE_MMQ` / cuBLAS path | default | prefill at the chosen `ubatch-size` | TBD |
| power limit | stock (220 W) | decode per watt | TBD |
| single-card models that fit in 12 GB | n/a | reference prefill and decode per quant | TBD |
| Linux: `ubatch-size` `1024` against `768` at 2.6k | `1024` | the prefill gain on a short prompt | TBD |
| Linux: last-device role at ~90k context | on the HIP card | does the decode gain measured at 2.5k and 43.6k hold? | TBD |
