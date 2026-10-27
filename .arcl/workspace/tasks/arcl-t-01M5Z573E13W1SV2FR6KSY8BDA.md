+++
id = "arcl-t-01M5Z573E13W1SV2FR6KSY8BDA"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Count root Markdown files as documentation"
status = "pending"
priority = "normal"
position = 3
+++

Reported from arclightning's PR 14.

`fab review-depth` counts only `PLAN.md`, `README.md` and `CHANGELOG.md` at
the root as documentation. A pull request that changed a design page and
`ROADMAP.md` got the full depth, "ROADMAP.md is code, configuration or agent
instructions", and an adversarial pass a docs-only change does not need.

Acceptance criteria:

- [ ] Every Markdown file at the repository root counts as documentation,
      except `AGENTS.md` and `CLAUDE.md`, which are agent instructions
- [ ] The Check workflow's skip regex in `.github/workflows/check.yml`, its
      header comment and `depth.rs`'s module doc move with it, so documentation
      stays the files that workflow skips the build for, and
      `the_documentation_paths_match_the_check_workflow` pins the new rule
- [ ] Tests cover `ROADMAP.md` alone at standard depth and `AGENTS.md` at full
