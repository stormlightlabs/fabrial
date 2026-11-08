+++
id = "arcl-t-01M6Y8S2MGNWDN7CKK6ZSCXC34"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Decide how workers stop sharing one repository's refs"
status = "pending"
priority = "high"
position = 0
+++

Owner decision, from the sandbox probe. Workers on one repository share its
Git directory through linked worktrees, and the sandbox leaves refs,
`packed-refs` and objects writable. A worker can move another task's
branch ref, and the host then pushes that history when the other task's
writer exits, as long as it fast-forwards origin.

Options:

- Give each task its own clone instead of a linked worktree, so a worker
  sees no other task's refs (more disk and fetch time).
- Run one writer per repository at a time.
- Have the host record each writer's head when it exits and push only a
  commit that descends from the head it recorded at dispatch and that this
  worker's run made, accepting that a worker can still rewrite local refs.

Acceptance criteria:

- [ ] The owner has chosen, and dispatch and `fab run` follow it
- [ ] `conf/sandbox/probe.sh` checks the choice
- [ ] docs/pi.md states it
