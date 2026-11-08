+++
id = "arcl-t-01M6W9F3QXHE3KPT0E6H1WSZ07"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Run the gates a change touches"
status = "in_progress"
priority = "high"
position = 0
+++

Agents run the whole gate command before every push, even for a change one
scope of the project covers. A repository should name which checks cover
which paths, and fabrial should run the ones a diff touches.

Acceptance criteria:

- [ ] `.github/fabrial-gates.toml` holds `full`, `unmatched` ("full" by
      default, or "skip") and `[[scoped]]` entries of `name`, `paths` globs
      and `run`. The repository owns it: `fab init` writes a starter, and
      `fab update` and `fab check` leave it alone
- [ ] `fab gates` runs each scope whose paths match the changed files: the
      commits since the merge base with `origin/main`, staged and unstaged
      changes, and untracked files. Files review depth counts as
      documentation or planning trigger nothing, and a changed code file no
      scope names runs `full` unless `unmatched = "skip"`
- [ ] `fab gates` says why each scope ran, streams its output and stops at
      the first failure with its exit code. `fab gates --full` runs `full`
- [ ] Without the file, `fab gates` runs `gates` from `.github/fabrial.toml`
- [ ] The implement and revise skills run `fab gates` before each push, and
      the storm skill runs `fab gates --full` before `Review finished`
- [ ] Tests cover a scoped change, an unmatched change, a docs-only change
      and a repository without the file
