---
name: optimize-preset
description: Tune one model's section of presets.ini for speed (prefill and decode t/s) and GPU memory (VRAM used, headroom), measuring every change with llama-cpp-config bench sweep and recording it in bench.db. Use when asked to optimize, tune, speed up, fit, or shrink a preset, to raise its context, to rebalance tensor-split / n-cpu-moe / override-tensor across GPUs, or to pick KV cache type, ubatch-size or a speculator for a model.
---

# Optimize a preset (speed and VRAM)

Goal: for ONE preset, find the configuration that is fastest on the user's real workload
while leaving a safe VRAM margin on every card, and prove it with measurements recorded in
bench.db. Never claim a number you did not measure; never apply a value that only loaded.

## 0. Read first

- `docs/AGENTS.md` and the guides it lists: the card guides (`docs/amd_ai_pro_r9700.md`,
  `docs/nvidia_rtx4070_super.md`) and the model guide if one exists
  (`docs/qwen3.8_*.md`). Their "Settings" tables are the measured optimum per key, their
  "look better than they measure" tables are the refuted levers, their placeholder
  tables are the open questions. Start from these, do not re-derive them.
- The memory notes (VRAM headroom, per-model tuning, workload): they hold the last
  measured state of each preset and the levers still untested.
- `llama-cpp-config/AGENTS.md` "Footguns": semantics of `tensor-split` (normalized over
  blocks + output layer), `override-tensor` (regex matched anywhere, first rule wins,
  anchor with `^` and `$`), `load-mode`, `parallel` (divides the context; absent means
  auto 4 slots), the embedding table left in pinned host RAM, `spec-type` vs MTP heads.

## 1. Establish the baseline

```bash
llama-cpp-config preset list                 # ids, model paths, switched-off ones
llama-cpp-config preset show <id>            # the section as INI
llama-cpp-config server show                 # Threads, ThreadsBatch, env knobs
llama-cpp-config bench query campaigns --db benchmarks/bench.db
llama-cpp-config bench query summary --db benchmarks/bench.db --model <name> \
  --show ctx-size,cache-type-k,n-cpu-moe,tensor-split,ubatch-size
```

Config lives in `~/.config/llama.cpp/` on Linux, `%LOCALAPPDATA%\llama.cpp\config\` on
Windows. **Back up before any edit**: `cp presets.ini presets.ini.bak-$(date +%Y%m%d-%H%M%S)`.

Ask the user (once, only what is unknown):
- the minimum acceptable context (`ctx-size`) and slot count (`parallel`);
- the workload. Default assumption on this machine: conversations grow gradually, so
  rank by **decode deep in the context** (the long prompt, ~40k tokens) and short-increment
  prefill; cold full-prompt prefill is secondary.

Record the baseline: one live run with the long prompt and one with the short prompt,
plus the memory readout of section 3. Without a baseline there is no "faster".

## 2. Levers, in the order they usually pay

Use the model's facts (Model info in the Models tab, or the GGUF header: layers,
experts, attention interval, MTP heads, `token_embd` quant) to skip levers that cannot
apply.

1. **Placement (VRAM, biggest decode effect).** Everything on the GPUs beats anything in
   RAM. Dense: `device` + `tensor-split` (layer split; the last device gets output head,
   drafter logits and sampling: give it the card with spare VRAM). MoE: lowest
   `n-cpu-moe` that fits, then fine balance with partial `override-tensor` moves
   (`^output\.weight$=<dev>`, single `blk\.N\.ffn_*_exps` tensors). Keep the device order
   explicit; dropping `device` can reorder the cards.
2. **KV cache type.** `f16` decodes faster deep in context and has smaller compute
   buffers; `q8_0` only to reach a context `f16` cannot fit. It fits ~66% of q8_0's
   context, not half: measure, do not compute.
3. **`ubatch-size`.** Prefill only (+5% from 512 to 1024 on dense), decode unaffected; it
   costs compute buffer (~0.5 MiB per ubatch token per device). On MoE with experts in
   RAM a larger ubatch is paid in experts moved off the GPU: usually not worth it.
4. **Speculator.** `draft-mtp` if the GGUF has MTP heads; otherwise `ngram-map-k`
   (size-m 64, size-n 12 measured best). Sweep `spec-draft-n-max`. Read per-rep numbers:
   a speculator changes the output path.
5. **MoE extras.** `no-host = true` with `op-offload false` (+9% prefill), `load-mode`
   per the guides, `moe-cache-mib` off.
6. **Server-wide** (`server set`, affects every preset; ask first): `Threads` =
   `ThreadsBatch` = physical cores.

## 3. Measuring VRAM (do this for every candidate)

A load-only check is not enough: the CUDA/HIP pools grow at the first real prefill
(~40 MiB on the 4070, ~85-170 MiB on the R9700). Read memory **after a long-prompt
prefill**:

```bash
nvidia-smi --query-gpu=index,name,memory.used,memory.free --format=csv   # use memory.free: ~406 MiB is driver-reserved
for d in /sys/class/drm/card*/device; do [ -r $d/mem_info_vram_used ] && \
  echo "$d used=$(( $(cat $d/mem_info_vram_used)>>20 ))MiB total=$(( $(cat $d/mem_info_vram_total)>>20 ))MiB gtt=$(( $(cat $d/mem_info_gtt_used)>>20 ))MiB"; done
grep -E 'model buffer size|KV buffer size|RS buffer size|compute buffer size' \
  ~/.local/share/llama.cpp/logs/llama-server.log | tail -30
```

Windows: `tools\report-memory.ps1` (requested vs actual, spill into shared memory).

Margins to keep (free memory after the prefill):
- R9700 / ROCm: >= ~500 MB. Below ~50 MB hipBLASLt runs out and the child hangs (kill -9).
- RTX 4070 SUPER / CUDA: >= ~300 MiB `memory.free` is comfortable; ~30 MiB has worked
  but `cudaGraphInstantiate` OOM has been seen at ~400 MiB with ngram-mod. Tell the user
  the margin when it is below ~500 MiB and let them choose.
- GTT growing above idle on an AMD card means spill into system RAM: a regression.

## 4. Measuring speed: one key per sweep

```bash
llama-cpp-config bench sweep --preset <id> --key <key> --values '<a>,<b>,<c>' \
  --reps 3 --prompt-file ~/.config/llama.cpp/bench-prompt-long.txt \
  --campaign <model>-<key>-<yyyymmdd> --question "<what this answers>"
```

- `--values` with commas inside a value (`device`, `tensor-split`): separate with `;`.
  `unset` removes the key for a leg. Ranges: `2..8`.
- Live mode restarts llama-server per leg and sends `cache_prompt: false`, temp 0.
  Synthetic mode (llama-bench) needs the server DOWN and cannot model the drafter,
  `ctx-size` or `parallel`: use it only for raw prefill/decode of placement or quant.
- Two keys that must move together (e.g. `n-cpu-moe` + `tensor-split`): set one by
  editing presets.ini, sweep the other; or run one sweep per combination into the same
  campaign. Restart (`llama-cpp-config control restart`) after every manual edit: the
  router reads presets.ini only at start.
- `--apply original` (default) leaves the preset as it was; apply the winner yourself
  after the memory check. After an interrupted sweep, restore with `--restore-to` (the
  value is in `~/.local/share/llama.cpp/bench/sweep-pending-*.json`).
- Differences under ~3% are noise unless sd says otherwise; re-run in reversed order
  (thermal drift lands on the last leg).
- A leg that does not load or OOMs is a result too: note the boundary (e.g. "180224 f16
  does not load").

Read results:

```bash
llama-cpp-config bench query summary --campaign <campaign> --show <key>
llama-cpp-config bench query diff <run-a> <run-b>
```

(Add `--db benchmarks/bench.db` when `settings.ini` `BenchDb` does not already point at
the repo's database; on this machine it does.)

## 5. Safety rules

- **Warn before the first live run**: a benchmark request replaces the slot's
  conversation, and the next restart with `SaveStateOnShutdown` on overwrites that
  model's `.llamastate` snapshot. Get an OK.
- Never `pkill -f <pattern>` with a pattern that appears in your own command line; use
  `llama-cpp-config control stop|restart`.
- Never drop or reorder `device` on a multi-GPU preset without meaning to.
- `spec-type = draft-mtp` on a model without MTP layers fails to load.
- One preset at a time; do not touch other presets or server.ini without asking.

## 6. Apply and record

1. Present a table: candidate, prefill and decode (short and long prompt, mean +- sd),
   free VRAM per card after prefill, context reached. Recommend one; the user decides
   when the trade is speed vs margin or context.
2. Apply the chosen values to presets.ini (back up first), restart, run one confirming
   long-prompt measurement and the memory readout.
3. `llama-cpp-config bench answer <campaign> "<conclusion with the numbers>"`, and
   `bench publish <campaign> --to benchmarks/bench.db` when the runs went to the
   working db. Scan for secrets before committing: the db is public.
4. Update the guides in the same change: the row in the model/card guide's settings
   table with "(campaign `<name>`)", fill or retire the placeholder it answers, no
   absolute paths, no em-dashes. Per-card guides stay about their card (no `CUDA0`,
   no split vector of one pairing).
5. Update the memory note for that model with the new preset, the numbers, the backup
   file name and the levers still untested.
6. Update this skill (section 7) before closing the task.

## 7. Keeping this skill current

This file is the procedure, and every optimization run is a chance to correct it. Before
reporting the result, review what the run taught about the METHOD and edit this file
(`.agents/skills/optimize-preset/SKILL.md`; `.claude/skills` is a symlink to the same
file) in the same change as the guides.

Update it when the run:
- used a command, flag or check that worked and is not here (a new `bench` subcommand or
  option, a better VRAM readout, a log line that tells a fact faster);
- found a step here wrong, stale or missing: a command that changed or failed, a margin
  that proved too small or too large, an ordering of levers that misled;
- found a new lever that pays, or refuted one listed in section 2 (keep the lever's
  numbers in the guide; here only the rule and where the evidence lives);
- hit a new failure mode or safety trap (an OOM pattern, a hang, a key combination that
  fails to load);
- changed with a new llama.cpp release or configurator version (a renamed key, a removed
  flag, a new default).

What goes where:
- **Here:** how to optimize, valid for any model and machine: steps, commands, checks,
  rules of thumb with the regime they hold in, traps.
- **The guides (`docs/`):** measured values of a card or a model, with their campaign.
- **Memory:** this machine's state: the current presets, backups, what is still untested.
  A fact that is only true of one preset never goes here.

How to edit:
- Change the section the lesson belongs to; do not append a log or changelog (git
  history is the changelog). Rewrite or delete what the run proved wrong instead of
  adding a contradicting line.
- Keep it terse and generic: cite a guide section instead of copying its numbers, keep a
  number here only when it is a threshold the procedure acts on (a VRAM margin, a noise
  level).
- Verify a command with `--help` or by running it before writing it down.
- Repo rules apply: no em-dashes, no absolute paths outside the per-user config and data
  directories already named here, no secrets. Keep the frontmatter `description` in step
  with the sections when the scope changes.
- In the final report, list what changed in the skill and why (one line each), or say
  that the run taught nothing new about the method.
