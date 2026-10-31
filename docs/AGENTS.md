# docs\

The configuration references and the developer references. The `.md` files are the source:
read as is on GitHub and by agents; an HTML site with charts will be generated from them
and from `benchmarks\bench.db` (not written yet). Nothing here may need the site to be read.

## Page types

Every page starts with a YAML front matter block (GitHub shows it as a table) whose `type`
says which rules apply. `README.md` is the index: a new page gets its line there.

| `type` | pages | what it is |
| --- | --- | --- |
| `gpu` | `amd_ai_pro_r9700.md`, `nvidia_rtx4070_super.md` | one card: the measured optimum per llama.cpp/server/preset parameter on it |
| `model` | `qwen3.8_27b.md`, `qwen3.8_flash_next.md` | one model: the same, on the cards it was measured on |
| `reference` | `build-pipeline.md` | developer reference; the measurement rules below do not apply |

A new `gpu` or `model` page starts from `templates\<type>.md` (a copy, then fill it; `templates\` holds no pages and the site skips it). File
names stay flat in `docs\` and are never renamed: code, the frozen benchmark prompts and
bench.db notes cite them by path.

## Front matter

```yaml
---
title: Qwen3.8-27B                  # the page's subject, as a heading would name it
type: model                         # gpu | model | reference
status: draft                       # draft | reference (every Settings row measured)
gpus: [AMD Radeon AI PRO R9700]     # bench.db gpu.name of each card it covers
models: [Qwen3.8-27B]               # bench.db model.name prefixes it covers
platforms: [linux, windows]
llama-cpp: v0.5.0 to v0.8.0         # the releases its rows were measured with
updated: 2026-10-31                 # the last change to a measured value
---
```

`gpus` and `models` are how the site links a campaign page to the guides that cite it;
a `reference` page has neither. The campaigns a page cites are NOT listed here: they are
read from its "(campaign `name`)" citations, so the list cannot drift.

## Sections (`gpu` and `model`)

Numbered `## N. <title>`, cited by number ("section 4") within the page. Required, under
these names:

- `gpu`: `Runtime prerequisites`, `Device facts`, `Settings`, `Choices that look better than they measure`, `Checking a configuration`, `Not yet measured (placeholders)`.
- `model`: `Model facts`, `Recommended settings`, `Choices that look better than they measure`, `Not yet measured (placeholders)`.

A required title may carry a qualifier in parentheses (`Recommended settings (two cards)`).
Other sections (a topic, a platform, a machine: `Two R9700s on Linux (v0.8.0)`) go in
between or after, numbered on; a new one is appended, never inserted, so citations by
number keep pointing where they did. The tables keep their columns: settings
`key | value | evidence`, choices `option | result`, placeholders
`parameter | current value | question | result` (`TBD` until a campaign answers it; then
the row moves to the settings or the choices table).

## Rules for `gpu` and `model` pages

- Terse: a reference, not a narrative.
- Every value traces to a measurement; a value recorded in `benchmarks\bench.db` names its campaign, as in (campaign `27b-kv-f16-20261016`) (`llama-cpp-config bench query campaigns --db benchmarks\bench.db` lists them), never a local archive file or path; no absolute paths. A cited campaign must exist in bench.db: a leg that failed records nothing (only finished runs are recorded), so name its sweep without the citation form ("the sweep `x` recorded nothing").
- When a campaign changes a result, update the row (and fill or retire its placeholder) in the same change, and `updated` in the front matter.
- Runs recorded without a spec are `spec = custom`: say the method in the row or the section intro (temperature, repetitions, prompt, output cap), since no spec version stands for it.
- A `gpu` page covers ITS card only: no machine-specific device ids (`ROCm1`, `CUDA0`), no split vector of one pairing, no mention of the other card.
- A `model` page names the cards it was measured on but stays about the model; per-card detail stays in the card guides, which it cites.

## Charts: `bench-chart` blocks

A chart is a fenced code block tagged `bench-chart`, placed right after the table that
holds the same numbers (the table stays: the chart illustrates it and never replaces it).
GitHub shows the block as text; the site draws it from bench.db. One `key = value` per line,
`;` starts a comment:

````md
```bench-chart
title    = Decode total by scenario, 43.7k tokens
campaign = swift27b-concurrency-2xr9700-linux-20261031
metric   = decode-total        ; see the list below
x        = concurrency         ; a run parameter (bench scope first, then preset, server)
series   = split-mode          ; one bar or line per value; unset reads as the default
where    = prompt=bench-prompt-long   ; optional filter
```
````

- `campaign`: one name, or several separated by `,`. Every one must exist in bench.db.
- `metric`, per run, mean over repetitions (± sd shown on hover):
  - `prefill`, `decode`: t/s per request (`result.prefill_tps`, `decode_tps`), cold requests only for `prefill`;
  - `prefill-total`, `decode-total`: the tokens of the requests that ran at once over the window they share, per repetition, then the mean (`bench::concurrent_totals`: recomputed from each request's send offset in `result.detail` and its timings, since bench.db stores no total row); a single request's total is its own rate;
  - `decode-warm`, `decode-cold`: per request, by the request's role in a concurrency scenario;
  - `acceptance`: drafted tokens accepted, %;
  - `free-mib`: free memory per device after the first prefill (`run_device`), one bar per device.
- `x`, `series`: a run parameter key (`concurrency`, `prompt`, `split-mode`, `ctx-size`...) or `label`. A key a run lacks reads as `unset` (the llama.cpp default).
- `where`: `key=value` pairs separated by `,`, all required to match.
- Optional: `title` (else the metric and campaign), `unit` (else from the metric).

A block that names a missing campaign or key will fail the site build; until the generator
exists, check by hand with `bench query runs --campaign <name> --show <key>`.

## Other rules

- No em-dashes (the repo rule): the glyph appears only as a not-applicable table cell, where it is data.
- Images go in `assets\` (screenshots in `assets\screenshots\`), referenced by relative path.
- `build-pipeline.md`: developer reference for the build phases and conventions; only the front matter and the em-dash rule apply.
