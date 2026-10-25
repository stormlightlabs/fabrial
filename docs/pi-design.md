# Pi design

This page holds the planned parts of fabrial's Pi setup: PR status collection
and the status area. [Pi dispatch](pi.md) covers what ships today, and
[PLAN.md](../PLAN.md#6-workers-and-pi-setup) tracks delivery.

## Ownership

During active development, fabrial can own the working Pi setup, including
extensions, UI, themes, editor defaults and configuration, not only the
dispatch package. A setting can later become personal or move to a separate
package when there is a reason to.

Owning the setup does not expose the owner's credentials to workers. The host
keeps the issuer credentials and selects trusted instructions. Each worker
still gets only its subscription login and a repository-scoped token, inside
its own sandbox and fresh session.

## PR status

A shared collector keeps the orchestrator informed of the PR's head, draft
state, checks, reviews, owner commands, and merge or closure. It works without
the status area, which reads its state instead of polling GitHub itself.

Key each observation by repository, PR and head SHA. A push invalidates the
old head's checks and reviews. Distinguish unknown, stale, pending, failed and
successful. Refresh after pushes and branch changes. Polling, if used, runs at
most every 30 seconds, backs off on errors and rate limits, and stops on
shutdown or reload. Webhooks or polling are a transport choice and do not
change this state model.

Relevant changes reach the orchestrator even when it is not making a tool
call. A merge starts task cleanup, and an authorized stop request cancels the
active run. A failed check or requested change is input to the current stage,
never permission to start another review cycle. PR text and worker output stay
untrusted and grant no authority.

## Status area

The fabrial Pi package will show the current task, workers and PR near Pi's
editor, in two rows, following a footer mockup that the repository does not
keep. The first row identifies the checkout and its PR. The second shows the run and the Pi
session. Links open the repository or PR in a browser, and the plain text
still works when the terminal cannot open links.

| Area         | Contents                                       | Source                        |
| ------------ | ---------------------------------------------- | ----------------------------- |
| Repository   | `stormlightlabs/fabrial` and its link          | Git remote                    |
| Branch       | Task branch, ahead and behind counts           | Git                           |
| Pull request | Number, link, and draft, ready or merged state | PR status, above              |
| Checks       | Passed of total, with pending and failed apart | PR status, current head       |
| Diff         | Added and deleted lines, changed-file count    | Git, against the fetched base |
| Directory    | Current worktree path                          | Session working directory     |
| Run          | Task or run ID and the current step            | Dispatch records              |
| Workers      | Active of total, with blocked and failed       | Worker events                 |
| Context      | Usage percentage and meter                     | Pi session                    |
| Cost         | Reported cost, and a budget if configured      | Pi usage and run settings     |
| Session      | Elapsed time and orchestrator model            | Pi session                    |

The mockup's `plan > act > verify`, iteration number and worker names only
illustrate activity. Show fabrial's real steps from the delivery sequence in
[AGENTS.md](../AGENTS.md#delivery-sequence). Do not invent a step, budget or
cost a source does not provide. Unknown cost is not zero, and a displayed
budget does not enforce a spending limit.

Worker activity goes in a compact widget or tool progress above the editor:
role, current action, file when known, and working, idle, blocked or failed.
Detailed output stays in the transcript.

### Layout and refresh

Start with Pi's `ctx.ui.setStatus()` and `setWidget()`. Replace the footer
only if the two rows need it, and keep Pi's context, usage and model data. The
extension must not start a second terminal renderer.

Update when the shared PR or worker state changes. Mark stale data with its
last successful refresh, and never show an earlier head's checks as the
current head's. A failed lookup is unknown, not green. Dispose subscriptions
on shutdown or reload.

At narrow widths, keep the PR and check state and the current step, and
collapse other fields rather than wrapping over the editor. Use labels as well
as color. Headless dispatch does not depend on these widgets or their timers.

### Delivery order

Show the PR link, its state and the current head's checks first. Add run and
worker progress next, from the worker events. Take context, model and cost
from Pi's own session data rather than a second accounting system. Test narrow
terminals, missing PRs, failed checks, changed heads, rate limits and reloads.
