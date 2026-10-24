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
project skills and extensions unless the project is trusted, so a worker needs
`--approve`. `pi --mode rpc` keeps a session alive for follow-up prompts, and
`createAgentSession` runs one in-process; neither opens a pane.

## The fabrial Pi package

A `package.json` at the repository root with one extension. The extension
registers an `agent` tool that takes a role and a task, runs `fab dispatch
<role>`, and returns the role's answer. While the role works, it reads the
worker's events as they arrive and passes a line of progress to `onUpdate`.
The `storm` skill tells a Pi orchestrator to use this tool for each fresh
agent.

The same extension guards `bash`. It splits a command on `;`, `&&`, `||`, `|`
and newlines, looks inside `sh -c` and `bash -c` strings, skips leading
`env`, `sudo` and variable assignments, and blocks a segment that runs any of:

- `gh pr merge`, or `gh pr review` with `--approve`;
- `gh api` against a pull request's `merge` endpoint, or a review with
  `APPROVE`;
- `git push` naming `main` as the destination, or with no refspec while
  `main` is checked out;
- `git merge` while `main` is checked out.

It blocks with a reason rather than asking, since a headless session cannot
answer. Because the extension comes from a user install, the orchestrator and
every worker `fab dispatch` starts load it. The guard is a second line: the
ruleset on `main` still holds every merge for the owner's approval.

Skills come from the repository through `.agents/skills`, so the package does
not ship them.

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
pi --mode json -p --approve --session-dir <run> --model <model> \
   --thinking <level> --tools <list> --append-system-prompt <role file> \
   -- '<task>'
```

in the worktree the run names, with stdout going to `<run>/events.jsonl` and
the exit status to `<run>/status`. `fab dispatch` waits for the status file,
prints the final assistant message, and exits non-zero when `stopReason` or
`errorMessage` says the run failed. thunderstorm's `tstorm dispatch` does the
same in Go and is the reference.

### herdr

[herdr](https://github.com/ogulcancelik/herdr) 0.9.5 is a multiplexer that
tracks each agent's state, and it knows Pi. Its Pi integration reports state
only from Pi's TUI, so a headless `--mode json` worker would always read as
idle there. With herdr, the worker runs as an interactive Pi instead:

1. `herdr pane split --current --direction right --cwd <worktree> --no-focus`
   from the orchestrator's pane, or `herdr workspace create` when
   `HERDR_ENV` is unset, reading the pane's ID from the JSON reply.
2. `herdr agent start <run> --kind pi --pane <id> -- --approve --session-dir
   <run> --model <model> --thinking <level> --tools <list>
   --append-system-prompt <role file>`.
3. `herdr agent prompt <run> "<task>" --wait --timeout <ms>`, which returns
   when herdr sees Pi finish; a `blocked` state goes to the operator, not to
   the orchestrator.
4. The final assistant message is read from the session file under `<run>`,
   since a TUI's screen is not a record.

The person watching sees a live Pi with herdr's working, blocked and done
marks, and can step in. herdr does not autostart a server, does not report a
pane's exit status, and removes a pane when its process exits, so
`fab dispatch` checks for a running server first and keeps its own record in
`<run>`.

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
