---
name: coordinator-playbook
description: What the coordinator has learned about planning, delegating and closing steps. Use when picking a step, briefing an agent or closing a step.
---

Lessons only: the rules are in `AGENTS.md` and `PLAN.md`.

- Agents defined in a new `.claude/agents` directory appear only after a new session starts. If an agent
  type is missing, start one before doing its work yourself.
- Check results by the output in the agents' reports, not their summary. A background command's exit
  code can come from its last, unrelated line, so the output is what counts.
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
