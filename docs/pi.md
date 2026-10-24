# Pi

How fabrial runs on Pi 1.x, where a Pi session orchestrates the `storm`
sequence and each role runs in a Pi session of its own. This is the design for
the phase 6 spike in `PLAN.md`. It was written from the docs and types shipped
in `@earendil-works/pi-coding-agent` 1.4.0 and has not been run yet.

## What Pi 1.x provides

- **Skills.** Pi reads `.agents/skills/<name>/SKILL.md`, searching from the
  working directory up to the git root, and follows symlinks. fabrial's
  `.agents/skills -> ../.claude/skills` link already serves it. Pi does not
  read `.claude/skills` itself. A skill needs `name` and `description`;
  `/skill:storm` forces one.
- **Packages.** A `package.json` whose `pi` key lists `extensions`, `skills`
  and `prompts`. `pi install git:github.com/stormlightlabs/fabrial@v<version>`
  installs one for the user, and `-l` for the project. Pi's own packages go in
  `peerDependencies` as `"*"`.
- **Extension tools.** `pi.registerTool(defineTool({ name, parameters,
  execute }))` with TypeBox parameters. `execute(id, params, signal, onUpdate,
  ctx)` returns `{ content: [{ type: "text", text }] }` and can stream progress
  through `onUpdate`.
- **Guards.** `pi.on("tool_call", ...)` returns `{ block: true, reason }`.
  There is no ask result; an interactive session can call `ctx.ui.confirm`,
  and a headless one has no UI.
- **No subagents.** Pi has no built-in task tool. Its `subagent` example
  extension spawns `pi --mode json -p --no-session` children with `--model`,
  `--thinking`, `--tools` and `--append-system-prompt`, headless and hidden.
- **Headless runs.** `--mode json` writes JSONL events. The answer is the last
  `message_end` with `message.role == "assistant"`; its `stopReason` and
  `errorMessage` say whether it failed, because a failed run in JSON mode can
  still exit 0. Workers need `--approve` to load project skills and
  extensions, which headless modes skip without trust.
- **RPC and SDK.** `pi --mode rpc` keeps a worker alive for follow-up prompts;
  `createAgentSession` runs one in-process. Neither opens a pane.

## The fabrial Pi package

A `package.json` at the repository root, with one extension:

- **`agent` tool.** Takes a role and a task, runs `fab dispatch <role>`, streams
  its progress through `onUpdate`, and returns the role's answer. The `storm`
  skill already says to run each pass in a fresh agent; on Pi this tool is that
  agent.
- **Guard.** Blocks `bash` calls that merge or approve a pull request (`gh pr
  merge`, `gh pr review`, the merge API) or push to `main`, with a reason. It
  blocks rather than asks, so a headless orchestrator cannot wait on a prompt.

Skills come from the repository through `.agents/skills`, so the package does
not ship them.

## `fab dispatch`

`fab dispatch <role> --task <file>` runs one role in its own tmux window or
Zellij tab, so each worker can be watched and stopped, and outlives the
orchestrator. It:

1. Reads the role from `.claude/agents/<role>.md`: the body becomes the
   appended system prompt, and the `tools` frontmatter maps to Pi's names
   (`Read` to `read`, `Grep` to `grep`, `Glob` to `find` and `ls`, `Bash` to
   `bash`, `Write` to `write`, `Edit` to `edit`). Names with no Pi tool are
   printed, not dropped.
2. Takes the model and thinking level for the role from the repository's
   settings, since the frontmatter's `model` is a Claude alias.
3. Starts `pi --mode json -p --approve --session-dir <run> --model <model>
   --thinking <level> --tools <list> --append-system-prompt <role file>
   '<task>'` in the pane, in the worktree the run names, with events going to
   `<run>/events.jsonl` and the exit status to `<run>/status`.
4. Waits for the status, then prints the final assistant message, and exits
   non-zero when `stopReason` or `errorMessage` says the run failed.

thunderstorm's `tstorm dispatch` does the same in Go and is the reference.

## Open questions

- A reviewer keeps `bash`, so dropping `write` and `edit` does not stop it
  editing. Separate worktrees isolate the roles from each other; Pi's
  `sandbox` example (bubblewrap through `@anthropic-ai/sandbox-runtime`) could
  isolate them from the machine.
- Where per-role models live. `.github/fabrial.toml` holds one-line values that
  the workflows read with `sed`, so a role table may belong in its own file.
- Whether the reviser should be an RPC worker, so a second round of findings
  goes to the same session.
