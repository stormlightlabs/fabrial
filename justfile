# The gates a pull request must pass. CI runs `just check`.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    node --test pi/dispatch.test.mjs
    cargo run -q --bin fab -- check

fmt:
    cargo fmt --all
