# The gates a pull request must pass. CI runs `just check`.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    node --test pi/*.test.mjs
    cargo run -q --bin fab -- check

fmt:
    cargo fmt --all

# Capture a terminal UI as a PNG: just shot <name> <cols> <rows> <command...>
[positional-arguments]
shot name cols rows +command:
    @.github/storm/shot.sh "$@"
