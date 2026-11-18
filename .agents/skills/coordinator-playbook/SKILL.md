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
- A second session can own `fe/` only (since 2026-11-14): it works from `fe/briefs/` (the bff API
  contract `api-contract.md`, the slice briefs) and `fe/DESIGN.md`, commits only `fe/`, and sends every
  change outside `fe/` (bff routes, plan entries, other skills) to the coordinator. A user decision it
  relays is recorded with "(relayed by the web UI session)"; when it changes a requirement, confirm it
  with the user here first.
- After a crash, a resume or a compaction, run `ListAgents` for a second copy of your own session
  before resuming agents, and settle with the user which one coordinates (one active session per
  chat); two copies otherwise resume the same subagents and launch duplicate coders on one tree. A
  copy stopped mid-slice leaves its coder's half-written code: brief the next coder to tell the
  tests from the production code in `git diff` first.
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
- The local model (Strata, PARALLEL=4) is shared with the web UI session's OpenCode agents: when both
  sessions ran two or three runs each, the other session's runs timed out at 55 minutes with no work
  done (2026-11-17). Keep one OpenCode run at a time while the other session builds; the reviewer and
  architect are Claude and do not count.
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
