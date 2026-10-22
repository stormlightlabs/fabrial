# The gates a pull request must pass. CI runs `just check`.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

fmt:
    cargo fmt --all
