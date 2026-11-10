---
title: Workflow commands
label: Workflows
description: "Fabrial's workflow commands: the review gate, Branch PR, the bot's comments and reviews, and fabrial's release steps."
section: Reference
group: Commands
order: 3
---

fabrial's GitHub workflows run these. When one fails in GitHub Actions it prints
the error as an annotation. Most read their input from variables the workflow
sets, so run them by hand only to test a workflow.

## fab gate

Set the `storm/review` check for a pull request event; the Review gate workflow
runs it from a checkout of `main`.

```text
fab gate [OPTIONS] --repo <REPO> --pr <PR> --event <EVENT>
```

| Option                  | Default                        | Description                                                                                                                  |
| ----------------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `--repo <REPO>`         | required unless `REPO` is set  | The repository, as owner/name. Read from `REPO` when not given.                                                              |
| `--pr <PR>`             | required unless `PR` is set    | The pull request number. Read from `PR` when not given.                                                                      |
| `--event <EVENT>`       | required unless `EVENT` is set | The event to answer. Takes `push`, `comment` or `edit`. Read from `EVENT` when not given.                                    |
| `--head-sha <HEAD_SHA>` | unset                          | The head to act on; without it, the pull request's current head. Read from `HEAD_SHA` when not given.                        |
| `--head-dir <HEAD_DIR>` | `head`                         | A checkout of the head, with origin/main fetched. Read from `HEAD_DIR` when not given.                                       |
| `--action <ACTION>`     | unset                          | The pull request event's action, such as `reopened`. Read from `ACTION` when not given.                                      |
| `--before <BEFORE>`     | unset                          | The head before a push. Read from `BEFORE` when not given.                                                                   |
| `--body <BODY>`         | unset                          | The comment's text. Read from `BODY` when not given.                                                                         |
| `--author <AUTHOR>`     | unset                          | Who wrote the comment. Read from `AUTHOR` when not given.                                                                    |
| `--sender <SENDER>`     | unset                          | Who edited the pull request. Read from `SENDER` when not given.                                                              |
| `--app-slug <APP_SLUG>` | unset                          | The slug of the App whose token the workflow made, checked against the settings' `app`. Read from `APP_SLUG` when not given. |
| `--dry-run`             | off                            | Print writes to GitHub instead of sending them. `DRY_RUN=1` sets it.                                                         |

## fab branch-pr

Open a draft pull request for an agent's work branch; the Branch PR workflow
runs it.

```text
fab branch-pr [OPTIONS] --repo <REPO> --branch <BRANCH>
```

| Option                  | Default                         | Description                                                                                                                  |
| ----------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `--repo <REPO>`         | required unless `REPO` is set   | The repository, as owner/name. Read from `REPO` when not given.                                                              |
| `--branch <BRANCH>`     | required unless `BRANCH` is set | The branch to open a pull request for. Read from `BRANCH` when not given.                                                    |
| `--head-dir <HEAD_DIR>` | `head`                          | A checkout of the branch, with origin/main fetched. Read from `HEAD_DIR` when not given.                                     |
| `--app-slug <APP_SLUG>` | unset                           | The slug of the App whose token the workflow made, checked against the settings' `app`. Read from `APP_SLUG` when not given. |
| `--dry-run`             | off                             | Print the pull request instead of opening it. `DRY_RUN=1` sets it.                                                           |

## fab pr-status

Set a section of the pull request's status comment; the Bot comment workflow
runs it.

```text
fab pr-status [OPTIONS] --repo <REPO> --pr <PR> --section <SECTION> --body-file <BODY_FILE>
```

| Option                    | Default                          | Description                                                                                                                  |
| ------------------------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `--repo <REPO>`           | required unless `REPO` is set    | The repository, as owner/name. Read from `REPO` when not given.                                                              |
| `--pr <PR>`               | required unless `PR` is set      | The pull request number. Read from `PR` when not given.                                                                      |
| `--section <SECTION>`     | required unless `SECTION` is set | The section to set, such as `Standard pass`. Read from `SECTION` when not given.                                             |
| `--body-file <BODY_FILE>` | required                         | A file holding the section's text.                                                                                           |
| `--app-slug <APP_SLUG>`   | unset                            | The slug of the App whose token the workflow made, checked against the settings' `app`. Read from `APP_SLUG` when not given. |
| `--dry-run`               | off                              | Print the write instead of sending it. `DRY_RUN=1` sets it.                                                                  |

## fab pr-review

Post a review pass's report as one review, its findings on the lines they name;
the Bot comment workflow runs it.

```text
fab pr-review [OPTIONS] --repo <REPO> --pr <PR> --head <HEAD> --body-file <BODY_FILE>
```

| Option                    | Default                           | Description                                                                      |
| ------------------------- | --------------------------------- | -------------------------------------------------------------------------------- |
| `--repo <REPO>`           | required unless `REPO` is set     | The repository, as owner/name. Read from `REPO` when not given.                  |
| `--pr <PR>`               | required unless `PR` is set       | The pull request number. Read from `PR` when not given.                          |
| `--head <HEAD>`           | required unless `HEAD_SHA` is set | The full SHA of the head the pass reviewed. Read from `HEAD_SHA` when not given. |
| `--body-file <BODY_FILE>` | required                          | A file holding the pass's report.                                                |
| `--dry-run`               | off                               | Print the write instead of sending it. `DRY_RUN=1` sets it.                      |

## fab pr-answer

Post a revise pass's answers as replies on the findings' threads, in one review,
resolve the threads they close and request a person's review again; the Bot
comment workflow runs it.

```text
fab pr-answer [OPTIONS] --repo <REPO> --pr <PR> --body-file <BODY_FILE>
```

| Option                    | Default                       | Description                                                                                                                                                               |
| ------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--repo <REPO>`           | required unless `REPO` is set | The repository, as owner/name. Read from `REPO` when not given.                                                                                                           |
| `--pr <PR>`               | required unless `PR` is set   | The pull request number. Read from `PR` when not given.                                                                                                                   |
| `--body-file <BODY_FILE>` | required                      | A file holding the answers: a line `<comment id> · <outcome>` per finding, and any other lines for the review's body.                                                     |
| `--reviewer <REVIEWER>`   | unset                         | The GitHub login of the person whose review the answers address. Once the answers post, the bot requests that person's review again. Read from `REVIEWER` when not given. |
| `--dry-run`               | off                           | Print the writes instead of sending them. `DRY_RUN=1` sets it.                                                                                                            |

## fab release

fabrial's own release steps, which its release workflows run.

```text
fab release <COMMAND>
```

### fab release changelog

Write the release's section of `CHANGELOG.md` from the commit subjects since the
last release tag.

```text
fab release changelog <VERSION> <DATE>
```

| Option      | Default  | Description                           |
| ----------- | -------- | ------------------------------------- |
| `<VERSION>` | required | The release, such as 0.2.0.           |
| `<DATE>`    | required | The release date, such as 2026-10-26. |

### fab release check-tag

Print the version to tag on a commit on `main`, or nothing when the commit is
not a release merge.

```text
fab release check-tag [OPTIONS] --repo <REPO>
```

| Option              | Default                       | Description                                                       |
| ------------------- | ----------------------------- | ----------------------------------------------------------------- |
| `--repo <REPO>`     | required unless `REPO` is set | The repository, as owner/name. Read from `REPO` when not given.   |
| `--commit <COMMIT>` | `HEAD`                        | The commit, checked out in the working directory with its parent. |
