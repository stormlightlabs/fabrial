+++
id = "arcl-t-01M5ZEK9485DKVKXNGN93F9PND"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Remove merged agent branches that have no worktree"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #75: fab cleanup deletes a merged agent branch that has no worktree when its tip equals the merged head, and keeps and reports one whose tip differs."
+++

`fab cleanup` removes a merged pull request's branch only with its worktree. A local `claude/`, `codex/` or `storm/` branch with no worktree, such as one a cloud session pushed and a person fetched, stays after its pull request merges.

Acceptance criteria:

- [ ] `fab cleanup` deletes a local agent branch with no worktree when its tip equals the merged pull request's head
- [ ] It keeps and reports one whose tip differs
