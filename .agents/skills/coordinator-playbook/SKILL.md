---
name: coordinator-playbook
description: How the coordinator plans, delegates and closes steps in this project, and the pitfalls met so far. Use when picking a step, briefing an agent or closing a step.
---

Rules are in `AGENTS.md` and `PLAN.md`. Format of this file: `AGENTS.md`, under "Skills".

## How to

### Briefing a local agent

- Write the brief to a file with a quoted heredoc (`<<'EOF'`) and pass `"$(cat file)"`.
- Start every brief with the hard limits: which files it may edit, and "do not read or write fe/" (and
  any other folder outside the slice).
- Brief long runs to write their file early, in steps.
- Every fe brief says "always run npm with `--prefix fe` from the repo root".
- The coder's fe report needs `npm --prefix fe run typecheck` (with the linter and format check):
  `npm --prefix fe run build` does not type-check the test files.
- Every designer brief says: stop the stubs you started, answer with SendMessage, no background job to
  wait on.
- Say what you expect from a real run before it starts. When the result differs, say so plainly and find
  out why: an unexpected result is often the most useful finding of the run.
- Ask the user for real runs one at a time, with the exact command and the expected result.
- Check results by the output in the agents' reports, not their summary, and read the end of the log
  before trusting a run.
- Run an OpenCode agent as the background command itself (no `nohup ... &`), so its exit is the
  notification.
- After each run check `git status` for paths outside the allowed list before reading the report.
- Give the coder one test file or one criterion group per run, tell it to run the tests and read only
  failing assertions, and stop a run at 2 hours, relaunching from what it saved.
- Bigger slices (user, 2026-11-19): one slice is one useful piece whole (a model profile's estimate,
  manager check and real probe in one tester/coder round), not a chain of sub-slices each with its full
  cycle.
- Lighter reruns (user, 2026-11-19): the tester's rerun is incremental; `cargo clean -p` and a full rerun
  of every crate only once, when the slice closes.
- Mutants only for security code (the manager's checks and permissions, the bff's guards and sessions,
  what root reads); elsewhere the red-before-green proof is enough. Never accept "mutant equivalent"
  without the input that shows it.
- A fe scratch copy for mutants needs a sibling `crates` symlink (tests read
  `crates/core/strings/en_US.toml`) and a `node_modules` symlink.
- Templates of every fe prompt used so far: git-ignored `build/oc/`; designer review stubs:
  `build/ux-review/` (`stub9.py`, a per-route 401 switch `/__r401`).
- Do not call the architect for a small change the user has already decided, even when it touches a trust
  boundary: read the code yourself, decide the minimal rule, and brief the tester. Keep the architect for
  a new component, protocol or permission model.
- Timing and geometry of a fe slice come from the designer's real-browser measurement, not from a
  reader's inference or jsdom.

### Concurrency

- The local model server, Strata on :8580 (`PARALLEL=4`): up to 3 OpenCode runs at once, 4 only briefly (user,
  2026-11-19, once this session ran both sides); the reviewer, designer and architect are Claude and do
  not count. Count the live runs before launching (`pgrep -fc 'opencode run'`).
- RAM is the other limit: one cargo build at a time, and one npm user in `fe/`, so pair a crate run with
  an `fe/` run or a docs run, not two crate builds. Never the designer's browsers (the UX check) and a
  Vitest run together.
- Run up to three local agents at once: one Rust build, one `fe/` user, and a third that builds nothing
  (a review, docs, a brief).

### The tree and the services

- When agents are running, commit only named paths: `git commit -m ... -- PLAN.md`, and `fe/` by path.
- Check `git status` before committing: another session can be editing the same tree. Stage only this
  task's hunks: `git diff -U0 <file>`, keep the hunks that are yours,
  `git apply --cached --unidiff-zero`, then a plain `git commit` of the index.
- The real service on 8590 is read-only for agents (user): never submit the real sign-in form; a save,
  switch or run verb on it needs the user's go-ahead.
- After a review, `ps -eo pid,etime,cmd | grep stub.py` and stop the orphan stubs by pid; `TaskStop` an
  agent whose task is still "waiting on its own background work".
- After a crash, a resume or a compaction, run `ListAgents` for a second copy of your own session before
  resuming agents, and settle with the user which one coordinates (one active session per chat). A copy
  stopped mid-slice leaves its coder's half-written code: brief the next coder to tell the tests from the
  production code in `git diff` first.

### Closing a slice

- Deploy (the standing go-ahead, `AGENTS.md` "Actions with real effects"): once no agent builds, run
  `xtask build framework`, `xtask package framework` and `xtask install` with the framework's four
  `.deb` files as arguments (a bare `xtask install` reinstalls llama.cpp too, whose prerm stops every
  running instance: user, 2026-11-24, "the running instances should not be touched"), then check
  `systemctl is-active private-ai-manager private-ai-bff`, `/` 200 and `/api/instances` 401 on :8590, a
  CSS rule or route the slice added, and that Strata on :8580 still answers. Report it in the slice's
  message.
- When a slice starts, mark it "In progress" on its PLAN.md item and move its card to the roadmap's Now
  in the same turn, then commit and publish; when it closes, the commit removes the mark and the card
  moves to Just closed.
- The roadmap page (user, 2026-11-21): https://claude.ai/artifact/9UmGaM7x8X9R34PKZKiK1P (private).
  After each slice commit, move its card to "Just closed" with the commit hash, put the next ones in
  "Now", redo the dates from the pace so far and change the "Status as of" line. A new session reads the
  page with the Artifact tool's `read` (its source is in the old session's scratchpad) and republishes to
  the same `url`.
- Sections mean status (user, 2026-11-21): "Just closed" only done cards, newest first; "Now" only what
  an agent is working on; "Next" only work not started. When a card moves, update EVERY place that names
  its state: the card's chip, the "Now" heading line and the slice's other cards. At each step close, or
  when the user asks, audit the whole page against `PLAN.md` and `git log` (a read-only agent) and fix
  every gap.
- Documents kept current at every closed slice, beside the roadmap: `docs/flyer/product-sheet.html` (the
  product vision for prospects: change it only when a capability is added or dropped, it keeps the
  generic "in development" footnote) and `docs/architecture/infrastructure.html` and `system.html` (the
  TARGET architecture: every accepted design drawn as the system, a short "Built today" note under each
  figure, only proposed designs dashed; user, 2026-11-22). A new service, account, port, route, method,
  package or file goes into the architecture pages (the `architect`, with `ARCHITECTURE.md`); a new
  capability goes into the flyer (the `docs` agent). Say in the commit which of them changed, or none.
- Their published previews (private), republished from the repo file after each change: flyer
  https://claude.ai/artifact/YWeQJxnxL67EqBDb1jdnYp, infrastructure
  https://claude.ai/artifact/3CoviYjx7ToXCnaAxACzm9, system
  https://claude.ai/artifact/E964mpfjYWoU5jToTUprJ3 (from a new session: publish with `url`). These
  previews are the only rendered copy the user reads (the docs site publishes only `docs/*.md`:
  `architecture/*.html` is 404 there), so republish them in the same turn as the commit that changes the
  file.
- Every decision the architect writes or the user takes (a ruling inside a slice included) goes into
  `PLAN.md` (the step it changes, its order, its open questions) and is committed with `ARCHITECTURE.md`
  in the same turn; then the `docs` agent updates every page that describes the changed design
  (`docs/architecture/*.html`, `AGENTS.md`, the READMEs), marking proposed designs as proposed and built
  facts only as built (user, 2026-11-22).
- When the user states a requirement or changes one, put it in `PLAN.md` in its place in the order (with
  the user's date and words) and commit it in the same turn, without asking; recompute the roadmap dates.
  When it touches a component, protocol, identity, trust boundary, deployment or packaging shape, brief
  the `architect` in the same turn to record it in `ARCHITECTURE.md` (and in `docs/architecture/*.html`
  once built). Tell the user in one line what changed in each.
- Never block the work on a question: a questionnaire waits for the user and stops the turn. Launch the
  next slices first, then ask; a go-ahead that only gates an optional check goes in the slice's message
  as a note, not as a blocking question, and the work carries on without it.

## Pitfalls

- The liveness check sees only OpenCode runs and stubs in the shell; a Claude subagent (designer, architect, reviewer) is invisible to `ps` and to TaskList. Its notification "is waiting on its own background work" means a command of its own still runs: note the time and, at the next check, stop it with TaskStop if it has not reported within 30 minutes (2026-11-24: a designer sat 11 h 50 min on a hung command, unnoticed).
- Agents defined in a new `.claude/agents` directory appear only after a new session starts: if an agent
  type is missing, start one before doing its work yourself.
- OpenCode's first answer after a file changes can be stale: `opencode debug agents` gave `[]`, and
  `debug config` missed a new file. Query again before believing it.
- An OpenCode brief with backticks or `<...>` inside a double-quoted shell argument is run as command
  substitution: the run dies before opencode starts (no log, exit 1, nothing changed).
- A waiter `until ! pgrep -f "<title>"` never ends: `pgrep -f` matches the waiter's own command line, and
  an hour went by after the agent had finished.
- Exit code 0 does not mean the agent finished: now and then the local model writes a tool call as plain
  text (`<tool_call><function=bash>...`), OpenCode takes it for the final answer and ends the run,
  leaving the work half done in the tree. Rerun the same brief, telling the agent to read the
  uncommitted diff first and finish it.
- A background command's exit code can come from its last, unrelated line, so the output is what counts.
- OpenCode agents on the local model wander out of their brief when their context fills (one rewrote two
  `fe/` tests before dying at compaction), and a run can lose its task at a compaction and write an
  unrelated file at the repo root.
- `git add X && git commit` commits the whole index, including what an agent staged with `git mv` while
  it works.
- A rewrite of a skill by the local model invents facts (names of files and functions that never existed,
  a false lesson about the tree): have an independent agent grep every named path and identifier before
  committing it, and give the correction to a Claude agent.
- A designer review left its stub servers running once, so its task showed as running for hours.
- A run from outside the repo root left a stray `node_modules/` at the repo root twice.
- A coder was reaped for memory when two cargo builds ran at once.
