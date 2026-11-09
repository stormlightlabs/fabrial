# Install, update and check fabrial

This page holds the detail behind the README's install and setup steps:
the ways to get `fab`, what `fab init` writes, the settings it records, and
what `fab update` and `fab check` do later.

## Get `fab`

The installer fetches the prebuilt `fab` for Linux x86_64 or Apple Silicon
into `~/.cargo/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/stormlightlabs/fabrial/releases/latest/download/fab-cli-installer.sh | sh
```

With Homebrew on those two platforms, `brew install stormlightlabs/tap/fab`
installs the same build, from releases after v0.7.0. `cargo binstall --git
https://github.com/stormlightlabs/fabrial fab-cli` fetches the same archives.
On another platform, build it:
`cargo install --git https://github.com/stormlightlabs/fabrial --tag v<version> fab-cli`.

## What `fab init` writes

```sh
fab init --owner <github-login> --app <app-slug> --trailer "Name <email>" --gates "<gate command>"
```

It writes the Branch PR, Branch PR open, Review gate and Bot comment
workflows, the action that installs `fab` for them, the review stages in
`.github/fabrial-sequence.toml`, `shot.sh` and its `sgr.pl`, `CODEOWNERS`,
`.github/fabrial.toml`, the skills and agents, and a section of `AGENTS.md`.
Each skill is named `fab-<name>`, such as `fab-review`, so it does not take
the name of a skill the repository keeps. Branch PR open and the Review
gate install the `fab` release that `.github/fabrial.toml` names and run
`fab branch-pr` and `fab gate`. Branch PR open runs only once it is on
`main`, so open the pull request that installs fabrial yourself.

`--app` names the repository's GitHub App by its slug, and `--environment`
the GitHub environment that holds its secrets, `fabrial` by default, as
[bot-setup.md](bot-setup.md) says.

A repository that uses the Developer Certificate of Origin adds
`--signoff "Name <email>"`: every squash commit then ends with the owner's
`Signed-off-by` line, since an agent cannot sign off. In an installed
repository, set it with `fab update --signoff`, which also installs a
release whose gate reads it; an older release ignores the key.

`--assisted-by on` adds an `Assisted-by: <model>` line to each squash commit
for every model its pull request's commits name in a `Co-authored-by`
trailer addressed to `noreply@anthropic.com`, so `git log --grep` on `main`
finds agent work by model. Other trailers add no line. `off`, or no
`assisted_by` key, adds none.

## The gates file

`fab init` also writes a starter `.github/fabrial-gates.toml`, which the
repository owns: `fab update` and `fab check` leave it alone. Agents run
`fab gates` before each push. It runs each `[[scoped]]` command whose `paths`
globs match a changed file, `full` for a changed file no scope names, and
nothing for documentation no scope names. It reads the file from the merge
base with `origin/main`, so a change to the file runs the base's `full`. The
starter has no scopes, so every change runs `full`, the `--gates` command.

## Where tasks come from

The skills take a task from wherever the repository keeps its work. Name the
tracker in a **Tasks** section of `AGENTS.md`, outside fabrial's section,
with the commands that read a task and its readiness, start it, complete it
with evidence, hand it off, pick it up and record new work. For
`fab-rubber-duck` and `fab-decompose`, add the commands that record an idea
and a spec, and map each primitive `fab-decompose` writes (collection, run
unit, task and dependency) to a record in the tracker.

Without that section, a task is a GitHub issue or a description, ideas and
specs go under `docs/`, and `fab-decompose` writes milestones, issues with
sub-issues and blocked-by links. fabrial tracks its own work in
[arclightning](https://github.com/stormlightlabs/arclightning), and its
`AGENTS.md` shows such a section.

## Cleanup and review depth

`fab cleanup` keeps a merged worktree that holds ignored files, other than
Cargo's `target/`, so nothing local is lost. A repository whose tools leave
caches in each worktree names them in `.github/fabrial.toml`, for example
`disposable = [".cache/db"]`; a path ending in `/` names a directory.

`fab review-depth` gives a pull request that changes only documentation,
tests and planning records the standard review rather than the full one.
Planning records are the Markdown and TOML files under the directories
`planning` names in the merge base's `.github/fabrial.toml`, for example
`planning = ["plans/"]`.

## The Review gate's comments

The Review gate reads three comments: `Review finished at <sha>`, which the
agent posts as the App, and `/storm:ready <sha>` or `/storm:done <sha>`,
with which a user with write access accepts that head as it is. Either one
naming the head turns `storm/review` green, marks the draft ready and sets
the squash message in one bot status comment. You merge it by hand or with
auto-merge, if the repository allows it. The gate runs from `main`, so on
the install pull request it does nothing: mark that draft ready yourself,
and require `storm/review` on `main` only after it merges.

## Update

To take a newer release, install that `fab` and run `fab update`. It keeps
the settings in `.github/fabrial.toml`, unless `--owner`, `--app`,
`--secret-prefix`, `--environment`, `--trailer`, `--gates`, `--signoff` or `--assisted-by`
changes one, and replaces the files with this build's. `--signoff ""`
removes the sign-off. An install with no `app` needs `--app`. An install
with no `secret_prefix` gets `THUNDERBOT`, the secret names 0.9.0 and
earlier read, and one with no `environment` gets `fabrial`, as
[bot-setup.md](bot-setup.md#moving-from-an-older-release) says. It refuses to install an older release over a newer one unless you
pass `--force`.

It deletes a file only older releases installed, such as
`.github/storm/gate.sh`, if you have not edited it. The same goes for a
skill under its name from before the `fab-` prefix, such as
`.claude/skills/review/`: an unedited copy goes and `fab-review` replaces
it. When an unedited `SKILL.md` there sits beside any other file, or an
edited one, the whole skill stays, and `fab update` and `fab check` name it.
A skill whose `SKILL.md` no release wrote is the repository's own, and is
left alone and not reported. `CHANGELOG.md` says what each release changes.

A repository moving to this release from one that named its tracker in the
skills adds a **Tasks** section to its `AGENTS.md`; without it the skills
stop and ask when given a task ID.

## Check

`fab check` lists files that differ from this build's, an installed version
that differs from it, and each retired file still there. It exits 1 when
there is any drift.
