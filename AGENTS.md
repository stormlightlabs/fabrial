# fabrial

A software factory that takes ready tasks from arclightning through implement,
review, adversarial review and revise until a pull request is ready for a
person to merge. The CLI is `fab`. [PLAN.md](PLAN.md) holds the decisions and
the phases.

## Gates

```sh
just check    # cargo fmt --check, clippy with warnings denied, cargo test
```

CI runs the same recipe as the `check` job, which the ruleset on `main`
requires. Clippy warnings fail it whether or not your change caused them.

## Layout

```text
crates/fab-core   run operations shared by every driver
crates/fab-cli    the `fab` binary
.claude/skills/   skills; .agents/skills links here for Codex and Pi
.claude/agents/   implementer, reviewer, adversarial-reviewer, verifier, reviser
.github/          CODEOWNERS and the Check, Branch PR, Review and Storm workflows
.github/storm/    the scripts the Review and Storm workflows run
docs/hosts.md     what each host reads, and where they differ
PLAN.md           decisions and phases
AGENTS.md         this file (CLAUDE.md links to it)
```

## Code

Follow arclightning's conventions, since the two share crates:

- Document exported and contextually important items with rustdoc.
- Keep an `impl` next to its type, and avoid `unsafe`.
- Order items as constants, enums, structs, then free functions, with `pub`
  before private.
- Use at most one level of `super`.

## Branches and pull requests

- Never commit to `main`. Push a `claude/<topic>`, `codex/<topic>` or
  `storm/<task>` branch; the Branch PR workflow opens the pull request as
  stormlight-thunderbot and turns on squash auto-merge.
- Commits on those branches may carry the agent's attribution trailers. The
  squash commit replaces them with the PR title, `Merged from <branch>.` and
  the owner's `Co-authored-by` line, so nothing on `main` carries AI
  attribution.
- The owner's approval merges the PR. Agents never approve or merge.
- Keep the PR body to what changed and why. Put verification in a PR comment.
- The Review workflow reviews every push and sets `storm/review`, which `main`
  requires. It fails on a `blocker` or `high` finding. On the PR, the owner or
  a collaborator can comment `/storm:review` to rerun it, `/storm:revise` to
  have the bot address it, `/storm:ready` to accept it, or `/storm:stop` to
  halt review and auto-merge.
- Include `PLAN.md` updates in the PR they describe.

## Writing

Everything here is read by a person: docs, commit messages, PR titles and
bodies, comments, and replies. Write to `.claude/skills/writing-docs/` and
check drafts against its `references/tells.md` before committing or posting.
