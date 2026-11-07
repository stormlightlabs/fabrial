+++
id = "arcl-t-01M60CMCGB835DNGWHZ80CZRC4"
phase = "arcl-ph-01M5S4SCGXWH09BAZNT1684VTF"
title = "Release a claim where the remote refuses branch deletion"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #83 at 18882e8: release pushes an already-expired claim when the remote refuses the delete; cloud sessions work on the claimed storm/<task> branch"
+++

From a Claude Code cloud session, `fab claim take` and `renew` push storm/<task> fine, but `fab claim release` fails: the git proxy refuses branch deletion (HTTP 403), and the REST ref delete is blocked too. Expiry still frees the task. When the delete is refused, release falls back to a claim commit that has already expired.

Decided by the owner: a cloud session pushes its work to the claimed `storm/<task>` branch, in place of the branch its harness names.

Acceptance criteria:

- [x] When the remote refuses the delete, `fab claim release` pushes a claim commit that has already expired, so another run can take the task at once
- [x] The owner decides which branch a cloud session pushes its work to while it holds a `storm/<task>` claim, and the implement skill and PLAN.md say so
