+++
id = "arcl-t-01M5ZR8QSZTSPXE57TNR09BYC0"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Upload worker artifacts to R2"
status = "cancelled"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5ZR8BMPW31BN1PQDSZFYTBV"]
+++


Acceptance criteria:

- [ ] A worker uploads its record, result and transcript to R2 through presigned URLs the control plane issues
- [ ] Each presigned URL expires within the worker's run and covers only that worker's object keys
- [ ] No worker holds R2 credentials

Cancelled: fabrial is the engine, not the host (PLAN.md). Hosting, other runner backends, artifact storage and web UIs are outside its scope.
