# Hosts

What Claude Code, Codex and Pi read from this repository, and where they
differ. The Claude Code rows were checked here with Claude Code 2.1.309 on
2026-10-23. The Codex and Pi rows come from thunderstorm's
`docs/internal/hosts.md`, checked there with `codex-cli` 0.146.0 and `pi`
0.85.1, and have not yet been checked against this repository.

## Skills

A skill is a directory holding `SKILL.md` with `name` and `description`
frontmatter. All three hosts read that shape, so each skill is written once
under `.claude/skills/`.

| Host        | Reads              | Checked here |
| ----------- | ------------------ | ------------ |
| Claude Code | `.claude/skills/`  | yes          |
| Codex       | `.agents/skills/`  | no           |
| Pi          | `.agents/skills/`  | no           |

`.agents/skills` is a symlink to `.claude/skills`, so the one copy serves all
three. Pi reads a skill under `.agents/skills` only as a subdirectory holding
`SKILL.md`, which is the form used here, and only once the project is trusted.

Claude Code answers `/<name>` for a project skill and passes the rest of the
line as arguments, so no command files are needed. `argument-hint` in the
frontmatter shows what to pass. Codex invokes a skill by name; Pi names one
`/skill:<name>`.

## Agents

| Host        | Reads                    | Format                         |
| ----------- | ------------------------ | ------------------------------ |
| Claude Code | `.claude/agents/*.md`    | Markdown with frontmatter      |
| Codex       | `.codex/agents/*.toml`   | TOML, in a trusted project     |
| Pi          | nothing                  | no subagents; dispatch starts a session per role |

The roles live once, in `.claude/agents/`. On Codex and Pi, the orchestrator
reads a role's Markdown and passes its body as the instructions of a built-in
agent or a dispatched session. Phase 6 of `PLAN.md` ports Pi's dispatch.

A Codex dispatch must set `fork_turns` to `"none"` or a number. A full-history
fork inherits the parent's context and refuses a model override, so the
reviewer would not start fresh.

## Running an agent headless

`claude -p --agent <role> --json-schema <file>` returns the answer in
`structured_output`, but only when the agent's `tools` frontmatter, if set,
lists `StructuredOutput`. Without it, Claude Code 2.1.309 answers in prose and
leaves `structured_output` null, with no error. Nothing here runs agents
headless today; this matters again for `fab run` in phase 5.
