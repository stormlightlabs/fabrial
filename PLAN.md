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
- **The two projects build each other, in this order.** fabrial first becomes
  usable across the stormlightlabs org, then implements arclightning's work,
  including the repository-native workspace. Only then does arcl take over
  fabrial's own tasks. That happened in phase 4: fabrial's open work now
  lives in arcl, and the skills take a task from arcl, a GitHub issue, or a
  description from the owner.
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
  its review finishes, so it cannot merge partway through. The bot then posts
  a suggested squash message: the PR title and number, a short summary and the
  owner's `Co-authored-by` trailer. The owner merges with it or edits it.
  Nothing reads the message after the merge. "Verified
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
  [Pi dispatch](docs/pi.md) documents shipped dispatch and the status area,
  and [Pi design](docs/pi-design.md) holds what is still planned.
- **Cleanup is lifecycle work.** The host cleans up worker-owned resources
  when a worker exits and task-owned resources after its PR merges. Preserve
  result evidence, shared resources, and unconfirmed credential recovery state.
  See [AGENTS.md](AGENTS.md#lifecycle-cleanup) for the completion rules.
- **Skills are written once.** Before porting thunderstorm's renderer, check
  whether Claude Code, Codex and Pi read the same `SKILL.md` layout. Port
  per-host differences only where thunderstorm's `docs/internal/hosts.md` shows
  they are needed.
- **Dashboard** per machine, reusing arclightning's Svelte `packages/ui`
  (`PlanningTree`, `Readiness`, `ReviewSurface`).
- **Releases:** a workflow run opens a release pull request whose changelog
  section lists the squash subjects since the last tag, which the owner
  edits there; pull requests do not edit `CHANGELOG.md`. The tag lands on its
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
`rustfmt.toml`. arclightning's crates are still pre-1.0, so expect the pin to
move with API changes.

## Open questions

- Which VPS: Hetzner CX33, Hostinger KVM 2 or Linode, decided at deploy time
  from the providers' live pricing pages. Aim for 8 GB and 4 vCPU.
- Isolation on the VPS: the local spike wraps whole Pi workers with pinned
  agent-sandbox.nix, but the deployment setup is still open.
- Whether the dashboard is reachable only over Tailscale.

## Work

Open work lives in arcl, in `.arcl/workspace`, as the Roadmap plan's phases 1
to 9. Install the pinned `arcl`, initialize the clone, and read the plan:

```sh
cargo install --locked --git https://github.com/stormlightlabs/arclightning --tag v0.1.0 arcl-cli
arcl init
arcl tree           # the whole plan
arcl ready          # tasks that can start
arcl context <id>   # one task, with its plan and blockers
```

When moving the arcl pin, rerun `arcl skill install` and commit the result.

### Shipped

What each phase has shipped. Phases 1, 2, 4 and 6 are done in part; their
remaining tasks are in arcl.

- Phase 0: the ruleset, `CODEOWNERS`, the bot, `AGENTS.md`, the writing skill,
  the Cargo workspace with `just check`, and the Branch PR workflow.
- Phase 1: the ported skills and agents, the five-stage `storm` review with
  its `storm/review` gate, `/storm:` commands, bot comments, and PR numbers in
  squash subjects.
- Phase 2: task sources and `.github/fabrial.toml` settings, `fab init`,
  `fab update`, `fab check`, the Release workflows, the first release
  (`v0.2.0`), and `fab update` in arclightning from it.
- Phase 3: fabrial installed in arclightning (arclightning#1), and
  arclightning's repository-native workspace, its roadmap's milestone 3.
- Phase 4: `arcl init` here, and `PLAN.md`'s open items moved into arcl.
- Phase 6: Rust dispatch of four sandboxed Pi review roles, exercised on
  PR #21, and asynchronous Pi workers with worker-exit cleanup, accepted in
  [#30](https://github.com/stormlightlabs/fabrial/pull/30).
