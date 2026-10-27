+++
id = "arcl-t-01M5YZ6NMH7JKQ527FMH6F6WBR"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Prepare fabrial for repositories that install it"
status = "pending"
priority = "normal"
position = 3
+++

From thunderstorm #74 and #83.

`fab init` writes skills named `review`, `implement` and `revise` into a
repository's `.claude/skills/`, where they can collide with the repository's
own. Nothing outside this repository explains how to set up the bot or what a
run looks like.

Acceptance criteria:

- [ ] Installed skills carry a prefix that cannot collide, and `fab update`
      moves an installed copy to the new names
- [ ] A page covers setting up stormlight-thunderbot for a repository: the
      App, its permissions, the secrets and the ruleset
- [ ] A page walks through one task from arcl to merge, naming who acts at
      each step
