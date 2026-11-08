+++
id = "arcl-t-01M6WNDCH4T131XXKRC2M5BTJN"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Push a writer's commits from the host, not the worker"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #98 at b4e4812: worker tokens are read-only, the host pushes a writer's branch from its own bare repository, and Branch PR runs fab from main"
+++

From the adversarial review of #91. A Pi worker's token with Contents write
can push to any branch or tag the rulesets leave open, and an App token's
write scope cannot be narrowed to one branch. The writer roles (implementer
and reviser) hold one today. The owner chose to take push access away from
workers rather than add rulesets or a second App.

A writer commits in its worktree and the host pushes once it exits, with a
stormlight-thunderbot token the host mints for that push and never hands to
the worker. Pushes, Branch PR and bot comments look as they do now.

Acceptance criteria:

- [x] Every worker's token has Contents read; none can push
- [x] After a writer exits, the host pushes its branch only when the
      worktree is still on the task's agent branch and the push is a
      fast-forward of the branch on origin; otherwise it reports and pushes
      nothing
- [x] The host pushes with git hooks off, so nothing the worker wrote runs
- [x] The host's push token is minted for the push, limited to the
      repository, and revoked after it
- [x] `fab run` and the writer role prompts expect the host to push; the
      worker is told to commit and not push
- [x] Tests cover the read-only token, a refused non-fast-forward and wrong
      branch, and the push
- [x] `docs/pi.md` describes who pushes and why
