//! Runs `fab claim` in two repositories, as two machines, that share a bare
//! remote, with the clock set through `FAB_CLAIM_NOW`.
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// A run's exit code, standard output and standard error.
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A bare remote and the repositories `a` and `b` whose `origin` it is.
fn machines(temp: &Path) -> (PathBuf, PathBuf) {
    let remote = temp.join("remote.git");
    git(temp, &["init", "--quiet", "--bare", remote.to_str().unwrap()]);
    let clone = |name: &str| {
        let dir = temp.join(name);
        git(temp, &["init", "--quiet", dir.to_str().unwrap()]);
        git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
        dir
    };
    (clone("a"), clone("b"))
}

/// Runs `fab claim <args>` in `dir` at the time `now`.
fn fab(dir: &Path, now: u64, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .arg("claim")
        .args(args)
        .current_dir(dir)
        .env("FAB_CLAIM_NOW", now.to_string())
        .output()
        .unwrap();
    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

#[test]
fn claims_are_taken_renewed_released_and_expire_across_machines() {
    let temp = tempfile::tempdir().unwrap();
    let (a, b) = machines(temp.path());
    let take = |dir: &Path, now: u64, run: &str, machine: &str| {
        fab(
            dir,
            now,
            &["take", "issue-12", "--run", run, "--machine", machine, "--ttl", "10"],
        )
    };

    let run = take(&a, 0, "run-a", "ma");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        run.stdout,
        "claimed issue-12 for run run-a on ma until 1970-01-01T00:10:00Z\n"
    );

    let run = take(&b, 60, "run-b", "mb");
    assert_eq!(run.code, Some(1));
    assert_eq!(
        run.stderr,
        "fab: run run-a on ma holds issue-12 until 1970-01-01T00:10:00Z\n"
    );

    // A worker the run dispatched finds the claim its run holds and goes on.
    let run = take(&b, 120, "run-a", "mb");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(
        run.stdout
            .starts_with("already held; renewed issue-12 for run run-a on mb"),
        "{}",
        run.stdout
    );

    let run = fab(&a, 180, &["renew", "issue-12", "--run", "run-b", "--machine", "ma"]);
    assert_eq!(
        (run.code, run.stderr.as_str()),
        (Some(1), "fab: run run-b does not hold issue-12\n")
    );
    let run = fab(
        &a,
        180,
        &["renew", "issue-12", "--run", "run-a", "--machine", "ma", "--ttl", "1"],
    );
    assert_eq!(
        run.stdout,
        "renewed issue-12 for run run-a on ma until 1970-01-01T00:04:00Z\n"
    );

    let run = fab(&b, 300, &["list", "--json"]);
    let listed: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(listed[0]["run"], "run-a");
    assert_eq!(listed[0]["expired"], true);
    assert_eq!(
        fab(&b, 300, &["list"]).stdout,
        "issue-12\trun run-a on ma\texpired 1970-01-01T00:04:00Z\n"
    );

    let run = take(&b, 300, "run-b", "mb");
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(
        run.stdout.ends_with("; run run-a's claim had expired\n"),
        "{}",
        run.stdout
    );

    let run = fab(&a, 360, &["release", "issue-12", "--run", "run-a", "--machine", "ma"]);
    assert_eq!(run.code, Some(1));
    let run = fab(&a, 360, &["release", "issue-12", "--run", "run-b", "--machine", "ma"]);
    assert_eq!(
        (run.code, run.stdout.as_str()),
        (Some(0), "released issue-12 for run run-b\n")
    );
    assert_eq!(fab(&a, 360, &["list", "--json"]).stdout, "[]\n");
}

#[test]
fn an_unusable_task_id_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let (a, _) = machines(temp.path());
    let run = fab(&a, 0, &["take", "a\nb", "--run", "r", "--machine", "m"]);
    assert_eq!(run.code, Some(2));
    assert!(run.stderr.contains("control character"), "{}", run.stderr);
}
