# Benchmark prompts

The prompts the published versions of the `standard` spec pin (`../specs/`), installed
read-only beside the binaries (`share/benchmarks/prompts/`, every package) and read in
place: nothing is copied into the user's config, and a prompt of your own is any other file.
Each name ends in its version, `<name>-<X.Y>`: a changed text is a new file with a new
version and a new spec version pinning it, never an edit, so an older spec version still
finds the prompts it pinned. The configurator carries no text, only the digest of every
published spec version (`../specs/standard.versions`); each spec pins the sha256 of its
prompts, and a prompt that does not match is refused before anything restarts. Only prompts
a spec pins ship: a prompt no spec pins measures nothing another machine can compare with.

A `.txt` prompt is sent as one user message. A `.json` one is a conversation,
`{"messages": [...], "tools": [...]}` in the OpenAI chat format, sent as given
(`bench::prompt_request`), so the model's own chat template renders it.

## `bench-prompt-43k-1.0.txt`

The default prompt of the live benchmark: **169,289 characters, roughly
40k tokens** (43,730 on Qwen3.8).

It exists because a benchmark ranks settings **in the regime it measured**, and
a prompt of a few hundred characters is a regime nobody works in. On this
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
- **Keep it LF and BOM-free.** `.gitattributes` pins that: a CRLF checkout would
  be a different prompt on a different machine, and its digest would no longer
  match the spec's pin.

Realistic technical prose was chosen over generated filler on purpose: prefill
timing barely cares about content, but decode does, and speculative acceptance
cares a great deal (it is dominated by how predictable the text is). Filler
would benchmark a workload the machine never runs.

## `bench-prompt-110k-1.0.txt`

`bench-prompt-110k-1.0.txt` (404,098 characters, 110k tokens on Qwen3.8) is the same
kind of frozen snapshot, taken on 2026-10-20 and larger: the agent guides, the
per-GPU and per-model references, the build pipeline guide, the configurator's
README and some of its sources, then one question. It measures the regime where a
setting's cost grows with the context, which 43k only hints at. It also holds the
one sanctioned em-dash, quoted inside `AGENTS.md`, and the same three rules apply.

## `bench-prompt-code-1.0.json`

A coding-agent turn (142,602 bytes, ~42.1k tokens with the Qwen3.8 vocabulary, plus
what the chat template adds): the workload of an agent like OpenCode or Claude Code
driving the model, which the two prose prompts do not exercise. It holds:

- four tool definitions (`read_file`, `grep`, `edit_file`, `run_command`), passed as
  the request's `tools`, so the template renders them as it does for a real agent;
- a system message with the agent's instructions and `llama-cpp-config/AGENTS.md`;
- a real task of this repo: the Benchmark tab's Database placeholder shows
  `bench\bench.db` with a Windows backslash on Linux; make it the platform's
  default path, filled in from Rust, with a test;
- five earlier assistant turns, each one tool call, and their results: a grep for
  the literal, `ui/bench_page.slint` lines 520 to 579, `ui/state.slint` lines 555 to
  574, all of `src/gui/bench_tab.rs` and `src/paths.rs`, numbered like `cat -n`.

The model's reply is its next step: reasoning, then a tool call (an `edit_file`
that copies code from the context, which is where an n-gram drafter pays) or an
answer. A tool call streams as `delta.tool_calls`, which the benchmark counts as
the answer phase. At ~42k tokens it sits at the depth of the 43k prose prompt, so
the two differ in content, not in context: the comparison says what code does to
decode and to drafter acceptance.

It is a **frozen snapshot of commit `87cda04`** (2026-10-31), built once by a
script that read those files with `git show 87cda04:<path>`. The same rules as
above apply: never regenerate it when the sources change (a new snapshot is
`bench-prompt-code-1.1.json` and a new spec version), keep it LF and BOM-free.
It contains no em-dash.
