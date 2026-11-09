+++
id = "arcl-t-01M70W2S5S3MDNFSZ4KM79CJZK"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Let resync resolve conflicts and choose rebase or merge"
status = "in_progress"
priority = "high"
position = 0
+++

`fab-resync` rebases a pull request onto `main` and stops at the first
conflict, handing it to a revise pass or the operator. Most conflicts are
mechanical, so the stop costs a round trip that the resyncing agent could
have saved.

Acceptance criteria:

- [ ] The skill chooses between rebasing and merging `main` in: a merge
      for a branch a person owns or anyone else commits to, a rebase for
      an agent branch where it applies cleanly, and a merge when a rebase
      would resolve the same conflict once per commit or drop a merge
      of `main`
- [ ] It resolves conflicts by reading both sides' intent (the PR body,
      `main`'s commits) and keeps both behaviours; it regenerates lockfiles
      and generated or managed files with the repository's tooling instead
      of editing them, and keeps the newer status in a task file
- [ ] It stops and asks only when both sides change the same logic and
      either choice loses behaviour, naming the files and the choice
- [ ] After resolving, it runs the gates and the before and after diff
      comparison, and reports whether the change now differs and so needs
      another review sequence
- [ ] `fab-storm`, `fab-revise` and the agents section say the same
