+++
id = "arcl-t-01M5ZR8QXTR076TPDJBXXNZPDG"
phase = "arcl-ph-01M5S4SCJ1A54Z514HCP508F9X"
title = "Add fab serve to offer the run operations over HTTP"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"]
+++

Acceptance criteria:

- [ ] `fab serve` offers the run operations over HTTP, the same ones `fab mcp` exposes
- [ ] It streams run events and PR status over a WebSocket
- [ ] It listens on loopback unless configured otherwise, and serves a request only after the configured identity check passes; a test shows a request that fails the check changes nothing
- [ ] It accepts a worker's events only with that worker's credential, and a test shows an expired or revoked one is refused
