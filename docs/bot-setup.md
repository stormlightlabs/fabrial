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
`GITHUB_TOKEN` would not. Agent sessions never hold the App's private key:
they post through the Bot comment workflow.

## What the App's token does

No file here lists the App's permissions. The table gives what each workflow
does with the token; grant the permissions those calls need.

| Workflow    | Calls                                                              | Permission it needs       |
| ----------- | ------------------------------------------------------------------ | ------------------------- |
| Branch PR   | `gh pr list`, `gh pr create --draft`                               | Pull requests write       |
| Review gate | creates `storm/review` check runs                                  | Checks write              |
| Review gate | `gh pr ready`, `gh pr comment`, edits the status comment           | Pull requests write       |
| Review gate | reads the pull request's commits and compares it with `main`       | Contents read             |
| Review gate | reads an editor's permission through `collaborators/<user>/permission` | not determined        |
| Bot comment | posts comments, edits the status comment, posts reviews, requests a reviewer, resolves review threads | Pull requests write |

Gaps:

- Comments on a pull request go through the issues API
  (`issues/<n>/comments`). Whether Pull requests write covers them or the App
  also needs Issues write is not recorded here.
- The permission that lets an App read `collaborators/<user>/permission` is
  not recorded here. Without it, `fab gate` treats every editor but the bot
  as lacking write access, and an owner's edit to a reviewed pull request
  does not post a new squash message.

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
- Require the `storm/review` check, which only the Review gate sets, and the
  repository's own CI checks.
- Block force pushes and deletion.

Leave the App off the ruleset's bypass list, so no token minted from it can
merge past these rules.

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
