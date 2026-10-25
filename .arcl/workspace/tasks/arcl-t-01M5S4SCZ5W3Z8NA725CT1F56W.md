+++
id = "arcl-t-01M5S4SCZ5W3Z8NA725CT1F56W"
phase = "arcl-ph-01M5S4SCJ1A54Z514HCP508F9X"
title = "Try Mire as the owner's local review surface"
status = "pending"
priority = "normal"
position = 3
+++

Try [Mire](https://github.com/stormlightlabs/mire) on one pull request. Mire
has no GitHub integration; the existing gate and owner approval remain the
source of merge decisions.

Acceptance criteria:

- [ ] In a fetched PR worktree, capture the comparison in a review file outside
      the repository
- [ ] Inspect it in Mire, and export anchored findings and human decisions
- [ ] Decide whether an adapter should send findings through fabrial's Bot
      comment workflow
