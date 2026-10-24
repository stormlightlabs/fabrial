# Pi

How fabrial runs on Pi 1.x, where a Pi session orchestrates the `storm`
sequence and each role runs in a Pi session of its own. This is the design for
the phase 6 spike in `PLAN.md`. It was written from the docs, types and code
shipped in `@earendil-works/pi-coding-agent` 1.4.0 and herdr 0.9.5, and has not
been run yet.

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

The same extension guards `bash` against the obvious mistakes: `gh pr merge`,
`gh pr review` with `--approve` or `-a`, and `git push` or `git merge` aimed at
`main`. It blocks with a reason rather than asking, since a headless session
cannot answer. It matches command text, so `$(...)`, a script file or another
spelling gets past it, and it is not what keeps a worker from merging.

What does is the token. A worker's `GH_TOKEN` is never the owner's: it can read
the repository and run the Bot comment workflow, and it cannot approve, merge
or push to `main`. With that token an approval from a worker is not the
owner's, and the ruleset on `main` holds every merge for the owner. Phase 7's
short-lived, repository-scoped tokens are where that token comes from; until
then the spike uses a fine-grained token with only those permissions.

The orchestrator finds skills through `.agents/skills`. A worker, with no
trust, is given the extension with `-e` and each skill its role uses with
`--skill`, both by path into the orchestrator's checkout of `main`, never into
the pull request's worktree, so a branch cannot change the code or the
instructions that review it.

## `fab dispatch`

`fab dispatch <role> --task <file>` runs one role in its own pane, so a person
can watch or stop it and it outlives the orchestrator. It reads the role from
`.claude/agents/<role>.md`. The body becomes the appended system prompt, with
a note that the session has no GitHub MCP tools and reaches GitHub through
`gh`, and that skills are found, not called. The `tools` frontmatter maps to
Pi's names: `Read` to `read`, `Grep` to `grep`, `Glob` to `find` and `ls`,
`Bash` to `bash`, `Write` to `write`, `Edit` to `edit`. `Skill` and the
`mcp__github__*` tools have no Pi tool and are printed, not dropped. The model
and thinking level come from the repository's settings, since the
frontmatter's `model` is a Claude alias.

There are two ways to run the worker, depending on the backend.

### tmux and Zellij

The pane runs the worker headless:

```sh
pi --mode json -p --no-approve -e <extension> --skill <skill>... \
   --session-dir <run dir> --model <model> --thinking <level> \
   --tools <list> --append-system-prompt <role file> -- '<task>'
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
   --thinking <level> --tools <list> --append-system-prompt <role file>`. The
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

## Open questions

- A reviewer keeps `bash`, so dropping `write` and `edit` does not stop it
  editing. Separate worktrees isolate the roles from each other; Pi's
  `sandbox` example (bubblewrap through `@anthropic-ai/sandbox-runtime`) could
  isolate them from the machine.
- Where per-role models live. `.github/fabrial.toml` holds one-line values that
  the workflows read with `sed`, so a role table may belong in its own file.
- Whether the reviser should be an RPC worker, or a herdr agent prompted twice,
  so a second round of findings goes to the same session.
- Whether herdr's interactive worker is close enough to the headless one that
  the `storm` sequence gives the same results on both.
