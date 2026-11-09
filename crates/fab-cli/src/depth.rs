//! Review depth: how many review passes a pull request takes, from the files
//! its diff touches. The `fab-storm` skill runs `fab review-depth` before the
//! review and skips the passes the depth leaves out.
//!
//! - `none`: a version bump, which changes only `version` lines in
//!   `Cargo.toml`, `Cargo.lock` or `.github/fabrial.toml`, and
//!   `CHANGELOG.md`. The owner reads its changelog lines before accepting it.
//! - `standard`: documentation, planning records and tests only. The
//!   documentation is the files the Check workflow skips the build for.
//!   At the root that is every Markdown file but agent instructions: any
//!   whose name starts with `agents` or `claude` in any case, such as
//!   `AGENTS.md`, Codex's `AGENTS.override.md` or `CLAUDE.local.md`. Under `docs/` only Markdown
//!   and images count, since another repository's `docs/` can hold a site
//!   with its own code. Planning records are the Markdown and TOML files
//!   under the directories `planning` names in the merge base's
//!   `.github/fabrial.toml`, so a pull request cannot widen them itself.
//!   Configuration under one of those directories stays code: anything
//!   under a directory such as `.github/` or `.claude/`, agent
//!   instructions, dotfiles, and manifests such as `Cargo.toml`. The Check
//!   workflow still builds for planning records.
//! - `full`: anything else, including skills and agent roles, which change
//!   how agents work, and workflows, the gate, release code and the sandbox.
//!
//! A path counts under both its old and new name, so a rename out of a
//! low-risk path keeps its risk.

use std::path::Path;

use clap::{Args, ValueEnum};

use fab_core::settings::{CONFIG_DIRS, SETTINGS_PATH, Settings};

use crate::run::{self, Result};

/// How root Markdown files that hold agent instructions start, in lowercase:
/// hosts read `AGENTS.md`, `AGENTS.override.md`, `CLAUDE.md` and
/// `CLAUDE.local.md`, and a case-insensitive checkout reads `agents.md` as
/// `AGENTS.md`. The Check workflow builds when one changes.
const AGENT_DOCS: &[&str] = &["agents", "claude"];
/// The extensions of the files under `docs/` that hold only documentation.
const DOC_KINDS: &[&str] = &[".md", ".png", ".jpg", ".gif"];
/// TOML files, in lowercase, that configure a build or a tool wherever they
/// sit, so none is a planning record.
const MANIFESTS: &[&str] = &[
    "cargo.toml",
    "clippy.toml",
    "deny.toml",
    "dist-workspace.toml",
    "fabrial.toml",
    "fabrial-sequence.toml",
    "pyproject.toml",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "taplo.toml",
    "wrangler.toml",
];
/// The extensions of the files under a planning directory that hold
/// planning records.
const PLANNING_KINDS: &[&str] = &[".md", ".toml"];
/// Files a version bump may change, each only on its `version` lines.
const VERSION_FILES: &[&str] = &["Cargo.toml", "Cargo.lock", ".github/fabrial.toml"];

/// How many review passes a pull request takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    /// No review pass.
    None,
    /// The standard pass and its revise.
    Standard,
    /// The standard pass, then the adversarial pass, each with its revise.
    Full,
}

impl Depth {
    /// The depth as `fab review-depth` prints it.
    pub fn name(self) -> &'static str {
        match self {
            Depth::None => "none",
            Depth::Standard => "standard",
            Depth::Full => "full",
        }
    }
}

/// The diff to classify.
#[derive(Args)]
#[group(skip)]
pub struct Options {
    /// The base to diff from, through the merge base.
    #[arg(long, default_value = "origin/main")]
    pub base: String,
    /// The head to classify.
    #[arg(long, default_value = "HEAD")]
    pub head: String,
}

/// Prints the depth for the diff from the merge base of `base` and `head`
/// to `head`, with the reason on standard error.
pub fn run(options: Options) -> Result<()> {
    let (depth, reason) = of_diff(&options)?;
    println!("{}", depth.name());
    eprintln!("{}: {reason}", depth.name());
    Ok(())
}

/// The depth for the diff from the merge base of `base` and `head` to
/// `head`, and why. An empty diff is an error.
pub fn of_diff(options: &Options) -> Result<(Depth, String)> {
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
    let planning = planning(&base);
    let version_only = |path: &str| {
        let show = |rev: &str| run::git(here, &["show", &format!("{rev}:{path}")]);
        match (show(&base), show(&options.head)) {
            (Ok(old), Ok(new)) => only_versions_differ(path, &old, &new),
            _ => false,
        }
    };
    Ok(classify(&paths, &planning, version_only))
}

/// The depth for a diff's paths, and why. `planning` lists the directories
/// that hold planning records, and `version_only` says whether a version
/// file changes only its own version.
pub fn classify(paths: &[&str], planning: &[String], version_only: impl Fn(&str) -> bool) -> (Depth, String) {
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
    match paths
        .iter()
        .find(|p| !is_doc(p) && !is_planning(p, planning) && !is_test(p))
    {
        None => (Depth::Standard, "documentation, planning records and tests only".into()),
        Some(path) => (
            Depth::Full,
            format!("{path} is code, configuration or agent instructions"),
        ),
    }
}

/// The planning directories the settings at `base` name. None when the file
/// or the setting is missing, and none, with a warning, when the file does
/// not parse, so planning records then take the full review.
pub fn planning(base: &str) -> Vec<String> {
    let spec = format!("{base}:{SETTINGS_PATH}");
    let Ok(text) = run::git(Path::new("."), &["show", &spec]) else {
        return Vec::new();
    };
    match Settings::parse(&text) {
        Ok(settings) => settings.planning.unwrap_or_default(),
        Err(e) => {
            eprintln!("settings: warning: {spec}: {e}; no path counts as planning");
            Vec::new()
        }
    }
}

/// Whether a path holds only documentation: what the Check workflow skips
/// the build for. `fab gates` runs nothing for these unless a scope names
/// them.
pub fn is_doc(path: &str) -> bool {
    (path.len() > ".md".len() && path.ends_with(".md") && !path.contains('/') && !is_instructions(path))
        || (path.starts_with("docs/") && DOC_KINDS.iter().any(|k| path.ends_with(k)))
}

/// Whether a path is a planning record: a Markdown or TOML file under one of
/// the `planning` directories that is not configuration.
pub fn is_planning(path: &str, planning: &[String]) -> bool {
    planning.iter().any(|dir| path.starts_with(dir.as_str()))
        && PLANNING_KINDS.iter().any(|k| path.ends_with(k))
        && !is_config(path)
}

/// Whether a Markdown or TOML file configures fabrial, a tool or an agent
/// wherever it sits: one under a directory such as `.github/` or
/// `.claude/`, agent instructions, a dotfile, a manifest or a version file.
/// A planning directory cannot lighten these.
fn is_config(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    let name = parts.last().copied().unwrap_or_default();
    parts[..parts.len() - 1].iter().any(|dir| CONFIG_DIRS.contains(dir))
        || is_instructions(name)
        || name.starts_with('.')
        || MANIFESTS.contains(&name)
        || VERSION_FILES.contains(&path)
}

/// Whether a Markdown file's name says it holds agent instructions.
fn is_instructions(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    AGENT_DOCS.iter().any(|start| lower.starts_with(start))
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
        classify(paths, &["plans/".into()], |_| true).0
    }

    #[test]
    fn a_version_bump_and_its_changelog_take_no_review() {
        assert_eq!(
            depth(&["CHANGELOG.md", "Cargo.toml", "Cargo.lock", ".github/fabrial.toml"]),
            Depth::None
        );
        // A version file that changes more than its version is code.
        assert_eq!(classify(&["Cargo.toml", "CHANGELOG.md"], &[], |_| false).0, Depth::Full);
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
            &["ROADMAP.md"],
            &["plans/tasks/t-1.md", "plans/config.toml", "PLAN.md"],
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
            ".claude/skills/fab-storm/SKILL.md",
            ".claude/agents/reviewer.md",
            "AGENTS.md",
            "CLAUDE.md",
            "AGENTS.override.md",
            "CLAUDE.local.md",
            "agents.md",
            "Claude.md",
            "docs/ROADMAP.md.rs",
            "crates/fab-cli/README.md",
            ".md",
            "conf/sandbox/worker.nix",
            "docs",
            "docs/package.json",
            "docs/astro.config.mjs",
            "docs/src/components/Nav.astro",
            ".tracker/config.toml",
            "plans/tasks/t-1.rs",
            "plansx/t-1.md",
            "crates/fab-cli/tests/fixtures/gh",
            "crates/fab-cli/tests/sub/x.rs",
            "justfile",
        ] {
            assert_eq!(depth(&["README.md", path]), Depth::Full, "{path}");
        }
    }

    #[test]
    fn planning_records_need_a_planning_setting() {
        let paths = ["plans/tasks/t-1.md"];
        assert_eq!(classify(&paths, &[], |_| true).0, Depth::Full);
        assert_eq!(classify(&paths, &["plans/".into()], |_| true).0, Depth::Standard);
        assert_eq!(classify(&paths, &["plans/tasks/".into()], |_| true).0, Depth::Standard);
        assert_eq!(classify(&paths, &["other/".into()], |_| true).0, Depth::Full);
    }

    #[test]
    fn a_planning_directory_cannot_lighten_configuration() {
        // The reproduced case: a planning setting that names .github/, which
        // validation refuses, still leaves the gate and sequence full.
        let github = [".github/".to_string()];
        for path in [
            ".github/fabrial.toml",
            ".github/fabrial-sequence.toml",
            ".github/CODEOWNERS.md",
        ] {
            assert_eq!(classify(&[path], &github, |_| true).0, Depth::Full, "{path}");
        }
        let crates = ["crates/".to_string()];
        for path in [
            "crates/fab-cli/Cargo.toml",
            "crates/fab-cli/rustfmt.toml",
            "crates/fab-cli/src/main.rs",
        ] {
            assert_eq!(classify(&[path], &crates, |_| true).0, Depth::Full, "{path}");
        }
        let plans = ["plans/".to_string()];
        for path in [
            "plans/.claude/skills/x/SKILL.md",
            "plans/.agents/x.md",
            "plans/.codex/agents/x.toml",
            "plans/AGENTS.md",
            "plans/sub/claude.local.md",
            "plans/.taplo.toml",
            "plans/Cargo.toml",
            "plans/pyproject.toml",
        ] {
            assert_eq!(classify(&[path], &plans, |_| true).0, Depth::Full, "{path}");
        }
        let root = ["".to_string()];
        assert_eq!(classify(&["Cargo.toml"], &root, |_| true).0, Depth::Full);
        assert_eq!(
            classify(&["crates/fab-cli/README.md"], &crates, |_| true).0,
            Depth::Standard
        );
    }

    #[test]
    fn the_documentation_paths_match_the_check_workflow() {
        // The workflow's own test, run on each path: it skips the build
        // exactly when depth counts a path outside docs/ as documentation.
        let check = include_str!("../../../.github/workflows/check.yml");
        let start = check.find("! grep -qvE").expect("the workflow's documentation test");
        let test = &check[start..check[start..].find("; then").map(|i| start + i).unwrap()];
        let test = test.replace("\\\n", " ");
        assert!(test.contains("&& ! grep -qiE"), "{test}");
        let script = format!("files=$1; if {test}; then echo skip; fi");
        for path in [
            "README.md",
            "ROADMAP.md",
            "PLAN.md",
            "AGENTS.md",
            "CLAUDE.md",
            "AGENTS.override.md",
            "CLAUDE.local.md",
            "agents.md",
            "Claude.md",
            ".md",
            "crates/fab-cli/README.md",
            "justfile",
            "plans/tasks/t-1.md",
        ] {
            let out = std::process::Command::new("bash")
                .args(["-c", &script, "test", path])
                .output()
                .unwrap();
            let skips = String::from_utf8_lossy(&out.stdout).trim() == "skip";
            assert_eq!(skips, is_doc(path), "{path}");
        }
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
