---
title: Qwen3.8-27B
type: model
status: draft
gpus: [AMD Radeon AI PRO R9700, NVIDIA GeForce RTX 4070 SUPER]
models: [Qwen3.8-27B]
platforms: [linux, windows]
llama-cpp: v0.5.0 to v0.8.0
updated: 2026-11-04
---

# Qwen3.8-27B: configuration reference (draft)

Qwen3.8-27B (architecture `qwen35`) is a dense model with an embedded MTP head. This guide says
how to run it for speed on two machines built around a 12-core AVX-512 host with 64 GB of RAM:

- **Two R9700s, on Linux and on Windows** (llama.cpp v0.8.0, the Swift-1.5 fine-tune of the Q8_0): one
  card in the CPU's Gen5 x16 slot (the x16 card), one behind the chipset at Gen4 x4 (the x4 card).
- **An R9700 and an RTX 4070 SUPER in one model** (llama.cpp v0.7.0, mostly the Coder390 fine-tune
  `Qwen3.8-27B-Coder390-EfficientThink-Q8_0-MTP.gguf`; the 43.7k rows of
  [section 4](#4-kv-cache-type-and-context) use the Swift-1.5), on Linux and on Windows. A row
  measured on another release names it.

**In short.** Use an `f16` KV cache at the largest context that fits: deep in the context it
decodes 11 to 12% faster than `q8_0`. Put an n-gram drafter in front of the MTP head: it more
than doubles decode on code and costs nothing on prose. On two R9700s use the tensor split with
an even `tensor-split`: on Linux it decodes 48 to 65% faster than the layer split for 16 to 22% less
cold prefill. Windows runs the same preset with one request's decode level with Linux and prefill
11 to 18% slower ([section 6](#6-linux-against-windows)); the gain over the layer split is unmeasured
there. With two different cards, card order decides most of the speed, and the best
order is the opposite on Windows and on Linux. [Section 7](#7-baseline-under-the-standard-spec-two-r9700s) holds the full baseline of the
two-R9700 preset.

All speeds are t/s, with ± the sample standard deviation over the repetitions. "43.7k" and "115k"
are prompts of 43,730 and 115,343 tokens (the second is named `bench-prompt-110k` in bench.db and the
specs), sent with the prompt cache off.

**The build behind these figures.** The framework bundles llama.cpp v1.0.0, and no figure here was measured on it.
Every figure from v0.8.0, and every recorded one from v0.7.0 except campaign `27b-kv-f16-20261016`, campaign `coder390-device-order-20261019`
and campaign `coder390-windows-vs-linux-20261020`, comes from a build carrying patches the bundled build does not have:
the FlashAttention band on RDNA4 and the backend sampler probe, and on all but the earliest v0.7.0 campaigns host buffers pinned for every runtime. v0.5.0 and v0.6.0 figures predate them; a figure read by hand on v0.7.0 does not record its build, so it may
or may not have carried them. The band is the one that moved speed: on the R9700 with `draft-mtp`, decode 25.9 t/s with
it against 24.5 without at 115k, prefill unchanged (not recorded). How far the figures move is open
([section 9](#9-not-yet-measured-placeholders)).

## 1. Model facts

| item | value | source |
| --- | --- | --- |
| blocks | 64 plus the MTP block (65 in the GGUF) | `block_count = 65`, `nextn_predict_layers = 1` |
| attention | on every 4th block: 16 layers hold a KV cache; 24 query heads, 4 KV heads, head size 256 | `full_attention_interval = 4` |
| memory type | hybrid (attention + recurrent) | `llm_arch_is_hybrid` |
| native context | 262144 | `context_length` |
| GGUF sampling defaults | temp 1.0, top-k 20, top-p 0.95 | `general.sampling.*` |
| file (Q8_0) | 27.04 GiB, 27.32 B parameters | `file size`, `model params` |
| KV cache per token | 64 KiB in `f16`, 34 KiB in `q8_0` | `KV buffer size` |
| recurrent state, one slot | 598.50 MiB | `llama_memory_recurrent: size` |
| MTP draft context | its own `f16` KV over the full context: 672 MiB at 172032 cells, 1024 at 262144 | `llama_kv_cache: ... 1 layers` |
| saved state | ~36 KiB per token held in `q8_0` (v0.6.0) | the snapshot file |
| context checkpoint | 149.6 MiB each, in host RAM | `slot create_check` log lines |

Three behaviours shape how to read every figure below:

- **Prefill slows with depth**: 984 t/s at 4k tokens, 142 at 218k, in one long prompt (v0.6.0).
- **The MTP head reads the prompt too**: 882 t/s against 1065 with no speculator at 115k. Decode
  pays that back.
- **The MTP head can change the greedy text.** On the R9700's tile kernel, MTP and unassisted decoding part
  after ~100 tokens at 115k, because the kernel's split changes with the number of queries; a build with the
  FlashAttention band agreed over 278 tokens (v0.7.0, not recorded, not re-measured on v1.0.0). Different draft
  widths on a mixed-card split write different texts too, each repeatable. Compare runs on the same kind of text.

## 2. Recommended settings

### Any setup

| key | value | evidence |
| --- | --- | --- |
| quant | Q8_0 | the fastest 27B file: Q5_K_M prefills 10 to 13% slower and decodes 7 to 15% slower (*Windows*) |
| `cache-type-k/-v` | `f16` | decodes 11 to 12% faster than `q8_0` deep in the context ([section 4](#4-kv-cache-type-and-context)) |
| `spec-draft-type-k/-v` | `f16` | `q8_0` halves the draft KV but grows the draft compute buffer from 648 to 1696 MiB (v0.6.0) |
| `load-mode` | `none` | every weight is on a GPU: nothing to map |
| `ubatch-size` | `1024` | best prefill at 115k: 902 t/s, against 895 at `768`, 837 at `1536`, 809 at `2048`; decode unchanged. Above `1024` the R9700 keeps 659 and 75 MiB, against 1232 (*Linux*, mixed pair; campaign `coder390-ubatch-20261020`) |
| `ctx-checkpoints` | `12` | covers ~98k tokens of history at the default 8192 spacing, for 1.8 GiB of host RAM; a client compaction should then reprocess under 8192 tokens (not yet observed), against 95,814 tokens in 101.7 s at `4` |

### Two R9700s

Measured on *Linux* unless a row says otherwise. *Windows* runs this same preset, with its own device ids
([section 6](#6-linux-against-windows)).

| key | value | evidence |
| --- | --- | --- |
| `split-mode`, `tensor-split` | `tensor`, `1,1` | decode 59.3 t/s against 40.4 with the layer split at 43.7k; balanced memory ([section 5](#5-splitting-the-model-across-two-cards)). *Windows*: 57.7, no layer run to compare |
| `device` | the x16 card first | decode 33.5 against 32.2, prefill 1491 against 1476 (layer split; campaign `swift27b-2xr9700-linux-device-20261028`). *Windows*, tensor split: prefill unchanged, decode an open lead ([section 9](#9-not-yet-measured-placeholders)) |
| `CudaAllreduce` | `internal` | RCCL cannot run: the x4 card's slot has no PCIe AtomicOps, and *Windows* has no RCCL |
| `ThreadsBatch` | `12` on *Linux*, `24` on *Windows* | what each baseline ran. It makes no difference here: *Windows* prefills 1034.3 at `24`, 1031.7 at `12` ([section 6](#6-linux-against-windows)); `24` is there for [Flash-Next](qwen3.8_flash_next.md) |
| `ubatch-size` | `1024` | `2048` prefills 24% slower: 1118.3 ±5.8 against 1472.3 ±8.0 (layer split, MTP alone, 262144, `parallel 1`; campaign `swift27b-2xr9700-linux-ubatch-20261028`) |
| `ctx-size`, `parallel` | `425984`, `2` | 212992 per slot, with 2476 / 1378 MiB free after a 115k prefill; *Windows* holds it too ([section 4](#4-kv-cache-type-and-context)) |
| `spec-type` | `ngram-map-k4v,draft-mtp`, `size-m 32` | code 119.9 t/s against 49.8 with MTP alone; prose 32.2 against 30.9 ([section 3](#3-drafters)) |
| `spec-draft-n-max` | `2` | code 136.5 t/s, 119.9 at `3`; prose stays near 30 ([section 3](#3-drafters)) |
| `mmproj-device` | the x4 card | a 3840x2160 image appended to a 115k prompt, the other slot holding 115k tokens: at least 2877 / 1032 MiB free during it (by hand, sysfs sampler, not recorded) |

### An R9700 and an RTX 4070 SUPER

| key | value | evidence |
| --- | --- | --- |
| `device` | the R9700 last on *Linux*, first on *Windows* | the order that wins on each system ([section 5](#5-splitting-the-model-across-two-cards), [section 6](#6-linux-against-windows)) |
| `tensor-split` | most layers on the R9700: `16,50` on *Linux*, `52,14` on *Windows* | free on the RTX 4070 SUPER: 1026 MiB after a 115k prefill on *Linux*; 687 at 43.7k and 527 at 115k on *Windows* |
| `ctx-size` | `180224` | the largest with margin: 638 / 1026 MiB free after a 115k prefill, with the embedding table off the RTX 4070 SUPER; with it on, 180224 and 196608 do not load ([section 4](#4-kv-cache-type-and-context)) |
| `split-mode` | `layer` | `tensor` across two backends aborts at load unless AllReduce is `none`; with `none` it prefills 599 t/s against 1290, decode the same (v0.6.0) |
| `override-tensor` | no `token_embd` rule; `output.weight` on the last device | [section 5](#5-splitting-the-model-across-two-cards) |
| `spec-type`, `spec-draft-n-max` | `draft-mtp`, `3` at long context, `4` otherwise | *Windows*: 50.6 t/s at 2.6k tokens at `4`, 30.1 at 93.6k at `3`; 17.9 unassisted (*Linux*, v0.6.0) |
| `mmproj-device` | the RTX 4070 SUPER | a 3840x2160 image at 115k succeeds, text speed unchanged ([section 5](#5-splitting-the-model-across-two-cards)) |

## 3. Drafters

Two R9700s, layer split, the preset's sampling (temperature 1), 5 repetitions, all at `q8_0` KV,
`ctx-size 514288`, `35,31`, `parallel 2`: compare these figures with each other, not with the `f16`
preset this page recommends. The code prompt is a rewrite with 1536 tokens out: at 256 tokens the reply ends inside the reasoning and the
n-gram stage has nothing to copy (campaign `swift27b-drafter-2xr9700-linux-20261030`;
campaign `swift27b-drafter-code1536-2xr9700-linux-20261030`).

| drafter | decode on 43.7k prose | decode on code |
| --- | --- | --- |
| none | 15.4 | 18.6 |
| MTP | 30.9 | 49.8 |
| DFlash2 on the second card, `spec-draft-n-max 5` | 28.8 | 58.2 |
| n-gram (`ngram-map-k4v`), then MTP | 32.2 | 119.9 |
| n-gram (`ngram-map-k4v`), then DFlash2 | 31.0 | 127.1 |

```bench-chart
title    = Swift: decode by drafter at the preset's sampling, 43.7k-token prose
campaign = swift27b-drafter-2xr9700-linux-20261030
metric   = decode
x        = spec-type
series   = prompt
where    = prompt=bench-prompt-long
rename   = unset=none, ngram-map-k4v,draft-dflash=k4v + dflash, ngram-map-k4v,draft-mtp=k4v + mtp, bench-prompt-long=43.7k prose
```

```bench-chart
title    = Swift: decode by drafter, code rewrite with 1536 tokens out
campaign = swift27b-drafter-code1536-2xr9700-linux-20261030
metric   = decode
x        = spec-type
series   = prompt
rename   = unset=none, ngram-map-k4v,draft-dflash=k4v + dflash, ngram-map-k4v,draft-mtp=k4v + mtp, bench-prompt-code=code rewrite
```

Either speculator doubles decode. The n-gram stage then doubles it again on code, where the reply
copies the context, and costs nothing on prose. MTP and DFlash2 are within noise on prose and in
the chains; alone on code DFlash2 is 17% faster, 58.2 ±3.4 against 49.8 ±1.3. MTP needs no second file but takes ~1.3 GB more on the last card, where its KV and compute
buffer sit with the output head.

The draft length matters less than the text (campaign `swift27b-draft-length-2xr9700-linux-20261030`):

| `spec-draft-n-max`, n-gram then MTP | decode on 43.7k prose | decode on code |
| --- | --- | --- |
| **2** | 29.9 | 136.5 ±8.4 |
| 3 | 32.2 | 119.9 ±19.8 |
| 4 | 30.5 | 107.3 ±35.0 |
| 5 | 27.3 | 124.0 ±17.1 |

The code legs swing with how much of the prompt each sampled reply copies. The n-gram stage has
its own length, `spec-ngram-map-k4v-size-m`: in the n-gram then DFlash2 chain, `16`, `32` and `64`
give 118.8, 127.1 and 134.9 ±33.6 on code.

MTP alone, with `f16` at 262144, `1,1`, `parallel 1`, temperature 0, 2 repetitions and 256 tokens out,
decodes prose at 33.4 at the default `spec-draft-n-max` (`3`). `2`, `4` and `5` give 31.6, 33.6 and
32.9: `4` is within noise, none is better (campaign `swift27b-2xr9700-linux-draft-n-max-20261028`).

If you prefer DFlash2, `spec-draft-n-max 5` is its best on prose. DFlash2 alone, on the plain
`Qwen3.8-27B-Q8_0.gguf` with `f16` at 262144, `parallel 1`, temperature 0 and 256 tokens out, decodes
34.1 ±2.0 at `5` against 32.2 ±1.4 at `7` (4 repetitions), 32.4 at `3` and 31.3 at `10` (2 repetitions)
(campaign `qwen27b-dflash-2xr9700-linux-draft-n-max-20261028`). That setup holds its speed at
`ctx-size 262144`: at `7`, 1396.9 prefill and 31.5 decode against 1405 and 31.1 at 172032
(campaign `qwen27b-dflash-2xr9700-linux-ctx-20261028`). On code, in the n-gram then DFlash2 chain on the Swift-1.5
file, with `q8_0` KV, temperature 1 and 1536 tokens out, it reaches 146.1 ±28.8 at `7`
(campaign `swift27b-draft-length-2xr9700-linux-20261030`).

## 4. KV cache type and context

`f16` takes more memory per token than `q8_0` but less compute buffer, so it reaches about 66% of
the `q8_0` context, not half, and decodes faster. The gap grows with the tokens held and does not
show on short prompts. Conversations that grow turn by turn pay that decode at every turn, so
`q8_0` is worth it only if you need a context `f16` cannot fit. A `q8_0` slot snapshot does not
restore into an `f16` cache.

An R9700 and an RTX 4070 SUPER, *Linux*, the R9700 last, `draft-mtp`
(campaign `27b-kv-f16-20261016`, the 43.7k rows; campaign `coder390-ubatch-20261020`, the 172032 row;
campaign `coder390-ctx-20261020`, 180224 and 188416):

| KV, context | free, R9700 / RTX 4070 SUPER (MiB) (a) | prefill | decode |
| --- | --- | --- | --- |
| `q8_0`, 262144 | 196 / 223 | 1158, 1169 at 43.7k | 31.5, 31.5 at 43.7k |
| `f16`, 163840 | 878 / 989 | 1161, 1162 at 43.7k | 35.2, 34.5 at 43.7k |
| `f16`, 172032 | 1232 / 1274 | 902 at 115k | 29.3 at 115k |
| **`f16`, 180224** | 638 / 1026 | 889 at 115k | 28.5 at 115k |
| `f16`, 188416 | 339 / 986: under the 500 MiB margin this card needs | 893 at 115k | 29.3 at 115k |

(a) The 43.7k rows ran the Swift-1.5 file with `token_embd` on the RTX 4070 SUPER and the image encoder
on the R9700, 128 tokens out, free memory read after a 2.9k prompt. The 115k rows ran the Coder390 file
with the table in host RAM and the encoder on the RTX 4070 SUPER, 256 tokens out, free memory read after
the 115k prefill. That placement is why the larger contexts show more free memory.

Among the `f16` contexts speed differs only by the draft's noise, so the largest with margin wins.
Short prompts decode about the same in both types: 37.7 in `f16`, 38.9 in `q8_0`.

Two R9700s, layer split at `35,31`, `ngram-map-k4v,draft-mtp` at `spec-draft-n-max 2`, `parallel 2`,
temperature 0, 256 tokens out: `f16` at 393216 decodes 30.8 t/s against 27.5 ±0.4 in `q8_0` at 514288, at
43.7k, prefill 1431 against 1412 (campaign `swift27b-kv-f16-mtp-2xr9700-linux-20261031`). Free memory after
that prefill (x16 / x4 card) is 3920 / 2580 MiB at 360448, 2768 / 1172 at 393216 and 464 / 0 at 458752;
425984 runs out of memory in the prefill, so 393216 is this split's limit. Within that range decode stays
30.8 to 31.0 and prefill 1425 to 1431. With MTP alone at `1,1` and `parallel 1`, 262144 matches
180224: 1472.3 / 33.4 against 1491.1 / 33.5 (campaign `swift27b-2xr9700-linux-ctx-20261028`;
campaign `swift27b-2xr9700-linux-device-20261028`).

```bench-chart
title    = Swift: free memory per card after a 43.7k prefill, f16 KV, by context
campaign = swift27b-kv-f16-mtp-2xr9700-linux-20261031
metric   = free-mib
x        = ctx-size
series   = ctx-size
where    = cache-type-k=f16
```

Two R9700s, *Linux*, tensor split, `parallel 2`, after a 115k prefill
(campaign `swift27b-ctx-tensor-2xr9700-linux-20261031`):

| context | free, one prefill (MiB) | free, both slots prefilling (MiB) | prefill | decode |
| --- | --- | --- | --- | --- |
| **425984** | 2476 / 1378 | 2460 / 1366 | 961.6 ±2.3 | 51.4 |
| 458752 | 1364 / 226 | 1308 / 214 | 962.0 ±1.4 | 51.4 |

Speed does not depend on the context, so `425984` keeps the margin. The native 262144 per slot
(524288) would need ~4 GiB more per card.

```bench-chart
title    = Free memory per card after a 115k prefill, both slots prefilling at once
campaign = swift27b-ctx-tensor-2xr9700-linux-20261031
metric   = free-mib
x        = ctx-size
series   = ctx-size
where    = concurrency=prefill
```

*Windows* holds `425984` too. Free memory after the first prefill of one 43.7k request, x16 / x4 card, in MiB:

| pass | *Windows* | *Linux* |
| --- | --- | --- |
| `engine` | 4829 / 3390 | 4552 / 3414 |
| `real` | 2758 / 1166 | 2518 / 1380 |

The *Windows* figures are system-wide, read from the Windows GPU counters per adapter (`pci-03:00.0` the x16 card,
`pci-14:00.0` the x4 card), under the method of [section 6](#6-linux-against-windows). They repeat exactly across
those runs, whatever the device order (campaign `swift27b-memory-integrity-windows-20261102`). The *Linux* figures
are the baseline's (campaign `swift27b-standard-1.0-2xr9700-linux-20261031`), from another probe, so compare the
two loosely. The *Windows* run of the standard spec holds no free memory.

With two slots at 115,343 tokens each at once on *Windows*, there are no errors and no spill to shared memory, and
peak dedicated memory reads 31304 and 29712 MiB of 32624, so 1320 and 2912 free (Windows performance counters,
read by hand, not recorded; the adapters are not formally mapped to the cards, and the fuller one is probably the
x4).

## 5. Splitting the model across two cards

### Two R9700s: the tensor split

![Two R9700s: the x16 card in the CPU slot, the x4 card behind the chipset without AtomicOps; the tensor split stages its AllReduce through host memory](assets/diagrams/two-r9700-topology.svg)

*Linux*, the Swift preset with `f16` KV, `parallel 2`, the n-gram stage then MTP at `spec-draft-n-max 2`,
`ubatch-size 1024`, temperature 0, 3 repetitions, output uncapped, every cold request with the
prompt cache off, no spec (campaign `swift27b-concurrency-2xr9700-linux-20261031`).

Two requests at once run in three scenarios: both cold (`prefill`), both decoding on a cached
prompt (`decode`), and one cold while the other decodes (`prefill+decode`). A total is the tokens of
both requests over the window they share.

| scenario | figure | layer, 43.7k | tensor, 43.7k | layer, 115k | tensor, 115k |
| --- | --- | --- | --- | --- | --- |
| one request | prefill | 1433.6 ±6.6 | 1114.7 ±33.2 | 1098.6 ±6.2 | 924.8 ±1.0 |
| one request | decode | 40.4 ±0.8 | 59.6 ±0.1 | 33.5 ±0.0 | 55.4 ±0.0 |
| `prefill` | prefill total | 1395.9 ±13.9 | 1112.2 ±0.3 | 1060.2 ±8.4 | 937.5 (a) |
| `prefill` | decode total | 42.4 ±3.8 | 45.5 ±0.4 | 20.4 ±0.3 | 22.7 (a) |
| `decode` | decode total | 64.3 ±0.5 | 82.5 ±6.8 | 52.1 ±0.2 | 68.7 ±0.2 |
| `decode` | decode per request | 32.3 ±0.2 | 49.9 ±4.0 | 26.1 ±0.1 | 34.6 ±0.1 |
| `prefill+decode` | prefill of the cold request | 1334.5 ±19.8 | 1085.4 ±1.9 | 1012.1 ±3.2 | 908.9 ±1.6 (b) |
| `prefill+decode` | decode of the warm request | 20.2 ±0.1 | 37.1 ±0.2 | 10.4 ±0.3 | 15.8 ±5.2 (b) |
| `prefill+decode` | decode total | 39.5 ±0.2 | 52.1 ±0.2 | 19.5 ±0.2 | 27.5 ±4.9 (b) |

Most tensor columns ran at `tensor-split 35,31`, `ctx-size 393216`, with the image encoder on the CPU.
The two 115k scenarios marked, five cells, come from runs of the same preset at the even split below,
`ctx-size 425984`, with the encoder on the x4 card. The even split matches `35,31` at 43.7k (1126.4 prefill
and 59.3 decode against 1136.1 and 59.8). At 115k it prefills 4% faster and decodes 7% slower (961.6 and
51.4 against 924.8 and 55.4), on another text.

- (a) one repetition at temperature 0 (campaign `swift27b-ctx-tensor-2xr9700-linux-20261031`);
- (b) the baseline of [section 7](#7-baseline-under-the-standard-spec-two-r9700s), with the preset's own sampling and 5 repetitions: compare its
  prefill freely, but its decode only loosely with the temperature 0 layer column.

```bench-chart
title    = Decode total by scenario, 43.7k tokens
campaign = swift27b-concurrency-2xr9700-linux-20261031
metric   = decode-total
x        = concurrency
series   = split-mode
where    = prompt=bench-prompt-long
rename   = unset=layer
```

```bench-chart
title    = Decode of one request by context depth
campaign = swift27b-concurrency-2xr9700-linux-20261031
metric   = decode
x        = prompt
series   = split-mode
where    = concurrency=single
rename   = unset=layer, bench-prompt-long=43.7k tokens, bench-prompt-110k=115k tokens
```

- **The tensor split decodes faster**: 48% for one request at 43.7k, 65% at 115k, 28 and 32% in
  total with two decodes. A request decoding beside a prefill nearly doubles at 43.7k.
- **It prefills 16 to 22% slower.** Two cold prefills run one after the other under both splits, so
  their total equals one request's rate.
- **The x4 link does not limit a layer-split decode**: under `rocprofv3` at 43.7k, cross-card copies
  take under 0.4% of the time and `mul_mat_vec_q` 67% of the GPU time.
- **Temperature 0 with a drafter writes a different text under each split** (acceptance 84% layer,
  76% tensor, two decodes at 115k), so each decode figure is the drafter's rate on its own text.
- **On *Windows* this comparison is unmeasured.** The tensor preset has its full run there
  ([section 6](#6-linux-against-windows)), but no run of this model uses the layer split on two R9700s on
  *Windows*, so neither the decode gain nor the prefill cost above can be claimed for it.

**Split evenly.** Under `split-mode tensor` an uneven split inflates the buffers: `35,31` takes
14238 MiB of model and 15360 of KV against 13229 and 12288 at `1,1`. After a 43.7k prefill that
leaves 0 / 7766 MiB free against 3624 / 2486, at the same speed: prefill 1136.1 ±2.3 and 1126.4
±1.9, decode 59.8 ±0.2 and 59.3 ±0.1 (campaign `swift27b-balance-mmproj-2xr9700-linux-20261031`). With
the x16 card that full, two 115k contexts at once run out of memory at `35,31`; `1,1` holds them.

**Check the link before measuring.** The tensor split stages every AllReduce through host memory,
so its prefill follows the x4 card's host link, and a boot can train the CPU-to-chipset link at half
width. Before a tensor-split benchmark run `tools\pcie-bench`; if the x4 card reads about half, power
cycle the machine.

- *Linux*: at Gen4 x2 the x4 card moved 3.5 GB/s instead of 7.0 and the same preset prefilled 785.6 ±0.5
  against 1136.1 ±2.3; decode 58.0 against 59.8 (campaign `swift27b-driver-check-2xr9700-linux-20261031`).
  The kernel's boot log names the width (`available PCIe bandwidth ... limited by ... x4 link`).
- *Windows* shows the same symptom but no link width on the bridges, so x2 is inferred from the halved
  bandwidth. The x4 card read 3.45 / 3.52 GB/s of DMA (device to host / host to device, 32 MiB) on one boot and
  6.75 / 6.77 after a power cycle; the x16 card reads ~45 to 49. Prefill at 43.7k read 674 on the slow boot
  against 945 after the power cycle, one request each with the drafter on (read by hand, not recorded).

### An R9700 and an RTX 4070 SUPER: order and placement

**The last device takes the MTP head, the output head and the sampling.** On *Linux*, the
R9700 last prefills 1133 and 1123 t/s at 43.7k against, with the RTX 4070 SUPER last, 1069 at `52,14`
(28 MiB free) and 1071 at `53,13`. It decodes 31.2 against 29.8 at `53,13`, the split that leaves that
card a usable 430 MiB; `52,14` decodes 31.3 with no margin, and `50,16` and `48,18` do not load
(campaign `coder390-device-order-20261019`). *Windows* reverses the order: [section 6](#6-linux-against-windows).

**Attention blocks decide the split.** Attention sits on blocks 3, 7, 11, 15 and so on, each with
672 MiB of KV at 172032 `f16`, so the boundary block moves its KV slice with it: block 15 costs
1061 MiB on the R9700, a linear block 389. Pin `blk\.15\.ffn_(up|down)` to the R9700 at the
boundary.

**The embedding table stays in host RAM** (llama.cpp's default), freeing 1288 MiB on the RTX 4070
SUPER at no cost: decode 31.2 t/s either way at 43.7k and 29.3 at 115k, prefill 1178 on the card
against 1164 in RAM and 897 against 899 (campaign `coder390-placement-20261020`).

**The output head stays on the last device.** Moving `output.weight` to the RTX 4070 SUPER lowers
prefill to 1026 t/s from 1139 at 43.7k and to 779 from 890 at 115k, because the MTP draft on the
other card then hops devices for every drafted token.

**The image encoder goes on the RTX 4070 SUPER**, in the room the embedding table left: 600 MiB of
weights plus ~418 MiB of compute at the first 3840x2160 image. Added to a 115k conversation
at 180224, that image leaves 801 / 716 MiB free. On the R9700 it aborts the server: at 172032 that card
has 342 MiB free after a 115k prefill, and the encoder runs it out of memory.

## 6. Linux against Windows

### Two R9700s: the same preset on both systems

**Expect cold prefill 10 to 18% slower on *Windows* and the same decode for one request; decode drops under
concurrency on code.** The machine, the two cards, the Swift-1.5 preset and the spec's legs are those of the
*Linux* baseline in [section 7](#7-baseline-under-the-standard-spec-two-r9700s), whose folded notes list what
differs (campaign `swift27b-standard-1.1-2xr9700-windows-20261101`). One request:

| prompt | pass | prefill, *Linux* / *Windows* | decode, *Linux* / *Windows* | accepted, *Linux* / *Windows* |
| --- | --- | --- | --- | --- |
| 43.7k prose | `engine` | 1210.3 / 1001.7 (17% slower) | 27.1 / 29.6 (9% faster) | n/a |
| 43.7k prose | `real` | 1125.4 / 952.1 (15% slower) | 59.3 ±1.5 / 57.7 ±2.0 | 79% / 78% |
| 115k prose | `engine` | 1025.5 / 872.8 (15% slower) | 24.5 / 26.4 (8% faster) | n/a |
| 115k prose | `real` | 960.1 / 823.3 (14% slower) | 59.2 ±7.0 / 53.9 ±2.9 | 82% / 82% |
| agent turn | `real` | 1138.2 / n/a | 52.0 / n/a | 69% / n/a |
| writing code | `engine` | 1211.0 / 996.7 (18% slower) | 27.1 / 29.4 (8% faster) | n/a |
| writing code | `real` | 1135.2 / 940.8 (17% slower) | 55.9 ±5.1 / 50.9 ±4.3 | 62% / 62% |
| image | `engine` | 612.7 ±67.3 / 545.1 ±71.1 (11% slower) | 28.9 / 31.8 (10% faster) | n/a |
| image | `real` | 613.4 ±33.0 / 536.4 ±47.3 (13% slower) | 50.6 ±0.5 / 51.5 ±0.5 | 63% / 62% |

The *Windows* run holds no cold leg of the agent turn, only its cached step below.

```bench-chart
title    = Cold prefill of one request by prompt, Linux against Windows, the preset's speculator
campaign = swift27b-standard-1.0-2xr9700-linux-20261031, swift27b-standard-1.1-2xr9700-linux-20261101, swift27b-standard-1.1-2xr9700-windows-20261101
metric   = prefill
x        = prompt
series   = ThreadsBatch
where    = concurrency=single, spec-pass=real
rename   = 12=Linux, 24=Windows, bench-prompt-43k-1.0=43.7k prose, bench-prompt-110k-1.0=115k prose, bench-prompt-code-1.0=agent turn, bench-prompt-code-write-1.1=writing code, bench-prompt-image-1.0=image
```

- **Prefill.** The gap holds on text and on the image, whose repetitions spread widely. Its cause is unmeasured
  (below). During a 43.7k prefill the *Windows* copy engine reads ~47% on the x4 card and 11 to 12% on the x16 card
  (read by hand, not recorded).
- **Decode with the speculator** is level: within the repetitions' spread on prose and code, under 2% apart
  on the image.
- **Decode without it** is 8 to 10% faster on *Windows*, and on the text prompts both systems write the same
  number of tokens (1670, 7840 and 9599). The run does not explain why.

**No *Windows* setting tried closes the prefill gap; leave them as they are.** The Swift preset on two R9700s,
tensor split at `1,1`, the 43.7k prompt, `spec = custom` derived from `standard-1.1`: one repetition per pass,
`engine` at temperature 0 with the speculator off, `real` with the preset's sampling and speculator. The first row
is the standard run, with 3 and 5 repetitions (campaign `swift27b-standard-1.1-2xr9700-windows-20261101`). The
others ran on one boot, where `tools\pcie-bench` read the x4 link at 6.8 GB/s, and compare with the second row
(campaign `swift27b-memory-integrity-windows-20261102`; campaign `swift27b-device-order-windows-20261102`;
campaign `swift27b-threads-batch-windows-20261102`; campaign `swift27b-power-plan-windows-20261102`).

| *Windows* setup | `engine` prefill / decode | `real` prefill / decode |
| --- | --- | --- |
| **the preset, Memory Integrity on** | 1001.7 / 29.6 | 952.1 / 57.7 (repetitions 56.1 to 60.2) |
| Memory Integrity off | 1034.3 / 30.0 | 970.9 / 59.4 |
| Memory Integrity off, x4 card first | 1029.0 / 30.1 | 968.6 / 63.0 |
| Memory Integrity off, `ThreadsBatch 12` | 1031.7 / 30.0 | n/a |
| Memory Integrity off, Ultimate Performance plan | 1031.5 / 30.0 | n/a |

- **Memory Integrity off** prefills 3% and 2% faster, with decode unchanged within noise (VBS and the hypervisor
  stayed on). That is a small part of the gap at most, *Windows* still 15% and 14% slower than *Linux*, and no
  reason to turn it off: the setting stays on. This comparison crosses boots, and that Memory Integrity was on during
  the standard run is inferred, not recorded: it was the machine's state until it was turned off for this test.
- **`ThreadsBatch 12` against `24`** changes nothing on this preset, every weight on the GPUs: prefill 0.3% slower,
  so the thread count does not explain the gap here. At `12`, the *Linux* value, *Windows* is still 15% slower, 1031.7
  against 1210.3. [Flash-Next](qwen3.8_flash_next.md), with experts on the CPU, is another case, with its own finding.
- **The Ultimate Performance plan** turns PCIe ASPM off (Balanced: moderate) and changes nothing: prefill 0.3%
  slower. The minimum processor state stays 0, so this does not test a pinned CPU frequency.
- **What remains is unmeasured.** The driver and HIP runtime, transfers and submissions under `split-mode tensor`,
  and the hypervisor are candidates, not results ([section 9](#9-not-yet-measured-placeholders)).

Two requests at once, pass `real`, as totals. The per-request prefill of a `prefill+decode` row in
`bench query summary` pools the long cold prompt with the warm request's very short one, so compare it only
with a like row.

| prompt | prefill total, `prefill` | cold prefill, `prefill+decode` | decode total, `prefill` | decode total, `decode` | decode total, `prefill+decode` |
| --- | --- | --- | --- | --- | --- |
| 43.7k prose | 1104.7 / 926.4 | 1079.5 / 896.9 | 57.8 ±10.1 / 48.2 ±4.7 | 87.3 ±5.7 / 83.4 ±5.9 | 46.9 ±3.8 / 48.0 ±6.7 |
| 115k prose | 935.1 / 800.9 | 908.9 / 778.4 | 31.6 ±3.9 / 34.8 ±3.1 | 69.7 ±3.8 / 69.3 ±4.7 | 27.5 ±4.9 / 23.7 ±5.4 |
| writing code | 1203.0 / 1003.6 | 1082.5 / 905.0 | 64.7 ±4.3 / 56.5 ±5.6 | 74.0 ±4.6 / 60.2 ±6.3 | 64.0 ±6.0 / 58.9 ±6.7 |
| image | 649.6 ±17.5 / 568.2 ±27.7 | 567.1 ±6.6 / 511.7 ±2.7 | 86.8 ±3.6 / 84.0 ±3.9 | 84.1 ±3.6 / 82.1 ±3.0 | 83.1 ±4.0 / 77.7 ±6.4 |

*Linux* / *Windows* in every cell.

```bench-chart
title    = Writing code: decode total by scenario, Linux against Windows
campaign = swift27b-standard-1.1-2xr9700-linux-20261101, swift27b-standard-1.1-2xr9700-windows-20261101
metric   = decode-total
x        = concurrency
series   = ThreadsBatch
where    = prompt=bench-prompt-code-write-1.1, spec-pass=real
rename   = 12=Linux, 24=Windows
```

- **On code, decode under concurrency is lower on *Windows*.** It shows most in the `decode` scenario: total 60.2
  against 74.0 (19% slower), per request 33.3 ±3.1 against 41.5 ±5.6. In `prefill` it is 31.3 ±4.6 against
  36.8 ±7.0 per request; in `prefill+decode` the difference, 33.5 ±7.0 against 37.7 ±4.0 with both requests
  pooled, stays within the spread. It sits mostly in the code itself: in `decode` the reasoning runs 33.2
  against 36.4 and the code 37.1 against 75.4, whose requests spread from 27 to 67 and from 35 to 150.
- **The 43.7k prose does not show it**: 47.0 against 48.2 per request in `decode`.
- **Draft acceptance does not explain it.** It reads 59% against 63% in `decode` but 65% against 62% in
  `prefill+decode`, the wrong way round for a lower decode there. The cause is not known.
- **The prefill total above one request's rate belongs to the prompt**, not to *Windows*: on code it reads 1003.6
  against 940.8 on *Windows* and 1203.0 against 1135.2 on *Linux*.

The agent step with its prefix cached, as in
[section 7](#7-baseline-under-the-standard-spec-two-r9700s), replying in 77 to 221 tokens on *Windows*:

| scenario | first token (s), *Linux* / *Windows* | turn (s), *Linux* / *Windows* | prefill, *Linux* / *Windows* | decode of the step, *Linux* / *Windows* | accepted, *Linux* / *Windows* |
| --- | --- | --- | --- | --- | --- |
| one request | 6.3 ±0.3 / 6.7 ±0.0 | 8.8 ±0.7 / 9.9 ±1.1 | 867.9 ±36.7 / 809.3 ±2.7 (the step) | 51.5 ±3.3 / 45.5 ±5.5 | 63% / 53% |
| `decode`: two steps at once | 9.7 ±2.2 / 10.9 ±1.8 | 16.6 ±3.3 / 16.5 ±0.8 | n/a | 31.1 ±11.2 / 34.1 ±8.0 | 64% / 70% |
| `prefill+decode`: beside a 42.5k cold prefill | 10.1 ±0.2 / 11.5 ±0.0 | 47.7 ±1.6 / 55.8 ±1.6 | 1045.2 ±3.4 / 856.1 ±33.7 (the cold one) | 3.7 ±1.4 / 2.9 ±1.0 | 60% / 64% |

A step alone costs about a second more. Two steps at once are level. A step beside another conversation's
cold prefill waits for it, so its turn takes 17% longer on *Windows*, 55.8 s against 47.7, in line with that
prefill being 18% slower. These replies are too short for their decode to separate the systems: in the first
two rows it moves with acceptance.

**The device order leaves prefill unchanged on this pair; keep the x16 card first.** In the one-repetition runs
above, the x16 card first prefills 1034.3 against 1029.0 under `engine` and 970.9 against 968.6 under `real`, and free
memory stays the same to the MiB ([section 4](#4-kv-cache-type-and-context)). The x4 card first reads 63.0 against 59.4
in `real` decode, one repetition of a noisy pass, above the 56.1 to 60.2 of the standard run's repetitions, with
acceptance 79% in both: an open lead, not a result ([section 9](#9-not-yet-measured-placeholders)). The
"R9700 first" rule below belongs to a layer split across two different cards and does not apply here. The
integrated GPU takes `ROCm0` on *Windows*, so a preset copied from *Linux* needs its ids renumbered: on this
machine `device = ROCm2,ROCm1`, with `ROCm2` the x16 card and `ROCm1` the x4 card.

### An R9700 and an RTX 4070 SUPER: the order reverses

The same machine and build, the Coder390 file, an R9700 and an RTX 4070 SUPER, the R9700 last, 172032 `f16`, three
repetitions of 256 tokens at temperature 0 (campaign `coder390-device-order-20261019`;
campaign `coder390-windows-vs-linux-20261020`; with the [section 5](#5-splitting-the-model-across-two-cards) placement,
campaign `coder390-placement-20261020` and campaign `coder390-placement-windows-20261021`):

| prompt | prefill, *Linux* / *Windows* | decode, *Linux* / *Windows* |
| --- | --- | --- |
| 100 tokens | 225 / 209 | 38.1 / 31.0 |
| 43.7k, [section 5](#5-splitting-the-model-across-two-cards) placement | 1164 / 1020 | 31.2 / 26.9 |
| 115k, [section 5](#5-splitting-the-model-across-two-cards) placement | 899 / 747 | 29.3 / 25.1 |

*Linux* prefills 8 to 20% faster, widening with the context, and decodes 16 to 23% faster. Weights and KV per card match to the
MiB and draft acceptance within a few points, so the gap is the time of each step.

**On *Windows*, put the R9700 first.** It removes most of that gap
(campaign `coder390-device-order-windows-20261021`):

| *Windows* | prefill at 43.7k | decode at 43.7k | prefill at 115k | decode at 115k |
| --- | --- | --- | --- | --- |
| the R9700 last | 1029 | 27.1 | 747 | 25.1 |
| **the R9700 first, `52,14`** | 1122 (9% faster) | 32.8 (21% faster) | 846 (13% faster) (a) | 30.9 (23% faster) (a) |

(a) At `180224`; the other cells at 172032.

Decode then passes the *Linux* figure and prefill comes within 4% at 43.7k and 6% at 115k. Free
memory after the 115k prefill at `180224`: 1714 MiB on the R9700, 527 on the RTX 4070 SUPER. *Linux*
goes the other way ([section 5](#5-splitting-the-model-across-two-cards)). *Windows* also lists the CPU's integrated GPU as a ROCm device
ahead of the R9700, so a preset copied from *Linux* needs its device ids renumbered.

**Why the order matters on *Windows*.** llama's input buffers are pinned for the first device only,
and the HIP runtime on *Windows* copies pageable memory at ~2 GB/s, blocking the thread. With the
R9700 second, ~200 MiB per ubatch crosses that way: the block-15 activations and the attention
mask, which grows with the context. That is ~12% of a prefill step; the R9700's own layers take
~65% and the RTX 4070 SUPER's ~19%. The *Linux* runtime pins pageable memory on the fly, so it never
pays this. The order is the setting: the bundled build pins those buffers for the first device
alone. A build that registered them with every runtime cut the blocking copies from 5.24 s to 2.50 s
and gained ~2% with the R9700 second and nothing with it first. Keeping the R9700 second, moving
`blk\.15\.ffn_gate` onto it buys 3.8% of prefill (campaign `coder390-gate-windows-20261021`, on a build
with the band, not re-measured on v1.0.0).

On *Linux*, registering them with every runtime gains nothing measurable. A build that did, at `180224`, prefills
1185.6 ±12.6 at 43.7k (campaign `coder390-patches-linux-20261021`) against 1164.0 ±15.7 for one that did not at 172032, the
same placement (campaign `coder390-placement-20261020`): within noise, decode the same. Both builds carried the band.

## 7. Baseline under the standard spec (two R9700s)

This is the reference for this preset, on *Linux*; [section 6](#6-linux-against-windows) sets the *Windows*
run of the same spec beside it. Compare a later change to the preset, the machine or the
software against the figures of its own system, measured again under the same spec version. `llama-cpp-config bench query run
<id> --db benchmarks/bench.db` prints every parameter of one run.

The tables merge two runs on the same machine, software and preset. The prose, image and agent-turn
legs ran under `standard-1.0` (campaign `swift27b-standard-1.0-2xr9700-linux-20261031`).
`standard-1.1` keeps the prose and image legs unchanged and adds writing code and the cached agent
step, run separately (campaign `swift27b-standard-1.1-2xr9700-linux-20261101`). Both ran on llama.cpp v0.8.0 with
the FlashAttention band and the other patches listed below; this baseline has not been re-measured on the bundled
v1.0.0, which carries neither.

<details>
<summary>Machine, software, preset and method</summary>

**Machine.** AMD Ryzen 9 9900X (12 cores, 24 threads), 64 GB DDR5 6000 MT/s (59 GiB visible), two
AMD Radeon AI PRO R9700 (32624 MiB each, vbios `113-R9700AT-F40`): the x16 card in the CPU's Gen5 x16
slot, the x4 card behind two chipset switches at Gen4 x4 with no PCIe AtomicOps. *Linux*: Ubuntu 26.04.1 LTS,
kernel 7.0.0-42-generic, amdgpu 7.1.9, stock 300 W power cap. *Windows*: Windows 11 Pro 10.0.26300, AMD
driver 32.0.32015.7029.

**Software.** llama-cpp-framework 2.2.0, llama.cpp v0.8.0 (build 11960, `8642541b2`) with the patches
`0001-qwen35-fastmtp-d2t`, `0002-sampler-backend-probe-chain-width`, `0003-fa-band-wmma-rdna4`,
`0004-sched-host-buffers-pinned-all-runtimes` and `0005-qwen35-mtp-graph-realloc-pr31415`; ROCm from
TheRock 10.1.0. Environment: `GGML_CUDA_ALLREDUCE=internal`, `GGML_HIP_FA_BAND_WMMA=4`,
`ROCP_TOOL_ATTACH=1`. The *Windows* build also lists `0006-qwen4exp-mtp-graph-realloc`, which changes only
`src/models/qwen4exp.cpp`, the Flash-Next architecture, where this model is `qwen35`; no run compares
this model with and without it.

**Preset.** `Swift-1.5-Qwen3.8-27B-Q8_0.gguf` with `Qwen3.8-27B-UD-mmproj-BF16.gguf` (`mmproj-offload
true`, on the x4 card); `device` the x16 card first, `split-mode tensor`, `tensor-split 1,1`; `ctx-size
425984` with `parallel 2`, `kv-unified false`; `cache-type-k/-v f16`, `ubatch-size 1024` (`batch-size`
the default 2048); `spec-type ngram-map-k4v,draft-mtp`, `spec-draft-n-max 2`,
`spec-ngram-map-k4v-size-m 32`, `spec-draft-type-k/-v f16`; `cache-ram 24576`, `cache-reuse 256`,
`ctx-checkpoints 12`; `jinja true`, `reasoning auto`, `reasoning-format auto`, a
`reasoning-budget-message`; `load-mode none`, `fit off`, `backend-sampling true`; `temp 1`, `top-k 20`,
`top-p 0.95`, `min-p 0`, `repeat-penalty 1`, `presence-penalty 0`. Server: `Threads 12`, `ThreadsBatch 12`,
`CudaAllreduce internal`, `ModelsMemoryMargin 1024`.

**What differs on *Windows*.** `ThreadsBatch 24`; `SaveStateOnShutdown` false against true; `device =
ROCm2,ROCm1`, since the integrated GPU is `ROCm0` there: `ROCm2` is the x16 card and `ROCm1` the x4 card, so the x16
card stays first. `RocblasUseHipblaslt` is unset on both. The *Windows* run is `standard-1.1` whole, with the
same prose and image legs as `standard-1.0` and no cold agent-turn leg.

**Prompts.** Technical prose of 43.7k and 115k tokens; a coding-agent turn of 42.5k tokens
(`bench-prompt-code-1.0.json`); writing code (`bench-prompt-code-write-1.1.json`, 42.2k tokens), the
agent's task and files in one message and a request to write `paths.rs` back in full, replying with
reasoning and then ~3.9k tokens of code; one photograph fitted into 1344x896 (1259 tokens).

**Passes.** Every leg restarts llama-server, every cold request has the prompt cache off, output
uncapped. `engine`: temperature 0, speculator off, 3 repetitions, one request. `real`: the preset's
sampling and speculator, 5 repetitions, one request and then the three two-request scenarios of
[section 5](#5-splitting-the-model-across-two-cards). `agent`: described below.

**Free memory** after the first prefill (x16 / x4 card), *Linux*: on text, 4488 to 4588 / 3350 to 3450 MiB under
`engine`, 2338 to 2518 / 1240 to 1380 under `real` and `agent`; on the image, 4552 / 3168 under `engine` and 2478 / 1094
under `real`. The *Windows* run of this spec holds none; one-repetition runs of the same preset do, at 43.7k
([section 4](#4-kv-cache-type-and-context)).

</details>

One request:

| prompt | pass | decode | prefill | accepted |
| --- | --- | --- | --- | --- |
| 43.7k prose | `engine` | 27.1 ±0.1 | 1210.3 ±2.6 | n/a |
| 43.7k prose | `real` | 59.3 ±1.5 | 1125.4 ±1.6 | 79% |
| 115k prose | `engine` | 24.5 ±0.0 | 1025.5 ±2.2 | n/a |
| 115k prose | `real` | 59.2 ±7.0 | 960.1 ±1.2 | 82% |
| agent turn | `engine` | 27.2 ±0.0 | 1214.8 ±2.4 | n/a |
| agent turn | `real` | 52.0 ±4.1 | 1138.2 ±1.7 | 69% |
| writing code | `engine` | 27.1 ±0.0 | 1211.0 ±3.6 | n/a |
| writing code | `real` | 55.9 ±5.1 | 1135.2 ±1.4 | 62% |
| image | `engine` | 28.9 ±0.0 | 612.7 ±67.3 | n/a |
| image | `real` | 50.6 ±0.5 | 613.4 ±33.0 | 63% |

```bench-chart
title    = Decode of one request by prompt: bare engine against the preset's speculator
campaign = swift27b-standard-1.0-2xr9700-linux-20261031, swift27b-standard-1.1-2xr9700-linux-20261101
metric   = decode
x        = prompt
series   = spec-pass
where    = concurrency=single
rename   = bench-prompt-43k-1.0=43.7k prose, bench-prompt-110k-1.0=115k prose, bench-prompt-code-1.0=agent turn, bench-prompt-code-write-1.1=writing code, bench-prompt-image-1.0=image
```

- **The speculator doubles decode**: 2.2x at 43.7k, 2.4x at 115k (it barely drops with depth when it
  drafts), 1.9x on the agent turn and 1.75x on the image, following acceptance, and 2.1x writing code
  despite its 62%. It costs 6 to 7% of cold prefill on text and nothing measurable on the image.
- **Writing code is not faster than prose**: 55.9 t/s against 59.3. The code itself (61.6 ±8.3)
  drafts better than the reasoning before it (51.4 ±2.7), but at `spec-draft-n-max 2` the copied
  parts lift acceptance little (62% against 79%). The reasoning varies from 3.1k to 7.3k tokens, so a
  repetition takes 2.5 to 6 minutes.
- **The agent turn replies in 75 to 170 tokens**, too short for a good decode rate. Writing code and
  the agent step below cover what it misses.
- **Power**, read by hand during the 43.7k `real` legs (sysfs, not recorded): both cards at the 300 W
  cap while prefilling, 210 to 270 W each while decoding, the CPU package ~80 W.

Two requests at once, pass `real`. In `prefill+decode` the decode per request is the warm one's
and the prefill the cold one's:

| prompt | scenario | decode per request | prefill per request | prefill total | decode total | accepted |
| --- | --- | --- | --- | --- | --- | --- |
| 43.7k prose | `prefill` | 38.6 ±11.0 | 797.0 ±257.9 | 1104.7 ±2.4 | 57.8 ±10.1 | 78% |
| 43.7k prose | `decode` | 48.2 ±3.8 | n/a | n/a | 87.3 ±5.7 | 78% |
| 43.7k prose | `prefill+decode` | 25.2 ±4.8 | 1079.5 ±6.3 | 1079.5 ±6.3 | 46.9 ±3.8 | 77% |
| 115k prose | `prefill` | 29.0 ±14.4 | 699.7 ±244.7 | 935.1 ±1.3 | 31.6 ±3.9 | 80% |
| 115k prose | `decode` | 39.6 ±4.2 | n/a | n/a | 69.7 ±3.8 | 80% |
| 115k prose | `prefill+decode` | 15.8 ±5.2 | 908.9 ±1.6 | 908.8 ±1.6 | 27.5 ±4.9 | 82% |
| agent turn | `prefill` | 22.5 ±21.2 | 812.9 ±219.8 | 1208.8 ±1.4 | 5.8 ±1.1 | 58% |
| agent turn | `decode` | 35.9 ±4.1 | n/a | n/a | 67.6 ±7.2 | 59% |
| agent turn | `prefill+decode` | 2.9 ±0.7 | 1090.8 ±1.2 | 1090.8 ±1.2 | 5.9 ±1.0 | 63% |
| writing code | `prefill` | 36.8 ±7.0 | 804.9 ±214.4 | 1203.0 ±4.4 | 64.7 ±4.3 | 62% |
| writing code | `decode` | 41.5 ±5.6 | n/a | n/a | 74.0 ±4.6 | 63% |
| writing code | `prefill+decode` | 36.2 ±4.5 | 1082.5 ±6.8 | 1082.5 ±6.8 | 64.0 ±6.0 | 62% |
| image | `prefill` | 44.9 ±0.8 | 327.5 ±9.0 | 649.6 ±17.5 | 86.8 ±3.6 | 62% |
| image | `decode` | 45.5 ±0.4 | n/a | n/a | 84.1 ±3.6 | 63% |
| image | `prefill+decode` | 44.0 ±0.8 | 567.1 ±6.6 | 566.7 ±6.6 | 83.1 ±4.0 | 62% |

```bench-chart
title    = Decode total by scenario, the preset's sampling and speculator
campaign = swift27b-standard-1.0-2xr9700-linux-20261031, swift27b-standard-1.1-2xr9700-linux-20261101
metric   = decode-total
x        = concurrency
series   = prompt
where    = spec-pass=real
rename   = bench-prompt-43k-1.0=43.7k prose, bench-prompt-110k-1.0=115k prose, bench-prompt-code-1.0=agent turn, bench-prompt-code-write-1.1=writing code, bench-prompt-image-1.0=image
```

- **Two decodes at once** total 1.47x one request at 43.7k, 1.18x at 115k, 1.32x on code and 1.66x on
  the image. The deeper the prose, the less the second slot adds; code at 42.2k adds less than prose at
  43.7k, because each request's lower acceptance leaves less room in the batch.
- **A request decoding beside a prefill almost stops.** Each server batch takes one `batch-size` chunk
  of the cold prompt (2048 tokens, ~1.7 s) and one decode step of the warm slot, so the warm slot runs
  at ~2 t/s until the prefill ends, then at full speed. Its mean depends on how far the reply runs
  past the prefill: 36.2 on code, 25.2 at 43.7k, 15.8 at 115k, 2.9 on the short agent turn. The cold
  prefill loses only 4 to 5% on text and 7.5% on the image.
- **Two cold prefills run one after the other**, so the total equals one request's rate and the
  per-request mean mixes the first-served request with the waiting one. On the agent turn and on code
  the total reads above one request's rate (1208.8 against 1138.2, 1203.0 against 1135.2). *Windows*
  shows the same on code, so this belongs to the prompt, not the system; the cause is not known.

### The agent step, with its prefix cached

The slot is first primed with the conversation up to its last assistant turn (37,043 tokens cached).
The whole turn then goes out with the cache on, so it prefills only the last tool call and its result
(5,457 tokens) and replies with one short step (86 to 236 tokens). The preset's sampling and speculator,
5 repetitions. First token is the server's prefill time of the turn; turn is send to last byte, what
the client waits.

| scenario | first token (s) | turn (s) | prefill of the step | decode of the step | accepted |
| --- | --- | --- | --- | --- | --- |
| one request | 6.3 ±0.3 | 8.8 ±0.7 | 867.9 ±36.7 | 51.5 ±3.3 | 63% |
| `decode`: two steps at once | 9.7 ±2.2 | 16.6 ±3.3 | n/a | 31.1 ±11.2 | 64% |
| `prefill+decode`: beside a 42.5k cold prefill | 10.1 ±0.2 | 47.7 ±1.6 | 1045.2 ±3.4 (the cold one) | 3.7 ±1.4 | 60% |

```bench-chart
title    = The agent step: decode of the step by scenario
campaign = swift27b-standard-1.1-2xr9700-linux-20261101
metric   = decode
x        = concurrency
series   = spec-pass
where    = spec-pass=agent
```

- **A step alone takes ~9 s**, against ~37 s just to prefill the same turn cold: the prefix cache is
  what makes an agent usable at this depth.
- **Two steps at once** take 16.6 s each, since their prefills run one after the other.
- **A step beside another conversation's cold prefill waits for it**: first token after 10.1 s, then
  ~2 t/s until the other prompt is done, so 47.7 s, five times the step alone. This is two agents on
  one server when one of them starts or compacts a conversation. `batch-size` is the lever to try
  ([section 9](#9-not-yet-measured-placeholders)).

## 8. Choices that look better than they measure

| option | result |
| --- | --- |
| `q8_0` KV to fit the native context | fits 262144 but decodes 11% slower at 43.7k ([section 4](#4-kv-cache-type-and-context)) |
| `split-mode tensor` across two backends | aborts at load unless AllReduce is `none`; with it, half the prefill of the layer split, no decode gain (v0.6.0) |
| the x4 card first on two R9700s, copying "the R9700 first" of the mixed pair | *Windows*, tensor split: prefill unchanged, 1034.3 against 1029.0; decode an open lead ([section 9](#9-not-yet-measured-placeholders)) |
| Memory Integrity off, for the *Windows* prefill gap on two R9700s | prefill 2 to 3% faster, decode unchanged within noise: a small part of the gap at most, and the setting stays on ([section 6](#6-linux-against-windows)) |
| `ThreadsBatch 12` against `24` on two R9700s on *Windows* | no change: prefill 1034.3 at `24`, 1031.7 at `12` ([section 6](#6-linux-against-windows)) |
| the Ultimate Performance plan, PCIe ASPM off, on two R9700s on *Windows* | no change: prefill 1034.3 against 1031.5; the minimum processor state stays 0, so no test of a pinned CPU frequency ([section 6](#6-linux-against-windows)) |
| Vulkan on the R9700 instead of ROCm | *Windows*: prefill 25 to 27% and decode 15 to 18% slower |
| a low `spec-draft-n-max` with MTP alone | *Windows*: `1` gives 9.8 t/s at 93.6k, against 13.0 with no drafter; `2` gives 21 to 25% less than `4` |
| `q8_0` draft KV | larger, not smaller (v0.6.0) |
| the whole sampling chain on the GPU | no decode change at 115k: 40.7 to 40.8 ms per token against 40.6 to 41.5 with `top-p` and `dist` on the CPU, the same text (mixed pair, a patched v0.7.0, not recorded). After `top-k 20` the CPU handles only 20 candidates, and a tool-call grammar disables backend sampling anyway. On ROCm the bundled build keeps them on the CPU and logs `does not have support for op ARGSORT` |

## 9. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `spec-draft-n-max` at 100k+ tokens | `2` | does the value chosen at 43.7k and on code hold deep in the context? | TBD |
| `q8_0` KV at 115k+ tokens | `f16` | does the decode gap over `f16` keep growing? | partly: `f16` is measured at 115k, `q8_0` not past 43.7k |
| `batch-size` on two R9700s | default (2048) | how much do `1024` or `512` give back to a request decoding beside a prefill, and what do they cost the cold prefill? | TBD |
| `split-mode` on two R9700s on *Windows* | `tensor` | what do the tensor split's decode gain and prefill cost come to against `layer` there? | TBD |
| the prefill gap to *Linux* on two R9700s on *Windows* | n/a | what causes it: the driver and HIP runtime, transfers and submissions under `split-mode tensor`, the hypervisor? | partly: Memory Integrity gives 2 to 3%, `ThreadsBatch` and PCIe ASPM nothing ([section 8](#8-choices-that-look-better-than-they-measure)); the candidates are unmeasured |
| `device` on two R9700s on *Windows* | the x16 card first | does the x4 card first really decode faster under `real`? | partly: prefill unchanged; decode 63.0 against 59.4, one repetition, above the standard run's 56.1 to 60.2, acceptance 79% in both ([section 6](#6-linux-against-windows)) |
| decode on two R9700s on *Windows* | n/a | why does `engine` run 8 to 10% faster than *Linux*, and code under concurrency slower? | TBD |
| free memory after a prefill on *Windows*, two R9700s | n/a | a recorded figure per card, comparable with the *Linux* baseline | partly: recorded per adapter at 43.7k, one request ([section 4](#4-kv-cache-type-and-context)); 115k only by hand, the other prompts and two requests not recorded |
| `ctx-checkpoints` | `12` | tokens reprocessed at a real client compaction (expected under 8192) | TBD |
| `reasoning-preserve` | default | what a divergent prompt costs on the 598.5 MiB recurrent state | TBD |
| a pinned staging buffer in ggml's cross-runtime copy | `malloc` staging | what it gives back of the staged hops with the R9700 second on *Windows*: 2.5 s of the 5.24 s of blocking copies, the rest being the buffers of [section 6](#6-linux-against-windows) | TBD: needs a ggml change first |
| the R9700 first on *Windows* at `53,13` | `52,14` | does `53,13` buy margin on the RTX 4070 SUPER for free? | TBD: needs the RTX 4070 SUPER in the test machine |
| the settings on the bundled llama.cpp v1.0.0 | as in [section 2](#2-recommended-settings) | how far do these figures move without the band and the other patches they ran with? | TBD; the mixed pair needs the RTX 4070 SUPER in the test machine |
| the HIP API log on *Linux*, R9700 second | n/a | is the *Linux* lead the copy rate alone, or the kernels too? | TBD: needs the RTX 4070 SUPER in the test machine |
