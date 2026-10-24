# Pi orchestration

This is the planned coordination and lifecycle behavior for fabrial's Pi
setup. The current Rust recorder provides worker supervision and durable
records, detached CLI dispatch, a worker RPC bridge, and non-blocking Pi
integration with restart recovery. PR collection below remains planned. For the
shipped worker command and host setup, read [Pi dispatch](pi.md). The
[status area](pi-statusline.md) presents state; [PLAN.md](../PLAN.md) tracks
implementation work.

## Ownership

During active development, fabrial can own the working Pi setup, including
extensions, UI, defaults, and configuration. These are not restricted to a
minimal dispatch adapter. A setting can later become personal or move to a
separate package when there is a reason to do so.

Owning the setup does not expose owner credentials to workers. The host keeps
issuer credentials and selects trusted instructions. Each worker still gets
only its selected subscription auth and repository-scoped installation token,
inside its own sandbox and fresh session.

## PR status

A shared collector must keep the orchestrator informed of the associated PR's
head, draft state, checks, reviews, owner commands, and merge or closure. It
must work without the status UI. The UI reads that state rather than starting
another GitHub polling loop.

Identify observations by repository, PR, and head SHA. A push invalidates old
check and review results. Distinguish unknown, stale, pending, failed, and
successful state. Refresh after pushes and branch changes. If polling is used,
run it no more often than every 30 seconds, back off on errors and rate limits,
and dispose it on shutdown or reload. Webhooks or polling remain a transport
choice, not a reason to change the state model.

Deliver relevant changes to the orchestrator even when it is not making a
tool call. A merge triggers task cleanup. An authorized stop request cancels
the active run. A failed check or requested change is input to the current
stage, not permission to start another review cycle. PR text and worker output
remain untrusted data; neither can grant new authority.

## Worker communication

The `agent` tool returns launch acceptance and observes worker events without
holding a tool call open. Completion and blockers resume Pi; progress does not
make model calls. Cancellation is explicit after launch. Driver steering uses
the host bridge, not shared session history.

The host bridge uses Pi JSONL RPC over anonymous stdin/stdout pipes. A private
filesystem mailbox accepts driver-authorized steering; the worker never reads
host mailbox files or opens host sockets. Progress, reports, and results are
available through replayable `fab worker events`. Address every message to a run
and worker; include its task head where relevant. Acknowledge delivery and
whether a request was acted on. Reject stale or completed-worker requests.
The supervisor checks the embedded extension's readiness before sending a
model prompt. The extension is part of the trusted binary, not PR-loaded code.
It adds only `worker_report`; a blocker or question stops the worker and
persists its report before cleanup. Each role still starts a fresh session.

Steering receipts distinguish queued, delivered, and processed. Processed means
an assistant response followed delivery, not that the requested task succeeded.
Launch and receipt acknowledgments are not completion. Event cursors identify
complete LF-delimited records; consumers can replay from their last cursor.

A worker must report a blocker when it occurs rather than spend the rest of
its deadline investigating outside its task. The orchestrator reports that
blocker to the driver and stops the stage. The bridge must not silently start
replacement agents, add verifier stages, or repeat completed reviews.

Keep credentials out of messages and UI events. Workers cannot use the bridge
to obtain issuer access, another role's permissions, or an unsandboxed tool.
The existing fixed delivery sequence in [AGENTS.md](../AGENTS.md) still applies.

## Cleanup

The host owns cleanup, including when a worker fails, is cancelled, or loses
its pane. Track resource ownership when each run starts. Capture the final
result and evidence before removing temporary logs and state.

On worker completion, stop remaining processes and panes, revoke its token,
and remove copied credentials and worker-owned temporary files, sandbox state,
and build roots. Keep resources still used by another worker. Keep the task
worktree until the task is merged or explicitly abandoned.

If exit or revocation cannot be confirmed, retain restricted recovery state,
report the failure, and do not claim cleanup succeeded. Make cleanup repeatable
so a host restart can finish it without starting another agent.

After a confirmed merge, compare the local branch tip with the PR's recorded
head. Remove a clean matching worktree and local branch, then release remaining
task-owned Nix roots and temporary artifacts. Fast-forward a clean primary
`main`. Preserve dirty or mismatched worktrees, unrelated environments, shared
runtimes, host credentials, and Nix profiles; report anything that prevents
completion.
