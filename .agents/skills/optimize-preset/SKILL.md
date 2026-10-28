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
3. **`ubatch-size`.** Prefill only (+5% from 512 to 1024 on dense, +3% from 512 to 768 on
   Flash-Next), decode unaffected; it costs compute buffer on EVERY card, and the slope
   depends on the model (~0.5 MiB per ubatch token per device on the dense 27B, ~1.0 to
   1.15 on Flash-Next): take it from the `compute buffer size` lines of the last load,
   not from a constant. On MoE with experts in RAM a larger ubatch is paid in experts
   moved off the GPU: usually not worth it (`docs/qwen3.8_flash_next.md` section 2).
4. **Speculator.** `draft-mtp` if the GGUF has MTP heads; otherwise `ngram-map-k`
   (size-m 64, size-n 12 measured best). Sweep `spec-draft-n-max`. Read per-rep numbers:
   a speculator changes the output path.
5. **MoE extras.** `no-host = true` with `op-offload false` (+9% prefill), `load-mode`
   per the guides, `moe-cache-mib` off.
6. **Server-wide** (`server set`, affects every preset; ask first): `Threads` =
   `ThreadsBatch` = physical cores.

## 3. Measuring VRAM (do this for every candidate)

A load-only check is not enough: the CUDA/HIP pools grow at the first real prefill,
and by an amount that depends on the model (~40 MiB on the 4070 and ~85-170 on the R9700
with the dense 27B, ~320 and ~510 with Flash-Next-XXS at `q8_0` KV). Read memory **after
a long-prompt prefill**:

```bash
nvidia-smi --query-gpu=index,name,memory.used,memory.free --format=csv   # use memory.free: ~406 MiB is driver-reserved
for d in /sys/class/drm/card*/device; do [ -r $d/mem_info_vram_used ] && \
  echo "$d used=$(( $(cat $d/mem_info_vram_used)>>20 ))MiB total=$(( $(cat $d/mem_info_vram_total)>>20 ))MiB gtt=$(( $(cat $d/mem_info_gtt_used)>>20 ))MiB"; done
grep -E 'model buffer size|KV buffer size|RS buffer size|compute buffer size' \
  ~/.local/share/llama.cpp/logs/llama-server.log | tail -30
```

Windows: `tools\report-memory.ps1` (requested vs actual, spill into shared memory). For
the background sampler of section 4: `nvidia-smi --query-gpu=memory.free` for the NVIDIA
card and `Get-Counter '\GPU Adapter Memory(*)\Dedicated Usage'` for the AMD ones (one
instance per adapter LUID; free = the card's total minus it).

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

- `--values` with commas inside a value (`device`, `tensor-split`): separate with `;`,
  and end a single such value with one (`'16,50;'`): without a `;` the spec splits on
  `,` and sweeps `16` and `50`. `unset` removes the key for a leg. Ranges: `2..8`.
- Not every preset key is sweepable (`mmproj-device` is refused, `mmproj-offload` is
  not; the refusal lists the sweepable keys): set the key in presets.ini, then run a
  one-leg sweep of a sweepable key (its current value) into the same campaign; the run
  records every key, so `bench query runs --show <key>` tells the legs apart even though
  their labels look alike. Launch-line levers are not preset keys at all: `Threads` and
  `ThreadsBatch` go through `server set --threads-batch N` (server-wide, every preset: ask
  first), then the same one-leg sweep records them (`ThreadsBatch` is a run parameter);
  `--poll`, `--cpu-strict`, `--prio-batch` have no configurator setting, so they can only
  be screened by hand on a standalone llama-server (the "where a prefill step goes" launch
  below, plus the flag), unrecorded, and only a winner is worth adding to the configurator.
- Before a leg that moves experts to RAM, size the RAM: `CPU model buffer size` (plus
  `CPU_REPACK`) from a previous load, or the layer count times the per-layer size, against
  the machine's free RAM; a leg that pages (free RAM near zero, single-digit t/s on the
  warm-up) is not a measurement, stop it. A MoE cache only holds non-repacked experts, so
  with `no-host` it needs `repack = false`, and the pinned host buffer (`no-host` off) is
  impossible at tens of GB.
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
- A speculator's first repetition after a restart can take another output path and read
  several t/s low (DFlash: on every leg): with a drafter, compare the later repetitions
  and use `--reps 4` when two legs are within a few percent.
- Differences under ~3% are noise unless sd says otherwise; re-run in reversed order
  (thermal drift lands on the last leg).
- Never re-measure a configuration bench.db already holds (same key values, build and
  prompt: `bench query runs --campaign <earlier>`): sweep only the untested values and
  cite the existing run as the baseline in the question and the guide. A repeat is only
  for a stated purpose (noise check, drift, a new build).
- A leg that does not load or OOMs is a result too: note the boundary (e.g. "180224 f16
  does not load").
- Swapping `device` order changes which card takes the output head, MTP head and
  sampling, so the mirrored `tensor-split` usually does not fit: size the split from the
  per-layer weights before sweeping. `llama-gguf <first shard> r` (in the build tree's
  `bin/`) lists every tensor with its bytes (each tensor twice: dedupe by name); sum per
  block, experts (`_exps`) apart, and add the output head. A split `a,b` over 48 blocks
  gives the first device blocks `0..a-1` and the second the rest plus the output layer
  (`a + b = 49`); with `n-cpu-moe N` the first N blocks weigh only their attention. Check
  the sums against the last load's `model buffer size` lines (within ~2%), then budget each
  card: weights + KV (per attention layer it holds) + RS + the compute buffer + the pool
  growth, against total minus the driver's reserve. Take the compute buffer from a load
  with the SAME KV type (`q8_0` KV makes it 874 MiB per card where `f16` takes 531 to 592,
  Flash-Next at `ubatch 512`) and the pool growth from the sampler of an earlier run of
  the same model (section 3), or the projection lands hundreds of MiB optimistic; the
  weight sums themselves came within 1 % of the log. Sweep the computed split and the next
  block up (one campaign): the one that fails to load fixes the boundary at the cost of
  one leg. Expert sizes vary per layer in mixed-precision quants, so an average misleads.
- A live run now records `run_device` (free MiB per card after the first measured prompt,
  from a `--list-devices` probe beside the running server, shown in `bench query summary`'s
  "free MiB" column): enough to rank legs, but the probe's own contexts make it read
  ~190 MiB low on CUDA and ~25 on ROCm, so near a margin threshold keep the sampler below.
- A sweep restarts the server per leg, so one readout after it shows only the last leg:
  run a background sampler of free memory per card (the section 3 commands every 2 s,
  into a file) and take each leg's minimum after its load phase.
- On Windows `llama-cpp-config.exe` is a GUI-subsystem binary: a shell does not wait for
  it (`&` returns at once, bash loses it after a minute) and a sweep started twice runs
  both at once and wrecks both. Run it from PowerShell with `Start-Process -FilePath
  <exe> -ArgumentList @(...) -NoNewWindow -PassThru -RedirectStandardOutput <out>
  -RedirectStandardError <err>` and wait with `$p.WaitForExit()` (not `-Wait`, which
  also waits for the llama-server the sweep leaves running). `-ArgumentList` does not
  quote an element with spaces: wrap the `--question` text in literal double quotes, or
  the sweep dies at once with `unexpected argument`. Before starting one, check
  `Get-CimInstance Win32_Process` for a running `bench sweep`.
- Where a prefill step goes (when a lever's effect needs explaining, not for every
  run): stop the server, start a standalone llama-server with the child's argument list
  from the router's log (`spawning server instance ... with args`, without
  `--slot-save-path`) and send one `cache_prompt: false` request of the long prompt.
  CUDA side: `nsys launch -t cuda`, then `nsys start` / `stop` around the request and
  `nsys stats --report cuda_gpu_kern_sum,cuda_gpu_mem_time_sum,cuda_api_sum` (CPU
  sampling needs an elevated shell; `nsys` cannot attach on Windows). HIP side on
  Windows (the TheRock dist ships no rocprofiler): `AMD_LOG_LEVEL=3` makes the runtime
  log every HIP call with entry and return stamps on stderr (~120 MB per 43k prefill, no
  measurable slowdown): the `hipStreamSynchronize` pairs are the GPU time the thread
  waits for, the `hipMemcpyAsync` durations the blocking host-to-device copies. Each
  card's busy share: `Get-Counter '\GPU Engine(pid_<pid>_*)\Utilization Percentage'`.
  Read the copies by size: a size that repeats once per ubatch names its tensor
  (`n_ubatch x n_embd x 4` bytes is a hidden state crossing between cards, `x n_ff` a
  gate or up output split by an `override-tensor` rule, a size growing with the context
  the attention mask), and a blocking copy of tens of ms per 20 MiB is pageable memory
  on a HIP runtime (the Windows one stages at ~2 GB/s): llama pins its input buffers for
  the FIRST device only, and cross-card hops go through `malloc`. Both show up as time
  only on Windows; on Linux the runtime pins pageable memory on the fly.
  HIP side on Linux: `rocprofv3 --attach` fails (status 5) under the default
  `kernel.yama.ptrace_scope = 1`; launch the standalone server under the profiler instead
  (`rocprofv3 --kernel-trace --memory-copy-trace --output-format csv -d <dir> -- llama-server
  ...`), prefill once with `cache_prompt: true`, send the decode request, stop the server
  with SIGINT so the trace is written, and analyse the window of the decode request from
  the trace's end: per card the union of kernel intervals over the wall time (on a layer
  split the cards run in series, so their shares add up), the top kernels, the copies by
  direction. The profiler slows decode ~12%; scale the shares before quoting them.
  Example of what it finds: `docs/qwen3.8_27b.md` section 4.

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
- When another session may use the GPUs, wrap every GPU job (sweep, standalone server,
  profiler) in `flock` on one agreed lock file and, after taking it, wait until no other
  process holds `/dev/kfd` open (`ls -l /proc/*/fd` lists it): an unwrapped job still
  shares the cards and wrecks both measurements. A live sweep owns server.ini's `Port`.

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
