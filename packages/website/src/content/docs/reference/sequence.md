---
title: Sequence file
label: fabrial-sequence.toml
description: The keys of .github/fabrial-sequence.toml, the review stages a pull request runs, and the stages fabrial installs.
section: Reference
group: Files
order: 7
---

`.github/fabrial-sequence.toml` lists the review stages the `fab-storm` skill
runs on a pull request, and `fab sequence` prints the ones the pull request's
review depth gives. Each stage runs once, in order, in a fresh agent.
`fab sequence` reads the file from the merge base with `main`, so a change to it
applies to the pull requests after the one that makes it. `fab init` installs
the file and `fab update` refreshes it.

## Keys

| Key      | Type            | Default  | Description                                                             |
| -------- | --------------- | -------- | ----------------------------------------------------------------------- |
| `stage`  | array of tables | required | The stages, as `[[stage]]` tables, in the order they run. At least one. |
| `finish` | table           | required | The last step, after every stage.                                       |

## Stages

Each `[[stage]]` table is one stage: a fresh agent in a role, run once.

| Key     | Type                 | Default  | Description                                                                                                             |
| ------- | -------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------- |
| `name`  | string               | required | Stage's name, such as `standard review`. No two stages share one: `fab run` tells them apart by name.                   |
| `role`  | string               | required | Agent role that runs the stage: a file in `.claude/agents/`, named in lowercase letters and hyphens.                    |
| `depth` | `standard` or `full` | required | Lowest review depth that runs the stage. A depth of `none` runs no stage.                                               |
| `when`  | string               | unset    | Condition the stage needs to run, such as the stage before it finding anything above nit. Unset, the stage always runs. |
| `posts` | string               | required | What the stage leaves on the pull request.                                                                              |

## Finish

The `[finish]` table is the last step, after every stage the depth gives.

| Key     | Type   | Default  | Description                                                                   |
| ------- | ------ | -------- | ----------------------------------------------------------------------------- |
| `posts` | string | required | What the last step leaves on the pull request: the `Review finished` comment. |

## Installed stages

The file fabrial installs runs these stages. A pull request of standard depth
runs the first two, and one of full depth runs all four.

| Stage              | Role                   | Depth      | Runs when                                       |
| ------------------ | ---------------------- | ---------- | ----------------------------------------------- |
| standard review    | `reviewer`             | `standard` | always                                          |
| standard revise    | `reviser`              | `standard` | the standard review found anything above nit    |
| adversarial review | `adversarial-reviewer` | `full`     | always                                          |
| adversarial revise | `reviser`              | `full`     | the adversarial review found anything above nit |
