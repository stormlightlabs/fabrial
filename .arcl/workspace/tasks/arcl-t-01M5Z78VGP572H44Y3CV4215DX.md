+++
id = "arcl-t-01M5Z78VGP572H44Y3CV4215DX"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Keep fabrial independent of any task tool"
status = "completed"
priority = "high"
position = 1
evidence = "Done in the pull request from `storm/arcl-t-01M5Z78VGP572H44Y3CV4215DX`: the installed skills and agents read the tracker from AGENTS.md's Tasks section, and `installed_files_name_no_task_tracker` fails when an installed file names arcl."
+++

fabrial uses arclightning to track its own work, but nothing it ships should
depend on arcl. The skills and agents `fab init` installs name `arcl`
commands, and PLAN.md's decisions tie the factory to arcl's crates and
workspace. A repository using GitHub issues, or another tracker, gets
instructions that do not apply to it.

Acceptance criteria:

- [x] The installed skills and agents name no task tool: they read where a
      repository's tasks live, and the commands for each step, from a Tasks
      section in its `AGENTS.md`, and otherwise take a GitHub issue or a
      description
- [x] fabrial's own `AGENTS.md` has that section, holding the arcl steps the
      skills used to carry
- [x] PLAN.md's decisions describe a task source a repository chooses, and
      mention arclightning only as fabrial's own tracker or a recommendation
- [x] A test fails when an installed file mentions arcl
