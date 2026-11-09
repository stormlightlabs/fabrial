+++
id = "arcl-t-01M6Z600PEASV6468HG7M7KJYQ"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Require write access for /storm:ready and /storm:done"
status = "in_progress"
priority = "high"
position = 0
+++

The Review gate turns `storm/review` green on `/storm:ready <sha>`,
`/storm:done <sha>` or a person's `Review finished at <sha>` from anyone whose
`author_association` is OWNER, MEMBER or COLLABORATOR
(`.github/workflows/review.yml`). That includes read-only collaborators and
every organization member. `Gate::comment` in `crates/fab-cli/src/gate.rs`
then checks no permission, unlike `Gate::edit`, which reads
`collaborators/<user>/permission` and returns unless it is `admin`,
`maintain` or `write`.

Fix: in `Gate::comment`, read the commenter's permission the same way and do
nothing for a person below write; the bot's own `Review finished` keeps its
login check. Add a gate test with a fake `gh` answering `read`. Then drop the
warning in `docs/bot-setup.md` and say "with write access" where `README.md`,
`docs/task-flow.md` and the `AGENTS.md` section in
`crates/fab-core/src/agents-section.md` name who may accept a head.
