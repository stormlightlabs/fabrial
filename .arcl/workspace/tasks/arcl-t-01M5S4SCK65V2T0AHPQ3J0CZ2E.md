+++
id = "arcl-t-01M5S4SCK65V2T0AHPQ3J0CZ2E"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Owner: release v0.2.0 and update arclightning"
status = "pending"
priority = "normal"
position = 1
+++

Owner action. The owner runs the Release workflow for the first release,
`v0.2.0`; then arclightning runs `fab update` from it.

The `v0.2.0` tag exists, and later releases up to `v0.3.3` followed it. The
owner confirms whether arclightning has been updated and completes this task.

Acceptance criteria:

- [ ] The owner runs the Release workflow for `v0.2.0`
- [ ] arclightning runs `fab update` from that release
