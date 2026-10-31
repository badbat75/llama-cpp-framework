---
title: <vendor> <card>
type: gpu
status: draft
gpus: [<bench.db gpu.name>]
models: []
platforms: [linux]
llama-cpp: <vX.Y.Z>
updated: <yyyy-mm-dd>
---

# <vendor> <card>: configuration reference

Reference settings for running llama.cpp on the <card> (<architecture>, `<arch id>`, <VRAM>)
under <OS>, through the <backend> backend. Values were measured on this framework with
llama.cpp <vX.Y.Z>; the method of each campaign (prompt, temperature, repetitions, output
cap, or its spec) is in the row or the section that cites it.

## 1. Runtime prerequisites

| item | requirement | why |
| --- | --- | --- |
| <runtime, driver, env var> | <what it must be> | <what breaks otherwise, measured> |

## 2. Device facts

- **<fact>.** <what it means for a configuration, and how it was found>.

## 3. Settings

| key | value | evidence |
| --- | --- | --- |
| `<preset or server key>` | <the measured optimum on this card> | <numbers against the alternatives> (campaign `<name>`) |

## 4. Choices that look better than they measure

| option | result |
| --- | --- |
| <option> | <what it measured> (campaign `<name>`) |

## 5. Checking a configuration

<how to read memory and speed on this card after a real prefill: commands, margins>

## 6. Not yet measured (placeholders)

| parameter | current value | question | result |
| --- | --- | --- | --- |
| `<key>` | <value> | <what to measure> | TBD |
