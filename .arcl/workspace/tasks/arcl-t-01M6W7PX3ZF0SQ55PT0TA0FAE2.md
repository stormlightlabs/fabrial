+++
id = "arcl-t-01M6W7PX3ZF0SQ55PT0TA0FAE2"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Keep the implementation loop off GitHub events"
status = "pending"
priority = "high"
position = 0
+++

PLAN.md's **fabrial is the implementation loop** decision: the loop never
waits on a GitHub event or takes its input from one. The skills still tell
agents to do both.

Acceptance criteria:

- [ ] The `storm` skill ends with `Review finished` and the report. It no
      longer polls `storm/review` or the required checks after posting, or
      hands off to "finish watching"; the report gives the checks' state as
      it stands then
- [ ] The `storm` skill no longer has agents act on `/storm:review` or
      `/storm:revise` comments; the driver asks the session for another
      sequence. `/storm:ready` and `/storm:stop` stay, since the gate
      workflow acts on them
- [ ] The `resync` skill no longer waits for a check and reruns the gate; it
      reports the check's state and what the driver can do
- [ ] The `handoff` skill no longer counts a pending check as work left to
      hand off
- [ ] The `revise` skill works from the findings the session hands it. It
      reads the pull request only for the review comment ids its answers
      name, and for a person's review when that is the input
- [ ] Posting is unchanged: each pass's review with findings on the diff,
      the answers, and `Review finished`
