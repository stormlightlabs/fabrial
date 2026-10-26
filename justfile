# The gates a pull request must pass. CI runs `just check`.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    node --test packages/pi/*.test.mjs packages/pi/lib/*.test.mjs .github/storm/*.test.mjs scripts/*.test.mjs
    cargo run -q --bin fab -- check

fmt:
    cargo fmt --all

# Capture a terminal UI as a PNG: just shot [options] <name> <cols> <rows> <command...>
[positional-arguments]
shot +args:
    @.github/storm/shot.sh "$@"
