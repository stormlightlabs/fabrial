+++
id = "arcl-t-01M71G2ZD28FN3J6MY6E7DMJC3"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Open the pull request when anything lands before Branch PR open"
status = "pending"
priority = "high"
position = 0
+++

Found while updating a host repository to 0.9.2. Branch PR open opens a
pull request only when the branch tip still equals the triggering push's
`head_sha` (`.github/workflows/branch-pr-open.yml`, the tip check near line
59). If another commit lands first, nothing opens, and nothing ever will:

1. A worker pushes work W onto a `storm/<task>` branch that held only claim
   commits, and `decide` sets `open=true`.
2. Before Branch PR open runs, another commit lands: a second push, or
   `fab claim renew`, which pushes a `Fabrial-Claim` commit each time.
3. The tip check sees tip ≠ W and opens nothing.
4. The later push's run opens nothing either: its BEFORE is W, which already
   holds work (`branch-pr.yml` near line 69).

The branch has work and no pull request, and a host waiting for it times
out. The comment near line 42, which says a later push opens it, is wrong.

Acceptance criteria:

- [ ] Branch PR open opens when the current tip holds work and the branch
      has no open pull request, whatever commit triggered it
- [ ] A test covers a claim renew or second push landing between the push
      and the open, and the comment states the rule that holds
