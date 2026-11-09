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

| Workflow       | Calls                                                              | Permission it needs       |
| -------------- | ------------------------------------------------------------------ | ------------------------- |
| Branch PR open | `gh pr list`, `gh pr create --draft`                               | Pull requests write       |
| Review gate    | reads and creates `storm/review` check runs                        | Checks write              |
| Review gate    | `gh pr ready`, `gh pr comment`, edits the status comment           | Pull requests write       |
| Review gate    | reads the pull request's commits through `pulls/<n>/commits`       | Pull requests read        |
| Review gate    | reads an editor's permission through `collaborators/<user>/permission` | Metadata read         |
| Bot comment    | posts comments, edits the status comment, posts reviews, requests a reviewer, resolves review threads | Pull requests write |

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
`secret_prefix` from `.github/fabrial.toml`. `fab init` writes `FABRIAL`
unless you choose another, and a file without the setting reads
`THUNDERBOT`:

| Secret                 | Holds                 | Read by                                  |
| ---------------------- | --------------------- | ---------------------------------------- |
| `<PREFIX>_APP_ID`      | the App ID            | Branch PR open, Review gate, Bot comment |
| `<PREFIX>_PRIVATE_KEY` | the whole `.pem` file | Branch PR open, Review gate, Bot comment |

Add them as repository or organization secrets. Those reach every job on
every ref, so anyone who can push a workflow change to a branch can read
the App's key from that branch, as
[Who can use the key](#who-can-use-the-key) says.

### Keep them in an environment

An environment open only to `main` closes that. It is opt-in:

1. Under the repository's **Settings > Environments**, create an
   environment, such as `fabrial`.
2. Under **Deployment branches and tags**, choose **Selected branches and
   tags**, and add the branch `main` and the tag pattern `v*`.
3. Under **Environment secrets**, add the two secrets.
4. Run `fab update --environment <name>` and merge its pull request.
5. Only then delete the repository secrets of the same names, and take the
   repository off the organization secrets' access list. Before the merge,
   the workflows on `main` name no environment and need them.

Each job that mints the App's token then names the environment, with
`deployment: false` so a run records no deployment. GitHub documents
environment branch rules as applying to such jobs, and a custom deployment
protection rule as failing them. Once it merges, confirm that a dispatch
of Bot comment on a branch cannot read the secrets. GitHub Free offers
environment secrets and branch rules only in public repositories.

To use secrets you already have, such as `MY_BOT_APP_ID` and
`MY_BOT_PRIVATE_KEY`, pass `fab init --secret-prefix MY_BOT`, or change it
later with `fab update --secret-prefix MY_BOT`. The prefix is uppercase
letters, digits and underscores, and cannot start with a digit or
`GITHUB_`. `--environment` names an environment the same way; its name is
ASCII letters, digits, `-` and `_`. `fab init` and `fab update` write both
into the installed workflows, so `fab check` reports a workflow that reads
others.

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
- Branch PR open, the Review gate and a Bot comment run that sets a status
  section stop with an error when the secrets belong to an App other than
  the one `app` names.

## Moving from an older release

fabrial 0.9.0 and earlier read `THUNDERBOT_APP_ID` and
`THUNDERBOT_PRIVATE_KEY`. `fab update` on an install with no
`secret_prefix` writes `THUNDERBOT`, so the workflows keep reading those and
there is nothing to rename. An install with no `app` needs
`fab update --app <slug>`.

`fab update` adds no `environment`; the workflows keep reading repository
and organization secrets.

The update to this release adds `branch-pr-open.yml`, which GitHub runs only
from `main`, so its pull request does not open itself. To have the bot open
it:

1. Commit the update with `main`'s copy of Branch PR, by running
   `git checkout origin/main -- .github/workflows/branch-pr.yml` before the
   commit.
2. Push the branch. `main`'s Branch PR opens the pull request on the push
   that creates the branch; for a branch that already exists, run
   `gh workflow run branch-pr.yml --ref <branch>`.
3. Once the pull request is open, run `fab update` again, commit the new
   `branch-pr.yml`, and push.

Someone other than the code owner can open it by hand instead; the code
owner cannot approve a pull request they opened.

## Who can use the key

A workflow runs from the copy of its file that GitHub picks for the event,
and a push or a dispatch picks the copy on the branch. A branch that edits
that copy controls what the job does with any secret it can read.

- Branch PR (`branch-pr.yml`) runs on push from the pushed branch's copy,
  so it reads no secrets. It decides whether to open a pull request and
  nothing else.
- Branch PR open (`branch-pr-open.yml`) runs on `workflow_run` when Branch
  PR finishes. GitHub runs it from `main`'s copy, with `main` as the ref,
  whatever branch triggered it. It treats the branch as data: `fab branch-pr`
  refuses a name outside `claude/`, `codex/` and `storm/`, and git reads the
  branch's commits without running anything from it. An edited Branch PR
  can at most make it open a pull request for that branch.
- The Review gate runs on `pull_request_target` and `issue_comment`, both
  from `main`'s copy with `main` as the ref.
- Bot comment runs on dispatch, from the copy on the ref dispatched. Its job
  runs only on `main`, but a branch's copy can drop that check and read
  repository and organization secrets.

Without an environment, anyone who can push a workflow change to a branch
can read the App's key. With the secrets kept in an environment as
[Keep them in an environment](#keep-them-in-an-environment) says, no copy
of a workflow a branch can edit reaches the key. Pushing a workflow change still
takes the `workflow` scope on a personal token, Workflows write on an App
token, or write access over SSH. Anyone who can push to `main`, or change
the environment's rules, still holds the key.

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
  ruleset counts them unless the source is set. A branch can post one as
  the App unless the App's secrets live in an environment, as
  [Who can use the key](#who-can-use-the-key) says.
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

fabrial's App is stormlight-thunderbot, and its secrets are the
organization secrets `THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY`, with
no environment. Its release workflows need more than the table above, and an
installing repository does not run them: Release PR and Release tag push a
branch and a tag with Contents write, and the Homebrew publish job mints a
token with Contents write on stormlightlabs/homebrew-tap. Release PR and
Release tag run only on `main`. The Homebrew job runs on the release's tag.

fabrial's own `main` requires the `check` job, owner approval with code owner
review, and squash merges. It does not require `storm/review`.

fabrial's `v*` tags start its Release workflow, and a job on one can read
the App's secrets. A tag ruleset on `v*` that blocks creating, updating and
deleting tags, with the owner and the App on its bypass list, keeps other
writers from publishing a release or reaching the key. An installing
repository needs this only if it publishes on tags too.
