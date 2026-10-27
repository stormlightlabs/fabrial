# fabrial

A software factory that takes ready tasks through implement, review,
adversarial review and revise until a pull request is ready for a person to
merge. The CLI is `fab`. [PLAN.md](PLAN.md) holds the decisions. fabrial's own
open work lives in arclightning's `arcl`, as **Tasks** below says; nothing
fabrial ships depends on it.

## Delivery sequence

Run this sequence once per task: implement the task, including integration
checks and required gates, then run the review stages `fab sequence` prints
from `.github/fabrial-sequence.toml` for the pull request's review depth, then
hand the PR to the driver.

Check host tools, credentials, permissions, and isolation early. Run the smallest
real end-to-end integration smoke during implementation, before review. Report
missing prerequisites and host-only failures as soon as they appear.

Skip a fix stage when there are no actionable findings. After the final
fixes, run the required gates, post `Review finished at <head>`, and hand off.
Do not start another review cycle or wait for the driver's approval.

This sequence takes precedence over extra verifier stages or dispatch loops in
skills. Reviewers must validate their own findings. Extra agents, review passes,
probes, helper scripts, or polish require the driver's explicit request.
Apply corrections to the current stage without repeating completed stages.
If a blocker prevents finishing a stage, report it and hand off; do not dispatch
more agents to work around it. Never mark an incomplete review finished.

## Lifecycle cleanup

Cleanup is part of completing a worker or a merged task, not a separate task.
Before dispatch, record which processes, directories, and Nix roots the run owns.

When a worker exits, capture its result, stop its processes and pane, revoke its
token, and remove credentials and worker-owned temporary state. Keep the task
worktree for later stages. Retain recovery state only when cleanup cannot be
confirmed, and report the failure immediately. Do not remove shared resources
while another worker uses them.

When a PR merges, confirm its recorded head. Remove its worktree only if it is
clean and its branch tip matches that head, then delete the local branch and
release task-owned Nix roots and temporary artifacts. Fast-forward a clean
primary `main` to `origin/main`; never reset, stash, or discard the user's work.
Leave unrelated worktrees, host credentials, shared runtimes, and Nix profiles
alone. If cleanup is blocked, name what remains and why in the handoff.

`fab cleanup` does the merge steps from git and GitHub alone, so it works
after a restart, and lists the open agent pull requests to resync. A task owns
no Nix roots outside its worktree: the sandbox wrapper is built once from a
trusted checkout and shared. An untracked `result` link in a worktree keeps
the worktree until you delete it.

## Gates

```sh
just check    # fmt --check, clippy with warnings denied, tests, fab check
```

It needs Rust, Node 22.18 or later, git, and jq for the gate tests' fake
`gh`. `gates` in `.github/fabrial.toml` names this command for the skills.
CI runs the same recipe as the `check` job, which the ruleset on `main`
requires. Clippy warnings fail it whether or not your change caused them.

## Layout

```text
crates/fab-core   install and settings; later, the run operations
crates/fab-cli    the `fab` binary, including what workflows and skills run:
                  `fab gate`, `fab branch-pr`, `fab release`,
                  `fab review-depth`, `fab sequence`, `fab pr-status`,
                  `fab pr-review`, `fab pr-answer` and `fab cleanup`
packages/pi/      the Pi package: extensions/ holds workers, PR status and the
                  status area, lib/ their shared code, and worker.ts the
                  extension `fab dispatch` gives each worker
conf/sandbox/     worker.nix, the Nix sandbox wrapper, and its Keychain patch
package.json      the Pi package manifest, at the root so git installs find it
.claude/skills/   skills; .agents/skills links here for Codex and Pi
.claude/agents/   implementer, reviewer, adversarial-reviewer, verifier, reviser
.claude/hooks/    session-start.sh, which installs arcl in cloud sessions
.github/          CODEOWNERS and the Check, Branch PR and Review gate workflows
.github/actions/fab/  the action that puts `fab` on the PATH for workflows
.github/storm/    shot.sh, which captures a terminal UI as a PNG, and sgr.pl, its color filter
.github/fabrial.toml  the owner, the squash trailer and the gate command
.github/fabrial-sequence.toml  the review stages the storm skill runs
docs/hosts.md     what each host reads, and where they differ
docs/pi.md        how to set up a host and run Pi workers
docs/pi-design.md planned Pi ownership and PR status transport
PLAN.md           decisions, and how to read the open work
.arcl/            arcl's config; workspace/ holds the tasks, arcl.db is local
AGENTS.md         this file (CLAUDE.md links to it)
```

## Code

Follow these conventions, which arclightning keeps too:

- Document exported and contextually important items with rustdoc.
- Keep an `impl` next to its type, and avoid `unsafe`.
- Order items as constants, enums, structs, then free functions, with `pub`
  before private.
- Use at most one level of `super`.

## Tasks

The installed skills read this section for the tracker's commands. fabrial's
tasks live in arcl, in tracked files under `.arcl/workspace`; `arcl.db` is a
local cache. In a cloud session `.claude/hooks/session-start.sh` installs the
pinned `arcl` and builds the cache in the session's checkout only. Elsewhere,
install it as PLAN.md's **Work** says. Every task worktree, cloud sessions
included, needs `arcl init` before its first `arcl` command.

| Step              | Command                                                       |
| ----------------- | ------------------------------------------------------------- |
| Read a task       | `arcl task show <id>` and `arcl context <id>`                 |
| Check readiness   | `arcl ready`, and `arcl task explain <id>` for one task       |
| Start             | `arcl init`, then `arcl task start <id>`, in the task's worktree |
| Complete          | `arcl task complete <id> --evidence "Done in #<n> at <sha>: <summary>"` |
| Hand off          | `arcl task handoff <id> --note-file <file>`                   |
| Pick up           | `arcl context <id>`, `arcl task unpark <id>`, `arcl task start <id>` |
| Record new work   | `arcl task create --phase <id> "<title>"`, or `arcl capture create` |

- Every change to a task is a file change: commit it on the branch it
  describes, so the status reaches `main` with the merge. Record new tasks and
  bugs found along the way on the branch in progress, never in a pull request
  of their own.
- `arcl task start` refuses a task that is not ready. Pass `--force` only when
  the owner asks after seeing the reasons `arcl task explain` gives.
- A handoff parks the task. Unpark only when `arcl task explain` gives parking
  as the only reason it is not ready.
- When a merge conflicts on a task file, keep the newer status.

## Plan

- Leave `CHANGELOG.md` alone. The Release PR workflow writes each release's
  section from the squash subjects since the last tag, and the owner edits it
  in the release PR.

<!-- fabrial:begin -->
## Branches and pull requests

This repository uses fabrial. `fab init` wrote this section and the files it
names, `fab update` refreshes them, and `fab check` reports where they have
drifted.

- Never commit to `main`. Push a `claude/<topic>`, `codex/<topic>` or
  `storm/<task>` branch; the Branch PR workflow opens the pull request as a
  draft from stormlight-thunderbot.
- Commits on those branches may carry the agent's attribution trailers. The
  squash commit replaces them with the PR title and number, such as
  `Add Pi dispatch (#21)`, the PR body above its
  `<!-- fabrial:footer` line, `Merged from <branch>.` and the owner's
  `Co-authored-by` line from `.github/fabrial.toml`. Attribution lines and
  session links in the body are dropped, so nothing on `main` carries AI
  attribution. Editing the title or body of a reviewed PR posts the new
  squash message. Auto-merge keeps the message it was turned on with.
- Before pushing, run the command `gates` names in `.github/fabrial.toml`.
- The owner approves the PR and merges it, with the posted squash message or
  their own edit of it. Agents never approve or merge.
- Keep the PR body to what changed and why. Put verification in a PR comment.
  Leave out attribution lines and session links.
- Agents post PR comments as stormlight-thunderbot through the Bot comment
  workflow (`.github/workflows/comment.yml`), never with their own GitHub
  access.
- The agent that opens a PR reviews it with the `storm` skill and ends with a
  `Review finished at <sha>` comment. That turns `storm/review` green for that
  head, marks the draft ready and sets the squash message in the bot's status
  comment, which it edits rather than posting anew. The owner merges, by hand
  or with auto-merge they turn on; agents and the gate never turn it on.
  `/storm:ready <sha>` from the owner, a member or a collaborator accepts
  that head as it is, and `/storm:stop` turns auto-merge off.

## Writing

Everything here is read by a person: docs, commit messages, PR titles and
bodies, comments, and replies. Keep it short and plain:

- Lead with the point, and say each thing once.
- Use short sentences, common words, and one term for one thing.
- Cut preambles, summaries of what was just said, and sign-offs.
- Name the property, the limit or the failure, not a judgment of it.
- Stay inside the length targets: a PR body around 150 words, a review comment
  200, any other comment 100.

`.claude/skills/writing-docs/` holds the full standard, and its
`references/tells.md` lists the patterns that make text read as generated.
Check a draft against both before committing or posting.
<!-- fabrial:end -->
