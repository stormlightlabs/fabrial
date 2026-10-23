# fabrial

A software factory that takes ready tasks through implement, review,
adversarial review and revise until a pull request is ready to merge. See
[PLAN.md](PLAN.md).

## Install in a repository

Build `fab` from this repository, then run it at the root of the repository
that should use fabrial:

```sh
cargo install --path crates/fab-cli
fab init --owner <github-login> --trailer "Name <email>" --gates "<gate command>"
```

It writes the Branch PR, Review gate and Bot comment workflows, `gate.sh`,
`CODEOWNERS`, `.github/fabrial.toml`, the skills and agents, and a section of
`AGENTS.md`. Commit the result on a `claude/` branch and push it; the Branch
PR workflow opens the pull request. Later runs of `fab init` take the settings
from `.github/fabrial.toml` and replace the files with this build's.

`fab init --check` lists files that differ from this build's and an installed
version that differs from it, and exits 1 when there is any drift.

Before the first pull request there, install stormlight-thunderbot on the
repository and share its `THUNDERBOT_APP_ID` and `THUNDERBOT_PRIVATE_KEY`
secrets with it. Turn on "Allow auto-merge" in the repository's settings too;
without it the gate leaves `storm/review` pending. The gate runs from `main`,
so on the install pull request it does nothing: mark that draft ready
yourself, and require `storm/review` on `main` only after it merges.
