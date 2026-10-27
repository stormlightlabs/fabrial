# Pi design

This page holds the planned parts of fabrial's Pi setup: who owns it, how PR
status could arrive, what the status area still lacks and the web UI.
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
[Pi dispatch](pi.md#pull-request-status) describes. App webhooks delivered to
the control plane on the VPS will replace the polling. They change the
transport, not the state the [status area](pi.md#status-area) reads, and the
web UI reads that same state.

## Status area

The [status area](pi.md#status-area) does not yet show:

- the task ID beside the run ID
- the session's elapsed time
- a worker's file when it is known, and an idle state between its turns
- a cost budget, when run settings configure one

Each needs a source that provides it. Do not invent a step, budget or cost,
and do not show unknown cost as zero. A displayed budget does not enforce a
spending limit.

## Web UI

The web UI is fabrial's frontend to Pi, written in GPUI. It has two views of
one state: runs from the claims ref, and the PR status the status area reads.
It adds no fields of its own, so the gaps above apply to it too, with the same
rules.

The pieces sit where PLAN.md's **Hosting** and **Web UI** decisions put them:

- A Cloudflare Worker serves the UI and holds the GitHub sign-in session. It
  keeps no run state.
- It reaches `fab serve` on the VPS through a Cloudflare Tunnel, over HTTP
  for the run operations and a WebSocket for live events.
- `fab serve` offers the operations `fab mcp` exposes, so the UI, Pi, the
  daemon and `/storm:` comments drive runs the same way.
- Workers report events to the control plane, never to the UI.

GPUI's browser target is unsupported upstream as of October 2026. Community
crates render it through WebGPU and are used for demos. A spike builds the
status view for `wasm32-unknown-unknown` and checks it in current Chrome,
Firefox and Safari. If it fails, the UI ships as a GPUI desktop client over the
same API first, and the web build waits for upstream support.

Workers for other people need their own model credentials. Hosted use of the
Claude Agent SDK takes an API key, not a subscription login, so the runner's
worker spec names the credential each worker uses.
