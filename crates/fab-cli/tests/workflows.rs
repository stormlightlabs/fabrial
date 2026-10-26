//! Runs `fab branch-pr` and `fab release check-tag` against scratch
//! repositories and a fake `gh` that logs its calls.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A scratch repository with a fake `gh` first on the PATH.
struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    /// A repository with one commit on main. The fake `gh` prints
    /// `TEST_GH_LIST`, `TEST_GH_COMPARE` or `TEST_GH_PULLS` for the calls
    /// these commands make, logs every call with the text of any
    /// `--body-file` on one line, and fails on anything else.
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let temp = tempfile::tempdir_in(root).unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let gh = bin.join("gh");
        fs::write(
            &gh,
            r#"#!/bin/bash
set -eu
body=
for ((i = 1; i < $#; i++)); do
  if [[ ${!i} == --body-file ]]; then j=$((i + 1)); body=$(cat "${!j}"); fi
done
printf '%s\t%s\n' "$*" "${body//$'\n'/\\n}" >> "$TEST_GH_LOG"
case "$*" in
  "pr list"*) printf '%s\n' "${TEST_GH_LIST:-}" ;;
  "pr create"*) ;;
  "api repos/o/r/compare/"*) printf '%s\n' "$TEST_GH_COMPARE" ;;
  "api repos/o/r/commits/"*/pulls) printf '%s\n' "$TEST_GH_PULLS" ;;
  *) exit 91 ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
        let fixture = Fixture { temp };
        fs::create_dir(fixture.repo()).unwrap();
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.commit(&[("CHANGELOG.md", "# Changelog\n")], "Start");
        fixture.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        fixture
    }

    fn repo(&self) -> PathBuf {
        self.temp.path().join("repo")
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .env_clear()
            .envs(self.env())
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
            .current_dir(self.repo())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Commits the files and returns the new head.
    fn commit(&self, files: &[(&str, &str)], message: &str) -> String {
        for (path, text) in files {
            fs::write(self.repo().join(path), text).unwrap();
            self.git(&["add", path]);
        }
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Runs fab in the repository with the fake `gh` and the given
    /// environment.
    fn fab(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fab"))
            .args(args)
            .current_dir(self.repo())
            .env_clear()
            .envs(self.env())
            .env("TEST_GH_LOG", self.temp.path().join("calls"))
            .envs(env.iter().copied())
            .output()
            .unwrap()
    }

    /// The only variables git and fab see, so nothing from the caller's
    /// shell, such as GIT_DIR or GH_TOKEN, reaches them: the fake `gh` first
    /// on the PATH, and a home in the scratch directory.
    fn env(&self) -> [(&'static str, String); 2] {
        let path = format!(
            "{}:{}",
            self.temp.path().join("bin").display(),
            std::env::var("PATH").unwrap()
        );
        [("PATH", path), ("HOME", self.temp.path().display().to_string())]
    }

    /// The fake `gh`'s calls, each as its arguments and any body.
    fn calls(&self) -> Vec<(String, String)> {
        fs::read_to_string(self.temp.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(|l| {
                let (args, body) = l.split_once('\t').unwrap_or((l, ""));
                (args.to_string(), body.replace("\\n", "\n"))
            })
            .collect()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

const BRANCH_ENV: [(&str, &str); 3] = [("REPO", "o/r"), ("BRANCH", "storm/x"), ("HEAD_DIR", ".")];

#[test]
fn branch_pr_with_no_commits_ahead_asks_nothing() {
    let fixture = Fixture::new();
    let output = fixture.fab(&["branch-pr"], &BRANCH_ENV);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "No commits ahead of main.");
    assert!(fixture.calls().is_empty());
}

#[test]
fn branch_pr_leaves_an_open_pull_request_alone() {
    let fixture = Fixture::new();
    fixture.commit(&[], "Add a thing");
    let output = fixture.fab(
        &["branch-pr"],
        &[BRANCH_ENV.as_slice(), &[("TEST_GH_LIST", "7")]].concat(),
    );
    assert!(output.status.success());
    assert!(stdout(&output).starts_with("#7 is open"));
    assert!(fixture.calls().iter().all(|(args, _)| !args.starts_with("pr create")));
}

#[test]
fn branch_pr_opens_a_draft_from_the_first_commit() {
    let fixture = Fixture::new();
    fixture.git(&[
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "-Add a thing",
        "-m",
        "Why it changed.\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
    ]);
    fixture.commit(&[], "Fix the thing");
    let output = fixture.fab(&["branch-pr"], &BRANCH_ENV);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fixture.calls();
    let (args, body) = calls.iter().find(|(args, _)| args.starts_with("pr create")).unwrap();
    assert!(
        args.contains("--draft --base main --head storm/x --title=-Add a thing --body-file"),
        "{args}"
    );
    assert!(body.starts_with("Why it changed.\n\n<!-- fabrial:footer: "), "{body}");
    assert!(!body.contains("Co-Authored-By"));

    let dry = Fixture::new();
    dry.commit(&[], "Add a thing");
    let output = dry.fab(&["branch-pr"], &[BRANCH_ENV.as_slice(), &[("DRY_RUN", "1")]].concat());
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(r#"DRY_RUN gh "pr" "create""#));
    assert!(dry.calls().iter().all(|(args, _)| args.starts_with("pr list")));
}

/// A repository whose newest commit sets the workspace version to `now`
/// after `before`, with notes for `now`, and a bare `origin`. Returns the
/// fixture and the commit.
fn release(before: &str, now: &str) -> (Fixture, String) {
    let fixture = Fixture::new();
    let cargo = |v: &str| format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{v}\"\n");
    fixture.commit(&[("Cargo.toml", &cargo(before))], "Earlier");
    let sha = fixture.commit(
        &[
            ("Cargo.toml", &cargo(now)),
            (
                "CHANGELOG.md",
                &format!("# Changelog\n\n## {now} - 2026-10-26\n\n- Add a (#3)\n"),
            ),
        ],
        &format!("Release {now} (#9)"),
    );
    let origin = fixture.temp.path().join("origin.git");
    let status = Command::new("git")
        .env_clear()
        .envs(fixture.env())
        .args(["init", "-q", "--bare"])
        .arg(&origin)
        .status()
        .unwrap();
    assert!(status.success());
    fixture.git(&["remote", "add", "origin", origin.to_str().unwrap()]);
    (fixture, sha)
}

fn pulls(sha: &str, version: &str) -> String {
    format!(
        r#"[{{"merge_commit_sha":"{sha}","merged_at":"2026-10-26T00:00:00Z","head":{{"ref":"storm/release-{version}","repo":{{"full_name":"o/r"}}}}}}]"#
    )
}

#[test]
fn check_tag_prints_the_version_of_an_untagged_release_merge() {
    let (fixture, sha) = release("0.1.0", "0.2.0");
    let output = fixture.fab(
        &["release", "check-tag", "--repo", "o/r"],
        &[
            ("TEST_GH_COMPARE", "identical"),
            ("TEST_GH_PULLS", &pulls(&sha, "0.2.0")),
        ],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(stdout(&output), "0.2.0");
}

#[test]
fn check_tag_prints_nothing_when_there_is_nothing_to_tag() {
    let (fixture, _) = release("0.2.0", "0.2.0");
    let output = fixture.fab(
        &["release", "check-tag", "--repo", "o/r"],
        &[("TEST_GH_COMPARE", "ahead")],
    );
    assert!(output.status.success());
    assert_eq!(stdout(&output), "");
    assert!(String::from_utf8_lossy(&output.stderr).contains("did not rise"));

    // A candidate parent is not below its release's predecessor.
    let (fixture, sha) = release("0.7.0-rc.1", "0.6.0");
    let env = [("TEST_GH_COMPARE", "ahead"), ("TEST_GH_PULLS", &pulls(&sha, "0.6.0"))];
    let output = fixture.fab(&["release", "check-tag", "--repo", "o/r"], &env);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "");

    let (fixture, sha) = release("0.1.0", "0.2.0");
    fixture.git(&["tag", "v0.2.0"]);
    fixture.git(&["push", "-q", "origin", "v0.2.0"]);
    let env = [
        ("TEST_GH_COMPARE", "identical"),
        ("TEST_GH_PULLS", &pulls(&sha, "0.2.0")),
    ];
    let output = fixture.fab(&["release", "check-tag", "--repo", "o/r"], &env);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "");
    assert!(String::from_utf8_lossy(&output.stderr).contains("already tagged"));
}

#[test]
fn check_tag_refuses_a_commit_off_main_or_a_tag_elsewhere() {
    let (fixture, sha) = release("0.1.0", "0.2.0");
    let output = fixture.fab(
        &["release", "check-tag", "--repo", "o/r"],
        &[("TEST_GH_COMPARE", "diverged")],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not on main"));

    fixture.git(&["tag", "v0.2.0", "HEAD^"]);
    fixture.git(&["push", "-q", "origin", "v0.2.0"]);
    let env = [
        ("TEST_GH_COMPARE", "identical"),
        ("TEST_GH_PULLS", &pulls(&sha, "0.2.0")),
    ];
    let output = fixture.fab(&["release", "check-tag", "--repo", "o/r"], &env);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is tagged on"));
}
