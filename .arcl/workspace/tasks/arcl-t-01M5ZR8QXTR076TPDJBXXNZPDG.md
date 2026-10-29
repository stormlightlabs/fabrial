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
- [ ] Workers on the tmux runner never connect to `fab serve`: it reads their events from dispatch's per-worker record and events under `state_dir`, as `fab worker events` does ([Pi dispatch](../../../docs/pi.md#events-and-steering)), so no worker needs a local port
- [ ] For workers that report over the network, it accepts a worker's events only with that worker's credential, and a test shows an expired or revoked one is refused
