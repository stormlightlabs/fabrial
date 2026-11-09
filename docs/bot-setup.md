# Set up the repository's App

The workflows `fab init` installs act as a GitHub App that belongs to you:
your user or your organization. The App opens each agent pull request,
posts the bot's comments and sets the `storm/review` check. A repository
needs four things before its first agent pull request:

1. An App with the permissions below, installed on the repository.
2. Two secrets holding the App's ID and private key.
3. The App's slug as `app` in `.github/fabrial.toml`.
4. A ruleset on `main`.

Each name below comes from the workflows in `.github/workflows/`, the code of
the `fab` commands they run, or the settings file.

## Create the App

Create it under **Settings > Developer settings > GitHub Apps > New GitHub
App**, in your account or your organization's settings:

- Name it as you like. Its page is `github.com/apps/<slug>`, and the slug
  in that address is what `app` in `.github/fabrial.toml` names. Its
  comments come from the login `<slug>[bot]`.
- Give it any homepage URL, such as the repository's.
- Clear **Active** under Webhook. fabrial reads no webhook events.
- Grant the repository permissions in the table below, and nothing else.
- Under **Where can this GitHub App be installed?**, choose **Only on this
  account**.

Then generate a private key on the App's page. GitHub downloads it as a
`.pem` file. Note the App ID at the top of the same page.

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

The Pi harness's `fab dispatch` mints worker tokens from the same App and
needs other permissions; [docs/pi.md](pi.md#github-app) lists them.

## Install it

On the App's page, choose **Install App**, then the account, then the
repositories that use fabrial. Each workflow mints a token for the App with
`actions/create-github-app-token`. A comment posted with that token, such as
`Review finished`, starts the Review gate, which a comment posted with
`GITHUB_TOKEN` would not.

## Add the secrets

The workflows read two secrets and no variables. Their names start with
`secret_prefix` from `.github/fabrial.toml`, `FABRIAL` unless you choose
another:

| Secret                 | Holds                 | Read by                             |
| ---------------------- | --------------------- | ----------------------------------- |
| `<PREFIX>_APP_ID`      | the App ID            | Branch PR, Review gate, Bot comment |
| `<PREFIX>_PRIVATE_KEY` | the whole `.pem` file | Branch PR, Review gate, Bot comment |

Add them under the repository's **Settings > Secrets and variables >
Actions**, or once as organization secrets shared with each repository that
uses fabrial. To use secrets you already have, such as `MY_BOT_APP_ID` and
`MY_BOT_PRIVATE_KEY`, pass `fab init --secret-prefix MY_BOT`, or change it
later with `fab update --secret-prefix MY_BOT`. The prefix is uppercase
letters, digits and underscores, and cannot start with a digit or
`GITHUB_`. `fab init` and `fab update` write the names into the installed
workflows, so `fab check` reports a workflow that reads others.

Agent sessions never hold the key. They post through the Bot comment
workflow.

## Name it in the settings

`app` in `.github/fabrial.toml` holds the App's slug:

```toml
app = "my-bot"
```

`fab init --app my-bot` writes it, and `fab update --app my-bot` sets or
changes it. The slug is lowercase letters, digits and hyphens, without
`[bot]`. The workflows read the file from `main`, never from a pull
request's head:

- The Review gate accepts `Review finished at` from `<app>[bot]` without
  checking its access, and from no other App.
- The gate counts only `storm/review` runs the App posted.
- The status comment is the one `<app>[bot]` posted.
- Branch PR, the Review gate and a Bot comment run that sets a status
  section stop with an error when the secrets belong to an App other than
  the one `app` names.

## Moving from an older release

fabrial 0.9.0 and earlier read `THUNDERBOT_APP_ID` and
`THUNDERBOT_PRIVATE_KEY`. `fab update` on an install with no
`secret_prefix` writes `THUNDERBOT`, so the workflows keep reading those and
there is nothing to rename. An install with no `app` needs
`fab update --app <slug>`.

## Who can use the key

Installing the App does not keep its key from anyone who can push a
workflow change. Branch PR runs on push from the pushed branch's own copy of
`branch-pr.yml`, with both secrets. A branch that edits that file can mint
the App's token, or read the key, before any review, and post a green
`storm/review` as the App. Pushing a workflow file takes the `workflow`
scope on a personal token, Workflows write on an App token, or write access
over SSH; Pi workers' tokens have none of these. Code owners on
`.github/workflows/` do not help, since the edit runs before any pull
request merges. Treat everyone who can push a workflow change to a branch
as holding the App's key.

## The ruleset on `main`

The workflows assume a ruleset on `main` with these rules:

- Require a pull request with one approval and a review from code owners.
  `fab init` writes `CODEOWNERS` from `owner` in `.github/fabrial.toml`, and
  the App opens every pull request, so the owner is never its author and can
  approve.
- Allow only squash merges. The gate posts the message for that squash
  commit.
- Require the `storm/review` check and the repository's own CI checks. Set
  the source of `storm/review` to your App. Any App with Checks write can
  post a check of that name, including `GITHUB_TOKEN` in a workflow on a
  branch that asks for `checks: write`. The gate ignores those runs, but the
  ruleset counts them unless the source is set. Setting it does not stop a
  branch that edits Branch PR, as [Who can use the key](#who-can-use-the-key)
  says.
- Block force pushes and deletion.

Leave the App off the ruleset's bypass list, so no token minted from it can
merge past these rules.

`/storm:ready`, `/storm:done`, a person's `Review finished at` and an edit
to the title or body count only from a user with write, maintain or admin
access. The gate ignores the rest without a reply.

The gate runs from `main` through `pull_request_target`. On the pull request
that installs fabrial it does nothing, so mark that draft ready yourself and
add `storm/review` to the ruleset only after it merges.

The workflows install the `fab` release that `version` in
`.github/fabrial.toml` names, from fabrial's GitHub releases, so the runners
need to reach github.com.

## fabrial's own repository

fabrial's App is stormlight-thunderbot, and its secrets are organization
secrets in stormlightlabs. Its release workflows need more than the table
above, and an installing repository does not run them: Release PR and
Release tag push a branch and a tag with Contents write, and the Homebrew
publish job mints a token with Contents write on stormlightlabs/homebrew-tap.

fabrial's own `main` requires the `check` job, owner approval with code owner
review, and squash merges. It does not require `storm/review`.

fabrial's `v*` tags start its Release workflow. A tag ruleset on `v*` that
blocks creating, updating and deleting tags, with the owner and the App on
its bypass list, keeps other writers from publishing a release. An
installing repository needs this only if it publishes on tags too.
