+++
id = "arcl-c-01M606YE8SM8D9X56RR2Q1E8Z5"
title = "Release a claim where branch deletion is refused"
status = "captured"
created-at = "2026-10-28T02:55:35.193Z"
+++

From a Claude Code cloud session, `fab claim take` and `renew` push storm/<task> fine, but `fab claim release` fails: the git proxy refuses branch deletion (HTTP 403), and the REST ref delete is blocked too. Expiry still frees the task. Decide whether release should fall back to an expired claim commit (expires now) when the delete is refused, and how a cloud session working on its harness branch (claude/<topic>) relates to the storm/<task> claim it holds.
