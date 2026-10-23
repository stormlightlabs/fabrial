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
.github/          CODEOWNERS and the Check, Branch PR and Review gate workflows
.github/storm/    gate.sh, which the Review gate workflow runs
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
- The agent that opens a PR reviews it with the `storm` skill and ends with a
  `Review finished at <sha>` comment. That turns `storm/review`, which `main`
  requires, green for that head. `/storm:ready <sha>` from the owner or a
  collaborator accepts that head as it is, and `/storm:stop` halts auto-merge.
- Include `PLAN.md` updates in the PR they describe.
- The [Fabrial Ledger](https://claude.ai/artifact/FeWinSfPXLQYoGyRAHoE39)
  shows `PLAN.md`'s items and a log of pull requests from its database. When a
  PR opens or merges, update its rows in the `items` and `log` collections.

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
