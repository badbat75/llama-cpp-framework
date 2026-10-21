# Qwen3.8-27B: configuration reference (draft)

Reference settings for Qwen3.8-27B (architecture `qwen35`, dense, with an embedded MTP
head) on this framework. The *Linux* rows were measured with llama.cpp v0.7.0 on the
AMD Radeon AI PRO R9700 (32 GB, ROCm backend) and the RTX 4070 SUPER (12 GB, CUDA
backend) in ONE model, layer split, a 12-core AVX-512 host and 64 GB of RAM (kernel 7.0,
TheRock 10.1.0, CUDA 13.4.2). The file is the Q8_0 with its MTP head, the speculator
`draft-mtp`, `Threads` = `ThreadsBatch` = 12. Rows marked v0.6.0 or *Windows* come from
the earlier campaigns of the two card guides (`amd_ai_pro_r9700.md`,
`nvidia_rtx4070_super.md`), which hold the per-card detail.

Measurements: a 43.7k-token prompt with the prompt cache off (two runs, 128 tokens out:
prefill, then decode right after it, deep in the context), and three different short
prompts after a warm-up (code, prose, math; 400 tokens, temperature 0). The MTP head
changes the greedy path, so read the per-run figures beside the mean.

## 1. Model facts

| item | value | source |
| --- | --- | --- |
| blocks | 64 plus the MTP block (65 in the GGUF) | `block_count = 65`, `nextn_predict_layers = 1`; `n_layer = 64` |
| attention | on every 4th block: 16 layers hold a KV cache; 24 query heads, 4 KV heads, head size 256 | `full_attention_interval = 4`; `llama_kv_cache: 16 layers` |
| Memory row (Model info) | hybrid (attention + recurrent) | `llm_arch_is_hybrid` |
| native context | 262144 | `context_length` |
| GGUF sampling defaults | temp 1.0, top-k 20, top-p 0.95 | `general.sampling.*` |
| file (Q8_0) | 27.04 GiB, 27.32 B parameters | `file size`, `model params` |
| KV cache per token | 64 KiB in `f16`, 34 KiB in `q8_0` (16 layers) | `KV buffer size`: 8704 MiB at 262144 cells in `q8_0`, 10240 at 163840 in `f16` |
| recurrent state at one slot | 598.50 MiB (with the MTP head's sequences) | `llama_memory_recurrent: size` |
| MTP draft context | its own KV over the full context, `f16`: 672 MiB at 172032 cells, 1024 at 262144 | `llama_kv_cache: ... 1 layers` |
| prefill cost by position | v0.6.0, 234k-token prompt: 0.95 ms plus 0.0287 ms per 1k of context per token, 984 t/s at 4k against 142 t/s at 218k | marginal cost per token over one long prompt |
| saved state (`SaveStateOnShutdown`) | ~36 KiB per token held in `q8_0` (4.38 GiB for 127,579 tokens, v0.6.0) | the snapshot file |
| context checkpoint | 149.6 MiB each, in host RAM; one at every user-message start and one at the end of each prompt; at the cap the server first drops those within `checkpoint-min-step` of an earlier one, then the oldest | `slot create_check` log lines; `tools/server/server-context.cpp` (v0.7.0) |
| MTP head in the prefill | the draft head processes the prompt too: 1065 t/s without a speculator against 882 with `draft-mtp` (*Linux*, v0.7.0, 115k-token prompt, same binary); it is paid back in decode | `prompt eval time` of the same request with `--spec-type none` |
| `draft-mtp` in greedy decoding | not lossless on the stock HIP FlashAttention tile kernel: at 115k tokens, temperature 0, the output diverges from unassisted decoding after ~100 tokens; with the FlashAttention band (section 2) the two are identical over 278 tokens, as are band and stock without a drafter. Not width-pure either, band on: at 43.7k tokens, 300 greedy tokens, `spec-draft-n-max` 2, 4 and no drafter give three different texts (first flip at token ~120 and ~250), each one byte-identical when its run is repeated; the device placement puts 16 layers on the RTX 4070 SUPER, whose MMA kernel takes a different column count per verify width (1, 4, 8 queries), which the band does not cover | four hand-launched runs at 115k, five at 43.7k, same prompt and seed (*Linux*, v0.7.0) |
| `split-mode tensor` with a card of the other backend | aborts at load unless Tensor-split AllReduce is `none`; with `none` it loses (section 5) | v0.6.0 |

## 2. Recommended settings (two cards)

| key | value | evidence |
| --- | --- | --- |
| quant | Q8_0 | on the R9700 (*Windows*) the fastest of the 27B files measured, prefill and decode; Q5_K_M -13%/-10% prefill and -7%/-15% decode, Q6_K mixes slower still |
| `cache-type-k/-v` | `f16`, at the largest context that fits: `172032` | *Linux*: decode at 43.7k tokens 35.2 and 34.5 t/s against 31.5 and 31.5 in `q8_0` at 262144 (+11%), prefill 1161 and 1162 against 1158 and 1169, three prompts 37.7 against 38.9 (36.4/33.4/43.4 against 37.3/34.9/44.4, the same draft acceptance). `q8_0` halves the KV but the compute buffers grow with it: 1712 and 1736 MiB on the two cards at 262144, against 456 and 604 in `f16` at 163840, so `f16` reaches 172032, not 131072 (section 3) (campaign `27b-kv-f16-20261016`) |
| `ctx-size` | `180224` with `f16`, once the embedding table and the image encoder are off the cards' critical paths (the `override-tensor` and `mmproj-device` rows) | with the embedding table on the RTX 4070 SUPER, 180224 and 196608 did not load (`cudaMalloc failed: out of memory`) and 172032 left 281 MB free on the R9700. With the table in host RAM and the encoder on the RTX 4070 SUPER: free after a 115k prefill (R9700 / RTX 4070 SUPER) 1232 / 1274 MiB at 172032, 638 / 1026 at 180224, 339 / 986 at 188416, prefill 902, 889 and 893 t/s, decode the same within the draft's noise. 188416 runs but is under the 500 MB margin this card needs for its pools. At 180224 a 3840x2160 image appended to a 115k conversation (119,406 prompt tokens) succeeds and leaves 801 / 716 MiB (campaign `coder390-ctx-20261020`) |
| `device` | the R9700 last on *Linux*, FIRST on *Windows* (`52,14`, section 4) | the last device takes the MTP head, the output head and the sampling: *Linux*, v0.6.0, decode +7% at 2.5k tokens and +4.5% at 43.6k against the RTX 4070 SUPER last, prefill -3% (the sign is reversed on *Windows*: v0.7.0, +9% prefill and +21% decode with the R9700 first, section 4). Repeated on v0.7.0 with the Coder390 Q8_0 MTP file at 172032 `f16`: at 43.7k decode 31.2 t/s (twice) against 31.3 with the RTX 4070 SUPER last at split 52,14, which leaves it 28 MiB free, and 29.8 at 53,13 (430 MiB free); prefill 1133 and 1123 against 1069 and 1071; short prompt 38.1 against 36.5 and 36.6. With the RTX 4070 SUPER last, 50,16 and 48,18 do not load (its compute buffer finds no room) (campaign `coder390-device-order-20261019`) |
| `tensor-split` | most layers on the R9700, the RTX 4070 SUPER keeping ~800 MiB free | the split that fits 172032 `f16` with `blk\.15\.ffn_(up|down)` pinned to the R9700. Attention sits on blocks 3, 7, 11, 15, ... (`(i + 1) % 4 == 0`, `models/qwen35.cpp`), each with 672 MiB of KV at 172032 `f16`, so the boundary block of a split moves its KV slice with it: moving block 15 to the R9700 costs 1061 MiB there, a linear block 389 |
| `override-tensor` | no `token_embd` rule: the embedding table (1288 MiB) stays in host RAM, llama.cpp's default | *Linux*, v0.7.0: with the table on the RTX 4070 SUPER against in host RAM, decode 31.2 against 31.2 t/s at 43.7k and 29.3 against 29.3 at 115k, prefill 1178 against 1164 and 897 against 899 (campaign `coder390-placement-20261020`, runs 22 and 28, 25 and 29). The *Windows* +1-2% of v0.5.0 does not repeat. The 1288 MiB freed on the card is what lets the image encoder move there (`mmproj-device`) |
| `override-tensor`, output head | `output.weight` left on the last device | `^output\.weight$` on the RTX 4070 SUPER (its GEMV would be faster there) costs prefill 1026 against 1139 and 779 against 890 t/s, decode 29.9 against 29.7 and 28.2 against 29.2 at 43.7k and 115k: the MTP draft on the other card hops devices for every drafted token (campaign `coder390-placement-20261020`) |
| `split-mode` | `layer` | `tensor` with `none` AllReduce: prefill 599 against 1290 t/s, decode 17.6 against 17.8 without a drafter, 44.7 against 45.0 with MTP (v0.6.0, ctx 16384) |
| `ubatch-size` | `1024` | fits at ctx 262144 in `q8_0` (v0.6.0); against `512`, +5% prefill at 2.6k and 93.6k on the R9700 (*Windows*); decode unaffected. *Linux*, v0.7.0, 115k-token prompt: prefill 895 t/s at `768`, 902 at `1024`, 837 at `1536`, 809 at `2048`; decode 28.5, 29.3, 29.3, 29.0 (draft acceptance 54 to 57%). Above `1024` the compute buffers also eat the margin (R9700 free after the prefill 1232, 659 and 75 MiB at `1024`, `1536`, `2048`) (campaign `coder390-ubatch-20261020`) |
| `spec-type` | `draft-mtp` | the head is in the GGUF. *Windows*, `spec-draft-n-max 4`: 50.6 t/s at 2.6k tokens; *Linux*, v0.6.0, unassisted (llama-bench `tg2048`): 17.9 |
| `spec-draft-type-k/-v` | `f16` | *Linux*, v0.6.0: `q8_0` halves the draft KV (544 MiB at 262144) but raises the draft compute buffer from 648 to 1696 MiB |
| `load-mode` | `none` | every weight is on a GPU: nothing to map |
| `mmproj-device` | the RTX 4070 SUPER, once the embedding table is off it | the image encoder (600 MiB of weights; its compute buffer, ~418 MiB for a 3840x2160 image, is allocated at the first image) only computes on image requests. *Linux*, v0.7.0, 172032 `f16`: on the R9700, with 342 MiB free after a 115k prefill, a 3840x2160 image aborts the server (`ROCm error: out of memory` in `clip_encode`); on the RTX 4070 SUPER the same request succeeds (612 t/s image prefill) and leaves 856 MiB free there and 1239 on the R9700. Text speed identical to the R9700 placement (campaign `coder390-placement-20261020`, runs 28 and 29). The *Windows* +2.4% of v0.5.0 came from the two blocks the encoder displaced on the smaller card |
| `ctx-checkpoints` | `12` (`checkpoint-min-step` at its default 8192) | the client compacts a long conversation into a shorter prompt (observed: a 154.9k-token turn followed by a 95.8k one); the server resumes from the newest checkpoint under the common prefix, or reprocesses the whole prompt. With `4` no checkpoint survived under 95.1k ("forcing full prompt re-processing": 95,814 tokens in 101.7 s). With the eviction rule of section 1, 12 checkpoints spaced 8192 apart cover ~98k tokens of history, so the reprocessing is bounded by 8192 tokens; cost 1.8 GiB of host RAM, no VRAM (*Linux*, v0.7.0; first compaction with `12` still to be observed, section 6) |
| `HipFaBand` (Server tab, SDK Tuning) | default (`4`) | the framework's FlashAttention band on RDNA4 (`patches\llama.cpp\0003`, `amd_ai_pro_r9700.md` section 3), covering the attention layers on the R9700: *Linux*, v0.7.0, 115k-token prompt, decode 25.9 against 24.5 t/s with `draft-mtp` at temperature 1.0 and 14.4 against 13.9 unassisted at temperature 0; prefill unchanged |

## 3. KV cache type and context (*Linux*, v0.7.0)

Both cards, the R9700 last, `ubatch-size 1024`, `draft-mtp` with an `f16` draft KV (campaign
`27b-kv-f16-20261016`):

| KV, context | free after a 2.9k prompt (R9700 / RTX 4070 SUPER) | prefill t/s at 43.7k | decode at 43.7k | decode, three prompts |
| --- | --- | --- | --- | --- |
| `q8_0`, 262144 | 196 MB / 223 MiB | 1158, 1169 | 31.5, 31.5 | 38.9 |
| `f16`, 163840 | 878 MB / 989 MiB | 1161, 1162 | 35.2, 34.5 | 37.7 |
| `f16`, 172032 | 281 MB / 829 MiB | 1165 | 35.2 | n/a |
| `f16`, 180224 and 196608 | do not load with the embedding table on the RTX 4070 SUPER | | | |
| **`f16`, 180224**, table in host RAM, encoder on the RTX 4070 SUPER | 638 / 1026 MiB after a 115k prefill | 889 at 115k | 28.5 at 115k | n/a |
| `f16`, 188416, same placement | 339 / 986 MiB after a 115k prefill: runs, under the margin | 893 at 115k | 29.3 at 115k | n/a |

The last two rows are campaign `coder390-ctx-20261020`, the others `27b-kv-f16-20261016`.
The gap grows with the tokens held, as an attention that reads a quantized KV would,
and does not show on short prompts. The same holds on Qwen3.8-Flash-Next
(`qwen3.8_flash_next.md`, section 9). For conversations that grow turn by turn, decode
deep in the context is what every turn pays; `q8_0` stays the choice only where a context
above 172032 is needed.

## 4. Linux against Windows (v0.7.0)

The same machine and preset on both systems: the Coder390 Q8_0 MTP file, 172032 `f16`,
the R9700 last, `ubatch-size 1024`, the same llama.cpp build and prompts, three
repetitions of 256 tokens at temperature 0 (campaigns `coder390-device-order-20261019`
for *Linux*, `coder390-windows-vs-linux-20261020` for *Windows*):

| prompt | prefill t/s *Linux* / *Windows* | decode t/s *Linux* / *Windows* | draft acceptance |
| --- | --- | --- | --- |
| 43.7k tokens | 1133, 1123 / 1030 (-9%) | 31.2, 31.2 / 25.3 (-19%) | 53% / 51% |
| 100 tokens | 225, 226 / 209 (-7%) | 38.1, 38.1 / 31.0 (-19%) | 61% / 59% |

Weights and KV per card are the same to the MiB and the draft acceptance is within two
points, so the gap is the time of each step (39.6 against ~32 ms per token at 43.7k), not
the placement or the drafter. `Threads` was 18 on *Windows* against 12, inert with every
layer on a GPU. *Windows* lists the CPU's integrated GPU as a ROCm device too, ahead of
the R9700, so a preset copied from *Linux* names the wrong card until its device ids are
renumbered.

The same comparison on the section 2 placement (`blk\.15\.ffn_(up|down)` on the R9700,
embedding table in host RAM, the same v0.7.0 build with its three patches, `Threads` 12 on
both; campaigns `coder390-placement-20261020` runs 28 and 29 for *Linux*,
`coder390-placement-windows-20261021` for *Windows*). Free memory after the 115k prefill
is the same to within 200 MiB on each card:

| prompt | prefill t/s *Linux* / *Windows* | decode t/s *Linux* / *Windows* | draft acceptance |
| --- | --- | --- | --- |
| 43.7k tokens | 1182, 1156, 1154 / 1026, 1017, 1016 (-12%) | 31.2 / 26.7, 27.0, 27.0 (-14%) | 51% / 54% |
| 115k tokens | 902, 895, 900 / 753, 745, 743 (-17%) | 29.3 / 24.9, 25.0, 25.3 (-14%) | 57% / 60% |

Where a *Windows* prefill step goes (one 43.7k prefill of that preset, the CUDA side under
Nsight Systems, the HIP side through its API log, `AMD_LOG_LEVEL=3`, since the Windows
TheRock dist ships no rocprofiler; the GPU-engine counters for each card's busy share).
A 1024-token ubatch takes ~1.04 s:

| share | what |
| --- | --- |
| ~65% | the R9700's 50 layers: one `hipStreamSynchronize` of ~0.65 s per ubatch |
| ~12% | host-to-device copies INTO the R9700, ~200 MiB per ubatch at ~2 GB/s: the block-15 activations crossing from the other card (the 68 MiB gate output and three 20 MiB hidden states) and the attention mask of the full-attention layers (85 MiB at 43.7k tokens, growing with the context). The card sits on the chipset link (3.2 GB/s pinned, `tools\pcie-bench`), and llama's input buffers are pinned for the first device only, so the R9700 copies them as pageable memory |
| ~19% | the RTX 4070 SUPER's 16 layers (its kernel time: `mul_mat_q` 58%, flash attention 20%, `silu` 4.5%) |
| ~2% | copies out of the 4070 (~110 MiB per ubatch at ~7.7 GB/s) and launch overhead |

The two cards never overlap: the R9700's compute engine reads 60-94% busy (~73% on
average) and idles while the 4070 computes and while the copies run. The *Linux* lead at
43.7k (5.8 s over the prefill) is of the order of those copies (5.2 s), which the Linux
HIP runtime stages faster; the kernels themselves were not compared.

What the profile suggested, measured the same morning at 43.7k tokens, three repetitions,
against the R9700-last placement re-measured minutes before (1041, 1029, 1018 prefill,
27.0-27.1 decode):

| change | prefill t/s | decode t/s | free after the prefill (R9700 / 4070) | campaign |
| --- | --- | --- | --- | --- |
| `blk\.15\.ffn_gate` on the R9700 too (no 68 MiB gate output crossing) | 1074, 1068, 1065 (+3.8%) | 27.1-27.5 (same) | 1222 / 1551 MiB | `coder390-gate-windows-20261021` |
| the R9700 FIRST: `device` R9700, 4070; `tensor-split 52,14` (the 4070 takes the output head and the sampling; the R9700 gets llama's pinned input buffers) | 1129, 1119, 1118 (+9%) | 32.6, 32.9, 33.0 (+21%) | 2146 / 687 MiB | `coder390-device-order-windows-20261021` |

On *Windows* the reversed order is therefore the setting (section 2): decode passes the
*Linux* R9700-last figure (31.2) and prefill comes within 3% of it. On *Linux* the same
swap lost (the `device` row) and the gate move changed nothing. At 115k tokens, `180224`
(the same campaign): prefill 850, 844, 843 t/s (+13% over the R9700-last 753, 745, 743;
*Linux* 902, 895, 900) and decode 31.0, 30.7, 31.0 (+23% over 24.9-25.3; *Linux* 29.3),
with 527 MiB free on the 4070 and 1714 on the R9700 after the prefill. The prefill gap to
*Linux* widens with the context (3% at 43.7k, 6% at 115k): the copies the swap does not
remove, the attention mask above all, grow with it. Not measured with the R9700 first:
the gate rule (moot: block 15 sits on the R9700 either way) and a `53,13` split for more
margin on the 4070.

Why *Linux* never showed the copy cost, tested with a llama.cpp change
(`patches\llama.cpp\0004`, 2026-10-21): llama allocates the scheduler's host compute
buffers (the graph inputs: token ids, positions, the attention masks) from the FIRST
device's pinned memory, so the second device's runtime copies them as pageable memory;
the patch registers those buffers (416 MiB for the main context, 376 for the draft's)
with every GPU runtime. One 43.7k prefill per placement on the patched build, the profile's
prompt, a single request (the HIP API log costs ~1% of prefill):

| placement | prefill t/s unpatched | patched | blocking HIP copies in the prefill |
| --- | --- | --- | --- |
| the R9700 second (`16,50`) | 1029 | 1036 with the HIP log, 1058 without | 5.24 s to 2.50 s: the masks now stream asynchronously; what remains is the cross-card hops llama stages through `malloc` (pageable on both sides): the 68 MiB block-15 gate output at 33 ms once per ubatch (the gate lever above removes exactly this) and 20 MiB hidden states at ~10 ms |
| the R9700 first (`52,14`) | 1105 | 1113 | none to begin with: the 4070's pageable copies cost 0.3 s in all (the CUDA runtime stages them at ~22 GB/s) |

So the *Windows* loss is the HIP runtime's pageable path (~2 GB/s, blocking the thread)
and nothing else; the Linux runtime pins pageable memory on the fly. The patch buys ~2%
with the R9700 second and nothing measurable with it first, the setting of section 2; a
placement that keeps the R9700 second still pays the staged hops, a ggml change (section
6).

## 5. Choices that look better than they measure

| option | result |
| --- | --- |
| `q8_0` KV to fit the native context | fits 262144, but costs 11% of decode at 43.7k tokens (section 3) |
| `split-mode tensor` across the two backends | half the prefill of the layer split, no decode gain (section 2) |
| Vulkan on the R9700 instead of ROCm | *Windows*, Q8_0: prefill -27% / -25% and decode -18% / -15% at 2.6k / 93.6k |
| a low `spec-draft-n-max`, to stay on the FlashAttention vector kernel | *Windows*: `n-max 1` gives 9.8 t/s at 93.6k, against 13.0 with no drafter; `2` costs 21-25% against `4` |
| `q8_0` draft KV | larger, not smaller, on v0.6.0 and later (section 2) |
| moving the whole sampling chain onto the GPU (`top-p` 1.0, or the probe patch `patches\llama.cpp\0002`) | no measurable decode change at 115k tokens (40.7 against 40.6 to 41.5 ms per token): after `top-k 20` the CPU side of the chain handles 20 candidates. And a request with a tool-call grammar disables backend sampling altogether (`common_sampler_init: backend sampling is not compatible with grammar`), at ~1 ms per token of CPU work (*Linux*, v0.7.0) |

## 6. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `spec-draft-n-max` on *Linux*, v0.7.0 | default | does `4` (`3` at long context) still win with the R9700 last? | TBD |
| decode at 87k+ tokens in `f16` | n/a | does the gap over `q8_0` keep growing with the context? | TBD |
| the IQ3_S file with its MTP head | `q8_0`, 393216, `parallel 2` | the same `f16` question at a context it can afford | TBD |
| `ctx-checkpoints` `12` | `12` | tokens reprocessed at the first client compaction (expected under 8192, section 2) | TBD |
| `reasoning-preserve` | default | the hybrid model reprocesses from a checkpoint on a divergent prompt: the cost on this model's 598.5 MiB recurrent state | TBD |
| `spec-draft-n-max` 2 to 5 at 100k+ tokens | `3` | the draft phase is ~20% of a decode step and acceptance moves decode by 20-30% between tasks (0.49 to 0.65 observed) | TBD |
| the R9700 first on *Windows* at `53,13` | `52,14` (section 4) | does `53,13` buy margin on the 4070 (527 MiB free after a 115k prefill at `180224`) for free? | TBD |
| the same HIP API log on *Linux* | n/a | is the Linux lead on the R9700-last placement the copy rate alone (section 4), or the kernels too? | TBD |
| a pinned staging buffer in ggml's cross-runtime tensor copy | `malloc` staging (section 4) | with the R9700 second on *Windows*, the 2.5 s of staged hops left after patch 0004 (~5% of the prefill); with it first, the 0.3 s copies out of the R9700 | TBD |
