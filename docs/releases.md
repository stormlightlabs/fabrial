# Releasing fabrial

These steps cut a release of fabrial itself. A repository that installs
fabrial does not run them.

Run the Release PR workflow from the Actions tab with the new version, such as
`0.2.0`. The bot opens a pull request that sets the version, records it in
`.github/fabrial.toml` and writes the release's section of `CHANGELOG.md`, with
`Cargo.lock` to match. The section lists each commit subject since the last
tag; a squash subject ends with its pull request number. Edit the lines on the
release branch, then accept it with `/storm:ready <sha>`, since it holds no
code to review.

When it merges, the Release tag workflow tags `v<version>` on the merge commit.
The tag starts the Release workflow, which dist generates: it builds `fab` for
each target and publishes a GitHub release with the archives, the installer and
that section of the changelog. Its publish job,
`.github/workflows/publish-homebrew.yml`, then commits the `fab` formula to
stormlightlabs/homebrew-tap with a bot token minted for that repository, so the
bot must be installed there with Contents write. Change targets or installers
in `dist-workspace.toml` and run `dist generate`; `dist plan` fails while the
workflow is out of date. The Release PR and Release tag workflows need the bot
to have Contents write access.

If Release tag fails, fix the cause and run Release tag from the Actions tab
with the merge commit. If the Release workflow fails after the tag exists,
re-run that run; a new Release tag run sees the tag and does nothing. When the
tagged commit itself is the cause, delete the tag, fix `main`, and release the
next version.

Any `v*` tag pushed to the repository starts the Release workflow and
publishes a release. A tag ruleset on `v*` with the owner and
stormlight-thunderbot on its bypass list keeps other writers, agents included,
from publishing one.
