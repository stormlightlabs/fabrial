+++
id = "arcl-t-01M5VGBYE31QJG5JGKSS35GXQY"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Move the squash message and release checks into fab"
status = "pending"
priority = "normal"
position = 0
+++

After the gate: `squash-body.sh`, Branch PR's title and body, `changelog.sh` and Release tag's checks become `fab` commands with Rust tests, so workflows install `fab` and call it.
