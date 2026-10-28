//! Runs `fab cleanup` against scratch repositories: a bare origin, a primary
//! clone on main, worktrees on agent branches, and a fake `gh` that answers
//! the REST calls from files the test writes.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;

/// The fake `gh`. It answers a pull request list by head branch with
/// `$TEST_PULLS/head-<branch with / as _>.json`, the paginated open list
/// with `$TEST_PULLS/open.json`, and one pull request with
/// `$TEST_PULLS/<n>.json`, and a comparison with main by head with
/// `$TEST_PULLS/compare-<head>.json`; a missing list is empty and a missing
/// comparison is not behind. It logs every call and fails on anything else.
const FAKE_GH: &str = r#"#!/bin/bash
set -eu
printf '%s\n' "$*" >> "$TEST_PULLS/calls"
answer() { if [[ -e $1 ]]; then cat "$1"; else echo '[]'; fi; }
case "$*" in
  "api repos/o/r/pulls?head=o:"*)
    head=${2#*head=o:}; head=${head%%&*}
    answer "$TEST_PULLS/head-${head//\//_}.json" ;;
  "api --paginate repos/o/r/pulls?state=open"*) answer "$TEST_PULLS/open.json" ;;
  "api repos/o/r/pulls/"*) cat "$TEST_PULLS/${2##*/}.json" ;;
  "api repos/o/r/compare/main..."*)
    f="$TEST_PULLS/compare-${2##*...}.json"
    if [[ -e $f ]]; then cat "$f"; else echo '{"behind_by":0}'; fi ;;
  *) exit 91 ;;
esac
"#;

/// The scratch repositories and the fake `gh`.
struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    /// A bare origin with one commit on main and a primary clone of it.
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        // Canonical, so paths match what git prints.
        let root = root.canonicalize().unwrap();
        let fixture = Fixture { temp: tempfile::tempdir_in(root).unwrap() };
        let bin = fixture.path("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(fixture.path("pulls")).unwrap();
        fs::write(bin.join("gh"), FAKE_GH).unwrap();
        fs::set_permissions(bin.join("gh"), fs::Permissions::from_mode(0o700)).unwrap();
        let temp = fixture.temp.path().to_path_buf();
        fixture.git(&temp, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
        fixture.git(&temp, &["clone", "-q", "origin.git", "primary"]);
        fixture.commit(&fixture.primary(), "README", "start\n");
        fixture.git(&fixture.primary(), &["push", "-q", "origin", "main"]);
        fixture
    }

    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }

    fn primary(&self) -> PathBuf {
        self.path("primary")
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
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
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Writes a file in `dir`, commits it and returns the new head.
    fn commit(&self, dir: &Path, file: &str, text: &str) -> String {
        fs::write(dir.join(file), text).unwrap();
        self.git(dir, &["add", file]);
        self.git(dir, &["commit", "-q", "-m", file]);
        self.git(dir, &["rev-parse", "HEAD"])
    }

    /// Adds a worktree on a new branch with one commit and returns its path
    /// and tip.
    fn worktree(&self, branch: &str) -> (PathBuf, String) {
        let dir = self.path(&branch.replace('/', "-"));
        let path = dir.to_string_lossy().to_string();
        self.git(&self.primary(), &["worktree", "add", "-q", "-b", branch, &path, "main"]);
        let tip = self.commit(&dir, "work", branch);
        (dir, tip)
    }

    /// Answers the pull requests for `branch`: one with this number, state,
    /// merged or not, and head.
    fn pull(&self, branch: &str, number: u64, state: &str, merged: bool, head: &str) {
        let merged_at = merged.then_some("2026-01-01T00:00:00Z");
        let list = json!([{"number": number, "state": state, "merged_at": merged_at, "head": {"sha": head}}]);
        let name = format!("head-{}.json", branch.replace('/', "_"));
        fs::write(self.path("pulls").join(name), list.to_string()).unwrap();
    }

    /// Answers the open pull requests, each with its branch, mergeable
    /// state and how many commits it is behind main.
    fn open(&self, pulls: &[(u64, &str, &str, u64)]) {
        let list: Vec<_> = pulls
            .iter()
            .map(|(n, branch, ..)| json!({"number": n, "head": {"ref": branch, "repo": {"full_name": "o/r"}}}))
            .collect();
        fs::write(self.path("pulls").join("open.json"), json!(list).to_string()).unwrap();
        for (n, _, state, behind) in pulls {
            let head = format!("head{n}");
            let one = json!({"number": n, "mergeable_state": state, "base": {"ref": "main"}, "head": {"sha": head}});
            fs::write(self.path("pulls").join(format!("{n}.json")), one.to_string()).unwrap();
            let compare = json!({"behind_by": behind});
            fs::write(
                self.path("pulls").join(format!("compare-{head}.json")),
                compare.to_string(),
            )
            .unwrap();
        }
    }

    /// Pushes a new commit to origin's main from another clone.
    fn advance_origin(&self) -> String {
        let temp = self.temp.path().to_path_buf();
        if !self.path("other").exists() {
            self.git(&temp, &["clone", "-q", "origin.git", "other"]);
        }
        let n = self.git(&self.path("other"), &["rev-list", "--count", "HEAD"]);
        let head = self.commit(&self.path("other"), "later", &format!("later {n}\n"));
        self.git(&self.path("other"), &["push", "-q", "origin", "main"]);
        head
    }

    fn cleanup(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fab"))
            .args(["cleanup", "--repo", "o/r"])
            .args(args)
            .current_dir(self.primary())
            .env_clear()
            .envs(self.env())
            .env("TEST_PULLS", self.path("pulls"))
            .output()
            .unwrap()
    }

    /// The fake `gh` first on the PATH, and a home in the scratch directory.
    fn env(&self) -> [(&'static str, String); 2] {
        let path = format!("{}:{}", self.path("bin").display(), std::env::var("PATH").unwrap());
        [("PATH", path), ("HOME", self.temp.path().display().to_string())]
    }

    fn has_branch(&self, branch: &str) -> bool {
        !self.git(&self.primary(), &["branch", "--list", branch]).is_empty()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Four agent worktrees, merged and clean, merged with changes, merged at
/// another head, and open, plus one on a person's branch.
fn scene(f: &Fixture) -> [PathBuf; 5] {
    let (merged, tip) = f.worktree("storm/merged");
    f.pull("storm/merged", 1, "closed", true, &tip);
    let (dirty, tip) = f.worktree("claude/dirty");
    f.pull("claude/dirty", 2, "closed", true, &tip);
    fs::write(dirty.join("work"), "changed\n").unwrap();
    let (moved, _) = f.worktree("codex/moved");
    f.pull(
        "codex/moved",
        3,
        "closed",
        true,
        "1111111111111111111111111111111111111111",
    );
    let (open, tip) = f.worktree("storm/open");
    f.pull("storm/open", 4, "open", false, &tip);
    let (person, _) = f.worktree("feature/mine");
    f.open(&[
        (4, "storm/open", "dirty", 0),
        (5, "storm/gone", "clean", 0),
        (6, "feature/x", "dirty", 0),
        (7, "claude/behind", "blocked", 2),
        (8, "codex/new", "unknown", 0),
    ]);
    [merged, dirty, moved, open, person]
}

#[test]
fn cleanup_removes_only_merged_clean_worktrees_and_lists_resyncs() {
    let f = Fixture::new();
    let [merged, dirty, moved, open, person] = scene(&f);
    let head = f.advance_origin();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{out}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        out.contains(&format!("removed storm/merged at {}: #1 merged", merged.display())),
        "{out}"
    );
    assert!(!merged.exists() && !f.has_branch("storm/merged"));
    assert!(
        out.contains("kept claude/dirty") && out.contains("uncommitted changes"),
        "{out}"
    );
    assert!(dirty.exists() && f.has_branch("claude/dirty"));
    assert!(
        out.contains("kept codex/moved") && out.contains("differs from #3's merged head 1111111"),
        "{out}"
    );
    assert!(moved.exists() && f.has_branch("codex/moved"));
    assert!(out.contains("left storm/open") && out.contains("#4 is open"), "{out}");
    assert!(open.exists() && f.has_branch("storm/open"));
    assert!(person.exists() && !out.contains("feature/mine"), "{out}");
    assert_eq!(f.git(&f.primary(), &["rev-parse", "main"]), head);
    assert!(out.contains("main: fast-forwarded 1 commit"), "{out}");
    assert!(
        out.contains("needs resync: #4 storm/open: conflicts with main"),
        "{out}"
    );
    assert!(
        out.contains("needs resync: #7 claude/behind: behind main by 2 commits"),
        "{out}"
    );
    assert!(out.contains("unknown: #8 codex/new:"), "{out}");
    assert!(!out.contains("needs resync: #8"), "{out}");
    assert!(!out.contains("#5") && !out.contains("#6"), "{out}");
}

#[test]
fn a_worktree_whose_directory_is_gone_is_reported_not_failed() {
    let f = Fixture::new();
    let (gone, tip) = f.worktree("storm/gone");
    f.pull("storm/gone", 1, "closed", true, &tip);
    fs::remove_dir_all(&gone).unwrap();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains(&format!(
            "stale storm/gone at {at}: the directory is gone; run `git worktree remove {at}`",
            at = gone.display()
        )),
        "{out}"
    );
    assert!(f.has_branch("storm/gone"));
}

#[test]
fn cleanup_leaves_a_dirty_main() {
    let f = Fixture::new();
    let before = f.git(&f.primary(), &["rev-parse", "main"]);
    f.advance_origin();
    fs::write(f.primary().join("README"), "edited\n").unwrap();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains("main: left:") && out.contains("uncommitted changes"),
        "{out}"
    );
    assert_eq!(f.git(&f.primary(), &["rev-parse", "main"]), before);
    assert_eq!(fs::read_to_string(f.primary().join("README")).unwrap(), "edited\n");
}

#[test]
fn cleanup_with_nothing_to_do_exits_zero() {
    let f = Fixture::new();
    let output = f.cleanup(&[]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert_eq!(stdout(&output), "main: up to date\n");
}

#[test]
fn a_dry_run_changes_nothing() {
    let f = Fixture::new();
    let [merged, ..] = scene(&f);
    f.git(&f.primary(), &["fetch", "-q", "origin"]);
    let before = f.git(&f.primary(), &["rev-parse", "main"]);
    let head = f.advance_origin();
    f.git(&f.primary(), &["fetch", "-q", "origin"]);
    // A dry run that fetched would move origin/main to this commit.
    f.advance_origin();
    let worktrees = f.git(&f.primary(), &["worktree", "list", "--porcelain"]);
    let output = f.cleanup(&["--dry-run"]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains(&format!("would remove storm/merged at {}: #1 merged", merged.display())),
        "{out}"
    );
    assert!(out.contains("main: would fast-forward 1 commit"), "{out}");
    assert!(merged.exists() && f.has_branch("storm/merged"));
    assert_eq!(f.git(&f.primary(), &["rev-parse", "main"]), before);
    assert_eq!(f.git(&f.primary(), &["rev-parse", "origin/main"]), head);
    assert_eq!(f.git(&f.primary(), &["worktree", "list", "--porcelain"]), worktrees);
}

#[test]
fn a_failed_lookup_keeps_the_worktree_and_exits_two() {
    let f = Fixture::new();
    let (merged, _) = f.worktree("storm/merged");
    fs::write(f.path("pulls").join("head-storm_merged.json"), "not json").unwrap();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(2), "{out}");
    assert!(out.contains("error storm/merged at"), "{out}");
    assert!(merged.exists() && f.has_branch("storm/merged"));
}

#[test]
fn ignored_files_other_than_target_keep_a_merged_worktree() {
    let f = Fixture::new();
    let (kept, tip) = f.worktree("storm/kept");
    f.pull("storm/kept", 1, "closed", true, &tip);
    fs::write(kept.join(".gitignore"), ".env\nresult\ntarget/\n").unwrap();
    f.git(&kept, &["add", ".gitignore"]);
    f.git(&kept, &["commit", "-q", "-m", "ignore"]);
    let tip = f.git(&kept, &["rev-parse", "HEAD"]);
    f.pull("storm/kept", 1, "closed", true, &tip);
    fs::write(kept.join(".env"), "SECRET=1\n").unwrap();
    std::os::unix::fs::symlink("/nix/store/x", kept.join("result")).unwrap();
    fs::create_dir_all(kept.join("target/debug")).unwrap();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains("kept storm/kept at") && out.contains("removal would delete ignored files: .env, result"),
        "{out}"
    );
    assert!(kept.join(".env").exists() && f.has_branch("storm/kept"));
}

#[test]
fn a_merged_worktree_with_only_target_ignored_is_removed() {
    let f = Fixture::new();
    let (merged, _) = f.worktree("storm/merged");
    fs::write(merged.join(".gitignore"), "target/\n").unwrap();
    f.git(&merged, &["add", ".gitignore"]);
    f.git(&merged, &["commit", "-q", "-m", "ignore"]);
    let tip = f.git(&merged, &["rev-parse", "HEAD"]);
    f.pull("storm/merged", 1, "closed", true, &tip);
    fs::create_dir_all(merged.join("target/debug")).unwrap();
    fs::write(merged.join("target/debug/fab"), "built").unwrap();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{out}");
    assert!(out.contains("removed storm/merged at"), "{out}");
    assert!(!merged.exists() && !f.has_branch("storm/merged"));
}

#[test]
fn a_branch_that_cannot_be_deleted_keeps_its_worktree() {
    let f = Fixture::new();
    let (merged, tip) = f.worktree("storm/merged");
    f.pull("storm/merged", 1, "closed", true, &tip);
    let common = f.git(&f.primary(), &["rev-parse", "--git-common-dir"]);
    let lock = f.primary().join(common).join("refs/heads/storm/merged.lock");
    fs::write(&lock, "").unwrap();
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(2), "{out}");
    assert!(
        out.contains("error storm/merged at") && out.contains("cannot delete the branch"),
        "{out}"
    );
    fs::remove_file(&lock).unwrap();
    assert!(merged.join("work").exists() && f.has_branch("storm/merged"));
    let listed = f.git(&f.primary(), &["worktree", "list", "--porcelain"]);
    assert!(listed.contains("branch refs/heads/storm/merged"), "{listed}");
}

#[test]
fn a_worktree_that_cannot_be_removed_keeps_its_branch() {
    let f = Fixture::new();
    let (merged, tip) = f.worktree("storm/merged");
    f.pull("storm/merged", 1, "closed", true, &tip);
    f.git(&f.primary(), &["worktree", "lock", &merged.to_string_lossy()]);
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(2), "{out}");
    assert!(out.contains("cannot remove the worktree"), "{out}");
    assert!(merged.join("work").exists());
    assert_eq!(f.git(&merged, &["symbolic-ref", "HEAD"]), "refs/heads/storm/merged");
    assert_eq!(f.git(&f.primary(), &["rev-parse", "storm/merged"]), tip);
}

#[test]
fn a_merged_branch_with_no_worktree_is_deleted_at_its_tip_only() {
    let f = Fixture::new();
    let tip = f.git(&f.primary(), &["rev-parse", "main"]);
    f.git(&f.primary(), &["branch", "claude/fetched", "main"]);
    f.pull("claude/fetched", 1, "closed", true, &tip);
    f.git(&f.primary(), &["branch", "codex/moved", "main"]);
    f.pull(
        "codex/moved",
        2,
        "closed",
        true,
        "1111111111111111111111111111111111111111",
    );
    f.git(&f.primary(), &["branch", "storm/open", "main"]);
    f.pull("storm/open", 3, "open", false, &tip);
    f.git(&f.primary(), &["branch", "feature/mine", "main"]);
    f.pull("feature/mine", 4, "closed", true, &tip);

    let dry = f.cleanup(&["--dry-run"]);
    let out = stdout(&dry);
    assert_eq!(dry.status.code(), Some(1), "{out}");
    assert!(
        out.contains("would remove claude/fetched (no worktree): #1 merged"),
        "{out}"
    );
    assert!(f.has_branch("claude/fetched"));

    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{out}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.contains("removed claude/fetched (no worktree): #1 merged"), "{out}");
    assert!(!f.has_branch("claude/fetched"));
    assert!(
        out.contains("kept codex/moved (no worktree): tip") && out.contains("differs from #2's merged head 1111111"),
        "{out}"
    );
    assert!(f.has_branch("codex/moved"));
    assert!(out.contains("left storm/open (no worktree): #3 is open"), "{out}");
    assert!(f.has_branch("storm/open"));
    assert!(!out.contains("feature/mine") && f.has_branch("feature/mine"), "{out}");
}

#[test]
fn a_branch_a_worktree_is_rebasing_is_not_touched() {
    let f = Fixture::new();
    let (dir, tip) = f.worktree("storm/rebasing");
    f.pull("storm/rebasing", 1, "closed", true, &tip);
    f.git(
        &dir,
        &[
            "-c",
            "sequence.editor=sed -i 1s/^pick/edit/",
            "rebase",
            "-q",
            "-i",
            "--root",
        ],
    );
    let listed = f.git(&f.primary(), &["worktree", "list", "--porcelain"]);
    assert!(!listed.contains("refs/heads/storm/rebasing"), "{listed}");
    let output = f.cleanup(&[]);
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{out}");
    assert!(!out.contains("storm/rebasing"), "{out}");
    assert_eq!(f.git(&f.primary(), &["rev-parse", "storm/rebasing"]), tip);
}
