+++
id = "arcl-t-01M5S4SCQN8Q81WS29ZF44G4NA"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Add an implementer role to Pi dispatch"
status = "pending"
priority = "normal"
position = 3
+++

Pi dispatch accepts only the reviewer, verifier, adversarial-reviewer and
reviser roles, so Pi workers cannot run the implement stage. This is the first
gap keeping Pi from running a storm task end to end.

Where the role list lives:

- `packages/pi/extensions/workers.ts`: the `agent` tool's `StringEnum` of roles.
- `crates/fab-cli/src/dispatch.rs`: `ROLES`, the clap `value_parser` on the
  role argument, the token permissions (only `reviser` gets Contents write),
  and the skills copied into the worker's read-only inputs (`review`, `storm`,
  `writing-docs`, plus `revise` and `resync` for the reviser).
- `.github/fabrial-roles.toml`: per-role Pi model and thinking. `fab init`
  installs this file, so a new `[implementer.pi]` entry reaches other
  repositories through `fab update`. Ask the owner which model and thinking
  level to use; there is no default.
- `.claude/agents/implementer.md`: the role text. `fab init` installs it too.
- `docs/pi.md`: the roles, their permissions and their skills.

What the implementer needs that the review roles do not:

- Contents write, to push its branch. Branch PR then opens the draft.
- The `implement` skill, and the `arclightning` skill when the repository
  commits its arcl workspace.
- A worktree the driver creates, as for every role (`--worktree`).

Known obstacles, from `docs/pi.md` and task
`arcl-t-01M5S4SCRGZTS5RNFK41ZT83WN` ("Host-check Git push and Rust gates from
dispatch"):

- Rust gates do not run in the sandbox: the linker cannot find `libiconv`.
  The sandbox's allowed domains (`conf/sandbox/worker.nix`) also omit
  `static.crates.io` and `index.crates.io`, which cargo may need; unverified.
  Until that task lands, an implementer worker cannot run `just check`, so it
  must report a blocker and leave the gates to the driver on the host.
- Git push through dispatch has not been host-checked.
- `arcl` is not in the sandbox. The driver starts and completes the arcl task
  on the host, in the task worktree.

Acceptance criteria:

- [ ] Pi dispatch accepts an `implementer` role, using
      `.claude/agents/implementer.md`, with Contents write and the
      `implement` skill
- [ ] `.github/fabrial-roles.toml` has an owner-chosen implementer model, and
      `fab check` reports no drift
- [ ] A Pi worker can run the implement stage on a small real task and push a
      branch, or reports the gate blocker above without pushing
- [ ] Tests cover the new role, its token permissions and its skills
- [ ] `docs/pi.md` lists the role and its permissions
