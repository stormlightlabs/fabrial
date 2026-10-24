# fabrial

A software factory that takes ready tasks through implement, review,
adversarial review and revise until a pull request is ready to merge. See
[PLAN.md](PLAN.md).

## Install in a repository

Install a release of `fab`, then run it at the root of the repository that
should use fabrial:

```sh
cargo install --git https://github.com/stormlightlabs/fabrial --tag v<version> fab-cli
fab init --owner <github-login> --trailer "Name <email>" --gates "<gate command>"
```

It writes the Branch PR, Review gate and Bot comment workflows, `gate.sh` and
`squash-body.sh`, `CODEOWNERS`, `.github/fabrial.toml`, the skills and agents,
and a section of `AGENTS.md`. Commit the result on a `claude/` branch and push
it; the Branch PR workflow opens the pull request.

To take a newer release, install that `fab` and run `fab update`. It keeps the
settings in `.github/fabrial.toml`, unless `--owner`, `--trailer` or `--gates`
changes one, and replaces the files with this build's. It refuses to install
an older release over a newer one unless you pass `--force`. `CHANGELOG.md`
says what each release changes.

`fab check` lists files that differ from this build's and an installed version
that differs from it, and exits 1 when there is any drift.

Before the first pull request there, install stormlight-thunderbot on the
repository and share its `THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY`
secrets with it. Turn on "Allow auto-merge" in the repository's settings too;
without it the gate fails `storm/review`. The gate runs from `main`,
so on the install pull request it does nothing: mark that draft ready
yourself, and require `storm/review` on `main` only after it merges.

## Local Pi workers

The preview `fab dispatch` command starts fresh Pi 1.4.0 review workers in
host tmux sessions. The whole worker runs in a pinned Nix sandbox. A trusted
local issuer supplies repository-restricted GitHub App tokens, and the root
Pi package exposes the same command as an `agent` tool. Follow
[Pi host setup](docs/pi.md) before running it; a normal development shell is
not worker isolation. This feature is under active development.

## Releases

Run the Release workflow from the Actions tab with the new version, such as
`0.2.0`. The bot opens a pull request that sets the version, records it in
`.github/fabrial.toml` and turns `CHANGELOG.md`'s Unreleased section into the
release's, with `Cargo.lock` to match. It holds no code, so accept it with
`/storm:ready <sha>` rather than a review. When it merges, the Release tag
workflow tags `v<version>` on the merge commit and publishes a GitHub release
with that section of the changelog. Both need the bot to have Contents write
access. If Release tag fails, fix the cause and re-run that run from the
Actions tab; a later push does not retry it.
