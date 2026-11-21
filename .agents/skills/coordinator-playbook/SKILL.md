---
name: coordinator-playbook
description: What the coordinator has learned about planning, delegating and closing steps. Use when picking a step, briefing an agent or closing a step.
---

Lessons only: the rules are in `AGENTS.md` and `PLAN.md`.

- Agents defined in a new `.claude/agents` directory appear only after a new session starts. If an agent
  type is missing, start one before doing its work yourself.
- Check results by the output in the agents' reports, not their summary. A background command's exit
  code can come from its last, unrelated line, so the output is what counts.
- Run an OpenCode agent as the background command itself (no `nohup ... &`), so its exit is the
  notification. A waiter `until ! pgrep -f "<title>"` never ends: `pgrep -f` matches the waiter's own
  command line, and an hour went by after the agent had finished.
- Say what you expect from a real run before it starts. When the result differs, say so plainly and find
  out why: an unexpected result is often the most useful finding of the run.
- Ask the user for real runs one at a time, with the exact command and the expected result.
- OpenCode's first answer after a file changes can be stale: `opencode debug agents` gave `[]`, and
  `debug config` missed a new file. Query again before believing it.
- `git add X && git commit` commits the whole index, including what an agent staged with `git mv` while
  it works. When agents are running, commit only named paths: `git commit -m ... -- PLAN.md`.
- Another session can be editing the same tree (seen 2026-11-14: core, manager and docs changes not
  from this session's agents, some inside `AGENTS.md` lines this session also changed). Check
  `git status` before committing and stage only this task's hunks: `git diff -U0 <file>`, keep the
  hunks that are yours, `git apply --cached --unidiff-zero`, then a plain `git commit` of the index.
- Since 2026-11-19 (user) one coordinator runs both sides again: the web UI session handed over after
  its visual identity, so this session briefs the `designer` for every slice that changes what users
  see, then the tester and coder on `fe/` as on the crates. The note below is how it worked before.
- Front-end work, from the web UI session's handover (2026-11-19):
  - Chain per web UI slice: designer brief (Claude, Opus; it edits only `fe/briefs/` and `fe/DESIGN.md`),
    tester red, coder green plus `npm --prefix fe run typecheck`, `lint`, `format:check` and `build`
    (`build` does not type-check the tests), tester rerun, then reviewer and designer review in parallel.
    Commit `fe/` by path; the tester writes its playbook under `.agents/` despite the brief.
  - One npm user in `fe/` at a time: never the designer's browsers (UX check) and a Vitest run together.
  - The real service on 8590 is read-only for agents (user): never submit the real sign-in form; a save,
    switch or run verb on it needs the user's go-ahead.
  - A fe scratch copy for mutants needs a sibling `crates` symlink (tests read
    `crates/core/strings/en_US.toml`) and a `node_modules` symlink.
  - Timing and geometry come from the designer's real-browser measurement, not from a reader's
    inference or jsdom (a "cannot happen" 401 on a dirty form was measured happening).
  - A handler that opens or closes something and then moves focus or scrolls commits the DOM first
    (`flushSync`): the tests record the DOM when `focus()` is called.
  - Never accept "mutant equivalent" without the input that shows it.
  - Every fe brief says "always run npm with `--prefix fe` from the repo root": a run from elsewhere left a stray `node_modules/` at the repo root twice (2026-11-20).
  - A shell UI read keyed on the session (the host name) must survive StrictMode's double effect and a new session object for the same user: compare the user when the answer lands, never cancel by effect identity (slice N, 2026-11-20).
  - Templates of every fe prompt used so far: git-ignored `build/oc/`; designer review stubs:
    `build/ux-review/` (`stub9.py`, a per-route 401 switch `/__r401`).
- A second session could own `fe/` only (2026-11-14 to 2026-11-19): it worked from `fe/briefs/` (the bff API
  contract `api-contract.md`, the slice briefs) and `fe/DESIGN.md`, commits only `fe/`, and sends every
  change outside `fe/` (bff routes, plan entries, other skills) to the coordinator. A user decision it
  relays is recorded with "(relayed by the web UI session)"; when it changes a requirement, confirm it
  with the user here first.
- After a crash, a resume or a compaction, run `ListAgents` for a second copy of your own session
  before resuming agents, and settle with the user which one coordinates (one active session per
  chat); two copies otherwise resume the same subagents and launch duplicate coders on one tree. A
  copy stopped mid-slice leaves its coder's half-written code: brief the next coder to tell the
  tests from the production code in `git diff` first.
- **Pace (user, 2026-11-19: "in 2 days only 3 small changes")**: slice 9 took 4 hours and 16 OpenCode
  runs, slice 10a 9 runs for a code move. The user chose two changes:
  - **Bigger slices**: one slice is one useful piece whole (e.g. all of 10b: estimate, manager check and
    real probe in one tester/coder round), not a chain of sub-slices each with its full cycle.
  - **Lighter reruns**: the tester's rerun is incremental; `cargo clean -p` and a full rerun of every
    crate only once, when the slice closes. Mutants only for security code (the manager's checks and
    permissions, the bff's guards and sessions, what root reads); elsewhere the red-before-green proof
    is enough.
- Brief the `architect` (added 2026-11-15) before a step or slice that adds or changes a component, a
  protocol, a trust boundary, an identity or permission rule, or a deployment or packaging shape; its
  design (in `ARCHITECTURE.md`) comes before the tester's tests, and it reviews the coder's result.
- The `reviewer` (added 2026-11-15, user) reads the coder's diff after the coder's green report, in
  parallel with the tester's rerun; a slice is committed only when its report has no open defect.
  Give it the slice's goal and the exact files; it edits nothing.
- OpenCode agents (local model, `--auto`) can wander out of their brief when their context fills:
  one rewrote two `fe/` tests on 2026-11-15 before dying at compaction. Every brief says "do not read
  or write fe/" (and any other folder outside the slice), and after each run check `git status`
  for paths outside the allowed list before reading the report.
- The local model (Strata, PARALLEL=4): up to 3 OpenCode runs at once, 4 only briefly (user,
  2026-11-19, once this session ran both sides); the reviewer, designer and architect are Claude and do
  not count. Count the live runs before launching (`pgrep -fc 'opencode run'`). RAM is the other limit:
  one cargo build at a time (a coder was reaped for memory on 2026-11-19) and one npm user in `fe/`, so
  pair a crate run with an `fe/` run or a docs run, not two crate builds.
- An OpenCode run can lose its task at a compaction: on 2026-11-17 a tester's third compaction summary
  named an unrelated "retry.test.js" brief that appeared nowhere before it, and the run wrote that file
  at the repo root. After each run check `git status` for files outside the slice; brief long runs to
  write their file early, in steps.
- Do not call the architect for a small change the user has already decided, even when it touches a
  trust boundary (2026-11-17: opening the bff's `Listen` to the LAN; the user: "there was no need to
  call the architect for this"). Read the code yourself, decide the minimal rule, and brief the tester.
  Keep the architect for a new component, protocol or permission model.
- An OpenCode brief with backticks or `<...>` inside a double-quoted shell argument is run as command
  substitution: the run dies before opencode starts (no log, exit 1, nothing changed). Write the brief
  to a file with a quoted heredoc (`<<'EOF'`) and pass `"$(cat file)"`.

## An OpenCode run can end without a report

Exit code 0 does not mean the agent finished. Now and then (3 of 185 logs on 2026-11-18) the local model writes a tool call as plain text (`<tool_call><function=bash>...`): OpenCode takes it for the final answer and ends the run, leaving the work half done in the tree. Read the end of the log before trusting a run; if it ends in a raw tool call, rerun the same brief, telling the agent to read the uncommitted diff first and finish it.
- A rewrite of a skill by the local model invents facts (names of files and functions that never existed, a false lesson about the tree): have an independent agent grep every named path and identifier before committing it, and give the correction to a Claude agent.

## The roadmap page is updated at every closed slice

The user asked (2026-11-21) for a roadmap with estimated dates, kept current: https://claude.ai/artifact/9UmGaM7x8X9R34PKZKiK1P (private). After each slice commit, move its card to "Just closed" with the commit hash, put the next ones in "Now", redo the dates from the pace so far and change the "Status as of" line. A new session reads the page with the Artifact tool's `read` (its source is in the old session's scratchpad) and republishes to the same `url`.

Sections mean status (user, 2026-11-21): "Just closed" holds only done cards, newest first; "Now" only
what an agent is working on; "Next" only work not started. Move a card when its status changes, and
update the page in the same turn as the slice's commit, not later.

## Documents kept current at every closed slice (user, 2026-11-21)

Beside the roadmap page: `docs/flyer/product-sheet.html` (the product vision for prospects: change it only
when a capability is added or dropped, it keeps the generic "in development" footnote) and
`docs/architecture/infrastructure.html` and `system.html` (what is built, planned parts labelled). At a
slice close, check whether the slice changed any of them: a new service, account, port, route, method,
package or file goes into the architecture pages (the `architect` updates them, with ARCHITECTURE.md); a
new capability goes into the flyer (the `docs` agent). Say in the commit which of them changed, or none.
Their published previews (private), republished from the repo file after each change: flyer
https://claude.ai/artifact/YWeQJxnxL67EqBDb1jdnYp, infrastructure https://claude.ai/artifact/3CoviYjx7ToXCnaAxACzm9,
system https://claude.ai/artifact/E964mpfjYWoU5jToTUprJ3 (from a new session: publish with `url`).

## Every new requirement updates the plan and the architecture at once (user, 2026-11-21)

When the user states a requirement or changes one, put it in `PLAN.md` in its place in the order (with
the user's date and words) and commit it in the same turn, without asking; recompute the roadmap dates.
When it touches a component, protocol, identity, trust boundary, deployment or packaging shape, brief the
`architect` in the same turn to record it in `ARCHITECTURE.md` (and in `docs/architecture/*.html` once
built), proposed or decided. Tell the user in one line what changed in each.

## Leftover processes after a review (2026-11-21)

A designer review left its stub servers running, so its task showed as running for hours (the user
noticed). Every designer brief says: stop the stubs you started, answer with SendMessage, no
background job to wait on. After a review, `ps -eo pid,etime,cmd | grep stub.py` and stop the
orphans by pid; `TaskStop` an agent whose task is still "waiting on its own background work".

## Deploy after every closed slice

Standing go-ahead (user, 2026-11-22, AGENTS.md "Actions with real effects"): after a slice's commit,
once no agent builds, run `cargo run -q -p xtask -- build framework && cargo run -q -p xtask -- package
framework`, then `cargo run -q -p xtask -- install`; check `systemctl is-active private-ai-manager
private-ai-bff`, `/` 200 and `/api/instances` 401 on :8590, a CSS or route the slice added, and that
Strata on :8580 still answers. Report it in the slice's message.

## Every start updates the plan and the roadmap

When a slice starts (user, 2026-11-22), mark it "In progress" on its PLAN.md item and move its card to
the roadmap's Now in the same turn, then commit and publish; when it closes, the commit removes the
mark and the card moves to Just closed. Run up to three local agents at once: one Rust build, one
`fe/` user, and a third that builds nothing (a review, docs, a brief).
