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

- **Rust.** Compile times are kept down with a crate-per-concern workspace,
  `cargo check` in the loop, `mold` and CI caching.
- **A task source owns the work, and fabrial depends on none.** A repository
  keeps its tasks where it chooses and names the tracker, with the commands
  for each step, in a **Tasks** section of its `AGENTS.md`. Without one, a
  task is a GitHub issue or a description from the owner. Nothing fabrial
  installs names a tracker, and fabrial links no tracker's code. GitHub keeps
  pull requests and reviews.
- **fabrial tracks its own work in arclightning.** arclightning is the owner's
  project, and fabrial dogfoods it: fabrial's tasks live in `arcl`'s
  repository-native workspace, as `AGENTS.md`'s **Tasks** says. arclightning
  appears elsewhere in fabrial only as a documented recommendation.
- **`fab init` installs fabrial by copying.** It writes the Branch PR, Review
  gate and Bot comment workflows, the action that installs `fab`,
  `CODEOWNERS`, the skills, the agents and an `AGENTS.md` section into a
  repository, from files built into the binary. `fab check` reports drift.
  While fabrial is private, a public repository such as arclightning cannot
  call its reusable workflows.
- **The skills execute it.** An agent session running fabrial's skills
  claims a task, works it on a `storm/<task>` branch in its own worktree,
  pushes so the bot opens the PR, runs the review loop, and writes the
  handoff and evidence back to the task source.
- **A task's branch is its claim.** A run claims a task by creating
  `storm/<task>` with a push that succeeds only when the branch does not exist
  yet, `--force-with-lease=refs/heads/storm/<task>:`. A rejected push means
  another run claimed first, so fetch, recheck and pick again. The first
  commit names the task, machine, run and an expiry. Expiry frees a dead run's
  claim without losing its commits. Branch pushes work wherever an agent can
  push, cloud sessions included; a custom ref such as `refs/fabrial/claims`
  does not. Work goes on the claimed branch, so it is also the pull request's
  branch; a cloud session uses it in place of the branch its harness names.
  Where a remote refuses to delete branches, releasing pushes a claim that
  has already expired. `fab run` claims the same way when it starts a task.
- **One set of run operations, three drivers.** A core crate holds the
  operations: claim, start a step, record a result, mark ready, release.
  `fab run`, in the Pi harness, drives the fixed sequence in code. An agent
  such as Pi drives it through the CLI. A person drives it by asking the session,
  with a `/storm:ready` PR comment the gate workflow reads,
  or through whatever a host builds on the same operations.
- **Enforcement is server-side:** rulesets, required checks and the review
  workflow. There is no shell-command parsing or general token broker. Worker
  sessions never hold the App's private key. Actions and the explicitly
  authorized local spike issuer may hold it, and so may a host's issuer. The
  local issuer retrieves only configured BWS secrets from the host's secret
  store, the Keychain on macOS and the Secret Service on Linux, and mints
  repository-restricted App installation tokens; it never accepts an owner PAT.
- **The bot is `stormlight-thunderbot`,** with its App ID and private key as
  org secrets shared with each repository that uses fabrial.
- **Pull requests.** The bot opens each PR as a draft, and it stays one until
  its review finishes, so it cannot merge partway through. The bot then posts
  a suggested squash message: the PR title and number, a short summary and the
  owner's `Co-authored-by` trailer. The owner merges with it or edits it.
  Nothing reads the message after the merge. "Verified
  with" and "Not covered" never reach `main`: verification goes in the review
  comment and in the task's completion evidence. PR bodies stay short.
- **The agent that opens a PR reviews it,** in its own Claude Code (or Codex)
  session: implement, one standard review by a fresh agent, implement its fixes,
  one adversarial review by a fresh agent, implement its fixes, then hand off
  to the driver. Skip fix stages without actionable findings. Extra verifier
  stages or review cycles require the driver's request. It ends with
  `Review finished at <sha>`. A model-free workflow turns the required
  `storm/review` check green when that comment names the head. It runs from
  `main` through `pull_request_target`, so a change cannot alter its own gate.
- **fabrial is the implementation loop.** It takes one task to a reviewed
  pull request and stops there: implement, review, revise, gates, then
  `Review finished`. The loop never waits on a GitHub event or takes its
  input from one. Reviewers hand findings to the session that dispatched
  them, the reviser works from those, and the gates run locally. The loop
  still posts outward: each pass's review with its findings on the diff, the
  answers and `Review finished`. What happens after hand-off, such as CI
  failing later, the owner's review comments or a request to review again,
  belongs to the harness or the person driving it. The Pi package's PR
  status shows a failed check or a requested change for the person to read
  and starts no turn. A larger loop that
  watches pull requests and merges without a person needs projects that can
  run that hands-off, and its own planning; agents turning on auto-merge,
  since removed, was the first lesson there.
- **Prose follows guidelines, not a CI check:** the writing standard and the
  tells catalogue from thunderstorm's `writing-docs`, in
  `.claude/skills/fab-writing-docs/`.
- **fabrial is not a harness.** It ships the repository contract, the skills,
  the workflows and the `fab` commands they run. Harnesses such as Claude
  Code, Codex and Pi run the agents, each its own way: they own worker
  dispatch, sandboxes, multiplexers, message queues and their own setup.
  Worker dispatch, the sandbox, `fab run`, run events and the Pi package in
  this repository are the Pi harness. They stay as they are with no new
  work, and may move to their own repository later.
- **Cleanup is lifecycle work.** The host cleans up worker-owned resources
  when a worker exits and task-owned resources after its PR merges. Preserve
  result evidence, shared resources, and unconfirmed credential recovery state.
  See [AGENTS.md](AGENTS.md#lifecycle-cleanup) for the completion rules.
- **Skills are written once.** Before porting thunderstorm's renderer, check
  whether Claude Code, Codex and Pi read the same `SKILL.md` layout. Port
  per-host differences only where thunderstorm's `docs/internal/hosts.md` shows
  they are needed.
- **fabrial runs no server.** A service that picks up tasks on its own
  belongs to the host, and fabrial assumes nothing about where its commands
  run or how they are reached.
- **Releases:** a workflow run opens a release pull request whose changelog
  section lists the squash subjects since the last tag, which the owner
  edits there; pull requests do not edit `CHANGELOG.md`. The tag lands on its
  merge commit, so nothing writes to `main` and the owner approves each
  release. The tag starts dist's workflow, which publishes prebuilt `fab`
  binaries and an installer with the release. Repositories install a tagged
  `fab` and refresh with `fab update`.

## Work

Open work lives in arcl, in `.arcl/workspace`, as the Roadmap plan's phases 1
to 9. Install the pinned `arcl`, initialize the clone, and read the plan. In a
Claude Code cloud session, `.claude/hooks/session-start.sh` does the first
two steps:

```sh
curl -LsSf https://github.com/stormlightlabs/arclightning/releases/download/v0.3.0/arcl-cli-installer.sh | sh
arcl init
arcl tree           # the whole plan
arcl ready          # tasks that can start
arcl context <id>   # one task, with its plan and blockers
```

When moving the arcl pin, rerun `arcl skill install` and commit the result.

### Shipped

What each phase, then each release, has shipped. Phases 1, 2 and 6 are
done in part; their remaining tasks are in arcl.

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
- Phase 4: `arcl init` here, `PLAN.md`'s open items moved into arcl, and
  the session hook that installs `arcl` in cloud sessions.
- Phase 6: Rust dispatch of four sandboxed Pi review roles, exercised on
  PR #21, and asynchronous Pi workers with worker-exit cleanup, accepted in
  [#30](https://github.com/stormlightlabs/fabrial/pull/30).
- v0.9.0, #68 to #102:
  - The loop scoped to one task ending at `Review finished`, off GitHub
    events, with fabrial no harness (#80, #82, #87, #90, #93).
  - A task's branch as its claim, a claim that lapses where the remote
    refuses deletion, and `fab status` (#78, #83, #99).
  - `fab cleanup`, for merged branches with or without a worktree and the
    caches `disposable` names (#72, #75, #88).
  - `fab gates`, `planning` paths and root Markdown as documentation
    (#89, #96, #97).
  - `signoff` and `assisted_by` in the squash message (#85, #86).
  - Tasks named in `AGENTS.md` rather than the skills, the `fab-rubber-duck`
    and `fab-decompose` skills, the `fab-` prefix, tighter review passes,
    retired files removed on update, and reviewing a fabrial update before
    `main` has its files (#70, #74, #81, #94, #95, #102).
  - In the Pi harness: `fab run`, an implementer role, host pushes of writer
    commits, run operations as a library and `fab events`, and a probe of
    what the sandbox lets a worker write in Git (#91, #92, #98, #100, #101).
  - The session hook that installs `arcl`, bash found on `PATH` in it and
    the test fakes, and planning records (#68, #69, #73, #76, #84).
