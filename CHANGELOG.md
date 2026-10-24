# Changelog

What each fabrial release changes for a repository that installs it. Add a
line under Unreleased in the pull request that makes the change; the Release
workflow moves Unreleased under the new version's heading.

## Unreleased

- Add preview `fab dispatch` for four fresh Pi review roles and a root Pi
  `agent` tool, with host tmux, pinned whole-worker isolation, explicit trusted
  instructions, and a local repository-restricted App issuer. Dispatch runs in
  Rust without a Python runtime. Installs include Pi role models.

- Skills watch checks after review, preserve progress across orchestrator
  sessions, and refresh local `main` and clean up task worktrees after merge.
- Quote optional skill argument hints so Pi can load the review, adversarial
  review and handoff skills.
- `fab init` and `fab update` record the release alone in
  `.github/fabrial.toml`, without the commit, so open pull requests no longer
  conflict on that line. The storm skill checks that a pull request still
  merges before posting `Review finished`.

## 0.2.0 - 2026-10-24

- `fab update` refreshes an install and refuses to downgrade one, and
  `fab check` reports drift. `fab init` is now for the first install only.
- The gate leaves auto-merge off on a pull request GitHub would merge now and
  posts the squash message instead, fails `storm/review` when GitHub refuses
  auto-merge, and keeps auto-merge off after a late `/storm:stop` (#10, #11).
- A pull request's description above its footer line becomes the squash
  commit's body, without attribution lines or session links (#13).
- On pickup, the handoff skill checks `arcl task explain` before unparking, and
  passes `--force` to `arcl task start` only when the operator asks (#12).
- PR bodies leave out attribution lines and session links (#9).
- `fab init` installs the workflows, gate, skills and agents (#8).
