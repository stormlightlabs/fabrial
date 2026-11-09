+++
id = "arcl-t-01M6W9F3RGJREBA622MHCZNS8Y"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Bring the docs up to date for v0.9.0"
status = "in_progress"
priority = "high"
position = 0
blocked-by = ["arcl-t-01M6W7PX3ZF0SQ55PT0TA0FAE2", "arcl-t-01M6W87DTBEH4VN7V9A06P6XF8", "arcl-t-01M6W9F3QXHE3KPT0E6H1WSZ07"]
+++

The v0.9.0 work changes what fabrial is and how it ends a task. The docs
should say what ships in that release, with nothing left from the larger
loop.

Acceptance criteria:

- [ ] README, AGENTS.md and the agents section, docs/pi.md, docs/pi-design.md
      and docs/hosts.md describe the implementation loop: it ends at
      `Review finished`, `/storm:ready` and `/storm:done` are the comments
      the gate reads, and nothing names `fab daemon`, `fab serve`,
      `/storm:stop`, `/storm:review` or `/storm:revise`
- [ ] The skills and agents match PLAN.md's decisions, and `fab check`
      reports no drift
- [ ] PLAN.md's **Shipped** list names what v0.9.0 shipped
- [ ] The README documents `fab gates`, `signoff` and `assisted_by`
- [ ] A page covers setting up stormlight-thunderbot for a repository: the
      App, its permissions, the secrets and the ruleset
- [ ] A page walks one task from claim to merge, naming who acts at each
      step
- [ ] The docs describe fabrial as the contract, skills, workflows and `fab`
      commands, and leave worker dispatch, the sandbox and the Pi package to
      the harness
