+++
id = "arcl-t-01M5S4SCZPZ0MFGDK1XD27DZ1P"
phase = "arcl-ph-01M5S4SCJESBP8SS02QAXGQCFD"
title = "Move thunderstorm's open issues into arcl"
status = "completed"
priority = "normal"
position = 1
evidence = "Done in the pull request from `storm/arcl-t-01M5S4SCZPZ0MFGDK1XD27DZ1P`: this task's body records where each open thunderstorm issue went."
+++

Each open issue in `stormlightlabs/thunderstorm` either became arcl work here
or stays behind, for the reason given.

Moved, combined where they share a change:

| Issues | Task |
| --- | --- |
| #143, #145, #146 | Tighten what review passes judge and post |
| #74, #83 | Prepare fabrial for repositories that install it |
| #77 | Owner: set up the bot and secrets across the org |
| #96, #97 | Answer the owner's review and cap open pull requests before new work |
| #127 | Claim tasks on refs/fabrial/claims |
| #142 | Clean up and resync when a pull request merges |
| #118, #121 | Verify Codex and Pi, then run one task on Codex |

Already done here: #6, #19, #45, #78, #82, #122 and #144.

Specific to thunderstorm's Go tool, payloads, site or board: #13, #18, #20,
#21, #29, #30, #33, #39, #40, #41, #75, #107, #112, #119, #120, #140, #141 and
#147. The board, shell-command parsing and hook gates are replaced by arcl and
by PLAN.md's server-side enforcement.

Umbrella issues or harnesses fabrial does not adopt: #2, #4, #11, #14, #54.

Left for later: #84, a size estimate reported against the diff, and #111, a
length limit on Markdown files.

Acceptance criteria:

- [x] Each open issue in `stormlightlabs/thunderstorm` becomes an arcl record
      here, or is recorded above as staying behind
