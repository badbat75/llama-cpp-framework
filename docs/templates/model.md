---
title: <model>
type: model
status: draft
gpus: [<bench.db gpu.name of each card it was measured on>]
models: [<bench.db model.name prefix>]
platforms: [linux]
llama-cpp: <vX.Y.Z>
updated: <yyyy-mm-dd>
---

# <model>: configuration reference (draft)

Reference settings for <model> (architecture `<arch>`, <dense or MoE, MTP head>) on this
framework, measured with llama.cpp <vX.Y.Z> on <cards> (<host: cores, RAM, OS>). The method
of each campaign (prompt, temperature, repetitions, output cap, or its spec) is in the row
or the section that cites it. Per-card detail lives in the card guides.

## 1. Model facts

| item | value | source |
| --- | --- | --- |
| <blocks, attention, context, KV per token, sampling defaults> | <value> | <the GGUF key or the log line> |

## 2. Recommended settings

| key | value | evidence |
| --- | --- | --- |
| `<preset key>` | <the measured optimum> | <numbers against the alternatives> (campaign `<name>`) |

## 3. Choices that look better than they measure

| option | result |
| --- | --- |
| <option> | <what it measured> (campaign `<name>`) |

## 4. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `<key>` | <value> | <what to measure> | TBD |
