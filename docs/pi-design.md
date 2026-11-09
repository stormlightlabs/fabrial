# Pi design

These notes are frozen with the Pi harness, which gets no new work
([PLAN.md](../PLAN.md#decisions)). Nothing here is scheduled. They record
design choices for parts that were never built: who owns the Pi setup, how
PR status might arrive, and what the status area lacks.
[Pi dispatch](pi.md) covers what ships.

## Ownership

The design let fabrial own the working Pi setup, including extensions, UI,
themes, editor defaults and configuration, not only the dispatch package,
with any setting free to become personal or move to a separate package.

Owning the setup does not expose the owner's credentials to workers. The host
keeps the issuer credentials and selects trusted instructions. Each worker
still gets only its subscription login and a repository-scoped token, inside
its own sandbox and fresh session.

## PR status

The package collects PR status by polling `gh`, as
[Pi dispatch](pi.md#pull-request-status) describes. A host that receives the
App's webhooks could use them as the main source, with polling as the
fallback when they stop arriving. That host checks each delivery's
`X-Hub-Signature-256` against the App's webhook secret, drops any that fail,
and passes the resulting PR status to Pi sessions by its own means.
Webhooks change the transport, not the state the
[status area](pi.md#status-area) reads.

Either way the status is something the person reads. The package starts no
turn on a PR change; acting on one belongs to the person or to the host.

## Status area

The [status area](pi.md#status-area) does not show:

- the task ID beside the run ID
- the session's elapsed time
- a worker's file when it is known, and an idle state between its turns
- a cost budget, when run settings configure one

Each would need a source that provides it. A step, budget or cost must not be
invented, and unknown cost must not show as zero. A displayed budget does not
enforce a spending limit.
