+++
id = "arcl-t-01M5S4SCVPWT0XNBSGC8RNAC58"
phase = "arcl-ph-01M5S4SCHN0SYKWW4XZ5ZF6JET"
title = "Run fab daemon under systemd"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5S4SCMSGFK66J2R9E22WY6C", "arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"]
+++

Acceptance criteria:

- [ ] `fab daemon` runs under systemd and takes ready tasks
- [ ] It honours a concurrency cap
- [ ] It wakes on App webhooks or polls
