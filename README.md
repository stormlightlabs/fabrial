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

## Releases

Run the Release workflow from the Actions tab with the new version, such as
`0.2.0`. The bot opens a pull request that sets the version, records it in
`.github/fabrial.toml` and turns `CHANGELOG.md`'s Unreleased section into the
release's. It changes nothing but the version, so accept it with
`/storm:ready <sha>` rather than a review. When it merges, the Release tag
workflow tags `v<version>` on the merge commit and publishes a GitHub release
with that section of the changelog. Both need the bot to have Contents write
access.
