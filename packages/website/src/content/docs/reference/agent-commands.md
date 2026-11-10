---
title: Agent commands
label: Agents
description: "The commands agents and skills run on a task: claims, gates, review depth, the review sequence and cleanup."
section: Reference
group: Commands
order: 2
---

Agents, and the skills they follow, run these from a task's worktree or the
primary checkout. People run them too, most often `fab status` and
`fab cleanup`.

## fab claim

Take, renew or release a task's claim, its `storm/<task>` branch.

```text
fab claim <COMMAND>
```

### fab claim take

Claim a task for a run by creating its branch. Exits 0 when the run holds the
claim, a new one or one it already held, and 1 when another run holds it.

```text
fab claim take [OPTIONS] --run <RUN> <TASK>
```

| Option                | Default  | Description                                         |
| --------------------- | -------- | --------------------------------------------------- |
| `<TASK>`              | required | The task ID, as the tracker or person gives it.     |
| `--run <RUN>`         | required | The run that holds or takes the claim.              |
| `--machine <MACHINE>` | unset    | The machine the run is on; the hostname by default. |
| `--remote <REMOTE>`   | `origin` | The remote that holds the task's branch.            |
| `--ttl <TTL>`         | `60`     | Minutes until the claim expires, at least 1.        |

### fab claim renew

Extend a run's claim. Exits 1 when the run does not hold it.

```text
fab claim renew [OPTIONS] --run <RUN> <TASK>
```

| Option                | Default  | Description                                         |
| --------------------- | -------- | --------------------------------------------------- |
| `<TASK>`              | required | The task ID, as the tracker or person gives it.     |
| `--run <RUN>`         | required | The run that holds or takes the claim.              |
| `--machine <MACHINE>` | unset    | The machine the run is on; the hostname by default. |
| `--remote <REMOTE>`   | `origin` | The remote that holds the task's branch.            |
| `--ttl <TTL>`         | `60`     | Minutes until the claim expires, at least 1.        |

### fab claim release

Delete the branch of a run's claim when it holds nothing else, or, where the
remote refuses the delete, end the claim with one that has already expired.
Exits 1 when the run does not hold it or the branch holds work.

```text
fab claim release [OPTIONS] --run <RUN> <TASK>
```

| Option                | Default  | Description                                         |
| --------------------- | -------- | --------------------------------------------------- |
| `<TASK>`              | required | The task ID, as the tracker or person gives it.     |
| `--run <RUN>`         | required | The run that holds or takes the claim.              |
| `--machine <MACHINE>` | unset    | The machine the run is on; the hostname by default. |
| `--remote <REMOTE>`   | `origin` | The remote that holds the task's branch.            |

## fab status

List the claimed tasks on the remote's `storm/` branches: each claim's holder,
run and expiry, its open pull request, and with `--config` the step its latest
worker is at.

```text
fab status [OPTIONS]
```

| Option              | Default  | Description                                                                                                                |
| ------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------- |
| `--remote <REMOTE>` | `origin` | The remote that holds the tasks' branches.                                                                                 |
| `--repo <REPO>`     | unset    | The GitHub repository, as owner/name; read from the remote's URL by default.                                               |
| `--config <CONFIG>` | unset    | The host's dispatch configuration, to add each task's latest worker role and state from the records under its `state_dir`. |
| `--json`            | off      | Print one JSON object per task, one per line.                                                                              |

## fab gates

Run checks relevant to the change, from `.github/fabrial-gates.toml`, and exit
with the first failure's code.

```text
fab gates [OPTIONS]
```

| Option          | Default       | Description                                                   |
| --------------- | ------------- | ------------------------------------------------------------- |
| `--full`        | off           | Run the `full` command whatever changed.                      |
| `--base <BASE>` | `origin/main` | The base the change is measured from, through the merge base. |

## fab review-depth

Print how many review passes the diff takes: none, standard or full.

```text
fab review-depth [OPTIONS]
```

| Option          | Default       | Description                                    |
| --------------- | ------------- | ---------------------------------------------- |
| `--base <BASE>` | `origin/main` | The base to diff from, through the merge base. |
| `--head <HEAD>` | `HEAD`        | The head to classify.                          |

## fab sequence

Print the review stages the diff's depth gives, from
`.github/fabrial-sequence.toml`.

```text
fab sequence [OPTIONS]
```

| Option            | Default       | Description                                                               |
| ----------------- | ------------- | ------------------------------------------------------------------------- |
| `--base <BASE>`   | `origin/main` | The base to diff from, through the merge base.                            |
| `--head <HEAD>`   | `HEAD`        | The head to classify.                                                     |
| `--depth <DEPTH>` | unset         | Use this depth instead of the diff's. Takes `none`, `standard` or `full`. |

## fab cleanup

After a merge: remove merged agent worktrees and their branches, fast-forward a
clean main, and list open agent pull requests that need a resync. Exits 1 when
something was kept or needs a resync.

```text
fab cleanup [OPTIONS]
```

| Option          | Default | Description                                                                            |
| --------------- | ------- | -------------------------------------------------------------------------------------- |
| `--repo <REPO>` | unset   | The GitHub repository, as owner/name; without it, read from the `origin` remote's URL. |
| `--dry-run`     | off     | Print what would be done and change nothing, not even `origin/main`.                   |
