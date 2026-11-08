+++
id = "arcl-t-01M6WNDCH4T131XXKRC2M5BTJN"
phase = "arcl-ph-01M5S4SCHA1F25KRETYKZYRAB6"
title = "Decide how rulesets limit what dispatch tokens push"
status = "pending"
priority = "normal"
position = 0
+++

Owner decision, from the adversarial review of #91. A dispatch token with
Contents write can push to any branch the rulesets leave open. `fab dispatch`
now refuses a writer role unless the worktree is on a `claude/`, `codex/` or
`storm/` branch, but that check limits mistakes, not a hostile worker: an App
token's write scope cannot be narrowed to one branch.

Decide:

- Whether a ruleset should block force-push and deletion on agent branches
  (`claude/**`, `codex/**`, `storm/**`), so one worker cannot rewrite or
  delete another's branch. `fab release` deletes `storm/` claim branches,
  so the rule has to allow for that.
- Which App the host's dispatch issuer uses. The `v*` tag ruleset lets App
  5254891 (stormlight-thunderbot) bypass it so releases can push tags. If
  dispatch mints tokens from that App, a writer worker could push a release
  tag. `docs/pi.md` already says to keep the dispatch App out of bypass
  lists; confirm the host follows it, or give dispatch its own App.

Acceptance criteria:

- [ ] The owner has decided both points and the rulesets match
- [ ] `docs/pi.md` states the outcome
