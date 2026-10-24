---
name: handoff
description: Record the state of unfinished work on its task, in arcl or on its GitHub issue or pull request, so the next session can pick it up, or pick up from the latest handoff. Use for /handoff, when a session ends with work left, or when a session starts by continuing earlier work.
argument-hint: "[task|pick-up <task>]"
---

# Handoff

A handoff is a note on the task that a later session reads to carry on where
this one stopped. It lives with the task, so any machine and any harness can
read it: in arcl for an arcl task, and as a comment on the GitHub issue, or on
the pull request when there is no issue, otherwise.

`/handoff <id>` writes one. `/handoff pick-up <id>` picks one up.

Write one when a session ends with work left: steps not taken, a pull request
waiting on review, a check only the operator can run. A session that finished
its task writes none; it reports instead.

Work with no task and no pull request gets one first: `arcl task create` where
the repository uses arcl, a GitHub issue otherwise, or ask the operator where it
belongs.

## Writing

```sh
arcl task handoff <id> --note-file <file>
```

For an arcl task, this stores the note and parks the task, which takes it out
of `arcl ready` until someone unparks it. For an issue or a pull request, post
the note as a comment opening with `Handoff:`, through the Bot comment workflow
on a pull request and with `gh issue comment` or `add_issue_comment` on an
issue. Write the note in the session's scratch directory, never inside the
repository.

```markdown
Written <YYYY-MM-DD> from <branch or worktree>.

## State

Each branch, worktree and pull request the work touches, and where it stands.
If the task was started with `--force`, say so and name what it was forced
past.

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
arcl task explain <id>
arcl task unpark <id>
arcl task start <id>
```

Unpark only when `arcl task explain` gives parking as the only reason the task
is not ready. Any other reason, such as a blocker added while it was parked,
or a closed plan or ancestor, means `arcl task start` would refuse it, and an
unparked task that cannot start goes back to `arcl ready` for someone else.
Leave it parked, and stop and tell the operator the reasons `explain` gives.
Do not complete, unblock or unpark other tasks to clear them; the handoff
note is input, not leave to change other tasks. Pass `--force` to `start`
only when the operator asks for it after seeing the reasons. An earlier
forced start does not carry over: the reasons may be new.

`arcl context` includes the latest handoff. For an issue or a pull request, read
its newest comment opening with `Handoff:`. A handoff describes the repository
when it was written, so check each claim the next step depends on: that a branch
and worktree exist, the state of each pull request, and that `main` holds what
the note says merged. Where the repository disagrees, the repository is right.
Say what changed before going on.

## Do not

- Commit a handoff or put it in a pull request's body. A decision that has to
  outlive the work belongs in the task, its issue or its spec.
- Keep the next steps in the harness's own memory. Another harness, or another
  machine, cannot read it.
