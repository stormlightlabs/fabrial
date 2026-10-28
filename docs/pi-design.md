# Pi design

This page holds the planned parts of fabrial's Pi setup: who owns it, how PR
status could arrive and what the status area still lacks.
[Pi dispatch](pi.md) covers what ships today, and arcl's roadmap phases 6
to 8 track delivery ([PLAN.md](../PLAN.md#work)).

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

The package collects PR status by polling `gh`, as
[Pi dispatch](pi.md#pull-request-status) describes. Where a host receives the
App's webhooks, they can become the main source, with polling as the fallback
when they stop arriving. The host checks each delivery's
`X-Hub-Signature-256` against the App's webhook secret and drops any that
fail, and Pi sessions read the resulting PR status from `fab serve`. Webhooks
change the transport, not the state the [status area](pi.md#status-area)
reads.

## Status area

The [status area](pi.md#status-area) does not yet show:

- the task ID beside the run ID
- the session's elapsed time
- a worker's file when it is known, and an idle state between its turns
- a cost budget, when run settings configure one

Each needs a source that provides it. Do not invent a step, budget or cost,
and do not show unknown cost as zero. A displayed budget does not enforce a
spending limit.

