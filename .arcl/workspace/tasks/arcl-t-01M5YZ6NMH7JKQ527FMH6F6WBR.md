+++
id = "arcl-t-01M5YZ6NMH7JKQ527FMH6F6WBR"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Prefix installed skills with fab-"
status = "in_progress"
priority = "normal"
position = 3
+++

From thunderstorm #74 and #83.

`fab init` writes skills named `review`, `implement`, `revise` and others into
a repository's `.claude/skills/`, where they can collide with the
repository's own.

Acceptance criteria:

- [ ] Installed skills carry the `fab-` prefix, such as `fab-review`
- [ ] `fab update` moves an installed copy to its new name, and `fab check`
      reports one left under the old name
- [ ] Skills, agents and docs that name a skill use the new names
