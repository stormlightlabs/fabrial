//! fabrial's own release steps, which the Release PR and Release tag
//! workflows run from a checkout of this repository.
//!
//! - `changelog` writes a release's section of `CHANGELOG.md`: one line per
//!   commit since the last release tag behind `HEAD` (`v` and a version,
//!   without a pre-release part), newest first, from its subject. A squash
//!   subject ends with its pull request number, so each line names the pull
//!   request. Release commits are left out. The section goes above the newest
//!   release's `## <version>` heading, or at the end when there is none. The
//!   owner edits the lines in the release pull request.
//! - `check-tag` decides whether a commit on main is a release to tag: the
//!   workspace version rose in it, it is the merge of the
//!   `storm/release-<version>` pull request from this repository, the tag is
//!   not on another commit, and `CHANGELOG.md` has notes for the version. It
//!   prints the version to tag, or nothing.

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use clap::Subcommand;
use serde_json::Value;

use crate::run::{self, Result};

/// The release step to run.
#[derive(Subcommand)]
pub enum Command {
    /// Write the release's section of CHANGELOG.md from the commit subjects
    /// since the last release tag.
    Changelog {
        /// The release, such as 0.2.0.
        version: String,
        /// The release date, such as 2026-10-26.
        date: String,
    },
    /// Print the version to tag on a commit on main, or nothing when the
    /// commit is not a release merge.
    CheckTag {
        /// The repository, as owner/name.
        #[arg(long, env = "REPO")]
        repo: String,
        /// The commit, checked out in the working directory with its parent.
        #[arg(long, default_value = "HEAD")]
        commit: String,
    },
}

impl Command {
    /// Runs the step in the working directory, a checkout of fabrial.
    pub fn run(self) -> Result<()> {
        match self {
            Command::Changelog { version, date } => changelog(Path::new("."), &version, &date),
            Command::CheckTag { repo, commit } => {
                if let Some(version) = check_tag(&repo, &commit)? {
                    println!("{version}");
                }
                Ok(())
            }
        }
    }
}

/// Writes the release's section into the `CHANGELOG.md` of the checkout
/// at `dir`. Fails, changing nothing, when there is nothing to list.
fn changelog(dir: &Path, version: &str, date: &str) -> Result<()> {
    let last = run::git(
        dir,
        &[
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            "v[0-9]*",
            "--exclude",
            "v*-*",
        ],
    )
    .unwrap_or_default();
    let range = if last.is_empty() { "HEAD".to_string() } else { format!("{last}..HEAD") };
    let subjects = run::git(dir, &["log", "--format=%s", &range])?;
    let Some(section) = section(version, date, subjects.lines()) else {
        let since = if last.is_empty() { "the start" } else { &last };
        return Err(format!("nothing to list: no commits since {since}."));
    };
    let path = dir.join("CHANGELOG.md");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read CHANGELOG.md: {e}"))?;
    std::fs::write(&path, insert(&text, &section)).map_err(|e| format!("cannot write CHANGELOG.md: {e}"))
}

/// The release's section, or `None` when no subject is left to list.
fn section<'a>(version: &str, date: &str, subjects: impl Iterator<Item = &'a str>) -> Option<String> {
    let lines: String = subjects
        .filter(|s| !s.is_empty() && !is_release_subject(s))
        .map(|s| format!("- {s}\n\n"))
        .collect();
    (!lines.is_empty()).then(|| format!("## {version} - {date}\n\n{lines}"))
}

/// Whether a subject is a release commit's, `Release 0.2.0` with or without
/// its pull request number.
fn is_release_subject(subject: &str) -> bool {
    let Some(rest) = subject.strip_prefix("Release ") else {
        return false;
    };
    let (version, number) = match rest.split_once(' ') {
        Some((version, number)) => (version, Some(number)),
        None => (rest, None),
    };
    let numbered = |n: &str| {
        n.strip_prefix("(#")
            .and_then(|n| n.strip_suffix(')'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    fab_core::release(version).is_some() && !version.contains('+') && number.is_none_or(numbered)
}

/// The changelog with the section above the first `## <digit>` heading, or
/// after a blank line at the end when there is none. Every line ends with a
/// newline.
fn insert(changelog: &str, section: &str) -> String {
    let mut out = String::with_capacity(changelog.len() + section.len() + 1);
    let mut done = false;
    for line in changelog.lines() {
        if !done
            && line
                .strip_prefix("## ")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        {
            out.push_str(section);
            done = true;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !done {
        out.push('\n');
        out.push_str(section);
    }
    out
}

/// The version to tag on `commit`, or `None` with the reason on standard
/// error when there is nothing to tag. Fails when the commit cannot be a
/// release: off main, a version raised outside its release pull request, a
/// tag on another commit, or no notes.
fn check_tag(repo: &str, commit: &str) -> Result<Option<String>> {
    let here = Path::new(".");
    let sha = run::git(here, &["rev-parse", &format!("{commit}^{{commit}}")])?;
    // A dispatched commit must be on main, not only somewhere in the
    // repository.
    let status = run::gh(&["api", &format!("repos/{repo}/compare/{sha}...main"), "--jq", ".status"])?;
    if status != "ahead" && status != "identical" {
        return Err(format!("{sha} is not on main; nothing was tagged."));
    }
    let now = workspace_version(&run::git(here, &["show", &format!("{sha}:Cargo.toml")])?)
        .ok_or("Cargo.toml has no [workspace.package] version that is a release like 0.2.0.")?;
    let before = run::git(here, &["show", &format!("{sha}^:Cargo.toml")])
        .ok()
        .and_then(|t| workspace_version(&t));
    if before.as_deref().and_then(fab_core::release) >= fab_core::release(&now) {
        eprintln!(
            "The version did not rise ({} to {now}); nothing to tag.",
            before.as_deref().unwrap_or("none")
        );
        return Ok(None);
    }
    // The owner's approval of the release pull request is the gate. GitHub
    // can take a moment to link a fresh merge to its pull request, so ask
    // three times.
    let mut found = false;
    for attempt in 1..=3 {
        let pulls = run::gh(&["api", &format!("repos/{repo}/commits/{sha}/pulls")])?;
        let pulls: Value =
            serde_json::from_str(&pulls).map_err(|_| format!("cannot read the pull request behind {sha}."))?;
        found = is_release_merge(&pulls, repo, &sha, &now);
        if found {
            break;
        }
        if attempt < 3 {
            sleep(Duration::from_secs(10));
        }
    }
    if !found {
        return Err(format!(
            "the version rose to {now} outside a storm/release-{now} pull request; nothing was tagged. \
             If it was one, run Release tag from the Actions tab with {sha}."
        ));
    }
    // A tag already on this commit is done; one elsewhere stops the run.
    let tag = format!("v{now}");
    let tagged = run::git(here, &["ls-remote", "--tags", "origin", &format!("refs/tags/{tag}")])?;
    if let Some(on) = tagged.split_whitespace().next() {
        if on == sha {
            eprintln!("{tag} is already tagged on {sha}.");
            return Ok(None);
        }
        return Err(format!("{tag} is tagged on {on}, not {sha}."));
    }
    // dist publishes this section as the release notes.
    let changelog = run::git(here, &["show", &format!("{sha}:CHANGELOG.md")])?;
    if notes(&changelog, &now).trim().is_empty() {
        return Err(format!("CHANGELOG.md has no notes under '## {now}'."));
    }
    Ok(Some(now))
}

/// The `[workspace.package]` version in a `Cargo.toml`, when it is a
/// release.
fn workspace_version(cargo_toml: &str) -> Option<String> {
    let table: toml::Table = cargo_toml.parse().ok()?;
    let version = table.get("workspace")?.get("package")?.get("version")?.as_str()?;
    (fab_core::release(version).is_some() && !version.contains('+')).then(|| version.to_string())
}

/// Whether the commit's pull requests include the merged
/// `storm/release-<version>` pull request from this repository that made it.
fn is_release_merge(pulls: &Value, repo: &str, sha: &str, version: &str) -> bool {
    let head = format!("storm/release-{version}");
    pulls.as_array().into_iter().flatten().any(|p| {
        p["merge_commit_sha"] == sha
            && !p["merged_at"].is_null()
            && p["head"]["ref"] == head.as_str()
            && p["head"]["repo"]["full_name"] == repo
    })
}

/// The text under a version's `## <version>` heading, up to the next `## `.
fn notes(changelog: &str, version: &str) -> String {
    let mut lines = changelog.lines().skip_while(|line| {
        !line
            .strip_prefix("## ")
            .and_then(|rest| rest.strip_prefix(version))
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    });
    lines.next();
    lines
        .take_while(|line| !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::process::Command as Process;

    use serde_json::json;

    use super::*;

    const TOP: &str = "# Changelog\n\nWhat each release changes.\n";
    const OLDER: &str = "## 0.1.0 - 2026-10-01\n\n- Old.\n";

    #[test]
    fn release_subjects_are_left_out() {
        for subject in ["Release 0.1.1", "Release 0.1.1 (#4)"] {
            assert!(is_release_subject(subject), "{subject}");
        }
        for subject in [
            "Release notes for 0.2",
            "Release 0.1.1 (#)",
            "Release 0.1.1 (#4) again",
            "Release 1.2",
        ] {
            assert!(!is_release_subject(subject), "{subject}");
        }
    }

    #[test]
    fn the_section_goes_above_the_newest_release() {
        let section = section(
            "0.2.0",
            "2026-10-25",
            ["Fix b (#5)", "Release 0.1.1 (#4)", "Add a (#3)"].into_iter(),
        );
        let section = section.unwrap();
        assert_eq!(section, "## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n- Add a (#3)\n\n");
        assert_eq!(
            insert(&format!("{TOP}\n{OLDER}"), &section),
            format!("{TOP}\n{section}{OLDER}")
        );
    }

    #[test]
    fn a_changelog_with_no_release_gets_the_section_at_the_end() {
        let section = "## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n";
        assert_eq!(insert(TOP, section), format!("{TOP}\n{section}"));
        let intro = "# Changelog\n\n```md\n## Example\n```\n";
        assert_eq!(
            insert(&format!("{intro}\n{OLDER}"), section),
            format!("{intro}\n{section}{OLDER}")
        );
    }

    #[test]
    fn nothing_but_release_commits_is_nothing_to_list() {
        assert_eq!(
            section("0.2.0", "2026-10-25", ["Release 0.1.1 (#4)", ""].into_iter()),
            None
        );
    }

    #[test]
    fn changelog_reads_the_commits_since_the_last_release_tag() {
        // A scratch repository: a commit tagged v0.1.0 (unless `tag` is
        // false), one empty commit per subject, then a candidate tag.
        let release = |subjects: &[&str], tag: bool| {
            let dir = tempfile::tempdir().unwrap();
            let git = |args: &[&str]| {
                let status = Process::new("git")
                    .args([
                        "-c",
                        "user.name=t",
                        "-c",
                        "user.email=t@t",
                        "-c",
                        "commit.gpgsign=false",
                    ])
                    .args(["-c", "tag.gpgsign=false"])
                    .args(args)
                    .current_dir(dir.path())
                    .status()
                    .unwrap();
                assert!(status.success());
            };
            git(&["init", "-q"]);
            std::fs::write(dir.path().join("CHANGELOG.md"), format!("{TOP}\n{OLDER}")).unwrap();
            git(&["add", "CHANGELOG.md"]);
            git(&["commit", "-qm", "Start"]);
            if tag {
                git(&["tag", "v0.1.0"]);
            }
            for subject in subjects {
                git(&["commit", "-q", "--allow-empty", "-m", subject]);
            }
            git(&["tag", "v0.2.0-rc.1"]);
            let result = changelog(dir.path(), "0.2.0", "2026-10-25");
            (
                result,
                std::fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap(),
            )
        };
        let (result, changelog) = release(&["Add a (#3)", "Release 0.1.1 (#4)", "Fix b (#5)"], true);
        assert_eq!(result, Ok(()));
        assert_eq!(
            changelog,
            format!("{TOP}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n- Add a (#3)\n\n{OLDER}")
        );
        let (_, changelog) = release(&["Fix b (#5)"], false);
        assert_eq!(
            changelog,
            format!("{TOP}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n- Start\n\n{OLDER}")
        );
        let (result, changelog) = release(&["Release 0.1.1 (#4)"], true);
        assert_eq!(result, Err("nothing to list: no commits since v0.1.0.".into()));
        assert_eq!(changelog, format!("{TOP}\n{OLDER}"));
    }

    #[test]
    fn the_workspace_version_must_be_a_release() {
        let cargo = |v: &str| format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{v}\"\n");
        assert_eq!(workspace_version(&cargo("0.5.0")).as_deref(), Some("0.5.0"));
        assert_eq!(workspace_version(&cargo("0.5.0-rc.1")), None);
        assert_eq!(workspace_version(&cargo("0.5")), None);
        assert_eq!(workspace_version("[package]\nversion = \"0.5.0\"\n"), None);
    }

    #[test]
    fn only_the_merged_release_pull_request_from_this_repository_counts() {
        let pull = |sha: &str, merged: bool, head: &str, repo: &str| {
            json!({"merge_commit_sha": sha, "merged_at": if merged { json!("2026-10-26T00:00:00Z") } else { json!(null) },
                   "head": {"ref": head, "repo": {"full_name": repo}}})
        };
        let check = |p: Value| is_release_merge(&json!([p]), "o/r", "abc", "0.5.0");
        assert!(check(pull("abc", true, "storm/release-0.5.0", "o/r")));
        assert!(!check(pull("def", true, "storm/release-0.5.0", "o/r")));
        assert!(!check(pull("abc", false, "storm/release-0.5.0", "o/r")));
        assert!(!check(pull("abc", true, "storm/release-0.5.1", "o/r")));
        assert!(!check(pull("abc", true, "storm/release-0.5.0", "fork/r")));
        assert!(!is_release_merge(&json!({}), "o/r", "abc", "0.5.0"));
    }

    #[test]
    fn notes_are_the_text_under_the_version() {
        let changelog = format!("{TOP}\n## 0.5.0 - 2026-10-26\n\n- Add a (#3)\n\n## 0.5.0-rc.1\n\n- x\n\n{OLDER}");
        assert_eq!(notes(&changelog, "0.5.0").trim(), "- Add a (#3)");
        assert_eq!(notes(&changelog, "0.1.0").trim(), "- Old.");
        assert_eq!(notes(&changelog, "0.5").trim(), "");
        assert_eq!(notes(&format!("{TOP}\n## 0.6.0\n\n{OLDER}"), "0.6.0").trim(), "");
    }
}
