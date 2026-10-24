# Pi status area

The fabrial Pi package will show the current task, workers and pull request
near Pi's editor. This is a design for the spike, not an implemented UI. The
reference is the two-row footer mockup discussed on October 24, 2026.

## What to show

The first row identifies the checkout and its pull request. The second row
shows the run and Pi session. Links open the repository or pull request in a
browser; plain text remains usable when the terminal cannot open links.

| Area | Contents | Source |
| --- | --- | --- |
| Repository | `stormlightlabs/fabrial` and repository link | Git remote |
| Branch | Task branch, upstream ahead/behind counts | Git |
| Pull request | PR number and link, draft/ready/merged state | GitHub |
| Checks | Passed/total, with pending and failed checks distinguished | GitHub, for the current head SHA |
| Diff | Added/deleted lines and changed-file count | Git comparison with the fetched base |
| Directory | Current worktree path | Session working directory |
| Run | Task/run ID and current review step | Dispatch/run records |
| Workers | Active/total, with blocked and failed states | Worker events |
| Context | Usage percentage and meter | Pi session |
| Cost | Reported cost and a budget, if configured | Pi usage and run settings |
| Session | Elapsed time and orchestrator model | Pi session |

The mockup's `plan > act > verify`, iteration number and worker names illustrate
activity; they do not define another review loop. Use fabrial's actual steps:
implement, standard review, verify findings, revise, adversarial review and
ready for owner. Do not invent a step, budget or cost when the source does not
provide one. Unknown cost is not zero, and a displayed budget is not an
enforced spending limit.

Worker activity belongs in a compact widget or tool progress above the editor:
role, current action, file when known, and working/idle/blocked/failed state.
Keep the transcript for detailed output. The mockup's test-running message is
progress, not another footer field.

## Refresh and layout

Start with Pi's `ctx.ui.setStatus()` and `setWidget()`. Replace the footer only
if the two-row layout needs it, retaining Pi's context, usage and model data.
The extension must not start a second terminal renderer.

Refresh after a push and when the branch changes. While an associated PR is
open, poll GitHub no more often than every 30 seconds. Cache results, back off
on errors and rate limits, and dispose timers on shutdown or reload. Mark
stale data with its last successful refresh; do not show an earlier head's
checks as the current head's result. A failed lookup is unknown, not green.

At narrow widths, retain PR/check state and the current step first. Collapse
secondary fields rather than wrapping over the editor. Use labels as well as
color. Headless dispatch does not depend on these widgets or their timers.

## Delivery

First show the PR link, state and current-head checks. Then add run and worker
progress when dispatch exposes reliable events. Use native Pi session data
for context, model and cost; avoid a second accounting system. Verify narrow
terminals, missing PRs, failed checks, changed heads, rate limits and reloads.
General themes and editor preferences remain personal Pi configuration.
