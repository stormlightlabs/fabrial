+++
id = "arcl-t-01M5Z573CQF7XCPHTS2HW8KC6T"
phase = "arcl-ph-01M5S4SCFT693V6SCF93BDMEH7"
title = "Review a repository's update with the release it updates from"
status = "pending"
priority = "high"
position = 4
+++

Reported from arclightning's update to 0.7.1 (its PR 12).

A pull request that updates a repository to a fabrial release runs its
review with the files on `main`, which are still the older release's.

- `fab sequence` reads `.github/fabrial-sequence.toml` from the merge base.
  Before a release that installs it, the file is missing, and `fab sequence`
  exits 2 with a `git show` error. The `storm` skill's fallback, a standard
  then an adversarial review, worked by hand.
- `comment.yml` runs from `main`, so a post using an input the older release
  lacks, such as `section`, or `review` and `answers` since 0.8.0, fails with
  "Unexpected inputs provided" until the update merges.

Acceptance criteria:

- [ ] Without the file at the base, `fab sequence` prints the sequence this
      release installs and says it used it, instead of failing
- [ ] The `storm`, `review` and `revise` skills post a plain comment with the
      same body when `main`'s `comment.yml` has no input for the mode a stage
      names
- [ ] Tests cover `fab sequence` at a base without the file
