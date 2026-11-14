+++
id = "arcl-t-01M7DFWXNCTQEH67SQKR76CXA1"
parent = "arcl-t-01M7DFWXKYMM842EHBGXRNTQQ7"
title = "Accept an approving review as /storm:ready"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #114: an approval of the head from a person with write access marks it ready as /storm:ready does; reviews with no reviewer or commit are skipped"
+++

Only `Review finished at <sha>`, `/storm:ready <sha>` or `/storm:done <sha>`
accepts a head. The owner wants an approving review from a user with write
access to count as `/storm:ready` for the commit it approves.

`pull_request_review` has no `_target` form: it runs the workflow file from
the head, so a pull request could change its own gate. Use the pattern
Branch PR and Branch PR open use: a workflow on `pull_request_review` that
holds no secrets and does nothing privileged, and a `workflow_run` job that
runs from main, reads the review from the API, and calls `fab gate`.

Done when an approval of the current head by a user with write access turns
`storm/review` green, marks a draft ready and sets the squash message as
`/storm:ready` does; an approval of an older commit, from anyone without
write access, or from another App does nothing; tests cover each; and
`fab init` installs the new workflow, with `fab check` reporting no drift.

Not in this task: requiring an approval.

Paths: .github/workflows/review.yml and a new relay workflow,
crates/fab-cli/src/gate.rs, crates/fab-core/src/install.rs, docs that list
what accepts a head (AGENTS.md section template, docs/task-flow.md,
docs/bot-setup.md, the fab-storm skill).
