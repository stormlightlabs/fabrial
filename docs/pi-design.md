# Pi design

This page holds the planned parts of fabrial's Pi setup: who owns it and how
PR status could arrive. [Pi dispatch](pi.md) covers what ships today, and
arcl's roadmap phase 6 tracks delivery ([PLAN.md](../PLAN.md#work)).

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
[Pi dispatch](pi.md#pull-request-status) describes. Webhooks could replace
the polling later; they would change the transport, not the state the
[status area](pi.md#status-area) reads.
