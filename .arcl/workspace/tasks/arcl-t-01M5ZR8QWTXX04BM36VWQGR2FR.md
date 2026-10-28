+++
id = "arcl-t-01M5ZR8QWTXX04BM36VWQGR2FR"
phase = "arcl-ph-01M5S4SCHN0SYKWW4XZ5ZF6JET"
title = "Receive App webhooks for PR status"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5S4SCXJXB2NAAH0D7F68PDN", "arcl-t-01M5ZR8QXTR076TPDJBXXNZPDG"]
+++

Acceptance criteria:

- [ ] GitHub delivers the App's webhooks to the control plane through the Cloudflare Tunnel
- [ ] The control plane accepts a delivery only when its `X-Hub-Signature-256` matches the App's webhook secret, and a test shows a forged delivery changes no state
- [ ] The control plane updates the PR status state, and Pi sessions on any machine read it from `fab serve`
- [ ] Pi falls back to polling `gh` when webhooks stop arriving
