+++
id = "arcl-t-01M6Z600NWYSJ9WBBM4B43P1WB"
parent = "arcl-t-01M6ZBSFYBJPAAR89R51V6A1R4"
title = "Keep the App key out of workflows a pushed branch can edit"
status = "pending"
priority = "high"
position = 0
+++

Branch PR (`.github/workflows/branch-pr.yml`) runs on `push` to `claude/**`,
`codex/**` and `storm/**`, so GitHub runs the pushed branch's own copy of the
file. That copy gets `THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY`. A
branch that edits it can mint the App's token or print the key before any
review, then post a green `storm/review` check as the App, which the ruleset
accepts even with the check's source pinned to the App. Running `fab` from
`main` and checking the branch out only for git to read does not help: the
workflow file itself is the branch's.

The only limit today is that pushing a workflow file takes the `workflow`
scope, Workflows write, or write access over SSH. The comment in
`branch-pr.yml` says no push token holds Workflows write; that holds for Pi
workers, not for every agent session or collaborator. Code owners on
`.github/workflows/` act only at merge. `docs/bot-setup.md` states the limit.

Options:

- Put the two secrets in a GitHub environment that only `main` may deploy
  to, and split Branch PR: the push job records the branch with no secrets,
  and a `workflow_run` job, which always runs from `main`, mints the token
  and opens the pull request.
- Open pull requests without the App key, for example from a scheduled job
  on `main` that lists agent branches with no pull request.

Done when no workflow a pushed branch can edit reads the App's secrets, and
`docs/bot-setup.md` and the comment in `branch-pr.yml` say so.
