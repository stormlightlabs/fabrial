![fabrial, a software factory](docs/banner.png)

fabrial takes one task through implement, review, adversarial review and
revise to a pull request ready for a person to merge. It ships the
repository contract in `AGENTS.md`, the `fab-` skills and agent roles, the
GitHub workflows, and the `fab` commands they run. A harness such as Claude
Code, Codex or Pi runs the agents its own way. The loop ends when the agent
posts `Review finished at <sha>`. The owner merges.

- [One task, from claim to merge](docs/task-flow.md) names who acts at each
  step.
- [Set up stormlight-thunderbot](docs/bot-setup.md) covers the App, its
  secrets and the ruleset on `main`.
- [PLAN.md](PLAN.md) holds the decisions.

## Install in a repository

Install a release of `fab`, then run it at the root of the repository that
should use fabrial:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/stormlightlabs/fabrial/releases/latest/download/fab-cli-installer.sh | sh
fab init --owner <github-login> --trailer "Name <email>" --gates "<gate command>"
```

A repository that uses the Developer Certificate of Origin adds
`--signoff "Name <email>"`: every squash commit then ends with the owner's
`Signed-off-by` line, since an agent cannot sign off. In an installed
repository, set it with `fab update --signoff`, which also installs a
release whose gate reads it; an older release ignores the key.
`--assisted-by on` adds an `Assisted-by: <model>` line to each squash commit
for every model its pull request's commits name in a `Co-authored-by`
trailer addressed to `noreply@anthropic.com`, so `git log --grep` on `main`
finds agent work by model. Other trailers add no line. `off`, or
no `assisted_by` key, adds none.

The installer fetches the prebuilt `fab` for Linux x86_64 or Apple Silicon
into `~/.cargo/bin`. With Homebrew on those two platforms,
`brew install stormlightlabs/tap/fab` installs the same build, from releases
after v0.7.0. `cargo binstall --git
https://github.com/stormlightlabs/fabrial fab-cli` fetches the same archives.
On another platform, build it:
`cargo install --git https://github.com/stormlightlabs/fabrial --tag v<version> fab-cli`.

It writes the Branch PR, Review gate and Bot comment workflows, the action
that installs `fab` for them, the review stages in
`.github/fabrial-sequence.toml`, `shot.sh` and its `sgr.pl`, `CODEOWNERS`,
`.github/fabrial.toml`, the skills and agents, and a section of `AGENTS.md`.
Each skill is named `fab-<name>`, such as `fab-review`, so it does not take
the name of a skill the repository keeps.
Branch PR and the Review gate install the `fab` release that
`.github/fabrial.toml` names and run `fab branch-pr` and `fab gate`. Commit
the result on a `claude/` branch and push it; the Branch PR workflow opens the
pull request.

`fab init` also writes a starter `.github/fabrial-gates.toml`, which the
repository owns: `fab update` and `fab check` leave it alone. Agents run
`fab gates` before each push. It runs each `[[scoped]]` command whose `paths`
globs match a changed file, `full` for a changed file no scope names, and
nothing for documentation no scope names. It reads the file from the merge
base with `origin/main`, so a change to the file runs the base's `full`. The
starter has no scopes, so every change runs `full`, the `--gates` command.

The skills take a task from wherever the repository keeps its work. Name the
tracker in a **Tasks** section of `AGENTS.md`, outside fabrial's section, with
the commands that read a task and its readiness, start it, complete it with
evidence, hand it off, pick it up and record new work. For `fab-rubber-duck` and
`fab-decompose`, add the commands that record an idea and a spec, and map each
primitive `fab-decompose` writes (collection, run unit, task and dependency) to a
record in the tracker. Without that section, a task is a GitHub issue or a
description, ideas and specs go under `docs/`, and `fab-decompose` writes
milestones, issues with sub-issues and blocked-by links. fabrial tracks its
own work in [arclightning](https://github.com/stormlightlabs/arclightning),
and its `AGENTS.md` shows such a section.

To take a newer release, install that `fab` and run `fab update`. It keeps the
settings in `.github/fabrial.toml`, unless `--owner`, `--trailer`,
`--gates`, `--signoff` or `--assisted-by` changes one, and replaces the files
with this build's. `--signoff ""` removes the sign-off. It refuses to install
an older release over a newer one unless you pass `--force`. It deletes a
file only older releases installed, such as `.github/storm/gate.sh`, if you
have not edited it. The same goes for a skill under its name from before
the `fab-` prefix, such as `.claude/skills/review/`: an unedited copy goes
and `fab-review` replaces it. When an unedited `SKILL.md` there sits beside
any other file, or an edited one, the whole skill stays, and `fab update`
and `fab check` name it. A skill whose `SKILL.md` no release wrote is the
repository's own, and is left alone and not reported. `fab check` reports
each other retired file still there. `CHANGELOG.md` says what each release
changes. A repository moving to this release from one that named
its tracker in the skills adds a **Tasks** section to its `AGENTS.md`;
without it the skills stop and ask when given a task ID.

`fab cleanup` keeps a merged worktree that holds ignored files, other than
Cargo's `target/`, so nothing local is lost. A repository whose tools leave
caches in each worktree names them in `.github/fabrial.toml`, for example
`disposable = [".cache/db"]`; a path ending in `/` names a directory.

`fab review-depth` gives a pull request that changes only documentation,
tests and planning records the standard review rather than the full one.
Planning records are the Markdown and TOML files under the directories
`planning` names in the merge base's `.github/fabrial.toml`, for example
`planning = ["plans/"]`.

`fab check` lists files that differ from this build's and an installed version
that differs from it, and exits 1 when there is any drift.

Before the first pull request there, install stormlight-thunderbot on the
repository, share its `THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY`
secrets with it, and set up the ruleset on `main`, as
[docs/bot-setup.md](docs/bot-setup.md) says. The Review gate reads three
comments: `Review finished at <sha>`, which the agent posts as the bot, and
`/storm:ready <sha>` or `/storm:done <sha>`, with which the owner, a member
or a collaborator, read-only included, accepts that head as it is. Either
one naming the head turns `storm/review` green, marks the draft ready and
sets the squash message in one bot status comment. You merge it by hand or with auto-merge,
if the repository allows it. The gate runs from `main`, so on the install
pull request it does nothing: mark that draft ready yourself, and require
`storm/review` on `main` only after it merges.

## The Pi harness

This repository also holds an optional Pi harness, kept as it is with no new
work: `fab dispatch` runs each role as a Pi 1.4.0 worker in a host tmux
session inside a pinned Nix sandbox, `fab run` takes a task through the whole
sequence with those workers, `fab events` and `fab worker` read their state,
and the root Pi package exposes dispatch as an `agent` tool. None of the
installed skills or workflows need it. [docs/pi.md](docs/pi.md) covers host
setup and use; a normal development shell is not worker isolation.

## Releases

Run the Release PR workflow from the Actions tab with the new version, such as
`0.2.0`. The bot opens a pull request that sets the version, records it in
`.github/fabrial.toml` and writes the release's section of `CHANGELOG.md`, with
`Cargo.lock` to match. The section lists each commit subject since the last
tag; a squash subject ends with its pull request number. Edit the lines on the
release branch, then accept it with `/storm:ready <sha>`, since it holds no
code to review.
When it merges, the Release tag workflow tags `v<version>` on the merge commit.
The tag starts the Release workflow, which dist generates: it builds `fab` for
each target and publishes a GitHub release with the archives, the installer and
that section of the changelog. Its publish job,
`.github/workflows/publish-homebrew.yml`, then commits the `fab` formula to
stormlightlabs/homebrew-tap with a bot token minted for that repository, so the
bot must be installed there with Contents write. Change targets or installers
in `dist-workspace.toml` and run `dist generate`; `dist plan` fails while the
workflow is out of date. The Release PR and Release tag workflows need the bot
to have Contents write access. If Release tag fails, fix the cause and run
Release tag from the Actions tab with the merge commit. If the Release workflow
fails after the tag exists, re-run that run; a new Release tag run sees the tag
and does nothing. When the tagged commit itself is the cause, delete the tag,
fix `main`, and release the next version.

Any `v*` tag pushed to the repository starts the Release workflow and
publishes a release. A tag ruleset on `v*` with the owner and
stormlight-thunderbot on its bypass list keeps other writers, agents included,
from publishing one.
