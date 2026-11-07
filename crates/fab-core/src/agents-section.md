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
  session links in the body are dropped, so `main` carries no AI attribution
  beyond the `Assisted-by` lines below. Editing the title or body of a reviewed PR posts the new
  squash message. Auto-merge keeps the message it was turned on with.
- When `.github/fabrial.toml` sets `signoff`, the squash message ends with
  the owner's `Signed-off-by` line: the owner certifies agent work at merge.
  Agents never add `Signed-off-by`, and the squash drops one written in a PR
  body.
- When `.github/fabrial.toml` sets `assisted_by = "on"`, the squash message
  names each agent model the commits' `Co-authored-by` trailers name, as
  `Assisted-by: <model>`. Agents never write `Assisted-by` in a PR body; the
  squash drops it.
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
