//! Review depth: how many review passes a pull request takes, from the files
//! its diff touches. The `storm` skill runs `fab review-depth` before the
//! review and skips the passes the depth leaves out.
//!
//! - `none`: a version bump, which changes only `version` lines in
//!   `Cargo.toml`, `Cargo.lock` or `.github/fabrial.toml`, and
//!   `CHANGELOG.md`. The owner reads its changelog lines before accepting it.
//! - `standard`: documentation, planning records and tests only. The
//!   documentation paths are the ones the Check workflow skips the build
//!   for, plus `.arcl/workspace/`.
//! - `full`: anything else, including skills and agent roles, which change
//!   how agents work, and workflows, the gate, release code and the sandbox.
//!
//! A path counts under both its old and new name, so a rename out of a
//! low-risk path keeps its risk.

use std::path::Path;

use clap::{Args, ValueEnum};

use crate::run::{self, Result};

/// Files that hold only documentation, matched as the Check workflow does.
const DOCS: &[&str] = &["PLAN.md", "README.md", "CHANGELOG.md"];
/// Directories whose files hold only documentation or planning records.
const DOC_DIRS: &[&str] = &["docs/", ".arcl/workspace/"];
/// Files a version bump may change, each only on its `version` lines.
const VERSION_FILES: &[&str] = &["Cargo.toml", "Cargo.lock", ".github/fabrial.toml"];

/// How many review passes a pull request takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Depth {
    /// No review pass.
    None,
    /// The standard pass and its revise.
    Standard,
    /// The standard pass, then the adversarial pass, each with its revise.
    Full,
}

impl Depth {
    fn name(self) -> &'static str {
        match self {
            Depth::None => "none",
            Depth::Standard => "standard",
            Depth::Full => "full",
        }
    }
}

/// The diff to classify.
#[derive(Args)]
pub struct Options {
    /// The base to diff from, through the merge base.
    #[arg(long, default_value = "origin/main")]
    base: String,
    /// The head to classify.
    #[arg(long, default_value = "HEAD")]
    head: String,
}

/// Prints the depth for the diff from the merge base of `base` and `head`
/// to `head`, with the reason on standard error.
pub fn run(options: Options) -> Result<()> {
    let here = Path::new(".");
    let range = format!("{}...{}", options.base, options.head);
    let names = run::git(here, &["diff", "--name-only", "--no-renames", &range])?;
    let paths: Vec<&str> = names.lines().filter(|l| !l.is_empty()).collect();
    let version_only = |path: &str| {
        run::git(here, &["diff", "-U0", "--no-renames", &range, "--", path])
            .is_ok_and(|diff| changes_only_versions(&diff))
    };
    let (depth, reason) = classify(&paths, version_only);
    println!("{}", depth.name());
    eprintln!("{}: {reason}", depth.name());
    Ok(())
}

/// The depth for a diff's paths, and why. `version_only` says whether a
/// version file's diff changes only `version` lines.
fn classify(paths: &[&str], version_only: impl Fn(&str) -> bool) -> (Depth, String) {
    if paths.is_empty() {
        return (Depth::None, "the diff changes nothing".into());
    }
    let bump = paths
        .iter()
        .all(|p| *p == "CHANGELOG.md" || (VERSION_FILES.contains(p) && version_only(p)));
    if bump && paths.iter().any(|p| VERSION_FILES.contains(p)) {
        return (Depth::None, "a version bump and its changelog".into());
    }
    match paths.iter().find(|p| !is_doc(p) && !is_test(p)) {
        None => (Depth::Standard, "documentation, planning records and tests only".into()),
        Some(path) => (
            Depth::Full,
            format!("{path} is code, configuration or agent instructions"),
        ),
    }
}

/// Whether a path holds only documentation or planning records.
fn is_doc(path: &str) -> bool {
    DOCS.contains(&path) || DOC_DIRS.iter().any(|d| path.starts_with(d) && path.len() > d.len())
}

/// Whether a path holds only tests: a Rust integration test or a Node test
/// file.
fn is_test(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let rust = parts.len() == 4 && parts[0] == "crates" && parts[2] == "tests" && path.ends_with(".rs");
    rust || path.ends_with(".test.mjs")
}

/// Whether a `git diff -U0` changes only `version = "..."` lines.
fn changes_only_versions(diff: &str) -> bool {
    let mut changed = 0;
    for line in diff.lines() {
        if line.starts_with("+++ ") || line.starts_with("--- ") {
            continue;
        }
        if let Some(text) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) {
            if !text.starts_with("version = \"") || !text.ends_with('"') {
                return false;
            }
            changed += 1;
        }
    }
    changed > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depth(paths: &[&str]) -> Depth {
        classify(paths, |_| true).0
    }

    #[test]
    fn a_version_bump_and_its_changelog_take_no_review() {
        assert_eq!(
            depth(&["CHANGELOG.md", "Cargo.toml", "Cargo.lock", ".github/fabrial.toml"]),
            Depth::None
        );
        assert_eq!(depth(&[]), Depth::None);
        // A version file that changes more than its version is code.
        assert_eq!(classify(&["Cargo.toml", "CHANGELOG.md"], |_| false).0, Depth::Full);
        // The changelog alone is documentation.
        assert_eq!(depth(&["CHANGELOG.md"]), Depth::Standard);
        assert_eq!(depth(&["Cargo.toml", "README.md"]), Depth::Full);
    }

    #[test]
    fn documentation_planning_and_tests_take_the_standard_pass() {
        for paths in [
            &["README.md", "docs/pi.md"][..],
            &[".arcl/workspace/tasks/arcl-t-1.md", "PLAN.md"],
            &["crates/fab-cli/tests/gate.rs", "packages/pi/lib/status.test.mjs"],
        ] {
            assert_eq!(depth(paths), Depth::Standard, "{paths:?}");
        }
    }

    #[test]
    fn code_workflows_and_agent_instructions_take_both_passes() {
        for path in [
            "crates/fab-cli/src/gate.rs",
            ".github/workflows/review.yml",
            ".github/actions/fab/action.yml",
            ".claude/skills/storm/SKILL.md",
            ".claude/agents/reviewer.md",
            "AGENTS.md",
            "conf/sandbox/worker.nix",
            "docs",
            "crates/fab-cli/tests/fixtures/gh",
            "crates/fab-cli/tests/sub/x.rs",
            "justfile",
        ] {
            assert_eq!(depth(&["README.md", path]), Depth::Full, "{path}");
        }
    }

    #[test]
    fn the_documentation_paths_match_the_check_workflow() {
        let check = include_str!("../../../.github/workflows/check.yml");
        assert!(check.contains(r"'^(PLAN\.md|README\.md|CHANGELOG\.md|docs/.+)$'"));
        assert_eq!(DOCS, ["PLAN.md", "README.md", "CHANGELOG.md"]);
    }

    #[test]
    fn only_version_lines_count_as_a_bump() {
        let bump = "diff --git a/Cargo.toml b/Cargo.toml\n--- a/Cargo.toml\n+++ b/Cargo.toml\n\
                    @@ -3 +3 @@\n-version = \"0.5.0\"\n+version = \"0.6.0\"\n";
        assert!(changes_only_versions(bump));
        let dependency = format!("{bump}@@ -9 +9 @@\n-serde = \"1\"\n+serde = \"2\"\n");
        assert!(!changes_only_versions(&dependency));
        assert!(!changes_only_versions("diff --git a/x b/x\n"));
        assert!(!changes_only_versions("+version = \"0.6.0\" # and more\n"));
    }
}
