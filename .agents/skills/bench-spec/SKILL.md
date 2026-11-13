---
name: bench-spec
description: Create, change or review a versioned benchmark spec (benchmarks/specs/*.ini, run by llama-cpp-config bench run --spec) or one of the prompts a spec pins. Use when asked to add or edit a spec, a pass, a prompt, a temperature, repetition or concurrency rule of the benchmark method, to bump a spec version, or to judge whether a measurement follows a spec.
---

# Benchmark specs: the method, versioned

A spec is the method of a measurement written down, so that two runs, two machines and
(later) a public portal compare like with like. Its rules are frozen per version: every run
records `spec`, `spec-version`, `spec-sha256` and `spec-pass`, and a number is only
comparable with another of the same spec, version and digest. Never change what a
published version means.

## 0. Read first

- `benchmarks/README.md`, "Specs: the method, versioned": the passes of `standard` and why.
- `crates/config/src/bench/spec.rs`: the format, what `parse` refuses, `canonical` (what
  the digest covers), and the tests that freeze the repo's spec.
- `benchmarks/specs/standard-<X.Y>.ini` (one file per published version) and
  `standard.versions`.
- `benchmarks/prompts/README.md`: what each shipped prompt is, and why it is a frozen snapshot.
- The `optimize-preset` skill, section 4: the measurement rules a spec encodes.

## 1. The format

```ini
[Spec]
Name = standard
Version = 1.0
Prompts = <file>@<sha256 of its text>, <file>@<sha256>
MaxTokens = 0
ImageSize = 1344x896

[Pass.<name>]
Temp = 0
Override = key=value; key=value
Concurrency = single
Reps = 3
```

- `Name`: anything but `custom` (what a sweep without `--spec` records).
- `Version`: `X.Y`, the same as the file name's (`standard-1.0.ini`).
- `Prompts`: a bare name is a shipped prompt (`benchmarks/prompts/`, installed as
  `share/benchmarks/prompts/`), else a file in the config directory. A shipped prompt's name
  ends in its version (`bench-prompt-code-1.0.json`); `.txt` is one user message, `.json` a
  conversation with tools (`{"messages": [...], "tools": [...]}`).
- `MaxTokens`: 0 = no cap.
- `ImageSize`: `WxH` (or `original`), the box an image prompt's image is fitted into when
  sent (aspect kept, never enlarged). In the digest only when set. An image prompt is a
  `.json` whose message holds `{"type": "image_file", "file": ..., "sha256": ...}`, the
  image shipped beside it (`bench-image-<name>-<X.Y>.jpg`) at full size: the resolution is
  the spec's setting, not baked into the file. It runs only on presets with an `mmproj`.
- One `[Pass.<name>]` per pass, run in file order, once per prompt. `Temp` is a number or
  `preset`; `Override` keys are written to the preset for the pass and put back after it;
  `Concurrency` is `single`, `prefill`, `decode`, `prefill+decode` or `all`. `Prompts` (file
  names among `[Spec] Prompts`) limits the pass to those; `Warm = prefix` makes every
  request an agent step with its prefix cached (needs a conversation with an assistant turn;
  `all` then leaves `prefill` out). Both enter the digest only when set.
- `bench run --spec <it> --only <file>,...` runs just the legs of those prompts, recorded as
  the spec's: a new version can measure only the legs it added and cite the older version's
  runs for the rest when their rules are unchanged.

Comments are whole lines (`;`), never after a value. `bench run --spec standard` runs the
newest published version and `--spec standard-<X.Y>` that one (every version stays installed,
read in place from `share/benchmarks/specs/`, never copied into the user's config; the binary
carries only `standard.versions`); `--spec <file>` runs any other, but only a published
`standard` version is a spec run: a file whose rules are not one, a method flag beside `--spec`,
or a swept key a pass overrides records every run as `spec = custom` with `spec-derived-from`,
`spec-derived-sha256` and `spec-derived-fields`.
The Benchmark tab's Save as… writes such a file of the user's (never named `standard`); it is
a personal method, not a version: to make it the framework's, follow section 4.

## 2. Decide what kind of change it is

| change | what it takes |
|---|---|
| a comment, wording, layout | nothing: the digest ignores them (check that `cargo test` still passes) |
| any rule: a pass, `Temp`, `Reps`, `Concurrency`, `Override`, `MaxTokens`, the prompt list or a pinned digest | a **new version**: a new file `standard-<X.Y>.ini` beside the old one, which stays |
| a different purpose (another workload, a quick screen, a drafter-only method) | a **new spec** (new `Name`, Version 1.0), not a version of `standard` |
| a prompt's text | a **new prompt file** (`<name>-<X.Y>` with the next version) and a new spec version pinning it; never edit or regenerate a shipped prompt in place, never delete one a published version pins |

A version bump never replaces the old one's meaning: version 1.0 stays in `standard.versions`,
in `specs/` and in bench.db, and its runs stay valid as version-1.0 runs. The exceptions on
record, both on 2026-10-31 and both because no run had recorded the version yet: `1` was
renamed `1.0` with the code prompt added, then 1.0 gained the image prompt and `ImageSize`
(its `.versions` line replaced, the reason noted above it). A version may change in place
only while no run has recorded it AND no tagged release ships it; after that it is frozen,
and a change is the next version.

## 3. Checks before accepting a change

1. **It measures what it says.** Temperature 0 with a speculator on inflates acceptance far
   above the preset's sampling: a temp-0 pass turns the speculator off
   (`Override = spec-type=none`); a pass that keeps the speculator uses `Temp = preset` and
   at least 5 repetitions (each one writes a different text: means, not single numbers).
   With `model-draft` in a preset, `spec-type` unset re-enables the drafter from the GGUF
   header: the override must say `none`, not `unset`. A sweep that varies a key a pass
   overrides (`--key spec-type` under `standard`) puts the leg's value over the pass's: it
   runs, recorded as derived (`spec-derived-fields = override`), never as the spec.
2. **It can run everywhere it claims.** `Concurrency` other than `single` needs a preset with
   `parallel` >= 2 (`all` falls back to single alone). Each prompt needs a slot context of
   its tokens plus the longest uncapped answer (for prose at least ~8k, the longest measured so
   far, on one model, so leave a margin: single-request temp-0 answers to the 115k prompt ran
   1,945 to 7,840 tokens across configurations; `bench-prompt-code-write-1.1.json`
   reasons 3k to 19k tokens before ~4k of code, so ~65-70k per slot): the 115k prompt needs ~124k per slot,
   `ctx-size` ~248k at `parallel = 2`, a minimum with no margin; on a smaller preset a prompt that
   does not fit gets llama-server's context-size error and one that fits with an answer the context
   cuts fails with `the reply was cut by the context limit`, which names the slot size and tokens
   (one such repetition discards that preset's run; an answer stopped by a `max-tokens` cap stays valid), so
   say which presets a spec fits. An `Override` key must be sweepable
   (`parse` refuses one that is not). A prompt must be one every machine has: a shipped one,
   or a file published with the spec.
3. **It is affordable.** Estimate the length: prompts x passes x scenarios x reps x legs, at
   the prefill and decode rates of the slowest machine it targets (on two R9700s a 115k cold
   prefill is ~2 minutes, an uncapped answer ~8000 tokens at the longest measured so far, a code-write one up to ~23k).
   Say the estimate. A `Warm = prefix` pass: check on one short run that the turn's `cache_n`
   is close to the prefix and `prompt_n` only the last turn's delta (a hybrid model restores
   the cache from context checkpoints).
4. **It does not re-measure for nothing.** A new version re-runs configurations bench.db
   already holds under the old one; that is the price of a method change, so the change must
   be worth it. State what the old version got wrong.
5. **The repo stays public-safe.** No absolute paths, no credentials, no em-dashes in the
   spec or its docs.

## 4. Apply a new version

1. Copy the newest `benchmarks/specs/standard-<X.Y>.ini` to `standard-<X.Y+1>.ini` (or
   `<X+1>.0` for a change of purpose within `standard`), change the rules and `Version`.
   Never edit or delete a published version's file.
2. Run `cargo test the_standard_spec_is_a_published_version` (in `crates/config/`): it
   fails and prints the line to add, `<X.Y> <sha256>`. Append that line to
   `standard.versions`; never edit an existing line.
3. A new prompt: add the file to `benchmarks/prompts/` as `<name>-<X.Y>.txt|json` and describe
   it in `benchmarks/prompts/README.md`; pin its digest in the spec (`sha256sum`, the files are
   LF and BOM-free). The packages install the whole directory and the Benchmark tab lists it,
   with no code change. `the_standard_spec_pins_the_shipped_prompts` checks every pin.
4. Update `benchmarks/README.md` (the pass table) and the `optimize-preset` skill when the
   measurement rules changed.
5. `cargo test`, then one short real run (`--values` with one value, the cheapest preset)
   to see every pass start, and `bench query runs --campaign <it> --show spec-version,spec-pass`.
   An installed build reads the spec from `share/benchmarks/`, so check that `xtask package`
   staged the new files.
6. Commit the spec, its `.versions` line and the docs together; the subject names the spec
   and the version (`bench spec standard-1.1: ...`).

## 5. Reviewing a change

Every change to a spec, a `.versions` file or a pinned prompt is reviewed against sections 2
and 3 before it is committed, by a reviewer that did not write it: the `spec-reviewer`
subagent (`.opencode/agents/spec-reviewer.md`, read by Claude Code through the
`.claude/agents` symlink; it only loads this skill and follows this section), or
any agent told to review with it. Read-only: the review judges, it does not edit.

Get the change with `git diff` (or `git diff <base>`), read `crates/config/src/bench/spec.rs`
(`parse`, `canonical`) and the files touched, then report each check as PASS, FAIL or N/A with
one line of evidence:

1. **Kind of change** (section 2): comment only, new version, new spec, or new prompt. A rule
   changed with the same `Version`, an existing `.versions` line edited or removed, or a
   shipped prompt's text edited in place, is a FAIL.
2. **`.versions`**: a new version has exactly one new line, appended, with the digest
   `cargo test the_standard_spec_is_a_published_version` expects (run it from
   `crates/config/` when a build is possible; say so when it is not).
3. **Measurement rules** (section 3, item 1): no temp-0 pass with the speculator on; a pass
   with the speculator on uses the preset's temperature and at least 5 repetitions; a
   speculator override says `none`, not `unset`.
4. **Runnability** (item 2): concurrency against `parallel`, override keys sweepable, prompts
   available on every machine and pinned.
5. **Cost** (item 3): an estimate of the run length on the slowest target machine.
6. **Re-measurement** (item 4): what the change re-runs, and whether its reason justifies it.
7. **Public safety** (item 5): no absolute paths, credentials or em-dashes.

End with a verdict: APPROVE, APPROVE WITH NOTES, or REJECT; for a REJECT, the smallest change
that would pass. A REJECT is not committed.

## 6. Keeping this skill current

When a spec change or a review teaches something about the method (a rule that misled, a
check that was missing), fold it into sections 2 and 3 in the same change.
