+++
id = "arcl-t-01M5S4SCXJXB2NAAH0D7F68PDN"
phase = "arcl-ph-01M5S4SCHN0SYKWW4XZ5ZF6JET"
title = "Set up the OVH VPS"
status = "cancelled"
priority = "normal"
position = 4
+++


PLAN.md's **Hosting** decision puts the control plane on an OVH VPS. Aim for
8 GB and 4 vCPU, chosen from OVH's live pricing page at deploy time.

Acceptance criteria:

- [ ] Choose the OVH plan
- [ ] Set it up with the control plane's host tools and a Cloudflare Tunnel

Cancelled: fabrial is the engine, not the host (PLAN.md). Hosting, other runner backends, artifact storage and web UIs are outside its scope.
