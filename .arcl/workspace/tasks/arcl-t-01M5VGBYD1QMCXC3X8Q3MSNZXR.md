+++
id = "arcl-t-01M5VGBYD1QMCXC3X8Q3MSNZXR"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Move the Review gate into fab gate"
status = "completed"
priority = "normal"
position = 0
evidence = "fab gate replaces .github/storm/gate.sh; the Review gate workflow builds or installs fab and runs it. The gate's 22 Node tests run against the binary, and the diff hash matches the old shell pipeline on three real diffs."
+++

Port `.github/storm/gate.sh` to a `fab gate` command in Rust, with the same events, environment and GitHub calls. The Review gate workflow runs `fab` instead of the script. The gate's Node tests run against the binary.
