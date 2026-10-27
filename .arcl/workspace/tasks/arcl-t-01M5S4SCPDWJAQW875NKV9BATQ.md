+++
id = "arcl-t-01M5S4SCPDWJAQW875NKV9BATQ"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Clean up and resync when a pull request merges"
status = "completed"
priority = "normal"
position = 4
evidence = "Done in the pull request from `storm/arcl-t-01M5S4SCPDWJAQW875NKV9BATQ`: `fab cleanup` removes merged, clean agent worktrees at the merged head with their branches, fast-forwards a clean primary main, lists open agent pull requests that conflict with main or are behind it for the resync skill, and reads only git and GitHub so it runs after a restart; tasks own no Nix roots outside their worktrees"
+++

Worker-exit cleanup shipped with the asynchronous Pi workers in phase 6. This
task covers what follows a merge, following AGENTS.md's "Lifecycle cleanup",
and the open pull requests the merge leaves behind `main` (thunderstorm #142).

Acceptance criteria:

- [x] Remove only clean worktrees and branches that match the merged head
- [x] Release task-owned Nix roots
- [x] Refresh a clean `main`
- [x] Find the bot's open pull requests that no longer merge cleanly and
      resync each one with the `resync` skill
- [x] Retain and report recovery state on failure
- [x] Support cleanup after a restart without another agent
