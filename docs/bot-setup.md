# Set up stormlight-thunderbot

The workflows `fab init` installs act as the GitHub App stormlight-thunderbot.
A repository needs the App installed, two secrets, and a ruleset on `main`
before its first agent pull request. Each name below comes from the
workflows in `.github/workflows/`, the code of the `fab` commands they run,
or this repository's own settings.

## The App

The App must be stormlight-thunderbot. The Review gate accepts a
`Review finished at` comment from a bot only when the bot is
`stormlight-thunderbot[bot]`, and `fab gate` takes an edit from that login
without checking its access. Another App's comments do nothing.

Install it on the repository. Each workflow mints a token for it with
`actions/create-github-app-token`. A comment posted with that token, such as
`Review finished`, starts the Review gate, which a comment posted with
`GITHUB_TOKEN` would not. Agent sessions post through the Bot comment
workflow, not with the key.

That does not keep the key from anyone who can push a workflow change.
Branch PR runs on push from the pushed branch's own copy of
`branch-pr.yml`, with both secrets. A branch that edits that file can mint
the App's token, or read the key, before any review, and post a green
`storm/review` as the App. Pushing a workflow file takes the `workflow`
scope on a personal token, Workflows write on an App token, or write access
over SSH; Pi workers' tokens have none of these. Code owners on
`.github/workflows/` do not help, since the edit runs before any pull
request merges. Treat everyone who can push a workflow change to a branch
as holding the App's key.

## What the App's token does

No file here lists the App's permissions. The table gives what each workflow
does with the token; grant the permissions those calls need.

| Workflow    | Calls                                                              | Permission it needs       |
| ----------- | ------------------------------------------------------------------ | ------------------------- |
| Branch PR   | `gh pr list`, `gh pr create --draft`                               | Pull requests write       |
| Review gate | reads and creates `storm/review` check runs                        | Checks write              |
| Review gate | `gh pr ready`, `gh pr comment`, edits the status comment           | Pull requests write       |
| Review gate | reads the pull request's commits through `pulls/<n>/commits`       | Pull requests read        |
| Review gate | reads an editor's permission through `collaborators/<user>/permission` | Metadata read         |
| Bot comment | posts comments, edits the status comment, posts reviews, requests a reviewer, resolves review threads | Pull requests write |

Comments on a pull request go through the issues API
(`issues/<n>/comments`), which takes Issues write or Pull requests write.
Every App has Metadata read. The gate compares the head with `main` using
git, in a checkout made with `GITHUB_TOKEN`, so the App token needs no
Contents permission.

fabrial's own release workflows need more, and an installing repository does
not run them: Release PR and Release tag push a branch and a tag with
Contents write, and the Homebrew publish job mints a token with Contents
write on stormlightlabs/homebrew-tap.

The Pi harness's `fab dispatch` mints worker tokens from the same App and
needs other permissions; [docs/pi.md](pi.md#github-app) lists them.

## Secrets and variables

The workflows read two secrets and no variables:

| Secret                   | Read by                                         |
| ------------------------ | ----------------------------------------------- |
| `THUNDERBOT_APP_ID`      | Branch PR, Review gate, Bot comment             |
| `THUNDERBOT_PRIVATE_KEY` | Branch PR, Review gate, Bot comment             |

In stormlightlabs they are organization secrets shared with each repository
that uses fabrial. The workflows also install the `fab` release that
`version` in `.github/fabrial.toml` names, from stormlightlabs/fabrial's
releases, so the runners need to reach github.com.

## The ruleset on `main`

The workflows assume a ruleset on `main` with these rules:

- Require a pull request with one approval and a review from code owners.
  `fab init` writes `CODEOWNERS` from `owner` in `.github/fabrial.toml`, and
  the bot opens every pull request, so the owner is never its author and can
  approve.
- Allow only squash merges. The gate posts the message for that squash
  commit.
- Require the `storm/review` check and the repository's own CI checks. Set
  the source of `storm/review` to the stormlight-thunderbot App. Any App
  with Checks write can post a check of that name, including `GITHUB_TOKEN`
  in a workflow on a branch that asks for `checks: write`. The gate ignores
  those runs, but the ruleset counts them unless the source is set.
  Setting it does not stop a branch that edits Branch PR, as
  [The App](#the-app) says.
- Block force pushes and deletion.

Leave the App off the ruleset's bypass list, so no token minted from it can
merge past these rules.

`/storm:ready`, `/storm:done`, a person's `Review finished at` and an edit
to the title or body count only from a user with write, maintain or admin
access. The gate ignores the rest without a reply.

The gate runs from `main` through `pull_request_target`. On the pull request
that installs fabrial it does nothing, so mark that draft ready yourself and
add `storm/review` to the ruleset only after it merges.

fabrial's own `main` requires the `check` job, owner approval with code owner
review, and squash merges. It does not require `storm/review`.

## Tags

fabrial's `v*` tags start its Release workflow. A tag ruleset on `v*` that
blocks creating, updating and deleting tags, with the owner and the App on
its bypass list, keeps other writers from publishing a release. An
installing repository needs this only if it publishes on tags too.
