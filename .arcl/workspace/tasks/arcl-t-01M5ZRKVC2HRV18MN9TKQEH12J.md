+++
id = "arcl-t-01M5ZRKVC2HRV18MN9TKQEH12J"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Let fab cleanup remove ignored caches a repository names"
status = "pending"
priority = "normal"
position = 0
+++

Seen after #72 merged.

`fab cleanup` keeps every fabrial task worktree. `arcl init` leaves the
ignored files `.arcl/arcl.db` and `.arcl/arcl.db-sync.lock` in each, and
cleanup refuses a worktree with ignored files other than a root `target/`.

One way to fix it, not yet decided: let the repository name ignored paths
that are safe to delete, for example in `.github/fabrial.toml`. fabrial must
not know about arcl itself.

Acceptance criteria:

- [ ] `fab cleanup` removes a merged task's worktree whose only ignored files
      are paths the repository names as safe to delete
- [ ] Without that setting, it keeps refusing ignored files other than
      `target/`
