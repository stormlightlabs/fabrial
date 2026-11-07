+++
id = "arcl-t-01M6SCJTNZ9MYTYEZ74XG6WCZG"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Let the owner sign off in the squash message"
status = "pending"
priority = "high"
position = 0
+++

A repository that uses the Developer Certificate of Origin (DCO) needs a
person to sign off, and agent commits on `claude/` and `codex/` branches
carry no `Signed-off-by`. The owner certifies agent work at merge, and the
squash commit is the one place that reaches `main`. The setting is
optional, so other repositories keep their current squash message.

```toml
# .github/fabrial.toml
trailer = "Owais <desertthunder.dev@gmail.com>"
signoff = "Owais <desertthunder.dev@gmail.com>"
```

```text
Add lasso selection (#12)

Lets people select freeform regions on the canvas.

Merged from claude/lasso.

Co-authored-by: Owais <desertthunder.dev@gmail.com>
Signed-off-by: Owais <desertthunder.dev@gmail.com>
```

Changes:

- `crates/fab-core/src/settings.rs`: add `signoff: Option<String>` to
  `Settings`, validated like the other values (one line, no quote,
  backslash or line break), written by `render()` when set, and kept by
  `fab init --signoff` and `fab update`. A settings file without the key
  still parses.
- `crates/fab-cli/src/squash.rs`: `body()` takes the sign-off and appends
  the line after `Co-authored-by`. `attributes_an_agent()` also drops
  `Signed-off-by:` lines from the PR body, so an agent cannot sign off in
  the owner's name.
- `crates/fab-cli/src/gate.rs`: read `signoff` next to `trailer` and pass
  it to `squash::body()`.
- `crates/fab-core/src/agents-section.md`: say that the owner signs off at
  merge when `signoff` is set, and that agents never add `Signed-off-by`.

Acceptance criteria:

- [ ] With `signoff` set, the posted squash message ends with the owner's
      `Signed-off-by` line
- [ ] Without it, the squash message is unchanged
- [ ] A `Signed-off-by` line in a PR body never reaches the squash message
- [ ] `fab update` keeps the setting
