+++
id = "arcl-t-01M5S4SCQN8Q81WS29ZF44G4NA"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Add an implementer role to Pi dispatch"
status = "pending"
priority = "normal"
position = 3
+++

Pi dispatch accepts only the reviewer, verifier, adversarial-reviewer and
reviser roles (`pi/extension.ts`, `crates/fab-cli/src/dispatch.rs`), so Pi
workers cannot run the implement stage.

Acceptance criteria:

- [ ] Pi dispatch accepts an `implementer` role, using
      `.claude/agents/implementer.md`
- [ ] A Pi worker can run the implement stage
- [ ] Tests cover the new role
