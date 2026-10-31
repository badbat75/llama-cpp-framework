---
title: Documentation
type: reference
updated: 2026-10-31
---

# Documentation

Configuration references measured on this framework, and the developer references. Every
measured value names the campaign in `benchmarks/bench.db` it comes from; how pages are
written is in [AGENTS.md](AGENTS.md).

## Cards (`type: gpu`)

| page | covers | measured with |
| --- | --- | --- |
| [AMD Radeon AI PRO R9700](amd_ai_pro_r9700.md) | RDNA4, 32 GB, ROCm (HIP); Windows and Linux | llama.cpp v0.5.0 to v0.7.0 |
| [NVIDIA GeForce RTX 4070 SUPER](nvidia_rtx4070_super.md) | Ada, 12 GB, CUDA; Windows and Linux | llama.cpp v0.5.0 to v0.7.0 |

## Models (`type: model`)

| page | covers | measured with |
| --- | --- | --- |
| [Qwen3.8-27B](qwen3.8_27b.md) | dense, embedded MTP head; the R9700 with the RTX 4070 SUPER, two R9700s (layer and tensor split, two requests at once) | llama.cpp v0.5.0 to v0.8.0 |
| [Qwen3.8-Flash-Next](qwen3.8_flash_next.md) | MoE (`qwen4exp`), experts in RAM, MoE cache, drafters; the same machines | llama.cpp v0.6.0 to v0.8.0 |

## References (`type: reference`)

| page | covers |
| --- | --- |
| [Build pipeline](build-pipeline.md) | the `xtask` phases, the bootstrap scripts, the configure line, sccache, LTO, HIP targets |
| [Benchmarks](../benchmarks/README.md) | bench.db, the versioned method (`standard-<X.Y>`), the shipped prompts, how to query |

New pages start from [templates/](templates/) (`gpu.md`, `model.md`).
