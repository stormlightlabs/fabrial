+++
id = "arcl-t-01M5S4SCN5MAH0WA63X7575DKX"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Claim tasks on refs/fabrial/claims"
status = "pending"
priority = "normal"
position = 1
+++

Claims on `refs/fabrial/claims`, as PLAN.md's "Claims live in git, off
`main`" decision describes.

Acceptance criteria:

- [ ] Claim, renew, release and expire a claim
- [ ] Tests cover two machines racing for one task
- [ ] A worker that a run dispatched for a task it claimed finds the claim
      held by that run and goes on; the `implement` skill stops only when
      another run holds it (thunderstorm #127)
