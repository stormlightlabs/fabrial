---
title: Gates file
label: fabrial-gates.toml
description: The keys of .github/fabrial-gates.toml, which says which checks fab gates runs for the paths a change touches.
section: Reference
group: Files
order: 6
---

`.github/fabrial-gates.toml` says which checks cover which paths, so `fab gates`
runs only the checks a change touches. It runs each scope whose `paths` match a
changed file. Documentation and planning records run nothing unless a scope
names them. The repository owns the file: `fab init` writes a starter when there
is none, and `fab update` and `fab check` leave it alone. Without the file,
`fab gates` runs the `gates` command from `.github/fabrial.toml`.

## Keys

| Key         | Type             | Default  | Description                                                                                                                      |
| ----------- | ---------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `full`      | string           | required | Command that runs every check. `fab gates --full` runs it, and so does a changed file no scope names when `unmatched` is `full`. |
| `unmatched` | `full` or `skip` | `full`   | What a changed file that no scope names runs: the `full` command, or nothing.                                                    |
| `scoped`    | array of tables  | none     | Checks for parts of the repository, as `[[scoped]]` tables, in the order they run.                                               |

## Scopes

Each `[[scoped]]` table is one part of the repository and the command that
checks it.

| Key     | Type            | Default  | Description                                                                                                                              |
| ------- | --------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `name`  | string          | required | Scope's name, as `fab gates` prints it. No two scopes share one.                                                                         |
| `paths` | list of strings | required | Globs over paths from the repository root. `*` and `?` match within one path segment, and a `**` segment matches any number of segments. |
| `run`   | string          | required | Command, run with `sh -c` at the repository root.                                                                                        |

## Starter

`fab init` writes this, with the `gates` command it was given as `full`, here
`just check`:

```toml title=".github/fabrial-gates.toml"
# The checks `fab gates` runs, by the paths a change touches. It runs each
# scope whose `paths` match a changed file. Documentation and planning
# records run nothing unless a scope names them. `unmatched` says what a
# changed file no scope names runs: "full" or "skip". `fab gates --full`
# runs `full`. Each command runs with `sh -c` at the repository root. This
# repository owns the file: `fab update` and `fab check` leave it alone.

full = "just check"
unmatched = "full"

# [[scoped]]
# name = "rust"
# paths = ["src/**", "Cargo.toml", "Cargo.lock"]
# run = "cargo test"
```
