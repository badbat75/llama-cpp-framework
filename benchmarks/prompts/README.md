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
(`bench::prompt_request`), so the model's own chat template renders it. A message's
content may hold an image as `{"type": "image_file", "file": "<name>", "sha256": "<of
its bytes>"}`, the file beside the prompt: at send time it is checked against that digest,
fitted into the run's image size (the spec's `ImageSize`) and sent as an `image_url` data
URL (`bench::resolve_images`). The image ships as is and the size is a setting, so one
image serves every resolution; the prompt's digest covers the image's, so the spec's pin
covers both.

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

`bench-prompt-110k-1.0.txt` (404,098 characters, 115k tokens on Qwen3.8) is the same
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

Measured cold (`standard-1.0`) the reply turned out to be a short step, ~100 to 200
tokens of reasoning and one `grep` or `read_file`, so it measured a latency more than a
decode. `standard-1.1` runs it only in its `agent` pass, as a step whose prefix is cached
(`Warm = prefix`), and measures the decode on code with `bench-prompt-code-write-1.1.json`.

It is a **frozen snapshot of commit `87cda04`** (2026-10-31), built once by a
script that read those files with `git show 87cda04:<path>`. The same rules as
above apply: never regenerate it when the sources change (a new snapshot is
`bench-prompt-code-1.1.json` and a new spec version), keep it LF and BOM-free.
It contains no em-dash.

## `bench-prompt-code-write-1.1.json`

Writing code (140,517 bytes, ~42.2k tokens on Qwen3.8): the task of
`bench-prompt-code-1.0.json` and every file its agent read, plus the three lookups it
asked for next (a grep for the default-path helpers, `src/bench/db.rs` lines 42 to 52,
`src/bench.rs` lines 1448 to 1453), pasted into one user message under a heading each,
with the same system message minus its line about tools. No tools, no tool turns. It
ends asking for `llama-cpp-config/src/paths.rs` written back in full: a function for the
default benchmark database beside the other path helpers, with a doc comment and a test,
the rest of the file unchanged, in one `rust` block.

The reply is reasoning, then ~3.9k tokens of code, most of it copied from the context
(the case an n-gram drafter is built for): `decode (answer)` is the rate of writing code,
`decode (reasoning)` that of thinking about it. Tried before it was pinned, on Swift-1.5
Qwen3.8-27B: every reply was the whole file with one correct function and test added,
the answer 3.8 to 4.0k tokens each time; the reasoning varied from 3k to 19k tokens, so
a repetition takes 2 to 7 minutes on two R9700s.

A frozen snapshot of commit `87cda04`, like the agent turn; LF, BOM-free, no em-dash.

## `bench-prompt-image-1.0.json`

Image to text: one user message holding a photograph and a request to describe it
(what it shows, how many animals, what they do, the setting and the light). It measures
what the text prompts cannot: the mmproj's image encoder (its prefill share, its device,
`mmproj-offload`) and a short multimodal turn. `standard` runs it only on a preset that
loads an `mmproj` and skips it on the rest, so one spec covers text and vision models.

The image is `bench-image-zebras-1.0.jpg`: the original 6000x4000 JPEG (6.1 MB, no
EXIF, GPS or XMP; only an ICC colour profile, which the decoder ignores), a herd of plains zebras drinking at a waterhole, photo by Rafael Peier on
Unsplash (Unsplash License: free to use, copy and modify). The prompt names it with its
sha256; `standard-1.0` sends it fitted into `ImageSize = 1344x896` (exactly 3:2, the
photo's own ratio, so nothing is cropped or padded; how many image tokens that makes
depends on the model's encoder). Never re-encode or replace the file: a new image is
`bench-image-<name>-<X.Y>.jpg`, a new prompt naming it, and a new spec version.
