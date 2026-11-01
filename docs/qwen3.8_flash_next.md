---
title: Qwen3.8-Flash-Next
type: model
status: draft
gpus: [AMD Radeon AI PRO R9700, NVIDIA GeForce RTX 4070 SUPER]
models: [Qwen3.8-Flash-Next]
platforms: [windows, linux]
llama-cpp: v0.6.0 to v0.8.0
updated: 2026-11-01
---

# Qwen3.8-Flash-Next: configuration reference (draft)

Qwen3.8-Flash-Next-GSQ-RCO (architecture `qwen4exp`) is a MoE model too large for most GPU
memory: each layer has 512 experts, so where the experts live decides the speed. This guide says
how to run it on two machines built around a 12-core AVX-512 host with 64 GB of RAM:

- **Two R9700s on Linux** (llama.cpp v0.8.0, IQ3_S): every expert fits in VRAM.
- **An R9700 and an RTX 4070 SUPER in one model** (llama.cpp v0.7.0), on Linux and on Windows: part
  of the experts stay in RAM. A row measured on another release names it.

**In short.** Keep as many experts as you can on the cards. Moving one layer's experts to the CPU
costs ~1.3% of decode, and the MoE cache never beats resident layers. With two R9700s all of them
fit: prefill reaches x3.5 and decode 21% faster than the mixed pair. Then the separate MTP head with
an n-gram drafter in front decodes 56 t/s on prose and 280 on a code rewrite. With the mixed pair,
put the RTX 4070 SUPER first and use `f16`, `n-cpu-moe 16` and `no-host`. Windows stays about a third
behind Linux on prefill, because the host experts run on the CPU.

All speeds are t/s, with ± the sample standard deviation over the repetitions. "43.7k" is a prompt of
that many tokens, sent with the prompt cache off.

## 1. Model facts

| item | value | source |
| --- | --- | --- |
| layers | 48; attention on every 4th (12 layers hold a KV cache), recurrent state on all 48 | `full_attention_interval = 4` |
| memory type | hybrid (attention + recurrent) | `llm_arch_is_hybrid` |
| experts | 512 per layer, 10 routed per token, plus a shared expert | `expert_count`, `expert_used_count` |
| native context | 262144 | `context_length` |
| GGUF sampling defaults | temp 1.0, top-k 20, top-p 0.95 | `general.sampling.*` |
| chat template | reads `enable_thinking`, `preserve_thinking`, `reasoning_effort` | template source |
| files (IQ3_S) | shard 1: 51.05 GiB of weights; shard 2: 26.82 GiB, the n-gram table `per_layer_token_embd` | file sizes |
| non-expert weights on the GPU | 3983 MiB | `model buffer size` at `n-cpu-moe 48` |
| fixed state, one slot | recurrent 112.57 MiB; KV 24 + 96 MiB per 4096 tokens (`f16`) | `RS buffer size`, `KV buffer size` |
| `split-mode tensor` | supported: [upstream enabled it for `qwen4exp`](https://github.com/ggml-org/llama.cpp/pull/28569) | `src/llama-arch.cpp` |

## 2. Recommended settings

### Any setup

| key | value | evidence |
| --- | --- | --- |
| quant | IQ3_S; IQ3_XXS where memory is short | [section 3](#3-quant-and-where-the-experts-live) |
| `cache-type-k/-v` | `f16`; `q8_0` with the separate MTP head on two R9700s | `q8_0` saves only 1 or 2 layers of experts and decodes slower: 21.8 t/s against 24.9 at 43.7k |
| `lazy-mode` | `auto` (the default) | keeps the 26.8 GiB n-gram table mapped; `off` reads it into 27 GiB of RAM and decodes the same |
| `moe-cache-mib` | `0` (off) | resident layers beat the cache in every setup ([section 4](#4-the-moe-cache)) |
| `threads` | one per physical core (12); `ThreadsBatch` the same on *Linux*, 24 on *Windows* | *Linux*: prefill 410 and 405 at 12 threads against 363 to 377 at 16 to 24. *Windows*: 275.7 and 279.0 at 24 against 250 to 262 at 12 (by hand, not recorded) |
| `reasoning-preserve` | unset (on) | a turn sending back the old thinking processes only its 19 new tokens; `false` reprocesses 60 from a checkpoint |
| `ctx-checkpoints` | unset (32) | a diverging prompt reprocesses 60 of 118 tokens with them, all 118 without; 112.6 MiB each |

### Two R9700s on Linux

Every expert in VRAM (campaign `flash-next-2xr9700-linux-ncm-20261028`). [Section 5](#5-drafters) has the drafter.

| key | value | evidence |
| --- | --- | --- |
| `n-cpu-moe` | unset | prefill 1978.8 ±5.7 against 570 at `n-cpu-moe 16`, decode 33.9 against 26.2 |
| `device` | the x16 card first | decode 33.9 against 33.3, 2% faster; prefill 3% slower (campaign `flash-next-2xr9700-linux-device-20261028`) |
| `tensor-split` | `24,25` with the MTP head, `1,1` without | makes room for the head on the first card ([section 5](#5-drafters)) |
| `cache-type-k/-v`, `model-draft`, `device-draft` | `q8_0`, the MTP GGUF, the first card, with the MTP head | `f16` at `1,1` does not load: the draft's KV finds no room on the first card ([section 5](#5-drafters)) |
| `ctx-size` | `262144` | same speed as 131072: 1962.3 ±17.7 prefill, 34.0 decode; 1872 and 772 MiB free (campaign `flash-next-2xr9700-linux-ctx-20261028`) |
| `ubatch-size` | `512` | larger is faster but leaves no margin on the second card at 262144 (table below; campaign `flash-next-2xr9700-linux-ubatch-20261028`) |
| `spec-type` | `ngram-map-k4v,draft-mtp`, `spec-draft-n-max 3`, `size-m 64` | decode 56.3 on prose and 280.1 on a code rewrite, the best measured ([section 5](#5-drafters)) |

| `ubatch-size`, at 131072 | prefill at 43.7k |
| --- | --- |
| 512 | 1978.8 ±5.7 |
| 1024 | 2273.1 ±3.8 |
| 2048 | 2312.8 ±0.8 |

```bench-chart
title    = Prefill at 43.7k by ubatch-size, every expert in VRAM, 131072
campaign = flash-next-2xr9700-linux-ncm-20261028, flash-next-2xr9700-linux-ubatch-20261028
metric   = prefill
x        = ubatch-size
series   = n-cpu-moe
where    = n-cpu-moe=unset
rename   = unset=all experts in VRAM
```

### An R9700 and an RTX 4070 SUPER

| key | value | evidence |
| --- | --- | --- |
| `device`, `tensor-split` | the RTX 4070 SUPER first, `23,26` | prefill 557.8 ±0.3 against 422.7 ±2.6 with the R9700 first at `40,9` (32% faster), decode 28.2 against 25.0 (13% faster; *Linux*) |
| `n-cpu-moe` | `16`: the lowest that leaves ~500 MB free on each card | below that margin hipBLASLt's workspace runs short on the first real prompt, which a 5-token test does not show |
| `ctx-size` | `131072` | 262144 prefills 14 to 17% slower and decodes 4 to 8% slower on short prompts ([section 3](#3-quant-and-where-the-experts-live)) |
| `no-host` | `true`, with `op-offload false` | the CPU repacks what it can: prefill 416 and 419 against 383 and 382, decode the same (campaign `flash-next-no-host-20261016`) |
| `load-mode` | `none` whenever experts sit in RAM, set explicitly; on *Windows*, `mmap` with part of the experts on the card | *Windows*, every expert in RAM: `mmap` decodes up to 12.5% (R9700) and 25 to 39% (RTX 4070 SUPER) slower. *Linux*: `none` 25.6 ±0.1, `mmap` 23.9 ±2.6. A partial `n-cpu-moe` needs `mmap` on *Windows* ([section 8](#8-choices-that-look-better-than-they-measure)) |
| `ubatch-size` | `512` | `768` buys 3% of prefill, decodes slower and halves the R9700's margin (campaign `flash-next-ubatch768-linux-20261021`). With `q8_0` at `n-cpu-moe 14`, `1024` and `2048` do not load |
| `spec-type` | `ngram-map-k`, `size-m 64` | code 61 to 65 t/s against 28.6 with none; prose reasoning 40.0 ±1.6 against 29.4 ±1.3 (IQ3_XXS, v0.6.0) |

**23 blocks is the RTX 4070 SUPER's limit at `n-cpu-moe 16`.** `24,25` does not load (campaign
`flash-next-device-order-linux-20261021`), and neither does `n-cpu-moe 15` at `23,26` (not recorded): weights
and KV fit, the compute buffer does not.

On *Linux* the IQ3_XXS preset (`q8_0` KV, `n-cpu-moe 7`) follows the same order: the RTX 4070 SUPER
first at `19,30` prefills 776.7 ±15.8 against 580.8 ±10.6 and decodes 27.8 ±0.4 against 26.1 ±0.8,
but leaves only 72 MiB free on that card, and `20,29` does not load (campaign
`flash-next-xxs-device-order-linux-20261021`). The order comes from where the work runs: the RTX 4070 SUPER takes blocks 0 to 22, among them the 16
host-expert layers, and the R9700 takes blocks 23 to 47, the output head and the sampling. Size the
split from the per-layer weights, since `llama-gguf <shard> r` shows experts of 738 to 1300 MiB per
layer, the late layers heavier (campaign `flash-next-device-order-linux-20261021`).

With `device` pinned to the discrete cards, `load-mode auto` resolves to `mmap` (read in the *Linux* log),
so set `none` explicitly.

## 3. Quant and where the experts live

Two cards, `ngram-map-k`, *Linux*, v0.6.0:

| | IQ3_XXS, `n-cpu-moe 7` | IQ3_S, `n-cpu-moe 14` |
| --- | --- | --- |
| prefill at 43.7k | 478, 485 | 342, 338 |
| decode at 43.7k | 24.0 ±0.2 | 20.9 ±0.1 |
| code rewrite | 49.9, 56.6, 64.1 | 47.0 ±2.0 |
| quality on the model card (BF16 = 93.12) | 92.57 | 93.26 |

Context and KV type at ubatch 512, `op-offload false`, the 43.7k prompt and three short prompts:

| context, KV | `n-cpu-moe`, split | prefill | decode at 43.7k | decode, short prompts |
| --- | --- | --- | --- | --- |
| 131072, `q8_0` | 14 plus `blk\.38\.ffn_down_exps=CUDA0`, `40,9` | 397, 414 | 21.8 | 27.1 |
| **131072, `f16`** | **16, `40,9`** | 381, 382 | **24.7, 25.1** | **27.4** |
| 262144, `q8_0` | 18, `41,8` | 349, 351 | 20.9, 20.7 | 26.0 |
| 262144, `f16` | 21, `41,8` | 311, 319 | 23.3, 23.4 | 25.2 |

Adding `no-host` and the RTX 4070 SUPER first to the bold row gives the recommended preset. `q8_0`
halves the KV but its compute buffer grows (877 against 533 MiB on the R9700 at 131072), so it saves
one layer of experts at 131072 (15 against 16; the row's 14 comes from also moving that override's
tensor to the RTX 4070 SUPER) and two at 262144.

**The second card is worth ten layers of experts and decodes 9 to 23% faster:**

| context, KV | both cards | R9700 alone |
| --- | --- | --- |
| 131072, `q8_0` | `n-cpu-moe 15`: 25.7 ±2.3 | `25`: 21.3 ±2.6 |
| 131072, `f16` | `16`: 26.9 ±0.2 | `26`: 21.8 ±1.5 |
| 262144, `q8_0` | `18`: 25.8 ±0.1 | `28`: 21.1 ±1.5 |
| 262144, `f16` | `21`: 23.3 ±2.8 | `30`: 21.3 ±0.1 |

**Decode is mostly a fixed cost per token.** The experts of 14 to 16 layers on the CPU give 26.9 to
27.3 t/s on short prompts whichever quant they hold, and 24 layers give 23.9: a layer costs ~1.3%. The
R9700 is ~75% busy during decode and the RTX 4070 SUPER ~5%.

**`no-host` is what lets the CPU repack.** Weights kept in RAM then skip the GPU's pinned host buffer,
so the CPU repacks the types it has interleaved kernels for: here `ffn_down_exps` in IQ4_NL, 4950 of the
14997 MiB in RAM at `n-cpu-moe 16`. The pinned buffer only speeds the per-batch copies of `op-offload
true`, which `false` never makes. `repack` itself can stay unset.

## 4. The MoE cache

The cache keeps hot experts in VRAM and uploads each miss. The routing is skewed, so a small cache gets
good hits: 4.2% of the experts as slots gives 50% hits, 8.5% gives 62 to 64%. Serving the misses is what
costs. Each layer reads the router's ids with a blocking sync (48 stalls per token) and then issues ~15
small transfers, ~720 per token. On the R9700 on *Windows* that gives ~0.36 ms per transfer, an estimate
from two mean token times rather than a trace; the RTX 4070 SUPER fits the same count within 44 ms per
token.

Every expert in RAM, `n-cpu-moe 48`, ctx 32768, `load-mode none`, *Windows* (hit rate in brackets):

| `moe-cache-mib` | R9700 | RTX 4070 SUPER |
| --- | --- | --- |
| 0 | 14.1, 11.6 | 22.7, 22.9 |
| 2048 | 3.0 (50%) | 23.2 (51%) |
| 4096 | 4.0 (64%) | 23.9, 19.7 (62 to 64%) |

The same VRAM spent as resident layers or as cache:

| RTX 4070 SUPER, *Windows*, about the same VRAM in each row | decode | hit rate |
| --- | --- | --- |
| **`n-cpu-moe 45`, no cache** | **25.8, 25.4** | n/a |
| `n-cpu-moe 46`, 1024 MiB | 19.7, 19.4 | 39% |
| `n-cpu-moe 47`, 2048 MiB | 21.8, 21.6 | 50% |
| `n-cpu-moe 48`, 3072 MiB | 23.8, 23.4 | 59% |

| both cards, *Linux*, the cache on the R9700 | decode | hit rate |
| --- | --- | --- |
| **`n-cpu-moe 14`, no cache** | **27.4 ±0.2** | n/a |
| `n-cpu-moe 15`, 896 MiB | 17.8 ±0.0 | 45% |
| `n-cpu-moe 16`, 1792 MiB | 19.9 ±1.4 | 65% |
| `n-cpu-moe 17`, 2688 MiB | 22.5 ±0.1 | 74% |
| `n-cpu-moe 28`, 12000 MiB | 25.8 | 92% |
| with `no-host`: **`n-cpu-moe 16`, no cache** | **27.0** | n/a |
| with `no-host`: `n-cpu-moe 18`, 2048 MiB | 26.6 | 93% |

Even at 93% hits the cache trails the layers it replaced (campaign `flash-next-moe-cache-20261016`).
Repacked experts leave the cache, and on two cards the whole cache goes to the card holding the first
layers, so size it against that card's free VRAM. A cache policy change upstream would reopen this.
Two changes would target the cost: read a layer's router ids one layer late rather than stalling, and
stage a layer's misses contiguously so that it issues 3 transfers rather than ~15.

## 5. Drafters

### The separate MTP head (two R9700s)

ggml-org publishes the MTP layer as its own GGUF (`mtp-Qwen3.8-Flash-Next-Q4_0.gguf`, 2.05 GiB), loaded
with `model-draft`, `spec-type draft-mtp` and `device-draft` on the first card. At 262144 it adds 1746 MiB
of weights, 640 MiB of `f16` draft KV and a 566 MiB compute buffer there. Making room takes a `q8_0` KV
and `24,25` (block 24 to the second card), since `f16` at `1,1` does not load: after a 43.7k prefill that leaves 882 / 814 MiB free at
`spec-draft-n-max 3`.

43.7k prose and a code rewrite (rename one function in a 356-line Rust file), `q8_0`, `24,25`, 262144,
temperature 0 (campaign `flash-next-mtp-base-2xr9700-linux-20261030`; campaign
`flash-next-mtp-ngram-2xr9700-linux-20261030`):

| `spec-type` | decode on prose | decode on code | prefill at 43.7k |
| --- | --- | --- | --- |
| none | 31.1 ±0.1 | 36.2 ±0.0 | 2020.4 ±2.9 |
| `ngram-map-k4v` | 31.2 ±0.0 | 274.5 ±16.4 | 2030.2 ±10.0 |
| `draft-mtp`, `n-max 3` | 54.9 ±0.4 | 95.3 ±0.2 | 1520.3 ±1.4 |
| **`ngram-map-k4v,draft-mtp`** | **56.3 ±1.4** | **280.1 ±37.6** | 1531.2 ±4.1 |

```bench-chart
title    = Decode by drafter, 43.7k prose and code rewrite
campaign = flash-next-mtp-base-2xr9700-linux-20261030, flash-next-mtp-ngram-2xr9700-linux-20261030, flash-next-mtp-graphs-2xr9700-linux-20261030
metric   = decode
x        = spec-type
series   = prompt
where    = spec-draft-n-max=3
rename   = unset=none, ngram-map-k4v,draft-mtp=k4v + mtp, bench-prompt-long=43.7k prose, bench-prompt-code=code rewrite
```

![spec-type ngram-map-k4v,draft-mtp: the n-gram drafter is tried first and copies from the context on a match; otherwise the MTP head drafts; the target verifies the draft in one batch](assets/diagrams/drafter-chain.svg)

The two cover each other's gap. The n-gram drafter finds nothing to copy in prose, where the MTP head
lifts decode by 77%; on a rewrite the n-gram drafts whole runs of the context and the head adds little.
The n-gram drafters are always tried first, whatever the order written. The chain costs 25% of cold
prefill, all of it the head's: without it the same `q8_0` preset prefills 2020.4.

`spec-draft-n-max` 3 is the peak, and HIP graphs help most there (campaign
`flash-next-mtp-graphs-2xr9700-linux-20261030`; graphs off with `CudaDisableGraphs = true`, campaign
`flash-next-mtp-2xr9700-linux-20261030`):

| `spec-draft-n-max` | decode, graphs on | decode, graphs off | accepted |
| --- | --- | --- | --- |
| 1 | 41.5 ±0.7 | 38.1 ±0.6 | 79% |
| 2 | 45.3 ±1.4 | 43.7 ±0.2 | 61% |
| **3** | **54.9 ±0.4** | 50.1 ±1.0 | 59% |
| 4 | 45.1 ±1.2 | 46.1 ±1.0 | 44% |

```bench-chart
title    = MTP head: decode at 43.7k by spec-draft-n-max, HIP graphs on and off
campaign = flash-next-mtp-graphs-2xr9700-linux-20261030, flash-next-mtp-2xr9700-linux-20261030
metric   = decode
x        = spec-draft-n-max
series   = CudaDisableGraphs
rename   = unset=graphs on, true=graphs off
```

Against the n-gram-only `f16` preset, the head decodes 54.9 t/s against 34.0 (61% faster) and prefills
1520 against 1962 (23% slower), a good trade for conversations that grow turn by turn.

**Where decode time goes**, live on a coding agent at ~75k tokens, 31.5 t/s: a decode pass takes ~41 ms,
of which ~25 ms of kernels on the two cards. The rest is the hand-off between the cards of a layer split,
which no preset key reaches. A tool-call grammar costs nothing measurable.

### N-gram settings

Code rewrite, IQ3_XXS, 8000 tokens at most, two runs, the mixed pair (v0.6.0):

| setting | decode | note |
| --- | --- | --- |
| none | 28.6 | |
| `ngram-simple` | 52.3 ±1.3 | |
| `ngram-mod` | 52.9 | one run; the second ran out of memory with ~400 MiB free on the RTX 4070 SUPER |
| `ngram-map-k` | 61.0 ±2.7 | answer 124 t/s at 87 to 92% accepted, reasoning 35 |
| `ngram-map-k`, `size-n` 4 / 8 / 12 / 16 | 35.0 / 51.5 / 61.3 / 56.0 | 12, the default, is best |
| `ngram-map-k`, `size-m` 16 / 32 / 48 / 64 | 48.2 / 49.7 / 59.8 / 63.6 | the copied answer rises with the draft length |

`min-hits` is read only by `ngram-map-k4v`. A drafter changes the greedy path, so compare settings on
runs that wrote the same kind of output.

Live on a coding agent's turns on two R9700s (server log, not recorded), n-gram drafters alone add
little where there is little to copy: at 140k tokens 28.3 (`ngram-map-k`) and 27.7 (`ngram-map-k4v`)
against 28.0 with none.

## 6. The tensor split and long prompts

**`split-mode tensor`** on two R9700s, with the internal AllReduce (RCCL cannot run without AtomicOps on
the x4 slot), `ngram-map-k4v`, 262144, ubatch 512. Live on a coding agent's turns, server log, not
recorded:

| | layer split | `split-mode tensor` |
| --- | --- | --- |
| decode at 160k+ tokens | 28.1 (42 turns) | 31.5 (20 turns) |
| prompt read | 1773 re-reading 71k tokens | 933 re-reading 166k tokens |
| lowest free VRAM | 772 MiB after a 43.7k prefill | 9 MiB on the second card during the turns |

Decode gains 12%, but the margin is too thin: use a smaller `ubatch-size` or context before making this
the default.

**A large ubatch for long prompts.** With `op-offload true`, each ubatch uploads the host experts and
the GPUs compute them, so a large ubatch amortizes the upload. This is the layout of
[Strata](https://github.com/niko1221/strata). Both cards, ctx 131072, `q8_0`, `load-mode none`,
`cache-ram 2048`:

| layout | prefill | decode at 43.7k | decode, short prompts |
| --- | --- | --- | --- |
| `n-cpu-moe 14`, ubatch 512, `op-offload false` | 410, 405 | 21.8 | 27.4 |
| experts of layers 25 to 38 on the R9700, ubatch 8192 | 669, 674 | 17.4, 17.5 | 21.6 |
| experts of layers 19 to 38 on the R9700, ubatch 4096 | 578, 581 | 18.4, 18.5 | 22.5 |

It buys 40 to 65% of prefill for 15 to 20% of decode: worth it where long prompts dominate, not for chat.
The compute buffer grows to 4.8 GiB per card at 4096 and 9.5 at 8192. Keep part of the experts in VRAM:
all of them in RAM pin ~47 GiB, which is too much for a 59 GiB host.

## 7. Windows

The same preset, files, llama.cpp and cards, with `device = ROCm1,CUDA0`, temperature 0. The *Windows* figures
are 3 runs per cell by hand, not recorded, except the `ThreadsBatch 24` row, two recorded runs:

| system | prefill at 43.7k | decode at 43.7k |
| --- | --- | --- |
| *Linux* | 416, 419 | 25.0, 25.1 |
| *Windows* | 250 to 262 | 20.4 ±0.5 |
| *Windows*, `ThreadsBatch 24` | 275.7, 279.0 | 20.8, 20.8 |

*Windows* prefills 37 to 40% slower at 12 batch threads and 34% at 24, and decodes 19% slower. The 27B, with
every weight on the GPUs, loses 12 to 17% of its long-prompt prefill to *Windows*, the gap
[Qwen3.8-27B, section 6](qwen3.8_27b.md#6-linux-against-windows) words as *Linux* 14 to 20% faster; what this model has that the 27B
has not is 16 layers of experts on the CPU, so the gap is the CPU side, not the cards. The build has the
same CPU feature set as *Linux* (`ggml-cpu-zen4.dll`: AVX-512, REPACK, LLAMAFILE, OpenMP).

Only the batch threads pay (campaign `flash-next-cpu-prefill-windows-20261021`), taking prefill 6 to 9%
higher with decode unchanged. `--poll 0`, `--cpu-strict 1`, `--prio-batch 2`, and a per-core `--cpu-mask`
add nothing beyond them, so scheduling is not the loss. Keep cmake's default `GGML_OPENMP=ON`: without it
prefill falls by a third (178.6 t/s). `op-offload true` at ubatch 512 is also a loss on *Windows*: 108 t/s
against 277.

**The band kernel on GQA 12** (`patches\llama.cpp\0003`) changes nothing here on either system: *Windows*
prefill 256 ±6 against 258 ±3, decode 20.5 ±0.5 against 20.1 ±0.2 (by hand, not recorded); *Linux* prefill 422.7 ±2.6 against
417.9 ±2.2, decode 25.0 both (campaign `flash-next-patches-linux-20261021`). On *Windows* the output is byte-identical,
and the band's gain sits on the 2 to 4 queries per step of an accepted draft, which these prompts do not
produce. It stays on, with nothing to tune.

## 8. Choices that look better than they measure

| option | result |
| --- | --- |
| the MoE cache, to keep hot experts in VRAM | 3 to 4.6x slower on the R9700; resident layers are 8 to 31% faster on the RTX 4070 SUPER and 22 to 54% on two cards ([section 4](#4-the-moe-cache)) |
| `--no-repack`, to let repacked layers into the cache | does nothing unless `no-host` is set |
| a larger `ubatch-size` paid for with experts in RAM, with `op-offload false` | 319 and 313 t/s at 2048 and `n-cpu-moe 26` (131072, `q8_0`), against 364 and 370 at 512 and `n-cpu-moe 14`; `op-offload true` is what pays ([section 6](#6-the-tensor-split-and-long-prompts)) |
| Vulkan (RADV) on the R9700 instead of ROCm | *Linux*: prefill 330 and 336 against 416 and 419; decode 7.6 and 9.6 against 25.0 |
| `load-mode none` with part of the experts on the card, on *Windows* | aborts right after the weight upload (`ROCm error: unspecified launch failure`, 4 of 4 runs); *Linux* runs it fine. Use `mmap` there |
| `mmap` with every expert in RAM | llama.cpp warns about it, and on *Windows* decode drops by up to 12.5% on the R9700 and 25 to 39% on the RTX 4070 SUPER |

## 9. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| HIP upload cost per miss | n/a | a profile of the miss path on ROCm against CUDA | TBD: needs a trace (rocprofv3, nsys), not a bench run |
| `device` order on *Windows* | the R9700 first | does the card on the fast link belong last? | partly: on two R9700s the x16 card last decodes 23.1 ±1.4 against 21.3 ±0.2, prefill 1499.8 against 1524.7 (campaign `flash-next-2xr9700-device-order-20261028`); with the RTX 4070 SUPER, TBD: needs that card in the test machine |
| `load-mode none` abort | n/a | a minimal reproduction for upstream; does it also hit CUDA or Vulkan on *Windows*? | TBD |
| the CPU backend alone, *Windows* against *Linux* | n/a | does `llama-bench` show the third lost in prefill, deciding between a build fix and a preset fix? | TBD |
