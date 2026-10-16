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
