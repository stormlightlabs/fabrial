+++
id = "arcl-t-01M7DFWXKYMM842EHBGXRNTQQ7"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Let an approving review finish the Review gate"
status = "in_progress"
priority = "high"
position = 0
+++

The Review gate keeps `storm/review` running until an agent or a person
comments, and only a comment accepts a head. The owner wants an approving
review to accept the head, and a waiting check that does not read as a run
that never ends.

Done when both tasks below are done, `fab gates` passes, and a pull request
here shows the waiting state and turns green on an approval.

Source: the owner, 2026-11-14. Not in this unit: the Branch PR open bugs.
