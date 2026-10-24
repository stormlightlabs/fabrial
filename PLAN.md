# Plan

fabrial is a software factory that adapts to the repository it runs on. It
takes ready tasks through implement, review, adversarial review and revise
until a pull request is ready, and a person merges it. It runs on any number of
machines tied to one repository. Its CLI is `fab`.

fabrial replaces `stormlightlabs/thunderstorm`, which stays as a reference until
its open issues move over and it is archived.

The repository is private until the first release. The stormlightlabs org is on
a paid plan, so rulesets on `main` are enforced.

## Next: asynchronous local Pi workers

Do these items next, in order, as the first part of phase 6. They take priority
over the remaining roadmap phases. Do not wait for arcl migration, distributed
claims, the full phase 5 run engine, or phase 7's VPS daemon. Use the minimum
local run operations needed by the supervisor. Status UI and additional worker
backends follow this milestone. The design lives in
[Pi orchestration](docs/pi-orchestration.md).

- [x] Extend the existing Rust tmux recorder with durable worker records.
      Record run/worker IDs, task head, state, result location, and resource
      ownership before credentials are issued. Persist redacted results outside
      temporary issuer state. The host owns the worker process and credentials;
      records preserve current sandboxing and contain no credential values.
      Synchronous dispatch remains available.
- [x] Add detached dispatch operations: start returns a run/worker ID promptly;
      inspect reads progress or a completed result; cancel targets one worker.
      Waiting is explicit and optional. Keep synchronous dispatch available
      for callers that need it, without making Pi hold a tool call open.
- [x] Add the host-owned worker event and control bridge. Deliver progress,
      completion, failure, blockers, and questions; support cancellation and
      driver-authorized steering. Address messages by run/worker and task head,
      acknowledge actions, and reject stale or completed-worker requests.
      Use Pi JSONL RPC over anonymous pipes and private host mailboxes. Preserve fresh role sessions;
      do not share histories or launch replacement agents automatically.
- [x] Make the Pi agent tool non-blocking. Return the worker ID after launch,
      then deliver events through the extension independently of that tool
      call. Queue relevant notifications and resume the orchestrator when
      appropriate, without another user prompt or repeated model-driven
      polling. Pi must remain available to answer the driver while work runs.
      A returned tool call does not cancel its worker; cancellation is explicit.
- [ ] Reconnect after Pi reload or restart. Recover worker state and results,
      replay missed notifications without duplicate stage transitions, and
      keep workers alive when Pi disconnects. The supervisor captures evidence,
      stops leftover processes/panes, revokes tokens, and removes worker-owned
      temporary state on completion, failure, or cancellation. Retain and
      report restricted recovery state if cleanup cannot be confirmed. Recover
      unfinished cleanup after a host restart without dispatching another agent.
- [ ] Prove the asynchronous path on the real local sandbox. Launch a worker,
      answer the driver while it runs, and receive completion or a blocker
      without a user prompt. Test Pi restart, missed/duplicate events, stale
      controls, cancellation, timeout, unexpected pane exit, and cleanup after
      disconnect. Results must survive reconnect; credentials and processes
      must not leak. This must work headlessly and without the status widgets.

Asynchronous execution does not parallelize dependent review stages. Keep
implement → review → implement fixes → adversarial review → implement fixes
→ driver. Skip empty fix stages; events cannot add agents or review cycles.

## Decisions

- **Rust.** fabrial shares crates with arclightning (`arcl-core`, `arcl-store`)
  instead of shelling out to `arcl`, and each project feeds the other's
  development. Compile times are kept down with a crate-per-concern workspace,
  `cargo check` in the loop, `mold` and CI caching.
- **arcl owns the work.** Tasks, blockers, readiness (`arcl ready`), handoffs,
  completion evidence and agent context (`arcl context`, MCP) come from
  arclightning. Its repository-native workspace is committed to git, so every
  machine sees the same work. There is no GitHub Projects board. GitHub keeps
  pull requests and reviews.
- **The two projects build each other, in this order.** fabrial first becomes
  usable across the stormlightlabs org, then implements arclightning's work,
  including the repository-native workspace. Only then does arcl take over
  fabrial's own tasks. Until phase 4, `PLAN.md` tracks fabrial's work, and the
  skills take a task from arcl, a GitHub issue, or a description from the
  owner.
- **`fab init` installs fabrial by copying.** It writes the Branch PR, Review
  gate and Bot comment workflows, `gate.sh`, `CODEOWNERS`, the skills, the
  agents and an `AGENTS.md` section into a repository, from files built into
  the binary. `fab check` reports drift. While fabrial is private, a
  public repository such as arclightning cannot call its reusable workflows.
- **fabrial executes it.** It picks ready tasks, claims one, runs the review
  loop on a `storm/<task>` branch in its own worktree, opens the PR, and writes
  the handoff and evidence back to arcl.
- **Claims live in git, off `main`.** A claim is a commit on a dedicated ref,
  `refs/fabrial/claims`, naming the task, machine, run and an expiry. The push
  is the lock: a rejected push means another machine claimed first, so fetch,
  recheck and pick again. Expiry frees a dead machine's claims. Every machine
  can read every claim, so any machine's dashboard shows every run.
- **One set of run operations, three drivers.** A core crate holds the
  operations: claim, start a step, record a result, mark ready, release. The
  daemon drives the fixed sequence in code. An agent such as Pi drives it
  through the CLI or `fab mcp`. A person drives it from the dashboard or with
  `/storm:` PR comments.
- **Enforcement is server-side:** rulesets, required checks and the review
  workflow. There is no shell-command parsing or general token broker. Worker
  sessions never hold the App's private key. Actions, the daemon, and the
  explicitly authorized local spike issuer may hold it. The local issuer
  retrieves only configured BWS secrets through macOS Keychain and mints
  repository-restricted App installation tokens; it never accepts an owner PAT.
- **The bot is `stormlight-thunderbot`,** with its App ID and private key as
  org secrets shared with each repository that uses fabrial.
- **Pull requests.** The bot opens each PR as a draft, and it stays one until
  its review finishes, so it cannot merge partway through. The bot sets the
  squash commit to the PR title, a short summary and the owner's
  `Co-authored-by` trailer, as okb does. "Verified
  with" and "Not covered" never reach `main`: verification goes in the review
  comment and in arcl's completion evidence. PR bodies stay short.
- **The agent that opens a PR reviews it,** in its own Claude Code (or Codex)
  session: implement, one standard review by a fresh agent, implement its fixes,
  one adversarial review by a fresh agent, implement its fixes, then hand off
  to the driver. Skip fix stages without actionable findings. Extra verifier
  stages or review cycles require the driver's request. It ends with
  `Review finished at <sha>`. A model-free workflow turns the required
  `storm/review` check green when that comment names the head. It runs from
  `main` through `pull_request_target`, so a change cannot alter its own gate.
- **Prose follows guidelines, not a CI check:** the writing standard and the
  tells catalogue from thunderstorm's `writing-docs`, in
  `.claude/skills/writing-docs/`.
- **Workers.** Pi on the VPS comes first, through `dispatch` into tmux panes,
  which lets Pi orchestrate and use opencode and OpenAI models. Claude Code and
  Codex run in their own clouds and push `claude/` and `codex/` branches; the
  workflows do the rest. Jules gets a spike later.
- **Pi setup during development.** fabrial can own the working Pi setup,
  including extensions, UI, defaults, and configuration. This is broader than
  dispatch integration. Keep worker isolation and credential custody intact.
  `docs/pi.md` documents shipped dispatch; `docs/pi-orchestration.md` designs
  coordination and cleanup; `docs/pi-statusline.md` designs presentation.
- **Cleanup is lifecycle work.** The host cleans up worker-owned resources
  when a worker exits and task-owned resources after its PR merges. Preserve
  result evidence, shared resources, and unconfirmed credential recovery state.
  See `AGENTS.md` and `docs/pi-orchestration.md` for the completion rules.
- **Skills are written once.** Before porting thunderstorm's renderer, check
  whether Claude Code, Codex and Pi read the same `SKILL.md` layout. Port
  per-host differences only where thunderstorm's `docs/internal/hosts.md` shows
  they are needed.
- **Dashboard** per machine, reusing arclightning's Svelte `packages/ui`
  (`PlanningTree`, `Readiness`, `ReviewSurface`).
- **Releases** follow okb: a changelog line in each PR, and a workflow run that
  tags `vX.Y.Z`. The run opens a release pull request; the tag lands on its
  merge commit, so nothing writes to `main` and the owner approves each
  release. Repositories install a tagged `fab` and refresh with `fab update`.

### Using the arcl crates

arclightning's crates are not published to crates.io, and its repository is
public. fabrial takes them as git dependencies pinned to a commit, so CI fetches
them without credentials and a change upstream lands here only when the pin
moves:

```toml
[workspace.dependencies]
arcl-core = { git = "https://github.com/stormlightlabs/arclightning", rev = "<sha>" }
arcl-store = { git = "https://github.com/stormlightlabs/arclightning", rev = "<sha>" }
```

Cargo finds each crate by name inside arclightning's workspace. To work on both
projects at once, a developer overrides the source with a path in an
uncommitted `.cargo/config.toml`:

```toml
[patch."https://github.com/stormlightlabs/arclightning"]
arcl-core = { path = "../arclightning/crates/arcl-core" }
arcl-store = { path = "../arclightning/crates/arcl-store" }
```

The workspace already matches arclightning's edition (2024), resolver (3) and
`rustfmt.toml`. arclightning is still extracting its shared application layer
(its roadmap's milestone 2), so expect the pin to move with API changes.

## Open questions

- Which VPS: Hetzner CX33, Hostinger KVM 2 or Linode, decided at deploy time
  from the providers' live pricing pages. Aim for 8 GB and 4 vCPU.
- Isolation on the VPS: the local spike wraps whole Pi workers with pinned
  agent-sandbox.nix, but the deployment setup is still open.
- Whether the dashboard is reachable only over Tailscale.

## 0. Repository

- [x] Ruleset on `main` (owner), `CODEOWNERS`, and stormlight-thunderbot
      installed with its org secrets.
- [x] A short `AGENTS.md`: gates, layout, branch and PR rules.
- [x] Port the writing skill and the tells catalogue.
- [x] Cargo workspace, `just check`, and a CI check job.
- [x] Branch PR workflow, ported from okb's `branch-pr.yml`.

## 1. Review loop

- [x] Port `implement`, `review`, `adv-rev`, `revise`, `resync` and `handoff`,
      and the `implementer`, `reviewer`, `adversarial-reviewer` and `reviser`
      agents.
- [x] Review sequence in the `storm` skill: one standard pass and one
      adversarial pass by fresh agents, each followed by a verifier that tries
      to refute its findings and a revise. A model-free gate sets the required
      `storm/review` check.
- [ ] Align the shipped `storm` skill and role instructions with the five-stage
      sequence in `AGENTS.md`. Verifiers and extra review cycles require the
      driver's request; blockers end the stage with a handoff.
- [x] `/storm:review|revise|ready|stop` comment commands.
- [x] Bot comments: agents post PR comments as stormlight-thunderbot through
      the Bot comment workflow.
- [x] Accept GitHub's native-stack auto-merge refusal without failing a completed
      review. Post the squash message for the person merging the stack.
- [ ] Keep the GitHub PR number in the final squash commit subject, for
      example `Add Pi dispatch (#21)`. Update the merge-message workflows in
      a follow-up; PR comments alone do not preserve this in git history.
- [ ] Verify the shared skill layout on Claude Code, Codex and Pi, and record
      what differs. `docs/hosts.md` records Claude Code; Codex and Pi are
      still to check.

## 2. Org rollout

- [x] The skills and agents take a task from arcl, a GitHub issue, or a
      description, and run the command `gates` names in `.github/fabrial.toml`.
- [x] Per-repository settings (the owner, the squash trailer and the gate
      command) come from `.github/fabrial.toml` instead of being written into
      the workflows.
- [x] `fab init`: write the workflows, `gate.sh`, `CODEOWNERS`, the settings
      file, skills, agents and an `AGENTS.md` section into a repository, and
      record the fabrial version it installed. `--check` reports files that
      differ from the running binary's, and an installed version that differs
      from it.
- [x] Releases: `fab update` refreshes an install and refuses a downgrade,
      `fab check` reports drift, `CHANGELOG.md` records each release, the
      Release workflow opens a release pull request, and Release tag tags its
      merge commit.
- [ ] First release, `v0.2.0` (owner runs the Release workflow), then
      `fab update` in arclightning from it.
- [ ] Org setup (owner): the bot installed on each repository and the secrets
      shared. A ruleset requiring `storm/review` and the repository's check
      applies to a repository only after its `fab init` pull request merges,
      because the gate runs from the repository's `main`.

## 3. arclightning

- [x] Install fabrial in `stormlightlabs/arclightning` with `fab init` (arclightning#1).
- [ ] Implement arclightning's repository-native workspace (its roadmap's
      milestone 3) through the `storm` skill.

## 4. arcl here

- [ ] A tagged arcl release to pin, and a setup step that installs it in
      fabrial's cloud sessions (asked of arclightning).
- [ ] `arcl init` in fabrial, and move `PLAN.md`'s open items into arcl as a
      plan with phases and tasks.
- [ ] Read tasks, readiness and context from arcl through its crates; write
      handoffs and evidence back.
- [ ] Point the ledger at arcl, or retire it for arcl's dashboard.

## 5. Core

- [ ] Claims on `refs/fabrial/claims`: claim, renew, release and expire, with
      tests for two machines racing for one task.
- [ ] `fab run <task>`: the fixed sequence, one worker per step, on a worktree
      per task.
- [ ] `fab status`: runs, their step and their PR, read from the claims ref.
- [ ] Automate post-merge task cleanup: remove only clean matching
      worktrees/branches, release task-owned Nix roots, and refresh clean `main`.
      Retain and report recovery state on failure; support cleanup after restart
      without another agent. Worker-exit cleanup belongs to the next milestone
      above and must not wait for the full run engine.

## 6. Workers and Pi setup

Start with the ordered [asynchronous local worker milestone](#next-asynchronous-local-pi-workers)
above. It covers the local supervisor, detached dispatch, worker communication,
Pi event delivery, reconnect, and worker-exit cleanup before the UI or VPS work.

- [x] Implement the first local dispatch path: four fresh headless Pi 1.4.0
      review roles, host tmux recorder, root Pi `agent` tool, explicit trusted
      inputs, per-role models, and a local App issuer. Pin agent-sandbox.nix
      v5.6.3 and its nixpkgs, removing only the two owner-Keychain Mach grants.
      Dispatch and the recorder now run in Rust in `fab-cli`, without a Python
      runtime. Rust subprocess tests cover issuance, cancellation during
      issuance, deadline expiry, worker failure, revocation, and retained
      cleanup state. App JWT requests use curl Bearer headers over stdin;
      installation-token requests stay on GitHub CLI. Node tests cover the Pi tool.
- [x] Exercise the Pi `agent` tool through Rust dispatch, real App issuance,
      subscription auth, tmux, and the pinned sandbox on PR #21. A fresh
      adversarial review completed and the tmux session exited. Standard review
      and its cleanup revision ran in fresh bootstrap workers. Host probes
      denied both owner-Keychain Mach lookups while HTTPS and Pi tools worked.
      Host gates and CI passed; the owner merged PR #21. Its task worktree,
      branch, scratch state, sandbox runs, and task-specific Nix outputs were
      removed, and primary `main` was fast-forwarded.
- [ ] Host-check forced cancellation, timeout, unexpected pane death, Git push,
      and Rust gates through the shipped Rust launcher. The dispatched worker
      could not link `libiconv`; host gates and CI passed. Earlier bootstrap
      builds and pushes do not verify these operations through Rust dispatch.
- [ ] Make sandbox settings configurable for deployment needs. Keep the
      pinned isolation policy fixed for this spike; choose the settings and
      validation before adding configuration options.
- [ ] Let Claude Code and Pi call the same dispatch command for fresh workers,
      passing a role, task, worktree and result without sharing session history.
- [ ] Collect PR status for Pi independently of the UI: current head, checks,
      reviews, authorized owner commands, and merge/closure. Deliver changes
      to the orchestrator without another tool call. Invalidate old-head state,
      handle stale/error states, and bound polling or use webhooks. Follow
      `docs/pi-orchestration.md`; do not let events start extra review cycles.
- [ ] Show the current pull request, its link and CI state in the fabrial Pi
      package's status area, following `docs/pi-statusline.md`. Consume shared
      PR/worker state instead of creating another status collector.
- [ ] Maintain fabrial's development Pi setup, including extensions, UI,
      defaults, and configuration as needed. Keep its ownership explicit;
      general Pi preferences are not automatically outside fabrial's scope.
- [ ] Pi dispatch for every role, into tmux, Zellij and herdr, with per-role models
      and effort caps (port thunderstorm's `internal/dispatch` and
      `internal/roles`).
- [ ] Cloud workers: start a Claude Code or Codex cloud task for a task and
      follow its branch.
- [ ] Jules spike.

## 7. Machines

- [ ] `fab daemon` under systemd: takes ready tasks, honours a concurrency cap,
      and wakes on App webhooks or polls.
- [ ] Per-run isolation and short-lived, repository-scoped tokens.
- [ ] Logs and transcripts per run.
- [ ] Choose and set up the first VPS.

## 8. Drivers

- [ ] `fab mcp`: the run operations as MCP tools, so Pi or another agent can
      orchestrate.
- [ ] Dashboard: runs, steps, logs, and the same operations the comments
      trigger.
- [ ] Try [Mire](https://github.com/stormlightlabs/mire) on one pull request
      as the owner's local review surface. In a fetched PR worktree, capture
      the comparison in a review file outside the repository, inspect it in
      Mire, and export anchored findings and human decisions. Decide whether
      an adapter should send findings through fabrial's Bot comment workflow.
      Mire has no GitHub integration; the existing gate and owner approval
      remain the source of merge decisions.

## 9. Migration

- [ ] Move thunderstorm's open issues into arcl.
- [ ] Archive `stormlightlabs/thunderstorm`, then delete it once nothing
      installs from it.
