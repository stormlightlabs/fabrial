//! Runs `fab claim` in two repositories, as two machines, that share a bare
//! remote, with the clock set through `FAB_CLAIM_NOW`.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// A bare remote with one commit on main, and the repositories `a` and `b`
/// whose `origin` it is.
fn machines(temp: &Path) -> (PathBuf, PathBuf) {
    let remote = temp.join("remote.git");
    git(temp, &["init", "-q", "--bare", "-b", "main", remote.to_str().unwrap()]);
    let clone = |name: &str| {
        let dir = temp.join(name);
        git(temp, &["init", "-q", "-b", "main", dir.to_str().unwrap()]);
        git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
        dir
    };
    let (a, b) = (clone("a"), clone("b"));
    git(&a, &["commit", "-q", "--allow-empty", "-m", "Start"]);
    git(&a, &["push", "-q", "origin", "main"]);
    (a, b)
}

/// Runs `fab claim <args>` in `dir` at the time `now`, as `run` on
/// `machine`.
fn fab(dir: &Path, now: u64, args: &[&str]) -> (Option<i32>, String, String) {
    let output: Output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .arg("claim")
        .args(args)
        .current_dir(dir)
        .env("FAB_CLAIM_NOW", now.to_string())
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn two_machines_claim_one_task_and_the_holder_releases_it() {
    let temp = tempfile::tempdir().unwrap();
    let (a, b) = machines(temp.path());
    let take = |dir: &Path, now: u64, run: &str, machine: &str| {
        fab(
            dir,
            now,
            &["take", "issue-12", "--run", run, "--machine", machine, "--ttl", "10"],
        )
    };

    let (code, stdout, stderr) = take(&a, 0, "run-a", "ma");
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(
        stdout,
        "claimed storm/issue-12 for run run-a on ma until 1970-01-01T00:10:00Z\n"
    );

    let (code, _, stderr) = take(&b, 60, "run-b", "mb");
    assert_eq!(code, Some(1));
    assert_eq!(
        stderr,
        "fab: storm/issue-12: run run-a on ma holds issue-12 until 1970-01-01T00:10:00Z\n"
    );

    // A worker the run dispatched finds the claim its run holds and goes on.
    let (code, stdout, _) = take(&b, 120, "run-a", "mb");
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        "already held: storm/issue-12 for run run-a on ma until 1970-01-01T00:10:00Z\n"
    );

    let (code, _, stderr) = fab(&b, 180, &["release", "issue-12", "--run", "run-b", "--machine", "mb"]);
    assert_eq!(
        (code, stderr.as_str()),
        (Some(1), "fab: run run-b does not hold issue-12\n")
    );
    let (code, stdout, _) = fab(&a, 180, &["release", "issue-12", "--run", "run-a", "--machine", "ma"]);
    assert_eq!(
        (code, stdout.as_str()),
        (
            Some(0),
            "released storm/issue-12 for run run-a; the branch is deleted\n"
        )
    );
    let (code, stdout, _) = take(&b, 240, "run-b", "mb");
    assert_eq!(
        (code, stdout.starts_with("claimed storm/issue-12 for run run-b")),
        (Some(0), true)
    );
}

#[test]
fn an_unusable_task_id_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = machines(temp.path());
    let (code, _, stderr) = fab(&a, 0, &["take", "a\nb", "--run", "r", "--machine", "m"]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("control character"), "{stderr}");
}

#[test]
fn a_zero_ttl_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = machines(temp.path());
    for op in ["take", "renew"] {
        let (code, _, stderr) = fab(&a, 0, &[op, "t", "--run", "r", "--machine", "m", "--ttl", "0"]);
        assert_eq!(code, Some(2), "{op}: {stderr}");
        assert!(stderr.contains("--ttl"), "{stderr}");
    }
}

#[test]
fn a_take_reads_git_in_english_under_any_locale() {
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = machines(temp.path());
    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(["claim", "take", "t", "--run", "r", "--machine", "m"])
        .current_dir(&a)
        .env("FAB_CLAIM_NOW", "0")
        .env("LC_ALL", "de_DE.UTF-8")
        .env("LANGUAGE", "de")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn the_holding_run_renews_its_expired_claim_on_take() {
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = machines(temp.path());
    let take = |now: u64| fab(&a, now, &["take", "t", "--run", "r", "--machine", "m", "--ttl", "10"]);
    assert_eq!(take(1000).0, Some(0));
    let (code, stdout, stderr) = take(99_999);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, "renewed storm/t for run r on m until 1970-01-02T03:56:39Z\n");
}

#[test]
fn a_refused_delete_says_when_the_claim_lapses() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = machines(temp.path());
    let take = fab(&a, 0, &["take", "t", "--run", "r", "--machine", "m", "--ttl", "10"]);
    assert_eq!(take.0, Some(0), "{}", take.2);
    let hook = temp.path().join("remote.git/hooks/pre-receive");
    let refuse =
        "#!/bin/sh\nwhile read old new ref; do\n  case $new in 0000000*) echo no deletes >&2; exit 1;; esac\ndone\n";
    std::fs::write(&hook, refuse).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, _, stderr) = fab(&a, 60, &["release", "t", "--run", "r", "--machine", "m"]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.starts_with(
            "fab: origin refused to delete storm/t, so the claim stays until it lapses at 1970-01-01T00:10:00Z: "
        ),
        "{stderr}"
    );
}
