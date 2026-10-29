+++
id = "arcl-t-01M60CM91MZYXGCHTC9VFJ0Y3F"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Port rubber-duck and decompose"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #81: rubber-duck interviews in rounds after grill-me and records an idea or a spec; decompose writes tracker-neutral collections, run units, tasks and dependencies through the Tasks section's mapping."
+++

Design conversations happen ad hoc and their decisions are not written down. thunderstorm has `rubber-duck`, `specify` and `decompose` for this; fabrial ports two skills from them.

`rubber-duck` combines thunderstorm's `rubber-duck` and `specify`, and runs as a conversation between agent and person, after Matt Pocock's `grill-me` (mattpocock/skills, `skills/productivity/grilling`). The agent maps the decisions as a tree, asks every question whose prerequisites are settled in one numbered round with its recommended answer, looks up facts itself rather than asking for them, and stops when no branch is left open and the person confirms. It records the decisions as an idea, or as a spec when work cannot be cut until one is made.

`decompose` names no tracker. It cuts an idea or spec into a small set of primitives: a collection that groups work and holds its order and crossings, a unit a run takes, the claimable tasks under it with a stop rule each, and dependencies between them. The repository's AGENTS.md **Tasks** section maps each primitive to its tracker, such as an arcl plan, phase and task, or a GitHub milestone, issue and sub-issue.

Acceptance criteria:

- [ ] fabrial installs `rubber-duck`, which interviews in rounds as described above and records an idea or a spec through the Tasks section's commands, or under `docs/` without one
- [ ] fabrial installs `decompose`, which writes collections, run units, tasks and dependencies through the Tasks section's commands
- [ ] The Tasks section format gains the primitive mapping, and fabrial's own AGENTS.md fills it in for arcl
- [ ] Both skills name no tracker, and `fab check` and the test that installed files name no tracker pass
