+++
id = "arcl-t-01M5S4SCJTAJGCA4TBYKHHRNMP"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Verify Codex and Pi, then run one task on Codex"
status = "pending"
priority = "normal"
position = 1
+++

Verify the shared skill layout on Claude Code, Codex and Pi, and record what
differs. `docs/hosts.md` records Claude Code. Its Codex and Pi rows came from
thunderstorm's `docs/internal/hosts.md`, so this task also covers checking the
skills on Pi itself. Then take one task through the whole sequence on Codex
(thunderstorm #118 and #121).

Acceptance criteria:

- [ ] Check on Codex that it reads the skills and agents as `docs/hosts.md` says
- [ ] Check on Pi itself that it reads the skills as `docs/hosts.md` says
- [ ] Record in `docs/hosts.md` what differs, and the version of each host checked
- [ ] In an attended Codex session, take one ready task to a reviewed pull
      request with the `storm` skill, and record the task, the pull request,
      the stages that ran and the final head; file each step that failed as
      its own task
