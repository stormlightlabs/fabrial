//! Claims against a bare repository, with two repositories as two machines.
use std::path::{Path, PathBuf};
use std::process::Command;

use fab_core::claims::{Action, Claim, Error, Outcome, Request, Store};

/// A bare remote with one commit on main, and two repositories, `a` and `b`,
/// whose `origin` it is.
struct Machines {
    _temp: tempfile::TempDir,
    remote: PathBuf,
    a: Store,
    b: Store,
    a_dir: PathBuf,
}

impl Machines {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        git(
            temp.path(),
            &["init", "-q", "--bare", "-b", "main", remote.to_str().unwrap()],
        );
        let clone = |name: &str| {
            let dir = temp.path().join(name);
            git(temp.path(), &["init", "-q", "-b", "main", dir.to_str().unwrap()]);
            git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
            dir
        };
        let (a_dir, b_dir) = (clone("a"), clone("b"));
        git(&a_dir, &["commit", "-q", "--allow-empty", "-m", "Start"]);
        git(&a_dir, &["push", "-q", "origin", "main"]);
        Self { remote, a: Store::new(&a_dir, "origin"), b: Store::new(b_dir, "origin"), a_dir, _temp: temp }
    }

    /// The remote's commit subjects on `branch`, newest first, or `None`
    /// when the branch does not exist.
    fn subjects(&self, branch: &str) -> Option<Vec<String>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.remote)
            .args(["log", "--format=%s", &format!("refs/heads/{branch}")])
            .output()
            .unwrap();
        output.status.success().then(|| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(String::from)
                .collect()
        })
    }
}

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

fn request(machine: &str, run: &str, action: Action, now: u64) -> Request {
    Request { task: "issue-12".into(), machine: machine.into(), run: run.into(), action, now }
}

fn take(machine: &str, run: &str, now: u64) -> Request {
    request(machine, run, Action::Take { ttl: 60 }, now)
}

#[test]
fn two_machines_racing_for_one_task_leave_one_claim() {
    let m = Machines::new();
    // Both read the task as free before either pushes.
    let first = m.a.prepare(&take("ma", "run-a", 1000)).unwrap();
    let second = m.b.prepare(&take("mb", "run-b", 1000)).unwrap();
    assert!(m.a.publish(&first).unwrap());
    assert!(
        !m.b.publish(&second).unwrap(),
        "the second create-only push is rejected"
    );
    let Error::Held(holder) = m.b.write(&take("mb", "run-b", 1001)).unwrap_err() else {
        panic!("the loser reads the winner's claim");
    };
    assert_eq!(
        (holder.run.as_str(), holder.machine.as_str(), holder.expires_at),
        ("run-a", "ma", 1060)
    );
    assert_eq!(
        m.subjects("storm/issue-12").unwrap()[0],
        "Claim issue-12 for run run-a until 1970-01-01T00:17:40Z"
    );
}

#[test]
fn the_holding_run_goes_on_renews_and_releases() {
    let m = Machines::new();
    assert!(matches!(
        m.a.write(&take("ma", "run-a", 1000)).unwrap(),
        Outcome::Claimed(_)
    ));
    // A worker the run dispatched, on another machine, finds its run's claim.
    assert!(matches!(
        m.b.write(&take("mb", "run-a", 1010)).unwrap(),
        Outcome::AlreadyHeld(Claim { expires_at: 1060, .. })
    ));
    assert!(matches!(
        m.b.write(&request("mb", "run-b", Action::Renew { ttl: 60 }, 1010)),
        Err(Error::NotHeld { .. })
    ));
    assert!(matches!(
        m.b.write(&request("mb", "run-a", Action::Renew { ttl: 60 }, 1020))
            .unwrap(),
        Outcome::Renewed(Claim { expires_at: 1080, .. })
    ));
    assert_eq!(
        m.subjects("storm/issue-12").unwrap().len(),
        3,
        "the renewal is a fast-forward"
    );
    assert!(matches!(
        m.a.write(&request("ma", "run-a", Action::Release, 1030)).unwrap(),
        Outcome::Released(_)
    ));
    assert_eq!(m.subjects("storm/issue-12"), None);
}

#[test]
fn an_expired_claim_is_taken_without_losing_its_work_and_work_is_never_released() {
    let m = Machines::new();
    m.a.write(&take("ma", "run-a", 1000)).unwrap();
    git(
        &m.a_dir,
        &["checkout", "-q", "-b", "storm/issue-12", "origin/storm/issue-12"],
    );
    git(&m.a_dir, &["commit", "-q", "--allow-empty", "-m", "Work"]);
    git(&m.a_dir, &["push", "-q", "origin", "storm/issue-12"]);
    assert!(matches!(
        m.a.write(&request("ma", "run-a", Action::Release, 1010)),
        Err(Error::HoldsWork(_))
    ));
    assert!(matches!(m.b.write(&take("mb", "run-b", 1059)), Err(Error::Held(_))));
    let Outcome::TookOver { claim, expired } = m.b.write(&take("mb", "run-b", 1060)).unwrap() else {
        panic!("an expired claim is taken over");
    };
    assert_eq!((claim.run.as_str(), expired.run.as_str()), ("run-b", "run-a"));
    let subjects = m.subjects("storm/issue-12").unwrap();
    assert!(subjects[0].starts_with("Claim issue-12 for run run-b"));
    assert_eq!(
        &subjects[1..3],
        ["Work", "Claim issue-12 for run run-a until 1970-01-01T00:17:40Z"]
    );
}

#[test]
fn a_branch_without_a_claim_is_not_taken() {
    let m = Machines::new();
    git(&m.a_dir, &["commit", "-q", "--allow-empty", "-m", "Work"]);
    git(&m.a_dir, &["push", "-q", "origin", "HEAD:refs/heads/storm/issue-12"]);
    assert!(matches!(m.b.write(&take("mb", "run-b", 1000)), Err(Error::NoClaim(b)) if b == "storm/issue-12"));
}
