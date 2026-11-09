//! `fab gates`: runs the checks a change touches, from
//! `.github/fabrial-gates.toml`.
//!
//! The change is every file that differs from the merge base with
//! `origin/main`: the commits since it, staged and unstaged changes, and
//! untracked files other than symlinks. Files review depth counts as
//! documentation or planning records run nothing unless a scope's globs name
//! them. Each scope whose globs match a changed file runs, in the file's
//! order. A changed file no scope names runs `full` alone, unless
//! `unmatched = "skip"`.
//!
//! The gates file is read from the merge base, so a change to it runs the
//! base's `full`. Without the file there, the command `gates` names in the
//! base's `.github/fabrial.toml` runs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Args;
use fab_core::gates::{GATES_PATH, Gates, Unmatched};
use fab_core::install;
use fab_core::settings::{SETTINGS_PATH, Settings};

use crate::depth;
use crate::run::{self, Result};

/// The gates file, or the command that stands in for it.
enum Loaded {
    Gates(Gates),
    /// The settings' `gates` command, and why it runs.
    Command(String, String),
}

/// What `fab gates` reads.
#[derive(Args)]
pub struct Options {
    /// Run the `full` command whatever changed.
    #[arg(long)]
    full: bool,
    /// The base the change is measured from, through the merge base.
    #[arg(long, default_value = "origin/main")]
    base: String,
}

/// One command to run, and why.
#[derive(Debug, PartialEq, Eq)]
struct Gate {
    /// The scope's name, or `full`.
    name: String,
    /// Why it runs.
    reason: String,
    /// The command, for `sh -c`.
    command: String,
}

/// What a change runs, and the files it leaves unchecked.
#[derive(Debug, PartialEq, Eq)]
struct Selection {
    gates: Vec<Gate>,
    /// Changed code files no scope names, under `unmatched = "skip"`.
    skipped: Vec<String>,
}

/// Runs the gates the change touches, or `full` with `--full`, and exits
/// with the first failure's code. A problem reading the change or the files
/// exits 2.
pub fn run(options: Options) -> ExitCode {
    match plan(&options) {
        Ok((root, gates)) => execute(&root, &gates),
        Err(e) => {
            eprintln!("fab: {e}");
            ExitCode::from(2)
        }
    }
}

/// The repository root and the gates to run there.
fn plan(options: &Options) -> Result<(PathBuf, Vec<Gate>)> {
    let root = PathBuf::from(run::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    let merge_base = run::git(root.as_path(), &["merge-base", &options.base, "HEAD"]).map_err(|e| {
        format!(
            "cannot find the merge base with {}; fetch it or pass --base: {e}",
            options.base
        )
    })?;
    let short = &merge_base[..merge_base.len().min(7)];
    let (gates, from) = match load(&root, &merge_base)? {
        Loaded::Gates(gates) => (gates, format!("the base {short}")),
        Loaded::Command(command, reason) => {
            return Ok((root, vec![Gate { name: "gates".into(), reason, command }]));
        }
    };
    if options.full {
        let gate = Gate { name: "full".into(), reason: "--full".into(), command: gates.full };
        return Ok((root, vec![gate]));
    }
    let (changed, untracked) = changed(&root, &merge_base)?;
    // A change to the gates file is checked with the file it replaces.
    if changed.iter().any(|p| p == GATES_PATH) {
        let reason = format!("{GATES_PATH} changed, so `full` from {from}");
        return Ok((root, vec![Gate { name: "full".into(), reason, command: gates.full }]));
    }
    // Review depth decides which files are documentation or planning.
    let planning = depth::planning(&merge_base);
    let selection = select(&gates, &changed, |p| {
        depth::is_doc(p) || depth::is_planning(p, &planning)
    });
    let strays: Vec<String> = untracked
        .into_iter()
        .filter(|p| !depth::is_doc(p) && !gates.scoped.iter().any(|s| s.matches(p)))
        .collect();
    if !strays.is_empty() && gates.unmatched == Unmatched::Full {
        eprintln!(
            "fab gates: untracked {} matches no scope; put build output in .gitignore",
            describe(&strays)
        );
    }
    if !selection.skipped.is_empty() {
        eprintln!(
            "fab gates: skipping {}, which no scope names (unmatched = \"skip\")",
            describe(&selection.skipped)
        );
    }
    if selection.gates.is_empty() {
        if changed.is_empty() {
            eprintln!("fab gates: nothing changed since {}; nothing to run", options.base);
        } else if selection.skipped.is_empty() {
            eprintln!("fab gates: only documentation and planning records changed; nothing to run");
        } else {
            eprintln!("fab gates: no scope covers the change; nothing to run");
        }
    }
    Ok((root, selection.gates))
}

/// Reads the gates from the merge base, as `fab sequence` reads its stages,
/// so a change to them applies to later changes, never to its own check.
/// Without the gates file there, the base's `gates` setting runs. A base
/// with neither, as in the change that installs fabrial, falls back to the
/// working tree's settings.
fn load(root: &Path, merge_base: &str) -> Result<Loaded> {
    let short = &merge_base[..merge_base.len().min(7)];
    let at_base = |path: &str| -> Result<Option<String>> {
        if run::git(root, &["cat-file", "-e", &format!("{merge_base}:{path}")]).is_err() {
            return Ok(None);
        }
        run::git(root, &["show", &format!("{merge_base}:{path}")])
            .map(Some)
            .map_err(|e| format!("cannot read {path} at the base {short}: {e}"))
    };
    if let Some(text) = at_base(GATES_PATH)? {
        let gates = Gates::parse(&text).map_err(|e| format!("{e} (at the base {short})"))?;
        return Ok(Loaded::Gates(gates));
    }
    if let Some(text) = at_base(SETTINGS_PATH)? {
        let settings = Settings::parse(&text).map_err(|e| format!("{e} (at the base {short})"))?;
        let reason = format!("the base {short} has no {GATES_PATH}, so the `gates` command in {SETTINGS_PATH}");
        return Ok(Loaded::Command(settings.gates, reason));
    }
    let settings = install::read_settings(root)
        .map_err(|e| e.to_string())?
        .ok_or(format!("{GATES_PATH} and {SETTINGS_PATH} are both missing."))?;
    let reason =
        format!("the base {short} has neither file, so the `gates` command in the working tree's {SETTINGS_PATH}");
    Ok(Loaded::Command(settings.gates, reason))
}

/// Every file that differs from `merge_base`: committed, staged, unstaged
/// and untracked, each once, sorted, and the untracked ones apart. An
/// untracked symlink, such as a Nix `result` link, is build output and does
/// not count; other untracked build output counts until `.gitignore` names
/// it.
fn changed(root: &Path, merge_base: &str) -> Result<(Vec<String>, Vec<String>)> {
    let diffed = run::git(root, &["diff", "-z", "--name-only", "--no-renames", merge_base])?;
    let listed = run::git(root, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    let untracked: Vec<String> = listed
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            !root
                .join(p)
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
        })
        .map(String::from)
        .collect();
    let files: BTreeSet<String> = diffed
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(String::from)
        .chain(untracked.iter().cloned())
        .collect();
    Ok((files.into_iter().collect(), untracked))
}

/// The gates `changed` runs. Files `quiet` accepts, documentation and
/// planning records, count for nothing unless a scope names them, since a
/// check may read them. A code file no scope names runs
/// `full` in place of every scope, since `full` covers them, unless
/// `unmatched` is skip.
fn select(gates: &Gates, changed: &[String], quiet: impl Fn(&str) -> bool) -> Selection {
    let code: Vec<&String> = changed
        .iter()
        .filter(|p| !quiet(p) || gates.scoped.iter().any(|s| s.matches(p)))
        .collect();
    let unmatched: Vec<String> = code
        .iter()
        .filter(|p| !gates.scoped.iter().any(|s| s.matches(p)))
        .map(|p| p.to_string())
        .collect();
    if !unmatched.is_empty() && gates.unmatched == Unmatched::Full {
        let reason = format!("{} matches no scope", describe(&unmatched));
        let gate = Gate { name: "full".into(), reason, command: gates.full.clone() };
        return Selection { gates: vec![gate], skipped: vec![] };
    }
    let scoped = gates
        .scoped
        .iter()
        .filter_map(|scope| {
            let matched: Vec<String> = code
                .iter()
                .filter(|p| scope.matches(p))
                .map(|p| p.to_string())
                .collect();
            (!matched.is_empty()).then(|| Gate {
                name: scope.name.clone(),
                reason: format!("{} changed", describe(&matched)),
                command: scope.run.clone(),
            })
        })
        .collect();
    Selection { gates: scoped, skipped: unmatched }
}

/// The first path, and how many more there are.
fn describe(paths: &[String]) -> String {
    match paths {
        [] => String::new(),
        [one] => one.clone(),
        [first, rest @ ..] => format!("{first} and {} more", rest.len()),
    }
}

/// Runs each gate with `sh -c` at `root`, its output going straight to the
/// terminal, and stops at the first failure with its exit code.
fn execute(root: &Path, gates: &[Gate]) -> ExitCode {
    for gate in gates {
        eprintln!("fab gates: {} ({}): {}", gate.name, gate.reason, gate.command);
        let status = match Command::new("sh")
            .arg("-c")
            .arg(&gate.command)
            .current_dir(root)
            .status()
        {
            Ok(status) => status,
            Err(e) => {
                eprintln!("fab: cannot run sh: {e}");
                return ExitCode::from(2);
            }
        };
        if !status.success() {
            // A command killed by a signal has no code.
            let code = status.code().unwrap_or(1);
            eprintln!("fab gates: {} failed with exit code {code}", gate.name);
            return ExitCode::from(u8::try_from(code).ok().filter(|c| *c != 0).unwrap_or(1));
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    const GATES: &str = r#"
full = "just check"

[[scoped]]
name = "rust"
paths = ["crates/**", "Cargo.toml"]
run = "cargo test"

[[scoped]]
name = "pi"
paths = ["packages/pi/**"]
run = "node --test"
"#;

    fn names(gates: &Gates, changed: &[&str]) -> (Vec<String>, Vec<String>) {
        let changed: Vec<String> = changed.iter().map(|p| p.to_string()).collect();
        let planning = vec![".arcl/workspace/".to_string()];
        let selection = select(gates, &changed, |p| {
            depth::is_doc(p) || depth::is_planning(p, &planning)
        });
        (selection.gates.into_iter().map(|g| g.name).collect(), selection.skipped)
    }

    #[test]
    fn a_scoped_change_runs_only_its_scopes() {
        let gates = Gates::parse(GATES).unwrap();
        assert_eq!(names(&gates, &["crates/fab-cli/src/gates.rs"]).0, ["rust"]);
        assert_eq!(
            names(&gates, &["packages/pi/lib/pr.mjs", "Cargo.toml", "README.md"]).0,
            ["rust", "pi"]
        );
        let changed = ["crates/a.rs".to_string(), "crates/b.rs".to_string()];
        let reason = &select(&gates, &changed, depth::is_doc).gates[0].reason;
        assert_eq!(reason, "crates/a.rs and 1 more changed");
    }

    #[test]
    fn an_unmatched_change_runs_full_unless_skipped() {
        let gates = Gates::parse(GATES).unwrap();
        let selection = select(&gates, &["crates/a.rs".into(), "justfile".into()], depth::is_doc);
        assert_eq!(
            selection.gates,
            [Gate { name: "full".into(), reason: "justfile matches no scope".into(), command: "just check".into() }]
        );
        let skip = Gates::parse(&GATES.replacen("\n[[scoped]]", "unmatched = \"skip\"\n\n[[scoped]]", 1)).unwrap();
        assert_eq!(
            names(&skip, &["crates/a.rs", "justfile"]),
            (vec!["rust".to_string()], vec!["justfile".to_string()])
        );
    }

    #[test]
    fn documentation_and_planning_records_run_nothing() {
        let gates = Gates::parse(GATES).unwrap();
        let docs = ["README.md", "docs/pi.md", ".arcl/workspace/tasks/t.md", "PLAN.md"];
        assert_eq!(names(&gates, &docs), (vec![], vec![]));
        // Agent instructions are not documentation.
        assert_eq!(names(&gates, &["AGENTS.md"]).0, ["full"]);
    }

    #[test]
    fn a_record_a_scope_names_runs_that_scope() {
        let gates = Gates::parse(&GATES.replace("\"Cargo.toml\"]", "\"Cargo.toml\", \"PLAN.md\"]")).unwrap();
        assert_eq!(
            names(&gates, &["PLAN.md", "README.md"]),
            (vec!["rust".to_string()], vec![])
        );
    }

    #[test]
    fn fabrials_own_gates_parse_and_cover_its_parts() {
        let own = Gates::parse(include_str!("../../../.github/fabrial-gates.toml")).unwrap();
        assert_eq!(own.full, "just check");
        assert_eq!(names(&own, &["crates/fab-cli/src/gates.rs"]).0, ["rust"]);
        assert_eq!(names(&own, &["packages/pi/lib/pr.mjs"]).0, ["pi"]);
        // dispatch builds the worker extension into fab.
        assert_eq!(names(&own, &["packages/pi/worker.ts"]).0, ["rust", "pi"]);
        assert_eq!(names(&own, &[".claude/skills/fab-implement/SKILL.md"]).0, ["full"]);
        // The Rust tests read PLAN.md's arcl install line.
        assert_eq!(names(&own, &["PLAN.md"]).0, ["rust"]);
    }
}
