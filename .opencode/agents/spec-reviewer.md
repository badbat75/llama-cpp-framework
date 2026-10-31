---
description: Reviews a proposed change to a versioned benchmark spec (benchmarks/specs/*.ini, its .versions file) or a prompt a spec pins, before it is committed. Give it the diff, or the paths and the base commit. Read-only.
mode: subagent
permission:
  edit: deny
  webfetch: deny
---

Load the bench-spec skill (.agents/skills/bench-spec/SKILL.md) and review the change with
its section 5 ("Reviewing a change"). Do not edit files: report each check and the verdict.
