+++
id = "arcl-t-01M5S4SCN5MAH0WA63X7575DKX"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Claim a task by creating its branch"
status = "in_progress"
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

A task's branch is its claim, as PLAN.md's "A task's branch is its claim"
decision describes. Creating `storm/<task>` with a push that succeeds only
when the branch does not exist yet is the lock, so it works wherever branch
pushes work, cloud sessions included. #71 tried a custom ref,
`refs/fabrial/claims`, which a cloud session cannot push.

Acceptance criteria:

- [ ] `fab claim` takes a task by creating its branch with a create-only
      push, and reports the run that holds it when the branch exists
- [ ] The claim commit names the task, machine, run and an expiry; a run
      renews it, and releases it by deleting a branch that holds no work
- [ ] An expired claim can be taken again without losing the commits on
      its branch
- [ ] The Branch PR workflow opens no pull request for a branch that holds
      only its claim
- [ ] Tests cover two machines racing for one task
- [ ] A worker that a run dispatched for a task it claimed finds the claim
      held by that run and goes on; the `implement` skill stops only when
      another run holds it (thunderstorm #127)
