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
- Run an OpenCode agent, or a chain of them, as the Bash tool's `run_in_background` command itself:
  never in the foreground (a 40-minute run leaves the session deaf to the user and to peers) and never
  as a `( ... ) &` or `nohup ... &` inside a foreground call (no exit notification). Its exit is the
  notification (IT-Admin, for the owner, 2026-11-26).
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
- Launch every OpenCode agent with plain `opencode run` (owner rule, 2026-11-25): never `--standalone`,
  whose private server writes the same `~/.local/share/opencode/opencode.db` as the background service
  (`opencode serve --service`), and the two collide (`database is locked`, seen as `Error: Failed to
  execute statement`). A run then lives in the service and keeps editing files after its client is killed
  (bb-auth, 2026-11-24), so to stop a stalled one: `opencode api session.active` (running ids),
  `opencode api session.get --param sessionID=<id>` (its title), then `opencode api session.interrupt
  --param sessionID=<id> --param resume=false`, and kill the client; after a chain, interrupt any of
  yours the service still lists as running. The service log stamps time in UTC; a store error while
  another `--standalone` run is alive is that run's lock: wait for it to exit, then relaunch. Service log: `~/.local/share/opencode/log/opencode.log`.
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

- An agent's `python3 - <<EOF` rewrite script can outlive its opencode run, spinning at 100% CPU with no reader (regex backtracking): the liveness check lists `ps -eo pid,etime,pcpu,cmd | grep 'python3 -$'` too, and kills such orphans by PID once their run has ended.

- `xtask build framework` runs `npm ci` and `npm run build` in `fe/`, so a deploy while the designer's UX check or a Vitest run is going pulls `fe/node_modules` and `fe/dist` from under it: deploy only when no web UI agent is running (2026-11-26).
- An interrupted `xtask install` of llama.cpp leaves dpkg half-configured and `apt-get` then refuses on the `Breaks:` between the engine and its runtimes; `sudo dpkg -i` of the engine, rocm and cuda debs together repairs it (2026-11-26). Before installing a new engine, check `ldd /usr/bin/llama-server` and the ggml libraries for "not found" in the staged tree.
- An OpenCode run that ends its report and then prints `waiting for 1 background job` never exits (2026-11-25, twice): read its report from the log, kill the run by PID and start the next agent of the chain by hand (the `&&` chain stops on the kill). Briefs say: every command in the foreground.

- A designer review that names a CSS fix as a guess ("try X", "about 7rem"): ask the designer to measure it first by injecting it with `page.addStyleTag` in its three browsers, then brief the tester with the measured CSS (2026-11-24: 22D4 D5 to D8 took four tester-coder rounds, D7's untried `contain: inline-size` broke the text).

- A slice that adds a server or model profile key: before briefing, list every surface a user sets it from (the web UI form, the GUI configurator, `pai`) and queue each one missing in PLAN (2026-11-24: GT1 built `RocmHostMemoryGtt` without the Server page field; the user had to ask).

- `server@<id>.ini` can hold `OpencodeApiKey`: read it with `grep -vE "Key|Token|Secret"` (or key-scoped), never whole (2026-11-24: a full read showed the key in the session output).

- A local-model coder can loop reading (the same brief section and greps again and again after a context compaction) while its log keeps growing: at each liveness check past 30 minutes look at `git diff --stat` of the production files it was given; unchanged means stuck, so stop it and relaunch on a narrower brief (one area of files, exact paths and lines, first step "run these red tests, then edit this file"). 2026-11-24: 22D4's coder read for 1 h 40 min and only added strings.

- Trouble with the local model (Strata :8580 stalls, out-of-memory, cut OpenCode runs) goes as a message, facts only, to the Claude agent working in `/mnt/data/git/strata-framework` (ListAgents), never a restart of Strata without the user (user, 2026-11-24).

- A layout request ("a minus beside the status") can be read two ways; before briefing the tester, show the user the designer's layout as an ASCII preview in AskUserQuestion and build only on a yes (2026-11-24: 22D2 put the minus on the profile page, the user meant the list rows, a whole slice redone as 22D4).
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
- A tester proving a flake fix may start CPU burners (16 `while :; do :; done` loops, its playbook's load harness) and leave them spinning beside Strata: say "no synthetic load" in follow-up briefs, and on a liveness check stop leftover burners by PID.
- An agent may launch `find /` and leave it hanging in D state for many minutes: check `pgrep -x find` on liveness checks and stop it by PID; brief docs runs with the source URL so they need not search the disk.
- A wait loop that tests `pgrep -f "<titles>"` matches its own shell, whose command line holds the same text, and waits forever: wait on the chain's task notification, or match with `pgrep -f "^opencode run.*title X"`.
- A run can end with `Error: Failed to execute statement` (the store locked, from `--standalone` writers): the work so far is in the tree; relaunch the same brief with a CONTINUATION note.
- An `opencode run` whose log has not grown in 15 minutes and that has no child process (only its `opencode-pty` daemon) is waiting on a model request that never comes back, while Strata serves the other agents (seen twice on 2026-11-28; the request never reached Strata). Stop it by PID with its children, and restart it with a brief to finish from its partial edits.
- With several chains in one tree, one agent's `git checkout`/`restore`/`stash` of a shared file wipes another's uncommitted slice (IP6c lost its page on 2026-11-28, cause not traced). Every brief for a parallel chain says: never run `git checkout`, `git restore`, `git stash` or `git reset`.
