# One task, from claim to merge

This page follows one task through fabrial and names who acts at each step.
Four parties take part:

- The driver is the person who asks a session for the work.
- The agent session runs the `fab-` skills in Claude Code, Codex or Pi.
- The bot is stormlight-thunderbot, acting through the Branch PR, Bot comment
  and Review gate workflows.
- The owner is the person `owner` in `.github/fabrial.toml` names, who
  approves and merges. The driver and the owner can be the same person.

The loop ends when the session posts `Review finished at <sha>`. What happens
after that belongs to the driver and the owner.

## 1. Claim

The driver asks a session to take a task, with `/fab-storm <task>` or
`/fab-implement <task>`. The session reads it with the commands the
**Tasks** section of `AGENTS.md` names.

A tracked task gets a branch named for it. The session checks that no pull
request has it, then runs `fab claim take <id> --run <run>`, which creates
`storm/<id>` on origin only if it does not exist yet. A claim lasts an hour,
and `fab claim renew` extends it. An issue or a description needs no claim:
the session works on a `claude/<topic>` or `codex/<topic>` branch.

## 2. Implement and push

The session, following `fab-implement`, makes the change and its test in the
task's worktree, runs `fab gates`, commits and pushes. The subject of the
first commit that is not a claim commit becomes the pull request title, and
its body the pull request body.

## 3. The bot opens a draft

The push starts the Branch PR workflow, which runs `fab branch-pr` and opens
the pull request as a draft from the bot. On a `storm/` branch this happens
on the first push that adds work above the claim commits.

Opening the pull request starts the Review gate, which sets `storm/review` to
pending on the head. Any CI the repository runs, such as fabrial's Check
workflow, starts as well.

## 4. Review

The session, following `fab-storm`, pins the head and runs
`fab sequence --head <sha>`. It prints the review depth from
`fab review-depth` and the stages that depth gives, read from
`.github/fabrial-sequence.toml` at the merge base with `main`:

| Depth      | Pull requests                                         | Stages                                  |
| ---------- | ----------------------------------------------------- | --------------------------------------- |
| `none`     | version bumps                                         | none                                    |
| `standard` | documentation, planning records and tests             | standard review and its revise          |
| `full`     | everything else                                       | both reviews and their revises          |

Each stage runs in a fresh agent. A reviewer uses `fab-review` and posts its
findings as one review through the Bot comment workflow with `review` set to
the head it read. A reviser uses `fab-revise`, pushes its fixes, and posts
its answers with `answers` set to `true`. The bot resolves each thread whose
answer starts with `Fixed` or `Deferred`. A revise runs only when its review
found something above a nit. The sequence runs once.

## 5. Review finished

The session completes a task whose tracker lives in the repository, runs
`fab gates --full`, checks that the pull request still merges, and posts a
comment through the Bot comment workflow whose first line is
`Review finished at <full sha>`.

The comment starts the Review gate. When the SHA names the head, it turns
`storm/review` green, marks the draft ready and writes the squash message
into the bot's status comment. Marking it ready makes GitHub request the
owner's review from `CODEOWNERS`. The session reports to the driver and stops.

A push after this sets `storm/review` back to pending, unless the push only
moved the base.

## 6. The owner merges

The owner reads the pull request and approves it. To accept a head the
review did not read, such as a release pull request or a push after
`Review finished`, a user with write access comments `/storm:ready <sha>`
or `/storm:done <sha>`. The gate treats it like `Review finished`, and
ignores the comment from anyone without write access. A comment naming another SHA gets a reply
asking for it again.

The owner merges with the squash message from the status comment: the title
and pull request number, the body above `<!-- fabrial:footer`,
`Merged from <branch>.`, any `Assisted-by` lines, the owner's
`Co-authored-by` trailer and, with `signoff` set, a `Signed-off-by` line.
Agents never approve or merge, and nothing turns on auto-merge but the owner.

If the driver wants another review, a fix for a failed check or answers to
the owner's comments, they ask the session, which runs that stage on the
current head.

## 7. Clean up

After the driver reports the merge, the session completes a task whose
tracker lives outside the repository, then runs `fab cleanup`. It removes
the worktree and local branch of each merged agent pull request whose
worktree is clean and whose tip matches the merged head, fast-forwards a
clean primary `main`, and lists open agent pull requests that need a resync.
