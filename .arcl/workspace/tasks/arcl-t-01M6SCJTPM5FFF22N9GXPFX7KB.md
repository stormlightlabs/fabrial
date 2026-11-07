+++
id = "arcl-t-01M6SCJTPM5FFF22N9GXPFX7KB"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Name the implementing model in the squash message"
status = "completed"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M6SCJTNZ9MYTYEZ74XG6WCZG"]
evidence = "Done in #86: with assisted_by on, the gate adds one Assisted-by line per agent model in the commits' trailers. The gate runs from main, so the first squash after #86 merges is where git log --grep shows it."
+++

`main` shows that an agent made a change, since stormlight-thunderbot
authors every squash commit with the owner's `Co-authored-by`, but not
which model. Review comments name the model of each review pass, only on
GitHub. Claude Code records the model on every commit as
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`, and
`kept_text()` strips it. Turn it into one structured line instead, using
the Linux kernel's convention from
`Documentation/process/coding-assistants.rst`:

```text
Merged from claude/lasso.

Assisted-by: Claude Opus 5.5
Co-authored-by: Owais <desertthunder.dev@gmail.com>
Signed-off-by: Owais <desertthunder.dev@gmail.com>
```

Changes:

- In the gate's `squash_message()`, read the pull request's commits
  (`gh pr view --json commits` gives each message body). Collect
  `Co-authored-by` trailers whose address is a known agent's, and add one
  `Assisted-by: <name>` line per distinct name. Do it in the gate rather
  than Branch PR, because the gate sees commits pushed after the PR opened.
- Keep a small table of agent addresses: `noreply@anthropic.com` is Claude
  to start; add Codex's address once it is confirmed.
- Drop hand-written `Assisted-by:` lines from PR bodies, so only fab writes
  them.
- Make it a setting, such as `assisted_by = "on"`, so a repository can keep
  model names off `main`. It can default to on for Stormlight Labs
  repositories.

Acceptance criteria:

- [x] A PR whose commits carry Claude Code's trailer gets one `Assisted-by`
      line naming the model
- [x] Several commits with the same trailer give one line, and commits from
      two models give two
- [x] A PR with no agent trailers gets no line
- [ ] `git log --grep "Assisted-by: Claude"` on `main` finds the change
