+++
id = "arcl-t-01M5ZR8QVVENQJM8YWKGWBMAB4"
phase = "arcl-ph-01M5S4SCHN0SYKWW4XZ5ZF6JET"
title = "Isolate workers on the VPS and home nodes"
status = "cancelled"
priority = "normal"
position = 0
+++


The local spike wraps whole Pi workers with pinned agent-sandbox.nix. The k3s and exe.dev runners need their own setup.

Acceptance criteria:

- [ ] Decide how a k3s worker and an exe.dev worker are isolated, and record it in PLAN.md
- [ ] Workers on one node cannot read each other's worktree, state or tokens

Cancelled: fabrial is the engine, not the host (PLAN.md). Hosting, other runner backends, artifact storage and web UIs are outside its scope.
