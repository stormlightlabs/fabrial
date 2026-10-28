+++
id = "arcl-t-01M60CM91MZYXGCHTC9VFJ0Y3F"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Port the rubber-duck, specify and decompose skills"
status = "pending"
priority = "normal"
position = 0
+++

thunderstorm's `rubber-duck`, `specify` and `decompose` skills turn a design conversation into a recorded idea, a spec when a decision has to come first, and then tasks. fabrial has no planning skills, so these conversations happen ad hoc and their decisions are not written down.

thunderstorm writes ideas and specs under the directory `.tstorm.toml` names, through `tstorm`. fabrial's versions name no tracker: they record ideas, specs and tasks with the commands the repository's AGENTS.md **Tasks** section names, and fall back to files under `docs/` when it names none.

Acceptance criteria:

- [ ] fabrial installs `rubber-duck`, `specify` and `decompose` skills adapted from thunderstorm's
- [ ] Each records its result through the Tasks section's commands, or under `docs/` without one, and names no tracker
- [ ] `fab check` covers the new skills, and the test that installed files name no tracker passes
