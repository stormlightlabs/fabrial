# Pi dispatch

`fab dispatch` runs reviewer, verifier, adversarial-reviewer and reviser as
fresh headless Pi 1.4.0 workers, each in a host tmux session with the whole Pi
process wrapped in agent-sandbox.nix. This guide sets up the host and runs a
worker. [Pi design](pi-design.md) covers what is still planned; arcl's
roadmap phase 6 tracks delivery ([PLAN.md](../PLAN.md#work)).

> **Note:** This is a preview feature under active development.

## Host setup

Run dispatch on the trusted host, outside a worker sandbox. You need Nix with
flakes enabled, tmux, GitHub CLI, curl with `--header @-` support, OpenSSL,
BWS CLI, and macOS `security`. The sandbox wrapper supports Linux, but the
token issuer reads macOS Keychain and has no other backend.

Build `fab` and the sandbox wrapper from a trusted checkout kept apart from
every PR worktree, never from a PR branch:

```sh
cargo install --path crates/fab-cli
nix-build conf/sandbox/worker.nix
```

The wrapper pins agent-sandbox.nix v5.6.3 and its nixpkgs, and applies
[`deny-owner-keychain.patch`](../conf/sandbox/deny-owner-keychain.patch) to
block the owner's Keychain. Plain `nix develop` or direnv is not isolation.

The wrapper used to live in `nix/`. The move left its store path unchanged,
so an existing `sandbox` value still works. Build it with the command above
from now on.

Install Pi 1.4.0 in a dedicated runtime directory holding its `node_modules`
and all dependencies, with no credentials or personal settings. Log in to its
`openai` subscription provider on the host. Dispatch copies only that OAuth
entry from `auth.json` into the worker and discards any changes the worker
makes to it. The login must stay valid until the worker deadline plus one
minute, so refresh it before dispatch if needed.

### Config file

Create a state directory with mode `0700` outside every worktree and Git
metadata directory, and keep the config file outside PR worktrees too. Every
path must be absolute. There are no default Keychain names or secret IDs:

```toml
sandbox = "/nix/store/<build>/bin/fab-pi-sandboxed"   # from nix-build
pi_runtime = "/trusted/pi-1.4.0"   # the runtime directory, not the package
auth_file = "/private/pi/auth.json"
state_dir = "/private/fabrial-runs"
repository = "org/repository"
keychain_service = "your-local-issuer-service"
keychain_account = "your-local-issuer-account"
app_id_secret = "<BWS secret UUID>"
installation_id_secret = "<BWS secret UUID>"
pem_secret = "<BWS secret UUID>"
```

Store the BWS machine-account token in the named Keychain entry yourself.
Run dispatch with a trusted PATH for tmux, gh, curl, security, BWS, OpenSSL
and Git, with no PR directories on it.

### GitHub App

Install the App on the named repository with Actions write, Contents write and
Pull requests read. Review workers get tokens with Actions write, Contents
read and Pull requests read; reviser also gets Contents write. Dispatch
rejects a token with wider permissions or another repository, and accepts no
personal access token. Keep the App out of ruleset bypass lists, and require
the owner's approval and checks on `main`, so workers cannot approve or merge.

The BWS token and App key stay on the host. A worker gets its token in its
environment, and the host revokes it when the worker finishes, fails, times
out or is cancelled. A failed revocation fails the worker, and the host keeps
the token's recovery state, so restrict access to the state directory. A host
crash during revocation can leave a token valid until its one-hour expiry.

## Run a role

Write a task file naming the PR, its pinned head SHA, the acceptance criteria
and any findings the role needs. Give each concurrent worker its own linked
worktree, never the trusted checkout:

```sh
fab dispatch reviewer --task /private/task.txt \
  --worktree /work/review-pr --trusted-checkout /trusted/fabrial \
  --config /private/issuer.toml
```

Dispatch reads the role from the trusted checkout's `.claude/agents` and its
model from `.github/fabrial-roles.toml`, which `fab init` installs. A missing
or invalid role entry fails with the role's name; there is no fallback model.
Role tools Pi lacks, such as `Skill` and GitHub MCP tools, are reported and
noted in the prompt. Reviewers keep Bash, so their worktree is not read-only.

A worker gets read-only copies of its role and skills, and no session history,
extensions, packages, settings or MCP servers from the host. Files in the PR
are task data, never instructions.

The sandbox exposes only the worker's worktree, its own state, those inputs
and the Pi runtime. It blocks host sockets, local ports and the Nix daemon,
and limits outbound domains. Workers can still read shared Git history, refs
and objects, so the sandbox does not isolate one branch from another. Keep
credentials out of shared Git config and remotes. Never
share a writable Cargo `target/` between workers. Rust does not link
`libiconv` inside the sandbox, so run Rust gates such as `just check` on the
host.

`--timeout` defaults to 1800 seconds and accepts 1 to 3000. Stdout is JSONL
progress ending in a `result` or an `error`, and the tmux pane shows the same
progress. A worker succeeds only when Pi stops normally with a nonempty final
answer. Nothing retries outside the sandbox.

Dispatch never posts comments or marks a review finished. A worker that posts
uses the Bot comment workflow and checks the comment landed, as the `storm`
skill describes.

Each worker's record is `state_dir/records/<worker-id>/worker.json`, with the
redacted final answer in `result.txt` beside it. Both outlast the worker's
private state, and neither holds raw tool output. The worker's installation
token and OAuth access and refresh tokens are redacted; any other secret in
the answer is kept as written.

## Detached workers

Add `--detach` to return a `started` record with the run and worker IDs once
tmux accepts the launch. The worker then cleans up after itself.

```sh
fab worker list --config /private/issuer.toml
fab worker inspect <worker-id> --config /private/issuer.toml
fab worker cancel <worker-id> --config /private/issuer.toml
fab worker wait <worker-id> --config /private/issuer.toml --timeout 300
```

`inspect` returns the last recorded state and any result, and needs no live
login or sandbox build. `cancel` sends a request and does not confirm the
worker stopped. A `wait` that times out or is interrupted leaves the worker
running.

## Events and steering

`fab worker events <worker-id> --config <config> --after <cursor>` returns up
to 64 events, the next cursor and the current state. Start at 0 and keep the
cursor when reconnecting. An unchanged cursor means nothing new.

Steer a worker only on the driver's instruction:

```sh
fab worker steer <worker-id> --config <config> --head <starting-sha> \
  --message <text>
```

Dispatch rejects a finished worker and a head other than the worker's starting
head. A message holds at most 16 KiB, and a worker holds at most 64 messages
not yet processed. A busy worker returns an error; retry it. Events report
each message as queued, delivered and processed. Processed means the model
responded, not that the instruction succeeded.

Workers report progress, blockers and questions with `worker_report`. A
blocker or question stops the worker and keeps its report for the driver; it
never starts a replacement agent or another review cycle.

## Recovery

When a worker's supervisor is gone, including after a host restart, a Pi
session that watches the worker recovers it on its next read. Otherwise, run
`fab worker recover <worker-id> --config <config>`. Restart handling is tested
with changed boot identities, not a physical reboot.

Recovery confirms the worker's processes are gone, revokes its token, removes
its private and temporary state, and marks it failed. It keeps the record,
result and events, and never reruns the task or removes a worktree. If it
cannot confirm that a process exited or a token is unusable, it refuses and
keeps the restricted state for you to inspect.

## Pi package

Install the root package with `pi install <trusted-checkout>`, or from a
reviewed tag with `pi install git:github.com/stormlightlabs/fabrial@v<version>`.
The manifest stays in the root `package.json` because Pi installs a git source
from the repository root and has no form for a subdirectory. It lists three
extensions under `packages/pi/extensions/`:

| Extension        | Adds                                                     |
| ---------------- | -------------------------------------------------------- |
| `workers.ts`     | The `agent` and `agent_control` tools and `/worker-*`    |
| `pr-status.ts`   | The pull request watch and its messages                  |
| `status-area.ts` | The status rows and the worker widget                    |

They share code in `packages/pi/lib/` and talk only through Pi extension
events: `fabrial:worker` for worker progress, `fabrial:pr` for each PR
reading, and `fabrial:stop`, which PR status sends on an authorized stop so
the workers extension can cancel its workers. The workers extension answers
with `fabrial:stop-outcome`. PR status waits one second for that reply, then
for the cancels, each of which gets three seconds. A worker whose cancel does
not answer in time is reported as not confirming cancellation. Each one also
loads alone. PR status without workers reports a stop with no cancel
outcome, and the status
area without the others shows `PR unknown` and no workers. To leave one out,
filter it in the package entry of your Pi settings, such as
`"extensions": ["!packages/pi/extensions/status-area.ts"]`.

Set these before starting Pi:

```sh
export FAB_BINARY=/trusted/bin/fab
export FAB_DISPATCH_CONFIG=/private/issuer.toml
export FAB_TRUSTED_CHECKOUT=/trusted/fabrial
```

The `agent` tool takes `role`, `taskFile` and `worktree`, runs
`fab dispatch --detach` and returns the worker ID; the model cannot choose the
config or trusted checkout. Pi stays free while the package reads events once
a second. Progress goes to `fabrial:worker` extension events without a model
call. Completion or a blocker starts a turn, even when Pi was idle.

| Control              | Use                                                           |
| -------------------- | ------------------------------------------------------------- |
| `agent_control`      | Tool to inspect or cancel one worker                          |
| `/worker-steer`      | Driver command taking JSON with `workerId`, `head`, `message` |
| `/worker-watch <id>` | Attach this session to a worker started elsewhere             |

Cancelling a Pi turn, closing Pi or reloading it leaves workers running. The
exception is cancelling during launch: if dispatch has already accepted the
worker, the package requests cancellation and ignores a failed request.
Resuming the same session restores its workers and delivers each missed
notification once. Print mode can exit after launch; the CLI still reaches the
worker.

### Pull request status

The package also watches the pull request for the session's branch with the
host's `gh`, which needs its own GitHub login. It reads at most every 30
seconds and backs off after errors, for at least 5 minutes after a rate
limit. A Git push, checkout or switch run in Pi marks the last reading stale
until the next one.

Each reading is keyed by repository, PR number and head SHA, and only counts
checks and reviews of that head. Its status is `unknown` before the first
reading or when there is no PR or no check, `stale` when the head may have
moved or the last read failed, and otherwise `pending`, `failed` or `success`
from the checks. Every reading goes out as a `fabrial:pr` extension event
without a model call.

These start a turn, once per session even across reloads:

| Change                                 | What the orchestrator is told                         |
| -------------------------------------- | ----------------------------------------------------- |
| Merge                                  | Start task cleanup                                    |
| Closure                                | Stop work and report to the driver                    |
| `storm:stopped` label or `/storm:stop` | Start nothing new; which workers were asked to cancel |
| `/storm:ready <sha>` naming the head   | Act on it within the current stage                    |
| Failed check                           | Input to the current stage, not a new review cycle    |
| Requested change                       | Input to the current stage, not a new review cycle    |

Commands and requested changes count only from an owner, member or
collaborator, and only when made after the Pi session started. Comment and
review text arrives marked as untrusted data, and no PR event starts a worker.
Messages quote comment bodies, review bodies and check names as untrusted
data. Readings and messages never include `gh` errors or credentials.

### Status area

In the terminal UI, two rows below the editor show the session's pull request
and run. The first shows the PR number and state, its checks and link, the
repository, the branch with ahead and behind counts, and the diff against the
fetched base. The second shows the current step, the workers active of total
with blocked and failed, and the run ID. Pi's own footer stays under them with
the worktree, context, cost and model.

Checks count only for the head they were read for. After a push, a branch
change, a failed read or a local head that differs from the PR's, they show as
stale, never as passed. A stale result names its cause (`rate limited`,
`lookup failed`, `head may have moved` or `local head differs`), then its
failed, pending and passed counts, followed by the time of the last good read,
the next retry and that head. A lookup that has not succeeded shows
`PR unknown` with the reason, such as a rate limit and when the next read is
due. `no PR` means GitHub has none for the branch.

The step comes from the roles of active workers, such as `review` or
`revise`; with none it shows `unknown`. A widget above the editor lists each
worker that is starting, working, blocked or failed, with its latest action.
It shows the kind of a worker report, not its text.

The PR number and repository are links where the terminal supports them; the
full PR URL appears as text when the row has room. Narrow terminals keep the
PR, checks and step, and drop the other fields. A stale result keeps its cause
and counts, and drops the word `checks` before it would cut them, so at 60
columns a rate limit reads `PR #47 · draft · stale, rate limited, 2 failed,
2/4 passed`. When the counts still overflow, the passed count is cut first.
The rest of the row drops in this order: branch name, PR URL, diff,
repository, ahead and behind counts, head, retry time, read time. The branch
name goes first since Pi's footer shows it. Each state has a word as well as
a color, and the row counts wide characters as two columns.

The rows update from the package's PR and worker events and from Git after
each turn, and start no timer. RPC clients get the same text through
`setStatus`. Print and JSON modes show nothing.

## Verification

`just check` runs the dispatch tests against fake host tools and the Pi
package's Node tests. They do not prove real credentials, permissions and
isolation work together, so run a host smoke before review and record it in
the PR's verification comment. To run the asynchronous path end to end with
real subscription calls and tokens, set the three variables above and run
`node scripts/async-acceptance.mjs --worktree <linked-worktree>`.

Use the `handoff` skill for unfinished work across sessions; a Pi resume is
not a task record. Follow the
[lifecycle cleanup rules](../AGENTS.md#lifecycle-cleanup) when a worker
finishes or its PR merges.
