+++
id = "arcl-t-01M6H40BDCN7BV7GK62EV0QTF5"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Spike Cursor agents and Grok models as workers"
status = "pending"
priority = "low"
position = 0
+++

Jules is being sunset, so this spike replaces it.

Acceptance criteria:

- [ ] Run one review role on a Cursor agent, through its CLI on a host or a
      cloud agent that pushes a branch, and record what it needs: sign-in,
      sandbox, how fabrial hands it a task, and how fabrial reads the result
- [ ] Run one review role on a Grok model through a harness fabrial already
      drives, such as Pi, and compare its findings with the same pass on the
      default model
- [ ] PLAN.md's **Workers** decision records whether each one is adopted,
      deferred or dropped
