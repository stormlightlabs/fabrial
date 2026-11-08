+++
id = "arcl-t-01M6W87DTBEH4VN7V9A06P6XF8"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Stop the Pi package starting turns on pull request events"
status = "pending"
priority = "high"
position = 0
+++

PLAN.md's **fabrial is the implementation loop** decision: the loop never
takes its input from a GitHub event. The Pi package's PR status still starts
an orchestrator turn on a merge, a closure, `/storm:stop`, `/storm:ready`, a
failed check or a requested change (docs/pi.md, **Pull request status**).

Acceptance criteria:

- [ ] The Pi package's PR status shows those changes in the status area and
      starts no orchestrator turn for any of them
- [ ] docs/pi.md and docs/pi-design.md describe the status as something the
      person reads, and say acting on it belongs to the person or a host
- [ ] A test shows a failed check reaching the status area without a turn
