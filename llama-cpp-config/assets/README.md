# Shipped assets

Files compiled into `llama-cpp-config.exe` with `include_str!` and written to
`%LOCALAPPDATA%\llama.cpp\config\` the first time the Benchmark tab runs. They
are seeds: an existing file on disk is never overwritten, so a user's edits
survive every upgrade. See `src\bench.rs` for why the installer does not place
them in `bin\` instead.

## `bench-prompt-long.txt`

The long-context prompt for the live benchmark: **169,289 characters, roughly
40k tokens**, seeded as `config\bench-prompt-long.txt` beside the short default.

It exists because a benchmark ranks settings **in the regime it measured**, and
the default prompt is 230 characters, i.e. a regime nobody works in. On this
machine a real agentic session sits at 40k tokens of context, where the same
preset that benchmarks at 57 t/s serves at 24 to 40, and, more importantly,
where the *ranking* of a knob can move: at depth a verify pass costs far more
relative to a draft iteration, so `spec-draft-n-max` peaks at 5 near the start
of a conversation and at 3 at 43k. A sweep run only at 2k answers a question
about a machine nobody is using.

Its content is a **frozen snapshot** of this repo's `AGENTS.md` and
`llama-cpp-config\README.md` as they stood on 2026-09-01, followed by a question
about them. Three things follow, and each one is a way an obvious-looking edit
would break something:

- **Do not regenerate it when the docs change.** The duplication is the point.
  A benchmark prompt that tracks a moving file is a prompt that silently differs
  between two runs, which is exactly the failure the report's sha256 exists to
  catch. Every measurement recorded against this file stops being comparable the
  moment it is refreshed, so refresh it only as a deliberate, announced break,
  and never as tidying.
- **Do not "fix" the em-dash in it.** The repo bans `—` in prose; this file
  contains exactly one, inside the sentence in `AGENTS.md` that documents the
  sanctioned exception. It is a verbatim quote of a legal use, and editing it
  would change the prompt (and its digest) to remove nothing.
- **Keep it LF and BOM-free.** `.gitattributes` pins that. It is compiled in as
  bytes, so a CRLF checkout would be a different prompt on a different machine.

Realistic technical prose was chosen over generated filler on purpose: prefill
timing barely cares about content, but decode does, and speculative acceptance
cares a great deal (it is dominated by how predictable the text is). Filler
would benchmark a workload the machine never runs.
