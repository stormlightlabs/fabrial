+++
id = "arcl-t-01M5S4SCPDWJAQW875NKV9BATQ"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Automate post-merge task cleanup"
status = "pending"
priority = "normal"
position = 4
+++

Worker-exit cleanup shipped with the asynchronous Pi workers in phase 6. This
task covers cleanup after a PR merges, following AGENTS.md's "Lifecycle
cleanup".

Acceptance criteria:

- [ ] Remove only clean worktrees and branches that match the merged head
- [ ] Release task-owned Nix roots
- [ ] Refresh a clean `main`
- [ ] Retain and report recovery state on failure
- [ ] Support cleanup after a restart without another agent
