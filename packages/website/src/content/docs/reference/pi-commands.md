---
title: Pi harness commands
label: Pi harness
description: "The commands that run Pi workers: fab run, fab dispatch, fab worker and fab events."
section: Reference
group: Commands
order: 4
---

These drive the Pi harness, which runs each step of a task in a fresh, sandboxed
Pi worker in a tmux session on the host. They exist on Unix only.

## fab run

Take one task through the fixed sequence: claim it, give it a worktree and run
one Pi worker per step, ending with `Review finished`.

```text
fab run [OPTIONS] --trusted-checkout <TRUSTED_CHECKOUT> --config <CONFIG> <TASK>
```

| Option                                  | Default  | Description                                                                                                           |
| --------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------- |
| `<TASK>`                                | required | A task ID for the `[tasks]` commands, an issue as `#<n>`, or a file holding a description.                            |
| `--trusted-checkout <TRUSTED_CHECKOUT>` | required | The trusted checkout `fab dispatch` reads roles and skills from.                                                      |
| `--config <CONFIG>`                     | required | The host's dispatch configuration.                                                                                    |
| `--timeout <TIMEOUT>`                   | `1800`   | Seconds each worker may run.                                                                                          |
| `--host-gates`                          | off      | Run the gates on the host before completing the task. They run code the workers wrote, beside the host's credentials. |

## fab dispatch

Run a fresh isolated Pi review worker (host tmux backend).

```text
fab dispatch [OPTIONS] --task <TASK> --worktree <WORKTREE> --trusted-checkout <TRUSTED_CHECKOUT> --config <CONFIG> <ROLE>
```

| Option                                  | Default  | Description                                                                                                                                    |
| --------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `<ROLE>`                                | required | The agent role the worker runs, a file in `.claude/agents/`. Takes `implementer`, `reviewer`, `verifier`, `adversarial-reviewer` or `reviser`. |
| `--task <TASK>`                         | required | A file holding the task the worker is given.                                                                                                   |
| `--worktree <WORKTREE>`                 | required | The task's linked worktree, on its agent branch.                                                                                               |
| `--trusted-checkout <TRUSTED_CHECKOUT>` | required | The trusted checkout the role and skills are read from.                                                                                        |
| `--config <CONFIG>`                     | required | The host's dispatch configuration.                                                                                                             |
| `--timeout <TIMEOUT>`                   | `1800`   | Seconds the worker may run, from 1 to 3000.                                                                                                    |
| `--detach`                              | off      | Return the worker identity once tmux accepts the launch.                                                                                       |

## fab worker

Inspect, cancel, or explicitly wait for an independently supervised worker.

```text
fab worker <COMMAND>
```

### fab worker list

List durable worker metadata, without result bodies or credentials.

```text
fab worker list --config <CONFIG>
```

| Option              | Default  | Description                        |
| ------------------- | -------- | ---------------------------------- |
| `--config <CONFIG>` | required | The host's dispatch configuration. |

### fab worker recover

Recover cleanup after the supervisor has stopped; never restart a stage.

```text
fab worker recover --config <CONFIG> <WORKER_ID>
```

| Option              | Default  | Description                                                        |
| ------------------- | -------- | ------------------------------------------------------------------ |
| `<WORKER_ID>`       | required | The worker's ID, as `fab dispatch` or `fab worker list` prints it. |
| `--config <CONFIG>` | required | The host's dispatch configuration.                                 |

### fab worker inspect

Read the last recorded state and any final result.

```text
fab worker inspect --config <CONFIG> <WORKER_ID>
```

| Option              | Default  | Description                                                        |
| ------------------- | -------- | ------------------------------------------------------------------ |
| `<WORKER_ID>`       | required | The worker's ID, as `fab dispatch` or `fab worker list` prints it. |
| `--config <CONFIG>` | required | The host's dispatch configuration.                                 |

### fab worker cancel

Request cancellation of one unfinished worker.

```text
fab worker cancel --config <CONFIG> <WORKER_ID>
```

| Option              | Default  | Description                                                        |
| ------------------- | -------- | ------------------------------------------------------------------ |
| `<WORKER_ID>`       | required | The worker's ID, as `fab dispatch` or `fab worker list` prints it. |
| `--config <CONFIG>` | required | The host's dispatch configuration.                                 |

### fab worker wait

Explicitly wait for one worker; ending this wait does not cancel it.

```text
fab worker wait [OPTIONS] --config <CONFIG> <WORKER_ID>
```

| Option                | Default  | Description                                                        |
| --------------------- | -------- | ------------------------------------------------------------------ |
| `<WORKER_ID>`         | required | The worker's ID, as `fab dispatch` or `fab worker list` prints it. |
| `--config <CONFIG>`   | required | The host's dispatch configuration.                                 |
| `--timeout <TIMEOUT>` | `3000`   | Seconds to wait, from 1 to 3600.                                   |

### fab worker events

Read a bounded batch of events after a byte cursor.

```text
fab worker events [OPTIONS] --config <CONFIG> <WORKER_ID>
```

| Option              | Default  | Description                                                          |
| ------------------- | -------- | -------------------------------------------------------------------- |
| `<WORKER_ID>`       | required | The worker's ID, as `fab dispatch` or `fab worker list` prints it.   |
| `--config <CONFIG>` | required | The host's dispatch configuration.                                   |
| `--after <AFTER>`   | `0`      | Print events after this byte cursor, the `cursor` of the last batch. |

### fab worker steer

Send driver-authorized steering to this worker at its starting head.

```text
fab worker steer --config <CONFIG> --head <HEAD> --message <MESSAGE> <WORKER_ID>
```

| Option                | Default  | Description                                                            |
| --------------------- | -------- | ---------------------------------------------------------------------- |
| `<WORKER_ID>`         | required | The worker's ID, as `fab dispatch` or `fab worker list` prints it.     |
| `--config <CONFIG>`   | required | The host's dispatch configuration.                                     |
| `--head <HEAD>`       | required | The head the worker started from; a worker at another head is refused. |
| `--message <MESSAGE>` | required | The steering message, 1 to 16384 bytes.                                |

## fab events

Print run events from dispatch's records as JSON lines, one per event, each with
the cursor to resume after it.

```text
fab events [OPTIONS] --config <CONFIG>
```

| Option              | Default  | Description                                                            |
| ------------------- | -------- | ---------------------------------------------------------------------- |
| `--config <CONFIG>` | required | The host configuration whose `state_dir` holds dispatch's records.     |
| `--after <AFTER>`   | empty    | Print events after this cursor, the `cursor` of the last event seen.   |
| `--follow`          | off      | Keep reading until interrupted, printing events as workers write them. |
