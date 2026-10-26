+++
id = "arcl-t-01M5VGBYE31QJG5JGKSS35GXQY"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Move the squash message and release checks into fab"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #54 (d180271): `fab branch-pr`, `fab release changelog` and `fab release check-tag`, with the squash body in `crates/fab-cli/src/squash.rs`."
+++

After the gate: `squash-body.sh`, Branch PR's title and body, `changelog.sh` and Release tag's checks become `fab` commands with Rust tests, so workflows install `fab` and call it.
