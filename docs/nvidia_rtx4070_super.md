# NVIDIA GeForce RTX 4070 Super: configuration reference

Reference settings for running llama.cpp on the RTX 4070 Super (Ada, `sm_89`, 12 GB)
under Windows, through the CUDA backend. Values were measured on this framework with
llama.cpp v0.5.0.

## 1. Runtime prerequisites

| item | requirement | why |
| --- | --- | --- |
| cuBLAS | CUDA 13 `cublas64_13.dll` + `cublasLt64_13.dll` next to `llama-server.exe` (pinned: 13.8.0.4) | without them the CUDA backend is skipped silently and the card falls back to Vulkan. The installer keeps the pair across upgrades |
| cudart | linked statically | nothing to install |
| CUDA toolkit (build) | newest installed version, picked by `02-build.ps1` | |
| `GGML_CUDA_FA_QUANTS` (build) | q5_0/q5_1 pairs added (`cmake-options.psd1`) | without them a q5 KV cache converts to f16 on every short decode step |

## 2. Device facts

- **The card is enumerated twice**, once by CUDA and once by Vulkan. Pin the CUDA id.
  An unselected device still holds ~160 MiB per llama-server child, because every child
  opens a driver context on each device it enumerates.
- **Usable VRAM is about 11.9 GiB dedicated** (measured at 11.93 GiB with no paging, the
  driver trimming other processes to make room); that leaves nothing for any other
  application on the card. The CUDA driver keeps 290 to 360 MiB of
  non-local (shared) memory in every configuration, even with 1.5 GiB free. That memory
  is not a spill; a spill is real only above ~512 MiB beyond the pinned buffers.

## 3. Settings

### server.ini

| key | value | evidence |
| --- | --- | --- |
| `MmprojDevice` | off this card, when it is the VRAM constraint | the image encoder (~1.14 GiB with its compute buffer for a BF16 Qwen3.8 mmproj) only computes on image requests. Moving it off and giving this card two more blocks: neutral at 2.6k, +2.4% decode at 93.6k |

### Preset (presets.ini)

| key | value | evidence |
| --- | --- | --- |
| `flash-attn` | omit (`auto` resolves to on) | a quantized KV cache needs FlashAttention |
| `cache-type-k/-v` | `q8_0` | `q5_1` decodes 6% slower; `iq4_nl` has no CUDA FlashAttention kernel |
| `override-tensor` | `token_embd\.weight=<this card>` | moves the table (~1.26 GiB at Q8_0) out of pinned host RAM, where Windows reports it as shared GPU memory: +1-2% decode. The quant must have a CUDA `get_rows` kernel (all K/IQ quants do since b10089) |
| `backend-sampling` | `true` when this card holds the logits | the sampler runs on the GPU instead of copying the logits to the host every token |
| `ubatch-size` | the largest that fits: `768` | each ubatch token costs ~0.53 MiB of model compute buffer, plus a drafter's logits (ubatch x vocab) where they land: `768` adds ~400 MiB over `512` on a last device carrying a DFlash2 drafter. Against `512`: +5.8% prefill at 2.6k, nothing at 93.6k, decode unchanged. `1024` did not fit next to a 27B Q8_0 at ctx 196608 |

### Embedded MTP head (runs entirely on the last device)

With a layer split, the MTP head is the model's last block, so its weights, its draft
KV and its compute all land on the last device. Measured with this card last (Qwen3.8-27B
Q8_0, 3 reps, temp 0):

| key | value | evidence |
| --- | --- | --- |
| `spec-draft-n-max` | `4` as a compromise (`3` at long context) | decode at 2.6k: n-max 2/3/4 = 38.0 / 45.9 / **50.6**; at 93.6k: 23.9 / **30.1** / 29.3 (3 and 4 tie within the text difference). `2` costs 21-25% |
| `spec-draft-type-k/-v` | `q8_0` | the draft KV spans the FULL context: ~2 MiB per 1k cells in q8_0, ~4 MiB in f16 (416 against 832 MiB at 212k cells). f16 gives identical text and acceptance and +1% decode at 93.6k only |
| memory | draft KV + ~1 GiB of draft compute on this card | at a context of ~200k, move about two blocks of a 27B Q8_0 model off this card to make room |

## 4. Multi-GPU

- **As the last device in `device`, the card receives the output head, a drafter's
  logits and compute, and the sampling.** Budget for them before giving it layers: with a
  27B Q8_0 model, a DFlash2 drafter and ctx 196608 that is ~1 GiB of model compute,
  ~0.5 GiB of drafter compute and the KV of its own layers.
- **An embedded MTP head adds its own draft context on the last device** (a full-context
  KV plus ~1 GiB of compute): budget for it by giving the card fewer layers (see
  section 3).
- **`split-mode tensor`** needs NCCL (not on Windows) or two devices of the same backend
  for its internal AllReduce; paired with a card of another backend it takes the slow
  generic path.

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| this card first in a split | llama-bench prefill improves (up to 1500 t/s) while live decode drops by 22%: llama-bench placement wins do not carry over to live |
| handing the last-device role (MTP head, output head, sampling) to a HIP card to free this one | with an embedded MTP head, decode -23% at 2.6k and -15% at 93.6k, +21 ms per speculative step. Not the sampler (host sampling is worse) and barely the output head (2.7 ms of the 21): the MTP draft loop, three batch-1 passes with a sync each, is latency-bound and runs faster on this card |

## 6. Checking a configuration

- `tools\report-memory.ps1`: requested vs actual VRAM, and spill into shared memory.
- `tools\pcie-bench\`: the card's real host link.
- Benchmark tab / `llama-cpp-config bench sweep`: measure at 2.6k and ~90k context, with
  temp 0 and 3 repetitions.

## 7. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| Vulkan instead of CUDA | CUDA | does the Vulkan backend lose anything on this card? | TBD |
| `backend-sampling` on vs off | on | the measured gain at 2.6k and 93.6k | TBD |
| `GGML_CUDA_FORCE_MMQ` / cuBLAS path | default | prefill at the chosen `ubatch-size` | TBD |
| CUDA graphs (`GGML_CUDA_DISABLE_GRAPHS`) | default (on) | decode cost or gain with a drafter's variable batch | TBD |
| `token_embd` left in host RAM | on the card | the cost of freeing ~1.26 GiB of VRAM | TBD |
| power limit | stock (220 W) | decode per watt | TBD |
| single-card models that fit in 12 GB | n/a | reference prefill/decode per quant | TBD |
