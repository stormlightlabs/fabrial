+++
id = "arcl-c-01M6W7PX4G9G4217APRZ6GCZCV"
title = "An outer loop for projects that can run hands-off"
status = "captured"
created-at = "2026-11-08T00:07:40.944Z"
+++

fabrial does best as the implementation loop. A larger loop would watch
pull requests after hand-off, react to CI and review comments, and merge
without a person. It needs projects that can run that hands-off, and its
own planning.

What we learned so far:

- Agents turning on auto-merge went away: the owner merges.
- Agents are often not woken by review comments, CI results or the gate, so
  a loop that depends on those events stalls.
- Comment commands that wake an agent, such as `/storm:review`, only work
  while something is subscribed to the pull request.

Open questions: which projects could run hands-off, what they need first
(tests, risk limits, a way to stop), and whether this loop belongs in
fabrial or in whatever hosts it.
