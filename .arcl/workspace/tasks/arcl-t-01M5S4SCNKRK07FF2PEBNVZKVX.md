+++
id = "arcl-t-01M5S4SCNKRK07FF2PEBNVZKVX"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Add fab run for the fixed sequence"
status = "completed"
priority = "high"
position = 2
evidence = "Done in #92 at 30dae06: fab run claims, adds a worktree, dispatches the implementer and each sequence stage as Pi workers, and reaches the tracker through the [tasks] commands"
+++

Acceptance criteria:

- [x] `fab run <task>` runs the fixed sequence
- [x] One worker runs each step
- [x] Each task gets its own worktree
- [x] It reads the task and writes its handoff and evidence through the
      repository's task source, and names no tracker
