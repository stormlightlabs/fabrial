+++
id = "arcl-t-01M5VFA88DVYDTBJF8F4KKTR47"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Generate the changelog with git-cliff"
status = "pending"
priority = "normal"
position = 0
+++

Start after #50 (dist) merges; it reshapes the release workflows this task touches.

Replace `.github/release/changelog.sh` with git-cliff, run by the Release PR
workflow at a pinned version.

- `cliff.toml` lists the commits since the last tag, one line per squash
  subject ending with its PR number, and leaves out `Release x.y.z` commits.
- Existing subjects are imperative, not conventional: group them by first verb
  (Add → Added, Fix → Fixed, everything else → Changed). Conventional prefixes
  (`feat:`, `fix:`, …) group the same way once they are used.
- Add guidance for agents to write conventional commit subjects: the
  implement skill, the implementer agent, writing-docs and the AGENTS.md
  section `fab init` installs. The squash subject is the PR title, so the
  guidance covers PR titles.
- dist keeps reading the version's section of `CHANGELOG.md` as the release
  notes; check `dist plan --tag v<version>` still finds it.
- Remove `changelog.sh` and its tests; test the git-cliff output instead.
