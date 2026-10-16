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
| `split-mode tensor` with a card of the other backend | aborts at load unless Tensor-split AllReduce is `none`; with `none` it loses (section 4) | v0.6.0 |

## 2. Recommended settings (two cards)

| key | value | evidence |
| --- | --- | --- |
| quant | Q8_0 | on the R9700 (*Windows*) the fastest of the 27B files measured, prefill and decode; Q5_K_M -13%/-10% prefill and -7%/-15% decode, Q6_K mixes slower still |
| `cache-type-k/-v` | `f16`, at the largest context that fits: `172032` | *Linux*: decode at 43.7k tokens 35.2 and 34.5 t/s against 31.5 and 31.5 in `q8_0` at 262144 (+11%), prefill 1161 and 1162 against 1158 and 1169, three prompts 37.7 against 38.9 (36.4/33.4/43.4 against 37.3/34.9/44.4, the same draft acceptance). `q8_0` halves the KV but the compute buffers grow with it: 1712 and 1736 MiB on the two cards at 262144, against 456 and 604 in `f16` at 163840, so `f16` reaches 172032, not 131072 (section 3) |
| `ctx-size` | `172032` with `f16` | 180224 and 196608 do not load (`cudaMalloc failed: out of memory`); 172032 leaves 281 MB free on the R9700 and 829 MiB on the RTX 4070 SUPER after a 2.9k-token prompt, prefill 1165 and decode 35.2 at 43.7k |
| `device` | the R9700 last | the last device takes the MTP head, the output head and the sampling: *Linux*, v0.6.0, decode +7% at 2.5k tokens and +4.5% at 43.6k against the RTX 4070 SUPER last, prefill -3% (the sign is reversed on *Windows*) |
| `tensor-split` | most layers on the R9700, the RTX 4070 SUPER keeping ~800 MiB free | the split that fits 172032 `f16` with `blk\.15\.ffn_(up|down)` pinned to the R9700 |
| `override-tensor` | `token_embd\.weight` on the RTX 4070 SUPER | keeps the embedding table (~1.26 GiB) out of pinned host RAM: +1-2% decode (*Windows*) |
| `split-mode` | `layer` | `tensor` with `none` AllReduce: prefill 599 against 1290 t/s, decode 17.6 against 17.8 without a drafter, 44.7 against 45.0 with MTP (v0.6.0, ctx 16384) |
| `ubatch-size` | `1024` | fits at ctx 262144 in `q8_0` (v0.6.0); against `512`, +5% prefill at 2.6k and 93.6k on the R9700 (*Windows*); decode unaffected |
| `spec-type` | `draft-mtp` | the head is in the GGUF. *Windows*, `spec-draft-n-max 4`: 50.6 t/s at 2.6k tokens; *Linux*, v0.6.0, unassisted (llama-bench `tg2048`): 17.9 |
| `spec-draft-type-k/-v` | `f16` | *Linux*, v0.6.0: `q8_0` halves the draft KV (544 MiB at 262144) but raises the draft compute buffer from 648 to 1696 MiB |
| `load-mode` | `none` | every weight is on a GPU: nothing to map |
| `mmproj-device` | the R9700 | the image encoder (~1.14 GiB with its compute buffer) only computes on image requests; off the smaller card it frees room for two blocks (*Windows*: +2.4% decode at 93.6k) |

## 3. KV cache type and context (*Linux*, v0.7.0)

Both cards, the R9700 last, `ubatch-size 1024`, `draft-mtp` with an `f16` draft KV:

| KV, context | free after a 2.9k prompt (R9700 / RTX 4070 SUPER) | prefill t/s at 43.7k | decode at 43.7k | decode, three prompts |
| --- | --- | --- | --- | --- |
| `q8_0`, 262144 | 196 MB / 223 MiB | 1158, 1169 | 31.5, 31.5 | 38.9 |
| `f16`, 163840 | 878 MB / 989 MiB | 1161, 1162 | 35.2, 34.5 | 37.7 |
| **`f16`, 172032** | 281 MB / 829 MiB | 1165 | **35.2** | n/a |
| `f16`, 180224 and 196608 | do not load | | | |

The gap grows with the tokens held, as an attention that reads a quantized KV would,
and does not show on short prompts. The same holds on Qwen3.8-Flash-Next
(`qwen3.8_flash_next.md`, section 9). For conversations that grow turn by turn, decode
deep in the context is what every turn pays; `q8_0` stays the choice only where a context
above 172032 is needed.

## 4. Choices that look better than they measure

| option | result |
| --- | --- |
| `q8_0` KV to fit the native context | fits 262144, but costs 11% of decode at 43.7k tokens (section 3) |
| `split-mode tensor` across the two backends | half the prefill of the layer split, no decode gain (section 2) |
| Vulkan on the R9700 instead of ROCm | *Windows*, Q8_0: prefill -27% / -25% and decode -18% / -15% at 2.6k / 93.6k |
| a low `spec-draft-n-max`, to stay on the FlashAttention vector kernel | *Windows*: `n-max 1` gives 9.8 t/s at 93.6k, against 13.0 with no drafter; `2` costs 21-25% against `4` |
| `q8_0` draft KV | larger, not smaller, on v0.6.0 and later (section 2) |

## 5. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `spec-draft-n-max` on *Linux*, v0.7.0 | default | does `4` (`3` at long context) still win with the R9700 last? | TBD |
| decode at 87k+ tokens in `f16` | n/a | does the gap over `q8_0` keep growing with the context? | TBD |
| the IQ3_S file with its MTP head | `q8_0`, 393216, `parallel 2` | the same `f16` question at a context it can afford | TBD |
| `reasoning-preserve`, `ctx-checkpoints` | default | the hybrid model reprocesses from a checkpoint on a divergent prompt: the cost on this model's 598.5 MiB recurrent state | TBD |
