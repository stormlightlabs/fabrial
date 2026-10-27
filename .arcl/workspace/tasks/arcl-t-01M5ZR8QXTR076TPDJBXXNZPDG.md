+++
id = "arcl-t-01M5ZR8QXTR076TPDJBXXNZPDG"
phase = "arcl-ph-01M5S4SCJ1A54Z514HCP508F9X"
title = "Add fab serve for the web UI"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"]
+++

Acceptance criteria:

- [ ] `fab serve` offers the run operations over HTTP, the same ones `fab mcp` exposes
- [ ] It streams run events and PR status over a WebSocket
- [ ] It listens only on the Cloudflare Tunnel and checks the GitHub identity the Worker forwards
