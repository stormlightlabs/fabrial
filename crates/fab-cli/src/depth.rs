//! Review depth: how many review passes a pull request takes, from the files
//! its diff touches. The `storm` skill runs `fab review-depth` before the
//! review and skips the passes the depth leaves out.
//!
//! - `none`: a version bump, which changes only `version` lines in
//!   `Cargo.toml`, `Cargo.lock` or `.github/fabrial.toml`, and
//!   `CHANGELOG.md`. The owner reads its changelog lines before accepting it.
//! - `standard`: documentation, planning records and tests only. The
//!   documentation is the files the Check workflow skips the build for,
//!   except that under `docs/` only Markdown and images count, since another
//!   repository's `docs/` can hold a site with its own code. Planning
//!   records are `.arcl/workspace/`'s Markdown and TOML.
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
/// Directories whose files with these extensions hold only documentation or
/// planning records.
const DOC_DIRS: &[(&str, &[&str])] = &[
    ("docs/", &[".md", ".png", ".jpg", ".gif"]),
    (".arcl/workspace/", &[".md", ".toml"]),
];
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
    let base = run::git(here, &["merge-base", &options.base, &options.head])?;
    let names = run::git(here, &["diff", "--name-only", "--no-renames", &base, &options.head])?;
    let paths: Vec<&str> = names.lines().filter(|l| !l.is_empty()).collect();
    // An empty diff is more likely a wrong base or head than a pull request
    // with nothing in it, so it reads as a failure and the skill runs both
    // passes.
    if paths.is_empty() {
        return Err(format!(
            "{base}..{} changes nothing; check --base and --head.",
            options.head
        ));
    }
    let version_only = |path: &str| {
        let show = |rev: &str| run::git(here, &["show", &format!("{rev}:{path}")]);
        match (show(&base), show(&options.head)) {
            (Ok(old), Ok(new)) => only_versions_differ(path, &old, &new),
            _ => false,
        }
    };
    let (depth, reason) = classify(&paths, version_only);
    println!("{}", depth.name());
    eprintln!("{}: {reason}", depth.name());
    Ok(())
}

/// The depth for a diff's paths, and why. `version_only` says whether a
/// version file changes only its own version.
fn classify(paths: &[&str], version_only: impl Fn(&str) -> bool) -> (Depth, String) {
    // A bump moves the workspace version and lists the release in the
    // changelog; a version file changed on its own, such as the release
    // fabrial.toml installs, is configuration.
    let bump = ["Cargo.toml", "CHANGELOG.md"].iter().all(|p| paths.contains(p))
        && paths
            .iter()
            .all(|p| *p == "CHANGELOG.md" || (VERSION_FILES.contains(p) && version_only(p)));
    if bump {
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
    DOCS.contains(&path)
        || DOC_DIRS
            .iter()
            .any(|(dir, kinds)| path.starts_with(dir) && kinds.iter().any(|k| path.ends_with(k)))
}

/// Whether a path holds only tests: a Rust integration test or a Node test
/// file.
fn is_test(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let rust = parts.len() == 4 && parts[0] == "crates" && parts[2] == "tests" && path.ends_with(".rs");
    rust || path.ends_with(".test.mjs")
}

/// Whether a version file's two texts parse as TOML and differ only in
/// their own version: the package or workspace version in `Cargo.toml`, the
/// workspace members' versions in `Cargo.lock`, and `version` in
/// `.github/fabrial.toml`. A dependency's version is not the file's own.
fn only_versions_differ(path: &str, old: &str, new: &str) -> bool {
    let strip = |text: &str| {
        let mut table: toml::Table = text.parse().ok()?;
        match path {
            "Cargo.toml" => {
                if let Some(package) = table.get_mut("package").and_then(|p| p.as_table_mut()) {
                    package.remove("version");
                }
                let workspace = table.get_mut("workspace").and_then(|w| w.as_table_mut());
                if let Some(package) = workspace
                    .and_then(|w| w.get_mut("package"))
                    .and_then(|p| p.as_table_mut())
                {
                    package.remove("version");
                }
            }
            "Cargo.lock" => {
                let packages = table.get_mut("package").and_then(|p| p.as_array_mut());
                for package in packages.into_iter().flatten().filter_map(|p| p.as_table_mut()) {
                    // A package with no source is a workspace member.
                    if !package.contains_key("source") {
                        package.remove("version");
                    }
                }
            }
            _ => {
                table.remove("version");
            }
        }
        Some(table)
    };
    matches!((strip(old), strip(new)), (Some(old), Some(new)) if old == new)
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
        // A version file that changes more than its version is code.
        assert_eq!(classify(&["Cargo.toml", "CHANGELOG.md"], |_| false).0, Depth::Full);
        // The changelog alone is documentation.
        assert_eq!(depth(&["CHANGELOG.md"]), Depth::Standard);
        assert_eq!(depth(&["Cargo.toml", "README.md"]), Depth::Full);
        // A bump moves Cargo.toml and lists the release; fabrial.toml alone
        // picks the fab that workflows install.
        assert_eq!(depth(&[".github/fabrial.toml"]), Depth::Full);
        assert_eq!(depth(&[".github/fabrial.toml", "CHANGELOG.md"]), Depth::Full);
        assert_eq!(depth(&["Cargo.toml", "Cargo.lock"]), Depth::Full);
    }

    #[test]
    fn documentation_planning_and_tests_take_the_standard_pass() {
        for paths in [
            &["README.md", "docs/pi.md", "docs/images/status.png"][..],
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
            "docs/package.json",
            "docs/astro.config.mjs",
            "docs/src/components/Nav.astro",
            ".arcl/config.toml",
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
    fn only_the_files_own_version_counts() {
        let cargo = |v: &str, serde: &str| {
            format!("[workspace.package]\nversion = \"{v}\"\n\n[dependencies.serde]\nversion = \"{serde}\"\n")
        };
        assert!(only_versions_differ(
            "Cargo.toml",
            &cargo("0.5.0", "1"),
            &cargo("0.6.0", "1")
        ));
        assert!(!only_versions_differ(
            "Cargo.toml",
            &cargo("0.5.0", "1"),
            &cargo("0.5.0", "*")
        ));
        assert!(!only_versions_differ(
            "Cargo.toml",
            &cargo("0.5.0", "1"),
            &cargo("0.6.0", "2")
        ));
        let package = |v: &str| format!("[package]\nname = \"x\"\nversion = \"{v}\"\n");
        assert!(only_versions_differ("Cargo.toml", &package("0.1.0"), &package("0.2.0")));

        let lock = |member: &str, dep: &str| {
            format!(
                "version = 4\n\n[[package]]\nname = \"fab-cli\"\nversion = \"{member}\"\n\n\
                 [[package]]\nname = \"serde\"\nversion = \"{dep}\"\nsource = \"registry+https://x\"\n"
            )
        };
        assert!(only_versions_differ(
            "Cargo.lock",
            &lock("0.5.0", "1.0.1"),
            &lock("0.6.0", "1.0.1")
        ));
        assert!(!only_versions_differ(
            "Cargo.lock",
            &lock("0.5.0", "1.0.1"),
            &lock("0.6.0", "1.0.2")
        ));

        let settings = |v: &str, gates: &str| format!("owner = \"o\"\ngates = \"{gates}\"\nversion = \"{v}\"\n");
        let path = ".github/fabrial.toml";
        assert!(only_versions_differ(
            path,
            &settings("0.5.0", "just check"),
            &settings("0.6.0", "just check")
        ));
        assert!(!only_versions_differ(
            path,
            &settings("0.5.0", "just check"),
            &settings("0.6.0", "true")
        ));
        assert!(!only_versions_differ(path, "not toml [", &settings("0.6.0", "true")));
    }
}
