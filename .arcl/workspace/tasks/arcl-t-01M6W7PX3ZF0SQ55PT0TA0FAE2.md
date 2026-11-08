+++
id = "arcl-t-01M6W7PX3ZF0SQ55PT0TA0FAE2"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Keep the implementation loop off GitHub events"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #90 at e506559: the storm skill ends at Review finished, revise, resync and handoff wait on nothing, /storm:stop is gone and /storm:done aliases /storm:ready"
+++

PLAN.md's **fabrial is the implementation loop** decision: the loop never
waits on a GitHub event or takes its input from one. The skills still tell
agents to do both.

Acceptance criteria:

- [x] The `storm` skill ends with `Review finished` and the report. It no
      longer polls `storm/review` or the required checks after posting, or
      hands off to "finish watching"; the report gives the checks' state as
      it stands then
- [x] The `storm` skill no longer has agents act on `/storm:review` or
      `/storm:revise` comments; the driver asks the session for another
      sequence
- [x] `/storm:stop` and the `storm:stopped` label go from the gate, the
      Review gate workflow's triggers, the agents section and the docs:
      auto-merge is for a person or an outer-loop orchestrator, and GitHub
      turns it off. The note under the squash message about auto-merge stays
- [x] The gate accepts `/storm:done <sha>` as an alias for
      `/storm:ready <sha>`, from the same people, and a gate test covers it.
      Both still mark the draft ready, set `storm/review` and post the
      squash message
- [x] The `resync` skill no longer waits for a check and reruns the gate; it
      reports the check's state and what the driver can do
- [x] The `handoff` skill no longer counts a pending check as work left to
      hand off
- [x] The `revise` skill works from the findings the session hands it. It
      reads the pull request only for the review comment ids its answers
      name, and for a person's review when that is the input
- [x] Posting is unchanged: each pass's review with findings on the diff,
      the answers, and `Review finished`
