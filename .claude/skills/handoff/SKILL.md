---
name: handoff
description: Record the state of unfinished work on its arcl task so the next session can pick it up, or pick up from a task's latest handoff. Use for /handoff, when a session ends with work left, or when a session starts by continuing earlier work.
argument-hint: [task-id|pick-up <task-id>]
---

# Handoff

A handoff is a note on the arcl task that a later session reads to carry on
where this one stopped. arcl stores it with the task, so any machine and any
harness can read it.

`/handoff <id>` writes one. `/handoff pick-up <id>` picks one up.

Write one when a session ends with work left: steps not taken, a pull request
waiting on review, a check only the operator can run. A session that finished
its task writes none; it reports instead.

Work with no task gets one first: `arcl task create` with the title of the
work, or ask the operator where it belongs.

## Writing

```sh
arcl task handoff <id> --note-file <file>
```

This stores the note and parks the task, which takes it out of `arcl ready`
until someone unparks it. Write the note in the session's scratch directory,
never inside the repository.

```markdown
Written <YYYY-MM-DD> from <branch or worktree>.

## State

Each branch, worktree and pull request the work touches, and where it stands.

## Done

What this session finished, by commit or pull request.

## Next

The steps left, in order, and what each one depends on.

## Waiting on the operator

Checks, merges and decisions only the operator can make.
```

Leave out a section with nothing in it. Record what a session cannot read back
cheaply; the task, the diff and the git log are already there to read. Use the
`writing-docs` skill for the prose.

## Picking up

```sh
arcl context <id>
arcl task unpark <id>
arcl task start <id>
```

`arcl context` includes the latest handoff. A handoff describes the repository
when it was written, so check each claim the next step depends on: that a
branch and worktree exist, the state of each pull request, and that `main`
holds what the note says merged. Where the repository disagrees, the
repository is right. Say what changed before going on.

## Do not

- Commit a handoff or copy it into a pull request. A decision that has to
  outlive the work belongs in the task or its spec.
- Keep the next steps in the harness's own memory. Another harness, or another
  machine, cannot read it.
