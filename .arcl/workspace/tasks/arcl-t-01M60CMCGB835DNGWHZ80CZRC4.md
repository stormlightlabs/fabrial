+++
id = "arcl-t-01M60CMCGB835DNGWHZ80CZRC4"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Release a claim where the remote refuses branch deletion"
status = "pending"
priority = "normal"
position = 0
+++

From a Claude Code cloud session, `fab claim take` and `renew` push storm/<task> fine, but `fab claim release` fails: the git proxy refuses branch deletion (HTTP 403), and the REST ref delete is blocked too. Expiry still frees the task. When the delete is refused, release falls back to a claim commit that has already expired. A cloud session pushes its work to its harness branch (claude/<topic>) while it holds the storm/<task> claim, and the implement skill says so.

Acceptance criteria:

- [ ] When the remote refuses the delete, `fab claim release` pushes a claim commit that has already expired, so another run can take the task at once
- [ ] The implement skill says which branch a cloud session pushes its work to while it holds a `storm/<task>` claim
