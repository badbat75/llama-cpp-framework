# benchmarks

`bench.db` (SQLite, committed as a binary blob) holds the measurements behind the
`docs\*.md` guides with everything needed to reproduce or compare them: the machine
(CPU, RAM amount, type and speed, GPUs with VRAM and driver, OS and kernel), the software
stack (llama.cpp tag, build and commit, framework version, ROCm, CUDA and Vulkan SDK dists,
the `patches\llama.cpp\` applied), the model file, every parameter of the run (all the
preset keys, `server.ini`, environment variables) and the prompts, stored in full.

`schema.sql` is the data model, with a comment per column. The unit is the **run**: one
model load with one configuration, inside a **campaign** (the question a set of runs
answers, and its answer). Its **results** are the requests sent to it (or the llama-bench
repetitions); `run_device` holds the memory per device. Only runs that finished are
recorded: a run that failed to load, crashed or was cancelled is left out. A run carries
its start and end as `YYYY-MM-DD HH:MM:SS UTC`; the first campaigns, recorded by hand,
have the start date only.

## Recording

The configurator compiles `schema.sql` in and records every finished run of its
Benchmark tab and of `bench sweep` into a WORKING database under its data directory
(`~/.local/share/llama.cpp/bench/bench.db`, `%LOCALAPPDATA%\llama.cpp\bench\bench.db`):
one campaign per tab run or sweep, machine and software detected (the ROCm, CUDA and
Vulkan SDK dist versions are the ones llama.cpp was built against). Credentials and
service URLs are never stored, and an absolute path is stored as its file name, in the
logs and reports too: the file is public with the repo. A campaign the docs cite is
copied here, with its answer:

```sh
llama-cpp-config bench answer sweep-20261016-181500 "4 wins up to 40k, 3 beyond"
llama-cpp-config bench publish sweep-20261016-181500 --to benchmarks/bench.db
```

`publish` copies whole campaigns and everything their runs point at (the machine matched
by a fingerprint over its description, not by id), the engine's logs and the report
included, and refuses a campaign already here. To record straight into this file instead,
point the configurator at it: `BenchDb = <clone>/benchmarks/bench.db` in its
`settings.ini`. Before committing, check the file for credentials (`strings bench.db`).

## Specs: the method, versioned

A number is only comparable with another measured the same way. `specs\` holds that way
written down: `standard.ini` is the framework's method, an INI file with the prompts (the
shipped `bench-prompt-43k.txt` and `bench-prompt-110k.txt`), the output cap (none) and
the passes, each with its temperature, repetitions, concurrency and the preset keys it
overrides for every leg. Version 1 has two passes:

| pass | temperature | speculator | concurrency | reps | measures |
|---|---|---|---|---:|---|
| `engine` | 0 | off (`spec-type=none`) | single | 3 | the bare engine: deterministic text, so only speed differs between legs |
| `real` | the preset's | the preset's | `all`: single, then every concurrent scenario at N = `parallel` | 5 | what the preset serves; each repetition writes a different text, so compare means |

Greedy text is what a drafter predicts best: a temp-0 run with the speculator on reads
far faster than the preset ever serves, which is why `engine` turns it off and `real`
keeps the preset's sampling. The off switch is for the benchmark only; the preset keeps
its speculator.

```sh
llama-cpp-config bench sweep --spec standard --preset <id> --key split-mode --values 'unset,tensor'
```

`standard` is compiled into the configurator from `specs/standard.ini`, so an installed
machine needs no checkout; `--spec <file>` runs a spec of your own. Every benchmark (a
sweep with or without a spec, and the Benchmark tab) holds the GPU lock for its whole
length: an exclusive lock on `$LLAMA_FW_GPU_LOCK`, default `gpu.lock` in the bench
directory (`flock` on Linux, so `flock -o <same file> <job>` keeps other GPU jobs out),
and on Linux it also waits until nothing but llama-server holds `/dev/kfd` or
`/dev/nvidia*`. The tab refuses instead of waiting. Every run
records `spec` (the Name), `spec-version`, `spec-sha256` (of its rules in canonical form,
`bench::spec::canonical`: comments and layout do not count, a changed rule does) and `spec-pass`; the leg label adds
`pass=<name>`. A sweep without `--spec` records `spec = custom`.

**Changing a rule means bumping `Version`**, never editing version 1 in place: runs of two
versions are two methods. The prompts are part of the method: each is listed as
`<file>@<sha256 of its text>` (the digest `bench query prompts` shows), a prompt that no longer
matches is refused before the first leg, and a test pins `standard.ini` to the prompts the
binary ships. A new prompt is a new spec version. `standard` needs a slot context of about
116k tokens (the 110k prompt plus an uncapped answer: `ctx-size` about 232k at `parallel = 2`)
and takes about 4 to 8 hours for a two-value sweep on two R9700s with the dense 27B: an
overnight job, not a quick screen (a screen is a spec of its own). A sweep cannot vary a key
a pass overrides (`--key spec-type` is refused). `specs/standard.versions` lists every
published version with its digest, and a test fails when `standard.ini`'s rules do not match
the line of its `Version`: a rule changed without a bump, or a published version edited in
place, cannot reach a build. The procedure for changing a spec, and what to check, is the
`bench-spec` skill (`.agents/skills/bench-spec/SKILL.md`); its section 5 is the review every change gets before it is
committed (the `spec-reviewer` subagent of Claude Code and OpenCode runs it). A spec of your own is a file with its own `Name` (anything but
`custom`). Compare like with like:

```sh
llama-cpp-config bench query summary --db benchmarks/bench.db --param spec-version=1 --param spec-pass=real
```

## Querying

`llama-cpp-config bench query` reads the working database, or this one with
`--db benchmarks/bench.db`, read-only:

```sh
bench query campaigns                                  # question, answer, runs per campaign
bench query models                                     # models tested, runs and campaigns each
bench query machines                                   # CPU, RAM (size, type, speed), GPUs, OS
bench query summary --model 27b --show ctx-size,cache-type-k
                                                       # per run and prompt (or llama-bench test):
                                                       # prefill and decode mean +- sd, draft acceptance, free VRAM
bench query runs --gpu r9700 --param no-host=true      # runs that computed on the R9700 with no-host
bench query results --prompt long-43k --llama v0.7.0   # one line per request
bench query run 3                                      # everything about one run
bench query diff 1 3                                   # the parameters that differ
bench query sql "SELECT ..."                           # anything else
```

Filters: `--campaign`, `--model` (name, file or quant), `--gpu` (a GPU the run computed on,
`run_gpu`), `--device` (the preset's device list), `--llama`, `--status`, `--since`,
`--param K=V` or `K~V` (contains), repeatable. `--format csv` or `json` for other tools,
`--full` for whole text cells.

## Reading with sqlite3

```sh
sqlite3 -header -column benchmarks/bench.db \
  "SELECT campaign, label, prompt, rep, prefill_tps, decode_tps FROM v_result ORDER BY campaign, label"
```

`v_result` joins each result with its run, campaign, model and llama.cpp version;
`v_run_params` gives a run's parameters per scope as one line.
