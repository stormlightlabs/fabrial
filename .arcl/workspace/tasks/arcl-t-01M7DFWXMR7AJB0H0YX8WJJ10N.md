+++
id = "arcl-t-01M7DFWXMR7AJB0H0YX8WJJ10N"
parent = "arcl-t-01M7DFWXKYMM842EHBGXRNTQQ7"
title = "Stop showing storm/review as running while it waits"
status = "pending"
priority = "high"
position = 0
+++

`fab gate` creates the `storm/review` check run with `status=in_progress`
(crates/fab-cli/src/gate.rs, `check`), so GitHub shows it as running from
the push until a review comment, with no end.

Done when a head waiting for review gets a completed check run whose
conclusion blocks the ruleset but does not read as running (such as
`action_required`, titled "Waiting for review"), the gate's tests cover it,
and nothing that reads past runs (the last-success lookup) changes meaning.

Not in this task: accepting approvals.

Paths: crates/fab-cli/src/gate.rs and its tests, docs that describe the
pending state (docs/task-flow.md, docs/bot-setup.md).
