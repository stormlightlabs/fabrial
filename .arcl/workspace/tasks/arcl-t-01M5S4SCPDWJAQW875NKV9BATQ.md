+++
id = "arcl-t-01M5S4SCPDWJAQW875NKV9BATQ"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Clean up and resync when a pull request merges"
status = "pending"
priority = "normal"
position = 4
+++

Worker-exit cleanup shipped with the asynchronous Pi workers in phase 6. This
task covers what follows a merge, following AGENTS.md's "Lifecycle cleanup",
and the open pull requests the merge leaves behind `main` (thunderstorm #142).

Acceptance criteria:

- [ ] Remove only clean worktrees and branches that match the merged head
- [ ] Release task-owned Nix roots
- [ ] Refresh a clean `main`
- [ ] Find the bot's open pull requests that no longer merge cleanly and
      resync each one with the `resync` skill
- [ ] Retain and report recovery state on failure
- [ ] Support cleanup after a restart without another agent
