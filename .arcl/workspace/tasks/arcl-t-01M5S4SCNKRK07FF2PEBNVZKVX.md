+++
id = "arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Add fab run for the fixed sequence"
status = "pending"
priority = "normal"
position = 2
+++

Acceptance criteria:

- [ ] `fab run <task>` runs the fixed sequence
- [ ] One worker runs each step
- [ ] Each task gets its own worktree
- [ ] It reads the task and writes its handoff and evidence through the
      repository's task source, and names no tracker
