---
name: resync
description: Rebase an open pull request onto the current main, run the gates and push. Use for /resync, or when a pull request has a conflict or its base has moved under it.
argument-hint: <pr-number>
---

# Resync

Move one pull request onto the current head of `main`.

## Read the pull request

```sh
gh pr view <n> --json headRefName,baseRefName,headRefOid,author
```

A cloud session reads the same with `pull_request_read` method `get`. Note the
head before the rebase.

The branch must be an agent's: `claude/`, `codex/` or `storm/`. Rebasing
rewrites it, so leave a person's branch alone and report instead.

## Work in the pull request's tree

Use the worktree `git worktree list` shows on the pull request's branch, or
take one as the `revise` skill does. Never rebase in the user's primary
checkout.

## Rebase

```sh
git fetch origin main <branch>
git diff "$(git merge-base origin/main HEAD)" HEAD | git patch-id --stable
git rebase origin/main
git diff origin/main HEAD | git patch-id --stable
```

The two ids say whether the review still applies. The same id means only the
base moved. A different id means the change itself differs, as when `main`
already held part of it.

A conflict is a decision about the change. Abort with `git rebase --abort`,
name the conflicting files, and stop: a `revise` pass or the operator resolves
it.

## Verify and push

Run `just check`. A gate that fails after the rebase is a finding for a
`revise` pass: report it and push nothing.

```sh
git push --force-with-lease origin <branch>
```

The push runs the Review workflow on the new head.

## Report

Report the pull request, the old and new heads, whether the change is the same
or differs, and the gates and their result. A stop names its reason and the
files.

## Do not

- Resolve a conflict yourself.
- Rebase a branch a person owns.
- Merge, approve, or push to `main`.
