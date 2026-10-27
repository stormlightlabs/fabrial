+++
id = "arcl-t-01M5ZR8QWTXX04BM36VWQGR2FR"
phase = "arcl-ph-01M5S4SCHN0SYKWW4XZ5ZF6JET"
title = "Receive App webhooks for PR status"
status = "pending"
priority = "normal"
position = 0
+++

Acceptance criteria:

- [ ] The control plane receives the App's webhooks and updates the PR status state the Pi status area reads
- [ ] Polling `gh` remains the fallback when no webhook arrives
