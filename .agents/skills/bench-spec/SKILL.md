---
name: bench-spec
description: Create, change or review a versioned benchmark spec (benchmarks/specs/*.ini, run by llama-cpp-config bench sweep --spec) or one of the prompts a spec pins. Use when asked to add or edit a spec, a pass, a prompt, a temperature, repetition or concurrency rule of the benchmark method, to bump a spec version, or to judge whether a measurement follows a spec.
---

# Benchmark specs: the method, versioned

A spec is the method of a measurement written down, so that two runs, two machines and
(later) a public portal compare like with like. Its rules are frozen per version: every run
records `spec`, `spec-version`, `spec-sha256` and `spec-pass`, and a number is only
comparable with another of the same spec, version and digest. Never change what a
published version means.

## 0. Read first

- `benchmarks/README.md`, "Specs: the method, versioned": the passes of `standard` and why.
- `llama-cpp-config/src/bench/spec.rs`: the format, what `parse` refuses, `canonical` (what
  the digest covers), and the tests that freeze the repo's spec.
- `benchmarks/specs/standard.ini` and `standard.versions`.
- `llama-cpp-config/assets/README.md`: why a shipped prompt is a frozen snapshot.
- The `optimize-preset` skill, section 4: the measurement rules a spec encodes.

## 1. The format

```ini
[Spec]
Name = standard
Version = 1
Prompts = <file>@<sha256 of its text>, <file>@<sha256>
MaxTokens = 0

[Pass.<name>]
Temp = 0
Override = key=value; key=value
Concurrency = single
Reps = 3
```

- `Name`: anything but `custom` (what a sweep without `--spec` records).
- `Prompts`: a bare name is a file in the config directory, seeded when it is a shipped one.
- `MaxTokens`: 0 = no cap.
- One `[Pass.<name>]` per pass, run in file order, once per prompt. `Temp` is a number or
  `preset`; `Override` keys are written to the preset for the pass and put back after it;
  `Concurrency` is `single`, `prefill`, `decode`, `prefill+decode` or `all`.

Comments are whole lines (`;`), never after a value. `bench sweep --spec standard` runs the
compiled-in copy of `standard.ini`; `--spec <file>` runs any other.

## 2. Decide what kind of change it is

| change | what it takes |
|---|---|
| a comment, wording, layout | nothing: the digest ignores them (check that `cargo test` still passes) |
| any rule: a pass, `Temp`, `Reps`, `Concurrency`, `Override`, `MaxTokens`, the prompt list or a pinned digest | a **new Version** of the same spec |
| a different purpose (another workload, a quick screen, a drafter-only method) | a **new spec** (new `Name`, Version 1), not a version of `standard` |
| a prompt's text | a **new prompt file** (new name) and a new spec version pinning it; never edit or regenerate a shipped prompt in place |

A version bump never replaces the old one's meaning: version 1 stays in `standard.versions`
and in bench.db, and its runs stay valid as version-1 runs.

## 3. Checks before accepting a change

1. **It measures what it says.** Temperature 0 with a speculator on inflates acceptance far
   above the preset's sampling: a temp-0 pass turns the speculator off
   (`Override = spec-type=none`); a pass that keeps the speculator uses `Temp = preset` and
   at least 5 repetitions (each one writes a different text: means, not single numbers).
   With `model-draft` in a preset, `spec-type` unset re-enables the drafter from the GGUF
   header: the override must say `none`, not `unset`. A sweep cannot vary a key a pass
   overrides (`run` refuses `--key spec-type` under `standard`): the leg's value would win.
2. **It can run everywhere it claims.** `Concurrency` other than `single` needs a preset with
   `parallel` >= 2 (`all` falls back to single alone). Each prompt needs a slot context of
   its tokens plus the longest uncapped answer (~5k): the 110k prompt needs ~116k per slot,
   `ctx-size` ~232k at `parallel = 2`; a smaller preset fails those legs (not recorded), so
   say which presets a spec fits. An `Override` key must be sweepable
   (`parse` refuses one that is not). A prompt must be one every machine has: a shipped one,
   or a file published with the spec.
3. **It is affordable.** Estimate the length: prompts x passes x scenarios x reps x legs, at
   the prefill and decode rates of the slowest machine it targets (on two R9700s a 110k cold
   prefill is ~2 minutes, an uncapped answer up to ~5000 tokens). Say the estimate.
4. **It does not re-measure for nothing.** A new version re-runs configurations bench.db
   already holds under the old one; that is the price of a method change, so the change must
   be worth it. State what the old version got wrong.
5. **The repo stays public-safe.** No absolute paths, no credentials, no em-dashes in the
   spec or its docs.

## 4. Apply a new version

1. Edit `benchmarks/specs/<name>.ini`: the rules, and `Version = <n+1>`.
2. Run `cargo test the_standard_spec_is_a_published_version` (in `llama-cpp-config/`): it
   fails and prints the line to add, `<n+1> <sha256>`. Append that line to
   `<name>.versions`; never edit an existing line.
3. A new prompt: add the file to `llama-cpp-config/assets/`, to `SHIPPED_PROMPTS` in
   `bench.rs` and to `assets/README.md`; pin its digest (`sha256sum`, the files are LF and
   BOM-free) in the spec. `the_standard_spec_pins_the_shipped_prompts` checks the pin.
4. Update `benchmarks/README.md` (the pass table) and the `optimize-preset` skill when the
   measurement rules changed.
5. `cargo test`, then one short real run (`--values` with one value, the cheapest preset)
   to see every pass start, and `bench query runs --campaign <it> --show spec-version,spec-pass`.
6. Commit the spec, its `.versions` line and the docs together; the subject names the spec
   and the version (`bench spec standard v2: ...`).

## 5. Reviewing a change

Every change to a spec, a `.versions` file or a pinned prompt is reviewed against sections 2
and 3 before it is committed, by a reviewer that did not write it: the `spec-reviewer`
subagent (Claude Code: `.claude/agents/spec-reviewer.md`; OpenCode:
`.opencode/agents/spec-reviewer.md`; both only load this skill and follow this section), or
any agent told to review with it. Read-only: the review judges, it does not edit.

Get the change with `git diff` (or `git diff <base>`), read `llama-cpp-config/src/bench/spec.rs`
(`parse`, `canonical`) and the files touched, then report each check as PASS, FAIL or N/A with
one line of evidence:

1. **Kind of change** (section 2): comment only, new version, new spec, or new prompt. A rule
   changed with the same `Version`, an existing `.versions` line edited or removed, or a
   shipped prompt's text edited in place, is a FAIL.
2. **`.versions`**: a new version has exactly one new line, appended, with the digest
   `cargo test the_standard_spec_is_a_published_version` expects (run it from
   `llama-cpp-config/` when a build is possible; say so when it is not).
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
