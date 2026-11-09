+++
id = "arcl-t-01M5YZ6NMH7JKQ527FMH6F6WBR"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Prefix installed skills with fab-"
status = "completed"
priority = "normal"
position = 3
evidence = "Done in #102 at 2293963: installed skills carry fab-; fab update moves unedited old copies"
+++

From thunderstorm #74 and #83.

`fab init` writes skills named `review`, `implement`, `revise` and others into
a repository's `.claude/skills/`, where they can collide with the
repository's own.

Acceptance criteria:

- [x] Installed skills carry the `fab-` prefix, such as `fab-review`
- [x] `fab update` moves an unedited copy to its new name, and `fab check`
      reports one left under the old name; a skill at an old name that no
      release wrote, edited or the repository's own, is left alone
- [x] Skills, agents and docs that name a skill use the new names
