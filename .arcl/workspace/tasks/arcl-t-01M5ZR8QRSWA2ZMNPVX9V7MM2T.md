+++
id = "arcl-t-01M5ZR8QRSWA2ZMNPVX9V7MM2T"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Add an exe.dev runner"
status = "cancelled"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5ZR8BMPW31BN1PQDSZFYTBV", "arcl-t-01M5ZR8QVVENQJM8YWKGWBMAB4"]
+++


Acceptance criteria:

- [ ] An exe.dev runner starts each worker in its own VM and deletes the VM when the worker exits
- [ ] A placement rule picks the runner for each worker

Cancelled: fabrial is the engine, not the host (PLAN.md). Hosting, other runner backends, artifact storage and web UIs are outside its scope.
