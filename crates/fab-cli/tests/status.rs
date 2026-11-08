//! Runs `fab status` against a bare origin whose `storm/` branches
//! `fab claim` writes, with a fake `gh` that answers the pull request list
//! from a file and the clock set through `FAB_CLAIM_NOW`.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

/// The fake `gh`: it logs each call and answers the paginated list of open
/// pull requests with `$TEST_DIR/pulls.json`.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >> "$TEST_DIR/calls"
case "$*" in
  "api --paginate repos/o/r/pulls?state=open"*) cat "$TEST_DIR/pulls.json" ;;
  *) exit 91 ;;
esac
"#;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A bare origin with one commit on main, a clone of it, and the fake `gh`.
fn fixture(temp: &Path) -> PathBuf {
    git(temp, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    let clone = temp.join("clone");
    git(temp, &["init", "-q", "-b", "main", clone.to_str().unwrap()]);
    git(
        &clone,
        &["remote", "add", "origin", temp.join("origin.git").to_str().unwrap()],
    );
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "Start"]);
    git(&clone, &["push", "-q", "origin", "main"]);
    let bin = temp.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("gh"), FAKE_GH).unwrap();
    fs::set_permissions(bin.join("gh"), fs::Permissions::from_mode(0o700)).unwrap();
    clone
}

/// Runs `fab <args>` in the clone at the time `now`, with the fake `gh`
/// first on the `PATH`.
fn fab(temp: &Path, now: u64, args: &[&str]) -> (Option<i32>, String, String) {
    let path = format!("{}:{}", temp.join("bin").display(), std::env::var("PATH").unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(args)
        .current_dir(temp.join("clone"))
        .env("FAB_CLAIM_NOW", now.to_string())
        .env("TEST_DIR", temp)
        .env("PATH", path)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn take(temp: &Path, now: u64, task: &str, run: &str) {
    let (code, _, stderr) = fab(
        temp,
        now,
        &["claim", "take", task, "--run", run, "--machine", "m", "--ttl", "10"],
    );
    assert_eq!(code, Some(0), "{stderr}");
}

/// A host configuration whose private `state_dir` holds one worker record
/// for each `(role, worktree name, created_at)`.
fn config(temp: &Path, workers: &[(&str, &str, u64)]) -> PathBuf {
    let state = temp.join("state");
    for (i, (role, worktree, created_at)) in workers.iter().enumerate() {
        let id = format!("00000000-0000-4000-8000-{i:012}");
        let dir = state.join("records").join(&id);
        fs::create_dir_all(&dir).unwrap();
        let record = json!({
            "version": 1, "run_id": "r", "worker_id": id, "task_head": "abc", "role": role,
            "repository": "o/r", "worktree": format!("/w/r-worktrees/{worktree}"), "created_at": created_at,
            "updated_at": created_at, "state": "running", "credential_cleanup": "pending",
            "result_file": null, "error": null,
            "resources": {"private_state": "/x", "tmux_session": "s", "supervisor_pid": null, "worker_pid": null},
        });
        fs::write(dir.join("worker.json"), record.to_string()).unwrap();
    }
    fs::create_dir_all(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let path = temp.join("host.toml");
    fs::write(
        &path,
        format!("state_dir = {:?}\n", state.canonicalize().unwrap().to_str().unwrap()),
    )
    .unwrap();
    path
}

#[test]
fn no_claims_lists_nothing_and_asks_github_nothing() {
    let temp = tempfile::tempdir().unwrap();
    fixture(temp.path());
    let (code, stdout, stderr) = fab(temp.path(), 0, &["status"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "no claimed tasks on origin\n");
    let (code, stdout, _) = fab(temp.path(), 0, &["status", "--json"]);
    assert_eq!((code, stdout.as_str()), (Some(0), ""));
    assert!(!temp.path().join("calls").exists());
}

#[test]
fn live_lapsed_and_worked_claims_show_their_holder_pull_request_and_step() {
    let temp = tempfile::tempdir().unwrap();
    let clone = fixture(temp.path());
    take(temp.path(), 0, "lapsed", "run-a");
    take(temp.path(), 600, "live", "run-b");
    take(temp.path(), 600, "worked", "run-c");
    git(&clone, &["fetch", "-q", "origin", "storm/worked"]);
    git(&clone, &["switch", "-q", "-c", "storm/worked", "FETCH_HEAD"]);
    fs::write(clone.join("file"), "work\n").unwrap();
    git(&clone, &["add", "file"]);
    git(&clone, &["commit", "-q", "-m", "Work"]);
    git(&clone, &["push", "-q", "origin", "storm/worked"]);
    let pulls = json!([
        {"number": 7, "html_url": "https://github.com/o/r/pull/7", "draft": true,
         "head": {"ref": "storm/worked", "repo": {"full_name": "o/r"}}},
        {"number": 8, "html_url": "https://github.com/x/r/pull/8", "draft": false,
         "head": {"ref": "storm/live", "repo": {"full_name": "x/r"}}},
    ]);
    fs::write(temp.path().join("pulls.json"), pulls.to_string()).unwrap();

    let (code, stdout, stderr) = fab(temp.path(), 900, &["status", "--repo", "o/r"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        stdout,
        "lapsed: lapsed at 1970-01-01T00:10:00Z, run run-a on m\n\
         live: live until 1970-01-01T00:20:00Z, run run-b on m\n\
         worked: live until 1970-01-01T00:20:00Z, run run-c on m\n  \
         draft pull request #7 https://github.com/o/r/pull/7\n"
    );
    let calls = fs::read_to_string(temp.path().join("calls")).unwrap();
    assert_eq!(calls.lines().count(), 1, "{calls}");

    let host = config(
        temp.path(),
        &[
            ("implementer", "worked", 1),
            ("reviewer", "worked-reviewer-abc1234", 2),
            ("implementer", "live-2", 3),
        ],
    );
    let (code, stdout, stderr) = fab(
        temp.path(),
        900,
        &["status", "--repo", "o/r", "--json", "--config", host.to_str().unwrap()],
    );
    assert_eq!(code, Some(0), "{stderr}");
    let tasks: Vec<Value> = stdout.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(tasks.len(), 3, "{stdout}");
    let (lapsed, live, worked) = (&tasks[0], &tasks[1], &tasks[2]);
    assert_eq!(lapsed["task"], "lapsed");
    assert_eq!(lapsed["claim"]["live"], false);
    assert_eq!(lapsed["claim"]["run"], "run-a");
    assert_eq!(lapsed["claim"]["expires"], "1970-01-01T00:10:00Z");
    assert_eq!(lapsed["step"], Value::Null);
    assert_eq!(live["claim"]["live"], true);
    assert_eq!(live["work"], false);
    assert_eq!(live["pull_request"], Value::Null);
    assert_eq!(worked["branch"], "storm/worked");
    assert_eq!(worked["work"], true);
    assert_eq!(
        worked["pull_request"],
        json!({"number": 7, "url": "https://github.com/o/r/pull/7", "draft": true})
    );
    assert_eq!(worked["step"]["role"], "reviewer");
    assert_eq!(worked["step"]["state"], "running");

    let (code, stdout, _) = fab(
        temp.path(),
        900,
        &["status", "--repo", "o/r", "--config", host.to_str().unwrap()],
    );
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("  step: reviewer running (worker 00000000-0000-4000-8000-000000000001)\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("live: live until 1970-01-01T00:20:00Z, run run-b on m\n  step: no worker recorded\n"),
        "{stdout}"
    );
}
