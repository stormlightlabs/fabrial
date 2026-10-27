+++
id = "arcl-t-01M5S4SCN5MAH0WA63X7575DKX"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Claim tasks on refs/fabrial/claims"
status = "parked"
priority = "normal"
position = 1
handoff = """
Parked by the owner. #71 holds an unreviewed implementation: `fab claim
take|renew|release|list` over commits on `refs/fabrial/claims`, with race tests
against a local bare repository. Two reasons to wait:

- Nothing picks tasks unattended or in parallel yet, so nothing contends for a
  task. Claims become useful with `fab run` or the daemon.
- A Claude Code cloud session's git access pushes branches only: a push to
  `refs/fabrial/claims` fails with HTTP 403. Decide where claims live (a
  branch such as `fabrial/claims`, the hidden ref, or a setting) before #71
  goes on."""
+++

Claims on `refs/fabrial/claims`, as PLAN.md's "Claims live in git, off
`main`" decision describes.

Acceptance criteria:

- [ ] Claim, renew, release and expire a claim
- [ ] Tests cover two machines racing for one task
- [ ] A worker that a run dispatched for a task it claimed finds the claim
      held by that run and goes on; the `implement` skill stops only when
      another run holds it (thunderstorm #127)
