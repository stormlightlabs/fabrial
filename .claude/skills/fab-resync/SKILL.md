---
name: fab-resync
description: Bring an open pull request up to date with main by rebasing or merging, resolve the conflicts, run the gates and push. Use for /fab-resync, or when a pull request has a conflict or its base has moved under it.
argument-hint: <pr-number>
---

# Resync

Bring one pull request up to date with the current head of `main`, resolving
what conflicts on the way.

## Read the pull request

```sh
gh pr view <n> --json headRefName,baseRefName,headRefOid,author,body,commits
```

A cloud session reads the same with `pull_request_read` method `get`. Note the
head before you start. The body and commits say what the change is for; a
conflict is resolved against them.

## Work in the pull request's tree

Use the worktree `git worktree list` shows on the pull request's branch, or
take one as the `fab-revise` skill does. Never resync in the user's primary
checkout.

```sh
git fetch origin main <branch>
git diff --binary "$(git merge-base origin/main HEAD)" HEAD > before.diff
```

## Choose rebase or merge

Preview the conflicts without touching the tree:

```sh
git merge-tree --write-tree --name-only origin/main HEAD
```

It exits 0 when the merge is clean, and 1 with the conflicting files listed
after the tree id. Then count the branch's commits that touch those files,
with `git log --oneline origin/main..HEAD -- <files>`.

Merge `main` in (`git merge origin/main`) when any of these holds:

- The branch is a person's: its name has no `claude/`, `codex/` or `storm/`
  prefix. Never rewrite it.
- Someone else works on the branch: `git log --format='%an <%ae>'
  origin/main..HEAD` names an author other than your own commits' author and
  the App `app` in `.github/fabrial.toml` names, or the driver says so.
- The branch already has `main` merged in: `git rev-list --merges
  origin/main..HEAD` prints anything. A rebase drops the merge commit and
  replays the conflict it resolved, even when `merge-tree` reports no
  conflict.
- More than one commit touches the conflicting files, so a rebase would stop
  on the same conflict once per commit.

Otherwise rebase (`git rebase origin/main`): an agent branch with no merge
commits that applies cleanly, or whose conflicts sit in one commit. A merge
commit never reaches `main`, because agent pull requests squash-merge.

Either way the Review gate sees the same change. It hashes the diff between
the head and its merge base with `origin/main`, leaving out `index` lines and
hunk line ranges. After a clean merge that merge base is the `main` you merged,
so the diff is the one a rebase would give, and a push that only moved the
base keeps `storm/review` green.

## Resolve conflicts

For each conflicting file, read why `main` changed it:

```sh
git log -p "$(git merge-base origin/main ORIG_HEAD)"..origin/main -- <file>
```

(`ORIG_HEAD` is the branch head before the merge or rebase started.) Keep
both behaviours: the pull request's, as its body and commits describe it, and
`main`'s, as its commits describe it. Then run the tests that cover the file.

Never hand-merge a generated or managed file. Take `main`'s copy and
regenerate it with the repository's tooling, for example:

| File                                | Regenerate with                                      |
| ----------------------------------- | ---------------------------------------------------- |
| `Cargo.lock`                        | any `cargo` build; `cargo update -w` if it disagrees |
| Files `fab update` manages          | `fab update`, then `fab check`                       |
| dist's `.github/workflows/release.yml` | `dist generate`                                   |

A task file in the repository's tracker keeps the newer status. Read the
repository's `AGENTS.md` for its own generated files and the command that
writes each one.

Finish without an editor, which a session without a terminal cannot answer:

```sh
git add <files>
git commit --no-edit                    # after a merge
GIT_EDITOR=true git rebase --continue   # after a rebase, once per stop
```

Stop and ask only when both sides change the same logic and either choice
loses one side's behaviour, or when the conflict is in code you cannot test.
Abort first (`git merge --abort` or `git rebase --abort`), then name the
files, the two behaviours and the resolution you recommend.

## Verify and push

Run `fab gates`, or `fab gates --full` when you are unsure what the
resolution touched. A gate that fails is a finding to fix within the
resolution; if the fix is outside it, report the failure and push nothing.

Then compare the change before and after:

```sh
git diff --binary origin/main HEAD > after.diff
norm() { sed -E '/^(index|similarity index|dissimilarity index) /d; s/^@@ -[0-9,]+ \+[0-9,]+ @@/@@/' "$1"; }
diff <(norm before.diff) <(norm after.diff)
```

No output means only the base moved. Any other difference, whitespace
included, means the change itself differs, as after a resolved conflict or
when `main` already held part of the change.

Push with `git push --force-with-lease origin <branch>` after a rebase, and a
plain `git push origin <branch>` after a merge.

When only the base moved, the Review gate keeps `storm/review` green.
Otherwise the check goes back to pending and the change needs another review
sequence, which the driver asks a `fab-storm` run for. Read the check once after
the push and do not wait on it. If it is pending after a push that only moved
the base, the previous head's check was not yet green when the gate read it:
say so, and that the driver can rerun this push's Review gate run once that
check is green, or ask for another sequence.

## Report

Report the pull request, the old and new heads, rebase or merge and why, each
resolved conflict and how, whether the change is the same or differs, the
gates and their result, and `storm/review`'s state after the push. A stop names
the files, the two behaviours and your recommendation.

## Do not

- Rebase or force-push a branch a person owns.
- Hand-edit a generated or managed file to resolve a conflict.
- Merge or approve a pull request, or push to `main`.
