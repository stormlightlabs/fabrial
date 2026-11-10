+++
id = "arcl-t-01M71G2ZDT3MFGTGMMZWJZYS1N"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Count an unfetched BEFORE as claim-only in Branch PR"
status = "pending"
priority = "normal"
position = 0
+++

Found while updating a host repository to 0.9.2. When a worker force-pushes
over a branch that held only claim commits, for example after rebasing onto
a renew, Branch PR does not fetch the old BEFORE (`branch-pr.yml` near line
63). `work` then counts it as work, so `decide` sees no claim-only-to-work
change and opens nothing. The result is the same as the tip-check bug: work
and no pull request.

Acceptance criteria:

- [ ] An unreachable or unfetched BEFORE counts as claim-only, or the
      decision rests on the current tip and whether a pull request exists
- [ ] A test covers a force-push over a claim-only branch
