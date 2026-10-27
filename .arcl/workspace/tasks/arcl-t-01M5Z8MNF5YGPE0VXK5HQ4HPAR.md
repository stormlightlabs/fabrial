+++
id = "arcl-t-01M5Z8MNF5YGPE0VXK5HQ4HPAR"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Read a repository's planning paths for review depth from its settings"
status = "pending"
priority = "normal"
position = 0
+++

## Problem

`fab review-depth` treats files under `.arcl/workspace/` as planning records
that need no review. That path is fabrial's own tracker, hard-coded in
`crates/fab-cli/src/depth.rs`, so the shipped binary names a task tool, and a
repository that keeps its planning records elsewhere gets them reviewed as
code.

## Acceptance criteria

- `fab review-depth` reads a repository's planning paths from
  `.github/fabrial.toml`.
- fabrial's own `.github/fabrial.toml` names `.arcl/workspace/`.
- The shipped code and installed files name no tracker.
- The Check workflow's documentation skip regex and depth's documentation
  rule stay in step, as `the_documentation_paths_match_the_check_workflow`
  checks.
