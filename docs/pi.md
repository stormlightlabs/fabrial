# Pi dispatch

This guide covers the shipped worker command and host setup. The planned
[orchestration lifecycle](pi-orchestration.md) covers PR events, worker
communication, cleanup, and ownership of the development Pi setup. The
[status area](pi-statusline.md) is a separate UI design;
[PLAN.md](../PLAN.md#6-workers-and-pi-setup) tracks delivery.

`fab dispatch` runs reviewer, verifier, adversarial-reviewer, and reviser as
fresh headless Pi 1.4.0 workers. The local spike uses host tmux and wraps the
whole Pi process in agent-sandbox.nix.

> **Note:** This is a preview feature currently under active development.

## Host setup

Run dispatch on the trusted host, outside a worker sandbox. You need Nix with
flakes enabled, tmux, GitHub CLI, curl with `--header @-` support, OpenSSL,
BWS CLI, and macOS `security` for the local issuer. App JWT requests use curl
with Bearer headers on stdin; installation-token requests use GitHub CLI. The
sandbox wrapper supports Linux, but this spike's issuer uses macOS Keychain.
There is no alternate issuer backend.

Keep a trusted checkout separate from every PR worktree. Build and install
`fab` from that checkout. Build the wrapper there, never from a PR branch:

```sh
cargo install --path crates/fab-cli
nix-build nix/worker.nix
```

Record the resulting store executable, `result/bin/fab-pi-sandboxed`, in the
local config below. The Nix expression pins agent-sandbox.nix v5.6.3 at
`bda0de5679017097f3a46f32b9d6b35dfd9b6327` and its nixpkgs revision
`7a0f122f5090cf4c2ade2a13a0e229d4e19ba71f`. It applies
[`deny-owner-keychain.patch`](../nix/deny-owner-keychain.patch), removing only
the Mach allowances for `com.apple.securityd.xpc` and
`com.apple.SecurityServer`. It retains `trustd.agent` and the system
certificate permissions. Plain `nix develop` or direnv is not isolation.

Install Pi 1.4.0 on the host in a dedicated runtime directory containing its
`node_modules` and all dependencies, with no credentials or personal settings.
Point `pi_runtime` at that directory, not the package below `node_modules`.
Log in to its `openai` subscription provider on the host. Dispatch copies only
that OAuth entry from `auth.json`, never an API key or the whole host Pi
directory. The
credential must remain valid past the worker deadline plus one minute;
refresh it on the host before dispatch if necessary. Worker credential
changes are discarded, not copied back to the host.

Create a private state directory outside every worktree and Git metadata
directory with mode `0700`. Keep the local issuer config outside PR worktrees
as well. All paths below must be absolute. Replace each example value; there
are no default Keychain identifiers or secret UUIDs:

```toml
sandbox = "/nix/store/<build>/bin/fab-pi-sandboxed"
pi_runtime = "/trusted/pi-1.4.0"
auth_file = "/private/pi/auth.json"
state_dir = "/private/fabrial-runs"
repository = "org/repository"
keychain_service = "your-local-issuer-service"
keychain_account = "your-local-issuer-account"
app_id_secret = "<BWS secret UUID>"
installation_id_secret = "<BWS secret UUID>"
pem_secret = "<BWS secret UUID>"
```

The local service `fabrial/bws/issuer` and account `fabrial-issuer-mac` are one
possible host configuration, not required names. Store the BWS machine-account
token in that Keychain entry yourself. Dispatch reads only the three configured
BWS secrets. Use a trusted host PATH for tmux, gh, curl, security, BWS,
OpenSSL, and Git. Do not put PR directories on it.

The App must be installed on the named repository. Give it Actions write,
Contents write, and Pull requests read. Review workers receive only Actions
write, Contents read, and Pull requests read; reviser receives Contents write.
GitHub also grants Metadata read. Dispatch rejects widened permissions or a
token that lists another repository. No owner PAT is accepted. Keep the App
out of ruleset bypass lists, and require the owner's approval and checks on
`main`. Workers cannot approve or merge with these permissions.

The host issuer holds the BWS token and App key; neither enters the sandbox.
The PEM is a private temporary file used for signing and removed before worker
launch. Installation tokens travel in the worker environment, not argv, logs,
or the Nix store. The host revokes them on normal completion, worker failure,
timeout, or cancellation. A host crash, SIGKILL, or network failure during
revocation can leave a token valid until its one-hour expiry. Revocation
failure makes the worker fail. The recorder confirms revocation before
removing private state, independently of whether a caller is waiting.
Cancellation or a deadline during issuance cannot launch a worker. Synchronous
dispatch allows a 120-second cleanup grace period. A failed revocation retains
the token recovery file and worker auth; restrict access to that state until
revocation and cleanup can finish.

## Run a role

Write a task file naming the PR, pinned head SHA, acceptance criteria, and any
findings the role needs. Give each concurrent worker its own linked worktree.
Do not use the trusted checkout as a worker worktree:

```sh
fab dispatch reviewer --task /private/task.txt \
  --worktree /work/review-pr --trusted-checkout /trusted/fabrial \
  --config /private/issuer.toml
```

Dispatch reads roles from the explicit trusted checkout's `.claude/agents` and
models from `.github/fabrial-roles.toml`. `fab init` and `fab update` install
that roles file. Reviewer uses `openai/gpt-6-sol`; the other three roles use
`openai/gpt-6.1-sol`. Reviewer and adversarial reviewer use `medium` thinking;
verifier and reviser use `low`. A missing or invalid role configuration fails
with the role's name; there is no fallback model.

The role tool mapping is `Read` → `read`, `Grep` → `grep`, `Glob` → `find,ls`,
`Bash` → `bash`, `Write` → `write`, and `Edit` → `edit`. Unmapped tools,
including `Skill` and GitHub MCP names, are reported and noted in the role
prompt. Workers read skills as files and use the sandbox's trusted `gh`.
Reviewers retain Bash, so tool selection does not make their worktree
read-only.

The host copies the role and its supporting skills into read-only inputs.
Symlinks escaping the trusted checkout are rejected. Workers use
`--no-approve`, disable resource discovery and context files, and load only
explicit trusted skill paths. The only extension is the embedded reporting
tool, loaded from read-only inputs after discovery is disabled. Workers receive
no session history, owner/project extensions, project packages, host settings,
or MCP servers. PR files remain task data,
not trusted executable configuration or role instructions.

Tmux creates a detached session without requiring an orchestrator TUI. Dispatch
and its recorder are Rust modules in `fab-cli`; tmux runs the same trusted
`fab` executable. Install that binary outside worker worktrees and shared Git
metadata. The host recorder starts the sandbox from the named worktree and
pipes both worker streams. Node never writes directly to host recorder files. The pane
shows progress; stdout from `fab dispatch` is JSONL progress followed by a
`result` or `error`. The final answer comes from the last assistant
`message_end`, never a delta or user message. RPC also requires `agent_settled`
and a successful readiness handshake before any model prompt. Only `stopReason: stop` with
nonempty text and no `errorMessage` counts as success, even if Pi exits zero.
Provider stderr and raw tool arguments/results are not persisted.

Each dispatch registers a version 1 worker record under
`state_dir/records/<worker-id>/worker.json` before the recorder issues
credentials. It contains run/worker UUIDs, the worktree's starting head,
role, repository, state, result location, and owned resources: private state,
tmux session, supervisor PID, and worker process-group PID. PIDs are last
observations, not proof that a process is still alive or safe to signal.
The task worktree and shared sandbox Nix root are not worker-owned resources.

The recorder claims the record once and replaces updates atomically. Record
files have mode `0600` in a `0700` directory. A validated final answer is
redacted before being saved as `result.txt` beside the record. Both survive
temporary issuer-state cleanup. Credential cleanup has its own pending,
complete, or unconfirmed state; it does not imply task or worktree cleanup.
Records contain no issuer configuration, credential values, or raw tool output.
A missing record prevents issuance; a final record-write failure retains
durable evidence and fails dispatch. A host crash can leave the last recorded
state stale; automatic recovery is planned, not implemented.

`--timeout` defaults to 1800 seconds and accepts 1–3000 seconds, below the App
token lifetime. Cancellation reaches the recorder, kills the worker process
group, revokes the token, and removes worker auth state. Unexpected pane exit
without a status fails dispatch. There is no unsandboxed retry.

The sandbox masks the owner home, blocks host sockets and local ports, denies
Nix daemon access, and restricts outbound domains. It exposes only the worker
worktree, independent writable worker state, read-only inputs, and the Pi
runtime. Upstream protects shared Git config, hooks, alternates, and worktree
pointers. Keep credentials out of shared Git config and remotes. Cargo builds
use that worktree's `target/`; never share a writable target between workers.
The sandbox permits access to shared Git history, refs, and objects, so it is
not isolation between repository branches.

When a worker posts a comment, it uses the trusted Bot comment workflow and
confirms the result's bot author and first line. A queued workflow is not a
posted comment. The `storm` skill describes this check. Dispatch itself does
not post comments or mark reviews complete.

## Detached workers

Add `--detach` to the role command to return a `started` record containing the
run and worker IDs once tmux accepts the launch. This is launch acceptance,
not provider readiness or completion. The worker owns cleanup after launch.

Use the returned worker ID for host operations:

```sh
fab worker inspect <worker-id> --config /private/issuer.toml
fab worker cancel <worker-id> --config /private/issuer.toml
fab worker wait <worker-id> --config /private/issuer.toml --timeout 300
```

Inspection returns the last recorded state and any redacted final result;
it does not require live OAuth or a current sandbox build. Cancellation is a
request, not confirmation of process exit or revocation. Wait returns the
worker's outcome when recorded; timing out or interrupting a wait does not
cancel the worker. Progress and results remain beside `worker.json` after
private issuer state is removed. Synchronous dispatch still streams progress
and waits for recorder exit.

## Worker events and steering

Read `fab worker events <worker-id> --config <config> --after <cursor>` to
receive at most 64 complete events plus the next byte cursor and current state.
An unchanged cursor means no new complete record. Keep the cursor when
reconnecting; do not split JSONL on Unicode line separators.

Use `fab worker steer <worker-id> --config <config> --head <starting-sha>
--message <text>` only for driver-authorized steering. Completed workers and
mismatched heads are rejected. Submission is followed by separate queued,
delivered, and processed events. Processed means a model response followed
message delivery, not proof that the instruction succeeded. Queues allow at
most 64 outstanding requests per worker; processed receipts free their slots.
Each message is limited to 16 KiB. Requests receive a durable sequence and are
delivered in submission order. Concurrent admission and claiming share a
per-worker lock; a busy submission returns an error to retry.

Workers use `worker_report` for progress, blockers, and questions. The host
redacts known credentials before persisting reports. Blockers and questions
stop the worker and retain its report for the driver; they do not authorize
replacement agents or another review cycle.

## Pi package

Install the root package from the trusted checkout with `pi install <path>`
or from a reviewed tag with `pi install git:github.com/stormlightlabs/fabrial@v<version>`.
Pi dependencies are peers, not bundled copies. Set these host variables before
starting the orchestrator:

```sh
export FAB_BINARY=/trusted/bin/fab
export FAB_DISPATCH_CONFIG=/private/issuer.toml
export FAB_TRUSTED_CHECKOUT=/trusted/fabrial
```

The package registers `agent` with `role`, `taskFile`, and `worktree`
parameters. Host configuration and trusted instructions are operator-selected,
not model-selected tool arguments. The tool calls shared `fab dispatch`,
streams progress through `onUpdate`, forwards cancellation, throws on failure,
and returns the final role text. It works in headless and interactive Pi.

## Verification

`just check` includes Rust subprocess/issuer tests and Node tool tests. Rust
tests run the recorder binary with fake host executables and check selected
auth, token scope, redaction, cancellation during issuance, deadline expiry,
worker failure, timeout, revocation, and retained cleanup state. They never
access Keychain or BWS.

Run a real host smoke before review; fake subprocess tests do not establish
that host credentials, tools, permissions, and isolation work together. Keep
verification results and remaining proof in
[PLAN.md](../PLAN.md#6-workers-and-pi-setup) and the PR's verification comments,
rather than treating this setup guide as a run log.

For unfinished work across orchestrator sessions, use the existing `handoff`
skill. A Pi resume or compact is not a durable task record. Follow the
[lifecycle cleanup rules](../AGENTS.md#lifecycle-cleanup) when a worker finishes
or its PR merges.
