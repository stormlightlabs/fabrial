# Plan

fabrial is a software factory that adapts to the repository it runs on. It
takes ready tasks through implement, review, adversarial review and revise
until a pull request is ready, and a person merges it. It runs on any number of
machines tied to one repository. Its CLI is `fab`.

fabrial replaces `stormlightlabs/thunderstorm`, which stays as a reference until
its open issues move over and it is archived.

The repository is private until the first release. The stormlightlabs org is on
a paid plan, so rulesets on `main` are enforced.

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
  workflow. There is no shell-command parsing and no local token broker. Worker
  sessions never hold the App's private key; only Actions and the daemon do.
- **The bot is `stormlight-thunderbot`,** with its App ID and private key as
  org secrets shared with fabrial.
- **Pull requests.** The bot sets the squash commit to the PR title, a short
  summary and the owner's `Co-authored-by` trailer, as okb does. "Verified
  with" and "Not covered" never reach `main`: verification goes in the review
  comment and in arcl's completion evidence. PR bodies stay short.
- **The agent that opens a PR reviews it,** in its own Claude Code (or Codex)
  session: one standard review and one adversarial review by fresh agents,
  each checked by a verifier and followed by a revise. It ends with
  `Review finished at <sha>`. A
  model-free workflow turns the required `storm/review` check green when that
  comment names the head. It runs from `main` through `pull_request_target`,
  so a change cannot alter its own gate.
- **Prose follows guidelines, not a CI check:** the writing standard and the
  tells catalogue from thunderstorm's `writing-docs`, in
  `.claude/skills/writing-docs/`.
- **Workers.** Pi on the VPS comes first, through `dispatch` into tmux panes,
  which lets Pi orchestrate and use opencode and OpenAI models. Claude Code and
  Codex run in their own clouds and push `claude/` and `codex/` branches; the
  workflows do the rest. Jules gets a spike later.
- **Skills are written once.** Before porting thunderstorm's renderer, check
  whether Claude Code, Codex and Pi read the same `SKILL.md` layout. Port
  per-host differences only where thunderstorm's `docs/internal/hosts.md` shows
  they are needed.
- **Dashboard** per machine, reusing arclightning's Svelte `packages/ui`
  (`PlanningTree`, `Readiness`, `ReviewSurface`).
- **Releases** follow okb: a changelog line in each PR, and a workflow run that
  tags `vX.Y.Z`.

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
- How workers are isolated on a machine: a container per run, or a user per
  worktree.
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
      to refute its findings and a revise. A model-free gate sets the required `storm/review` check.
- [x] `/storm:review|revise|ready|stop` comment commands.
- [ ] Verify the shared skill layout on Claude Code, Codex and Pi, and record
      what differs. `docs/hosts.md` records Claude Code; Codex and Pi are
      still to check.

## 2. Core

- [ ] Read tasks, readiness and context from arcl through its crates; write
      handoffs and evidence back.
- [ ] Claims on `refs/fabrial/claims`: claim, renew, release and expire, with
      tests for two machines racing for one task.
- [ ] `fab run <task>`: the fixed sequence, one worker per step, on a worktree
      per task.
- [ ] `fab status`: runs, their step and their PR, read from the claims ref.

## 3. Workers

- [ ] Pi dispatch into tmux and zellij, with roles and effort caps (port
      thunderstorm's `internal/dispatch` and `internal/roles`).
- [ ] Cloud workers: start a Claude Code or Codex cloud task for a task and
      follow its branch.
- [ ] Jules spike.

## 4. Machines

- [ ] `fab daemon` under systemd: takes ready tasks, honours a concurrency cap,
      and wakes on App webhooks or polls.
- [ ] Per-run isolation and short-lived, repository-scoped tokens.
- [ ] Logs and transcripts per run.
- [ ] Choose and set up the first VPS.

## 5. Drivers

- [ ] `fab mcp`: the run operations as MCP tools, so Pi or another agent can
      orchestrate.
- [ ] Dashboard: runs, steps, logs, and the same operations the comments
      trigger.

## 6. Migration

- [ ] Move thunderstorm's open issues into arcl.
- [ ] Archive `stormlightlabs/thunderstorm`, then delete it once nothing
      installs from it.
