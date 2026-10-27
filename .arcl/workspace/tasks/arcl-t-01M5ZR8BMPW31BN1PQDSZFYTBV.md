+++
id = "arcl-t-01M5ZR8BMPW31BN1PQDSZFYTBV"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Run workers through one runner interface"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5VGBYGVK08V4GFD22ZZD8WS"]
+++

PLAN.md's **Hosting** decision runs workers through one runner interface.

Acceptance criteria:

- [ ] A runner takes a worker spec: role, task file, repository at a pinned SHA, scoped token, model credential and timeout
- [ ] Every runner emits the JSONL progress `fab dispatch` prints today
- [ ] tmux dispatch runs as the first runner
