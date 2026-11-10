---
title: Setup commands
label: Setup
description: Install fabrial into a repository, refresh it, and report where it has drifted.
section: Reference
group: Commands
order: 1
---

These commands install fabrial into a repository and keep it current. Run them
at the repository root, or name the root with `--dir`.

## fab init

Install fabrial's workflows, gate, skills and agents into a repository that does
not have them yet.

```text
fab init [OPTIONS] --owner <OWNER> --app <APP> --trailer <TRAILER> --gates <GATES>
```

| Option                            | Default   | Description                                                                                                                                                                      |
| --------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--dir <DIR>`                     | `.`       | The repository root.                                                                                                                                                             |
| `--owner <OWNER>`                 | required  | GitHub login of the person who approves pull requests.                                                                                                                           |
| `--app <APP>`                     | required  | The slug of the repository's GitHub App, which the workflows act as: `my-bot` for `github.com/apps/my-bot`.                                                                      |
| `--secret-prefix <SECRET_PREFIX>` | `FABRIAL` | The prefix of the secrets holding the App's ID and key: the workflows read `<PREFIX>_APP_ID` and `<PREFIX>_PRIVATE_KEY`.                                                         |
| `--environment <ENVIRONMENT>`     | unset     | A GitHub environment for the jobs that mint the App's token to name; keep the secrets there, restricted to `main`. Without it the jobs read repository and organization secrets. |
| `--trailer <TRAILER>`             | required  | The squash commit's `Co-authored-by` value, as `Name <email>`.                                                                                                                   |
| `--gates <GATES>`                 | required  | The command agents run before pushing.                                                                                                                                           |
| `--signoff <SIGNOFF>`             | unset     | The owner's `Signed-off-by` value, as `Name <email>`, to end every squash commit with; for repositories that use the DCO.                                                        |
| `--assisted-by <ASSISTED_BY>`     | unset     | `on` adds an `Assisted-by` line naming each agent model to every squash commit. Takes `on` or `off`.                                                                             |
| `--force`                         | off       | Replace files fabrial did not write.                                                                                                                                             |

## fab update

Replace an installed copy's files with this build's, keeping its settings unless
a flag changes one. Exits 1 when a file this release no longer installs could
not be removed.

```text
fab update [OPTIONS]
```

| Option                            | Default | Description                                                                                                                 |
| --------------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------- |
| `--dir <DIR>`                     | `.`     | The repository root.                                                                                                        |
| `--owner <OWNER>`                 | unset   | A new GitHub login for the person who approves pull requests.                                                               |
| `--app <APP>`                     | unset   | The slug of the repository's GitHub App; an install without one needs it.                                                   |
| `--secret-prefix <SECRET_PREFIX>` | unset   | A new prefix for the secrets holding the App's ID and key. An install without one keeps `THUNDERBOT`, the names 0.9.0 read. |
| `--environment <ENVIRONMENT>`     | unset   | A new GitHub environment for the jobs that mint the App's token. An install without one keeps naming none.                  |
| `--trailer <TRAILER>`             | unset   | A new `Co-authored-by` value for the squash commit.                                                                         |
| `--gates <GATES>`                 | unset   | A new command for agents to run before pushing.                                                                             |
| `--signoff <SIGNOFF>`             | unset   | A new `Signed-off-by` value for the squash commit; an empty value removes it.                                               |
| `--assisted-by <ASSISTED_BY>`     | unset   | Whether the squash commit names agent models. Takes `on` or `off`.                                                          |
| `--force`                         | off     | Install this build's files over a newer release's.                                                                          |

## fab check

Report where an installed copy differs from this build; exits 1 when there is
any drift.

```text
fab check [OPTIONS]
```

| Option        | Default | Description          |
| ------------- | ------- | -------------------- |
| `--dir <DIR>` | `.`     | The repository root. |
