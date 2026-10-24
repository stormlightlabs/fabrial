# Changelog

What each fabrial release changes for a repository that installs it. Add a
line under Unreleased in the pull request that makes the change; the Release
workflow moves Unreleased under the new version's heading.

## Unreleased

- `fab update` refreshes an install and refuses to downgrade one, and
  `fab check` reports drift. `fab init` is now for the first install only.
- The gate leaves auto-merge off on a pull request GitHub would merge now and
  posts the squash message instead, fails `storm/review` when GitHub refuses
  auto-merge, and keeps auto-merge off after a late `/storm:stop` (#10, #11).
- On pickup, the handoff skill checks `arcl task explain` before unparking, and
  passes `--force` to `arcl task start` only when the operator asks (#12).
- PR bodies leave out attribution lines and session links (#9).
- `fab init` installs the workflows, gate, skills and agents (#8).
