+++
id = "arcl-t-01M5S4SCN5MAH0WA63X7575DKX"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Claim tasks on refs/fabrial/claims"
status = "completed"
priority = "normal"
position = 1
evidence = "Done in the pull request from `storm/arcl-t-01M5S4SCN5MAH0WA63X7575DKX`: fab claim take, renew, release and list on refs/fabrial/claims, the push as the lock; tests in fab-core and fab-cli cover expiry and two machines racing; the implement skill claims with fab claim take and goes on when its run holds the claim"
+++

Claims on `refs/fabrial/claims`, as PLAN.md's "Claims live in git, off
`main`" decision describes.

Acceptance criteria:

- [x] Claim, renew, release and expire a claim
- [x] Tests cover two machines racing for one task
- [x] A worker that a run dispatched for a task it claimed finds the claim
      held by that run and goes on; the `implement` skill stops only when
      another run holds it (thunderstorm #127)
