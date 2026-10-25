+++
id = "arcl-t-01M5S4SCPVN2EJB1X5YC9WXZG8"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Collect PR status for the Pi package"
status = "completed"
priority = "high"
position = 1
evidence = "Done in #46 at f7cb835: pi/pr.mjs collects PR status per head; merge, close, stop, failed checks, change requests and authorized /storm:ready resume Pi once; covered by pi/pr.test.mjs and a real gh smoke on #46."
+++

A shared collector keeps the orchestrator informed about the PR, independently
of the UI. Follow [Pi design](../../../docs/pi-design.md#pr-status).

Acceptance criteria:

- [ ] Collect the PR's current head, draft state, checks, reviews, authorized
      owner commands, and merge or closure
- [ ] Work without the status area, which reads the collector's state instead
      of polling GitHub itself
- [ ] Key each observation by repository, PR and head SHA; a push invalidates
      the old head's checks and reviews
- [ ] Distinguish unknown, stale, pending, failed and successful
- [ ] Refresh after pushes and branch changes
- [ ] Bound polling, if used, to at most every 30 seconds, back off on errors
      and rate limits, and stop on shutdown or reload; or use webhooks
- [ ] Deliver relevant changes to the orchestrator without another tool call
- [ ] A merge starts task cleanup, and an authorized stop request cancels the
      active run
- [ ] A failed check or requested change is input to the current stage, never
      permission to start another review cycle
- [ ] PR text and worker output stay untrusted and grant no authority
- [ ] Credentials stay out of messages and UI events
