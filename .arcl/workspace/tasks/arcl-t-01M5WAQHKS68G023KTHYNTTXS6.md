+++
id = "arcl-t-01M5WAQHKS68G023KTHYNTTXS6"
phase = "arcl-ph-01M5S4SCFE01K31R17FQ29AHJ8"
title = "Port the gate tests to a Rust integration test"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #55: `.github/storm/gate.test.mjs` became `crates/fab-cli/tests/gate.rs`, which `cargo test` runs."
+++

The Review gate is Rust, but its tests are `.github/storm/gate.test.mjs`, run by Node against the built binary with a fake gh. Move them to `crates/fab-cli/tests/`, beside the Branch PR and release tests, so `cargo test` runs every workflow command and `.github` holds no Node tests.
