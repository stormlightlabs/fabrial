+++
id = "arcl-t-01M7C0PDV3BX3KA074HVVWQPXY"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Implement a task with all its subtasks in one pull request"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #113: implement works a task with all its subtasks in one pull request"
+++

`fab-implement` stops on a task with open subtasks and says to work each one
alone, and `fab-decompose` defines a task as one claim, one branch and one
pull request, with run units never claimed. The owner wants a task or issue
worked with all of its subtasks or sub-issues, in one pull request.

Done when:

- `fab-implement` takes a task or issue with open children: it claims the
  parent, works every open child on the parent's `storm/<id>` branch in
  dependency order, and opens one pull request. It completes each child and
  then the parent, or closes each sub-issue and the parent from the pull
  request body.
- It stops and reports when a child is blocked by work outside the parent,
  in progress elsewhere, or has a pull request of its own.
- `fab-decompose` claims a run unit, sizes it to fit one review, and still
  allows a lone task with no children.
- `fab-storm`, `docs/task-flow.md` and `AGENTS.md` say the same.
- `fab gates` passes, and `fab check` reports no drift.

Not in this task: the frozen Pi harness (`fab run`, `fab dispatch`), and any
change to `fab claim`, which already claims any ID.

Paths: `.claude/skills/fab-implement/`, `.claude/skills/fab-decompose/`,
`.claude/skills/fab-storm/`, `docs/task-flow.md`, `AGENTS.md`, and the
install tests that pin skill text.

Source: the owner, 2026-11-14.
