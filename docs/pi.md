# Pi

How fabrial runs on Pi 1.x, where a Pi session orchestrates the `storm`
sequence and each role runs in a Pi session of its own. This is the design for
the phase 6 spike in `PLAN.md`. The design draws on the docs, types and code
shipped in `@earendil-works/pi-coding-agent` 1.4.0 and herdr 0.9.5. Fresh,
headless Pi sessions reviewed fabrial PRs #18 and #19; `fab dispatch`, pane
backends and worker isolation have not been run.

## What Pi 1.x provides

Pi reads skills from `.agents/skills/<name>/SKILL.md`, searching from the
working directory up to the git root. Its loader follows symlinks
(`dist/core/skills.js`), so fabrial's `.agents/skills -> ../.claude/skills`
link serves it; it does not read `.claude/skills` itself. A skill needs a
`description`, and its `name` falls back to the directory's. `/skill:storm`
forces one.

A Pi package is a `package.json` whose `pi` key lists `extensions`, `skills`
and `prompts`. `pi install git:github.com/stormlightlabs/fabrial@v<version>`
installs one for the user, and `-l` for the project. Pi's own packages go in
`peerDependencies` as `"*"`.

An extension registers a tool with `pi.registerTool(defineTool({ name,
parameters, execute }))`, where `parameters` is a TypeBox schema and
`execute(id, params, signal, onUpdate, ctx)` returns `{ content: [{ type:
"text", text }] }`, calling `onUpdate` for progress. It blocks another tool's
call from `pi.on("tool_call", ...)` by returning `{ block: true, reason }`.
There is no ask result; an interactive session can call `ctx.ui.confirm`, and a
headless one has no UI.

Pi has no subagents. Its `subagent` example extension spawns `pi --mode json
-p --no-session` children with `--model`, `--thinking`, `--tools` and
`--append-system-prompt`, headless and hidden.

In `--mode json`, the answer is the last `message_end` event whose
`message.role` is `assistant`. A failed run can still exit 0, so its
`stopReason` and `errorMessage` are what say it failed. Headless sessions skip
project skills and extensions unless the project is trusted. Trusting a
worktree with `--approve` would also run any `.pi/extensions` code or
`.pi/settings.json` package the pull request's branch adds, so workers run
with `--no-approve` and load what they need by path (`-e` for an extension,
`--skill` for a skill). `pi --mode rpc` keeps a session alive for follow-up prompts, and
`createAgentSession` runs one in-process; neither opens a pane.

## The fabrial Pi package

A `package.json` at the repository root with one extension. The extension
registers an `agent` tool that takes a role and a task, runs `fab dispatch
<role>`, and returns the role's answer. While the role works, it reads the
worker's events as they arrive and passes a line of progress to `onUpdate`.
The `storm` skill tells a Pi orchestrator to use this tool for each fresh
agent.

The extension does not parse shell commands to enforce merge policy. A
worker with `bash` can invoke a script or use another spelling. The
repository's ruleset and isolated worker credentials enforce that policy.

A restricted token alone is not enough on a local machine: a worker running
as the owner can still read the owner's GitHub keyring, SSH keys or other
credentials. Dispatch must give workers a separate user or container with no
access to the owner's home, agent sockets or environment. Only then give the
worker a `GH_TOKEN` that can read the repository and run the Bot comment
workflow, but cannot approve, merge or push to `main`. The ruleset on `main`
still requires the owner. Phase 7 replaces the spike's restricted token with
short-lived, repository-scoped tokens.

The orchestrator finds skills through `.agents/skills`. A worker, with no
trust, is given the extension with `-e` and each skill its role uses with
`--skill`, both by path into the orchestrator's checkout of `main`, never into
the pull request's worktree, so a branch cannot change the code or the
instructions that review it.

## `fab dispatch`

`fab dispatch <role> --task <file>` runs one role in its own pane, so a person
can watch or stop it and it outlives the orchestrator. It reads the role from
`.claude/agents/<role>.md`. It writes the body to a trusted temporary file
for the appended system prompt, with a note that the session has no GitHub
MCP tools and reaches GitHub through `gh`, and that skills are found, not
called. The `tools` frontmatter maps to
Pi's names: `Read` to `read`, `Grep` to `grep`, `Glob` to `find` and `ls`,
`Bash` to `bash`, `Write` to `write`, `Edit` to `edit`. `Skill` and the
`mcp__github__*` tools have no Pi tool and are printed, not dropped.

The model and thinking level come from a roles file of their own,
`.github/fabrial-roles.toml`, since the frontmatter's `model` is a Claude
alias and `.github/fabrial.toml` holds one-line values the workflows read
with `sed`. It has a table per role, with a model and thinking level for each
host:

```toml
[reviewer.pi]
model = "openai/gpt-6-sol"
thinking = "low"

[adversarial-reviewer.pi]
model = "openai/gpt-6.1-sol"
thinking = "low"
```

A role with no entry for the host fails the dispatch with its name, rather
than falling back to a default model.

There are two ways to run the worker, depending on the backend.

### tmux and Zellij

The pane runs the worker headless:

```sh
pi --mode json -p --no-approve -e <extension> --skill <skill>... \
   --session-dir <run dir> --model <model> --thinking <level> \
   --tools <list> --append-system-prompt <role body file> -- '<task>'
```

in the worktree the run names, with stdout going to `<run dir>/events.jsonl`
and the exit status to `<run dir>/status`. `fab dispatch` waits for the status
file, prints the final assistant message, and exits non-zero when `stopReason`
or `errorMessage` says the run failed. It also gives up after a timeout (30
minutes unless the role sets one), and stops waiting when the pane is gone
without a status: in tmux it asks whether the pane still exists, and in Zellij
it relies on the timeout. thunderstorm's `tstorm dispatch` does all of this in
Go and is the reference.

### herdr

[herdr](https://github.com/ogulcancelik/herdr) 0.9.5 is a multiplexer that
tracks each agent's state, and it knows Pi. Its Pi integration reports state
only from Pi's TUI, so a headless `--mode json` worker would always read as
idle there. With herdr, the worker runs as an interactive Pi instead:

1. `herdr pane split --current --direction right --cwd <worktree> --no-focus`
   from the orchestrator's pane, or `herdr workspace create` when
   `HERDR_ENV` is unset, reading the pane's ID from the JSON reply.
2. `herdr agent start <name> --kind pi --pane <id> -- --no-approve -e
   <extension> --skill <skill>... --session-dir <run dir> --model <model>
   --thinking <level> --tools <list> --append-system-prompt <role body file>`. The
   agent name is short and plain (`fab-<role>-<n>`, matching herdr's
   `[a-z][a-z0-9_-]{0,31}`); the run directory is a separate path.
3. `herdr agent prompt <name> "<task>" --wait --until idle --until done
   --timeout <ms>`. Without the `--until` flags it returns at the first
   `blocked`, which is not an answer. A `blocked` state goes to the operator,
   and `fab dispatch` waits again once it clears.
4. The final assistant message is read from the session file under the run
   directory, since a TUI's screen is not a record.
5. `herdr pane close <id>` on the pane `fab dispatch` opened, since an
   interactive Pi never exits on its own.

The person watching sees a live Pi with herdr's working, blocked and done
marks, and can step in. herdr does not autostart a server, does not report a
pane's exit status, and removes a pane when its process exits, so
`fab dispatch` checks for a running server first, treats a `pane.exited` or
`pane.closed` event for its pane as a failed run, and keeps its own record in
the run directory.

## Local spike and cleanup

Land the work in separate pull requests. First, prove `fab dispatch` for the
four review roles and the Pi package's `agent` tool on one real pull request.
A fresh worker gets only the role instructions and skills from a trusted
checkout, not the pull request's branch or the orchestrator's history. The
standard reviewer uses GPT-6 Sol and the adversarial reviewer uses GPT-6.1 Sol. Fail the
run if the final JSON event reports an error, a worker stops or a comment does
not appear; never post `Review finished` after a partial run.

Next, let Claude Code call the same dispatch command to start a Pi worker.
The result is the role's answer and run status, not a shared conversation.
Keep PR and CI status in the fabrial Pi package's status area as a separate
change; [the status area design](pi-statusline.md) records its fields and
refresh behavior. The dispatch path must work without a TUI.

Each task gets a worktree beside the primary checkout. Keep Cargo targets
per worktree to avoid concurrent builds fighting over one target directory;
use a compiler cache to share compiled work. After GitHub reports the PR
merged, compare its recorded head SHA with the worktree's branch tip and
confirm the worktree is clean. If either differs, stop: there may be local
work to keep. Otherwise remove it with `git worktree remove`, then delete
its local branch. A squash merge may require `git branch -D` after those
checks. Do not delete an open PR's worktree.

## Changing orchestrator sessions

A Pi session is not the task's durable record. The existing `handoff` skill
records unfinished work on the arcl task, issue or PR; no second handoff skill
is needed. Until `fab` owns durable run state, the next session reconstructs
progress from that note, git and the PR's comments and checks.

Before clearing context, ask for `/skill:handoff <task-or-pr>`. The orchestrator
waits for workers to finish or stops them, confirms the handoff was stored,
and reports its link and a pickup command. If a worker must keep running,
record its identity, run directory, worktree and how to check or stop it.
Do not start a replacement worker while its predecessor may still be working.

Use `/new` for a fresh Pi conversation, or exit and start `pi` again. Then
send `/skill:handoff pick-up <task-or-pr>`. Pickup checks the current PR head,
review results, pending checks and worker state before continuing the next
unfinished step. It does not repeat completed passes solely because the
orchestrator changed. A Pi `/resume` restores a saved conversation, while
`/compact` reduces its context; neither replaces a durable handoff.

If the task is finished, there is nothing to hand off. After a merge, refresh
the clean primary `main` checkout and remove the old task worktree using the
checks above. Start the next task with `/skill:storm <task description>` in a
fresh session. Creating the new task worktree is the orchestrator's job.
Do not leave a finished task's PR as the pickup target for a new task.

The skills can check PRs while the session runs; they are not a background
watcher after it exits. A PR waiting for the owner's approval may be left with
a clear report. If checks or review steps remain, write a handoff before
leaving. Autonomous watching across sessions belongs to the later daemon.

## Open questions

- A reviewer keeps `bash`, so dropping `write` and `edit` does not stop it
  editing. Separate worktrees isolate the roles from each other; Pi's
  `sandbox` example (bubblewrap through `@anthropic-ai/sandbox-runtime`) could
  isolate them from the machine.
- Whether the reviser should be an RPC worker, or a herdr agent prompted twice,
  so a second round of findings goes to the same session.
- Whether herdr's interactive worker is close enough to the headless one that
  the `storm` sequence gives the same results on both.
