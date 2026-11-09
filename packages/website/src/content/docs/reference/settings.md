---
title: Settings file
label: fabrial.toml
description: The keys of .github/fabrial.toml, the settings the workflows, the review gate and the skills read.
section: Reference
group: Files
order: 5
---

`.github/fabrial.toml` holds a repository's fabrial settings. The workflows, the
review gate and the skills read it. `fab init` writes it, and `fab update`
changes a value its flags name. The workflows read each value with `sed`, so a
value is one line, `key = "value"`, with no quote, backslash or line break. A
list is one line too: `key = ["a", "b"]`.

## Keys

| Key             | Type            | Default      | Description                                                                                                                                                                                                                                                                                                                          |
| --------------- | --------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `owner`         | string          | required     | GitHub login of the person who approves pull requests. `fab init` writes `CODEOWNERS` from it.                                                                                                                                                                                                                                       |
| `app`           | string          | unset        | Slug of the repository's GitHub App, which the workflows act as: `fabrial-bot` for `github.com/apps/fabrial-bot`, whose comments come from `fabrial-bot[bot]`. `fab init` and `fab update` need it.                                                                                                                                  |
| `secret_prefix` | string          | `THUNDERBOT` | Prefix of the secrets that hold the App's ID and key: the workflows read `<prefix>_APP_ID` and `<prefix>_PRIVATE_KEY`. `fab init` writes `FABRIAL`. Unset, the workflows read the `THUNDERBOT_` names fabrial 0.9.0 and earlier used.                                                                                                |
| `environment`   | string          | unset        | GitHub environment that every job minting the App's token names. Keep the App's secrets there with a deployment rule of `main`, so no job on another ref reads them. Unset, the jobs read repository and organization secrets.                                                                                                       |
| `trailer`       | string          | required     | `Co-authored-by` value on every squash commit, as `Name <email>`.                                                                                                                                                                                                                                                                    |
| `gates`         | string          | required     | Command agents run before pushing. `fab gates` runs it when the repository has no `.github/fabrial-gates.toml`, and `fab init` writes it into that file's starter as `full`.                                                                                                                                                         |
| `signoff`       | string          | unset        | `Signed-off-by` value that ends every squash commit, for repositories that use the Developer Certificate of Origin. Unset leaves the line out.                                                                                                                                                                                       |
| `assisted_by`   | `on` or `off`   | `off`        | `on` adds an `Assisted-by: <model>` line to the squash commit for each agent model the pull request's commits name. `off` keeps model names off `main`.                                                                                                                                                                              |
| `disposable`    | list of strings | unset        | Ignored paths, relative to a worktree's root, that `fab cleanup` may delete with a merged worktree beside Cargo's `target/`: caches a tool rebuilds, such as a tracker's local database. A path ending in `/` names a directory.                                                                                                     |
| `planning`      | list of strings | unset        | Directories, relative to the repository root and each ending in `/`, whose Markdown and TOML files are planning records, such as a tracker's task files. `fab review-depth` gives a pull request that changes only those, documentation and tests the standard pass. None may sit under `.github`, `.claude`, `.agents` or `.codex`. |
| `version`       | string          | unset        | fabrial version that last wrote the installed files. `fab init` and `fab update` write it, and `fab check` compares it with its own.                                                                                                                                                                                                 |
| `tasks`         | table           | unset        | Commands `fab run` uses to reach the repository's task source. Unset leaves a run to GitHub issues and description files.                                                                                                                                                                                                            |

## The tasks table

The `[tasks]` table names the commands `fab run` uses for each step of a tracked
task. Each is split on whitespace and run without a shell in the task's
worktree, after `{id}`, `{evidence}` and `{file}` in a word are replaced. A
replacement stays one argument, whatever it holds. Each command must use the
placeholders its row names.

| Key        | Type   | Default  | Description                                                                                                                                              |
| ---------- | ------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `init`     | string | unset    | Prepares a new worktree for the other commands, run once before `start`, such as a tracker that keeps a local cache per checkout. Takes no placeholders. |
| `skill`    | string | unset    | Skill under `.claude/skills` that `fab dispatch` gives an implementer for working the tracker, such as `arclightning`. A plain directory name.           |
| `show`     | string | required | Prints the task, given `{id}`.                                                                                                                           |
| `start`    | string | required | Starts the task, given `{id}`.                                                                                                                           |
| `complete` | string | required | Completes the task, given `{id}` and `{evidence}`.                                                                                                       |
| `handoff`  | string | required | Records a handoff note, given `{id}` and the note's `{file}`.                                                                                            |
