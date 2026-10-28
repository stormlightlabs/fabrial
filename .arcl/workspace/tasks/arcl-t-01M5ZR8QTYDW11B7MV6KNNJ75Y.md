+++
id = "arcl-t-01M5ZR8QTYDW11B7MV6KNNJ75Y"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Give each worker its own model credential"
status = "cancelled"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5ZR8BMPW31BN1PQDSZFYTBV"]
+++


Hosted use of the Claude Agent SDK takes an API key, not a subscription login ([Pi design](../../../docs/pi-design.md#web-ui)).

Acceptance criteria:

- [ ] The worker spec names the model credential each worker uses
- [ ] A worker run for another person uses that person's API key, never the owner's subscription login

Cancelled: the runner interface's worker spec already carries a model credential (Run workers through one runner interface); choosing whose credential a hosted run uses belongs to whoever hosts it.
