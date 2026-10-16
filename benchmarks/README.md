# benchmarks

`bench.db` (SQLite, committed as a binary blob) holds the measurements behind the
`docs\*.md` guides with everything needed to reproduce or compare them: the machine
(CPU, RAM amount, type and speed, GPUs with VRAM and driver, OS and kernel), the software
stack (llama.cpp tag, build and commit, framework version, ROCm, CUDA and Vulkan SDK dists,
the `patches\llama.cpp\` applied), the model file, every parameter of the run (all the
preset keys, `server.ini`, environment variables) and the prompts, stored in full.

`schema.sql` is the data model, with a comment per column. The unit is the **run**: one
model load with one configuration, inside a **campaign** (the question a set of runs
answers, and its answer). A run that failed is recorded too (`status` `load-oom`,
`run-oom`, `crash`, `interrupted`): what does not fit is a result. Its **results** are
the requests sent to it; `run_device` holds the memory per device.

## Recording

`benchdb.py record <campaign.json>` (Linux) detects the machine and the software itself,
reads the run's preset from `presets.ini` and overlays its `overrides`, so every key is
stored, not only the varied ones. Credentials and service URLs are never stored
(`SECRET` in `benchdb.py`): the file is public with the repo.

```json
{
  "campaign": {"name": "27b-kv-f16-20261016", "started": "2026-10-16",
               "question": "...", "answer": "..."},
  "preset": "Swift-1.5-Qwen3.8-27B",
  "model": {"path": "<models dir>/Swift-1.5-Qwen3.8-27B-Q8_0.gguf", "name": "Qwen3.8-27B (Swift-1.5)",
            "arch": "qwen35", "quant": "Q8_0", "params_b": 27.32, "layers": 64, "mtp": 1},
  "prompts": {"long-43k": {"file": "<path>", "tokens": 43730}, "code": {"text": "...", "tokens": 43}},
  "runs": [{
    "label": "K1 f16 163840", "started_utc": "2026-10-16",
    "overrides": {"ctx-size": "163840"}, "env": {}, "status": "ok",
    "devices": [{"device": "ROCm0", "free_mib": 878, "kv_mib": 7680, "compute_mib": 456.03}],
    "results": [{"prompt": "long-43k", "rep": 1, "temperature": 0, "cache_prompt": 0,
                 "prompt_tokens": 43730, "prefill_tps": 1160.9, "gen_tokens": 128, "decode_tps": 35.2}]
  }]
}
```

An override `"unset"` removes the key. A run label already recorded in the campaign is
skipped, so a file can be recorded again after adding runs.

## Querying

`benchq.py` (any OS, standard library only; opens the file read-only):

```sh
benchq.py campaigns                                  # question, answer, runs per campaign
benchq.py models                                     # models tested, runs and campaigns each
benchq.py machines                                   # CPU, RAM (size, type, speed), GPUs, OS
benchq.py summary --model 27b --show ctx-size,cache-type-k
                                                     # per run: long-prompt prefill and decode,
                                                     # short-prompt decode, draft acceptance, free VRAM
benchq.py runs --gpu r9700 --param no-host=true      # runs that computed on the R9700 with no-host
benchq.py results --prompt long-43k --llama v0.7.0   # one line per request
benchq.py runs --status load-oom                     # what did not fit
benchq.py run 3                                      # everything about one run
benchq.py diff 1 3                                   # the parameters that differ
benchq.py sql "SELECT ..."                           # anything else
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
