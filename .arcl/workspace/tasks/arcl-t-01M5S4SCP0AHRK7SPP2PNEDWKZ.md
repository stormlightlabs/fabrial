+++
id = "arcl-t-01M5S4SCP0AHRK7SPP2PNEDWKZ"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Add fab status"
status = "completed"
priority = "high"
position = 3
blocked-by = ["arcl-t-01M5S4SCN5MAH0WA63X7575DKX"]
evidence = "Done in #99 at 86a3efc: fab status lists claimed tasks with claim, step and PR"
+++

A person or a host wants to see which tasks are being run, at what step,
and with which pull request, without reading branches and records by hand.
Claims are `storm/<task>` branches (PLAN.md, **A task's branch is its
claim**), and dispatch keeps each worker's record under its `state_dir`.

Acceptance criteria:

- [x] `fab status` lists each claimed task from origin's `storm/` branches:
      its holder, run and expiry from the claim commit, and whether the
      claim is live or lapsed
- [x] For each, it shows the open pull request on that branch, if any, read
      once with `gh`
- [x] With `--config`, it adds the step each run is at from dispatch's
      worker records (role and state), as `fab worker` reads them
- [x] `--json` prints the same as one JSON object per task
- [x] Tests cover a live claim, a lapsed one, a branch with work and a pull
      request, and no claims
