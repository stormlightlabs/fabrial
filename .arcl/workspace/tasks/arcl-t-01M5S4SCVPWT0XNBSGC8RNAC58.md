+++
id = "arcl-t-01M5S4SCVPWT0XNBSGC8RNAC58"
phase = "arcl-ph-01M5S4SCHN0SYKWW4XZ5ZF6JET"
title = "Run fab daemon as a service"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5S4SCN5MAH0WA63X7575DKX", "arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"]
+++

Acceptance criteria:

- [ ] `fab daemon` runs as a long-lived service and takes ready tasks; a systemd unit is the documented example, and any service manager works
- [ ] It honours a concurrency cap
- [ ] It polls GitHub for work
