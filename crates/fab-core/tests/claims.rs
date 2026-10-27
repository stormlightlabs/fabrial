//! Claims against a bare repository, with two clones as two machines.
use std::path::Path;
use std::process::Command;

use fab_core::claims::{Action, Claim, Error, Outcome, Request, Store};

/// A bare remote and two repositories, `a` and `b`, whose `origin` it is.
struct Machines {
    _temp: tempfile::TempDir,
    a: Store,
    b: Store,
}

impl Machines {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        git(temp.path(), &["init", "--quiet", "--bare", remote.to_str().unwrap()]);
        let store = |name: &str| {
            let dir = temp.path().join(name);
            git(temp.path(), &["init", "--quiet", dir.to_str().unwrap()]);
            git(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
            Store::new(dir, "origin")
        };
        let (a, b) = (store("a"), store("b"));
        Self { _temp: temp, a, b }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git").arg("-C").arg(dir).args(args).status().unwrap();
    assert!(status.success(), "git {args:?}");
}

fn take(task: &str, machine: &str, run: &str, now: u64) -> Request {
    Request { task: task.into(), machine: machine.into(), run: run.into(), action: Action::Take { ttl: 60 }, now }
}

fn holder(error: Error) -> Claim {
    match error {
        Error::Held(claim) => claim,
        other => panic!("expected a held claim, got {other}"),
    }
}

#[test]
fn a_claim_is_taken_renewed_released_and_expires() {
    let m = Machines::new();
    assert!(m.a.list().unwrap().is_empty());
    let claimed = m.a.write(&take("issue-12", "ma", "run-a", 1000)).unwrap();
    assert!(matches!(claimed, Outcome::Claimed(Claim { expires_at: 1060, .. })));
    // The other machine reads it, and its run is refused.
    assert_eq!(m.b.list().unwrap().len(), 1);
    let held = holder(m.b.write(&take("issue-12", "mb", "run-b", 1010)).unwrap_err());
    assert_eq!((held.run.as_str(), held.machine.as_str()), ("run-a", "ma"));
    // The holding run takes it again from anywhere and goes on.
    let again = m.b.write(&take("issue-12", "mb", "run-a", 1020)).unwrap();
    assert!(matches!(
        again,
        Outcome::AlreadyHeld(Claim { claimed_at: 1000, expires_at: 1080, .. })
    ));
    let renew = Request { action: Action::Renew { ttl: 100 }, ..take("issue-12", "ma", "run-a", 1050) };
    assert!(matches!(
        m.a.write(&renew).unwrap(),
        Outcome::Renewed(Claim { expires_at: 1150, .. })
    ));
    // Only the holder renews or releases.
    let release_b = Request { action: Action::Release, ..take("issue-12", "mb", "run-b", 1060) };
    assert!(matches!(m.b.write(&release_b), Err(Error::NotHeld { .. })));
    // After expiry another run takes it over.
    let over = m.b.write(&take("issue-12", "mb", "run-b", 1150)).unwrap();
    let Outcome::TookOver { claim, expired } = over else { panic!("{over:?}") };
    assert_eq!((claim.run.as_str(), expired.run.as_str()), ("run-b", "run-a"));
    let renew_a = Request { action: Action::Renew { ttl: 10 }, ..take("issue-12", "ma", "run-a", 1160) };
    assert!(matches!(m.a.write(&renew_a), Err(Error::NotHeld { .. })));
    assert!(matches!(m.b.write(&release_b), Ok(Outcome::Released(_))));
    assert!(m.a.list().unwrap().is_empty());
}

#[test]
fn a_write_prunes_expired_claims_on_other_tasks() {
    let m = Machines::new();
    m.a.write(&take("old", "ma", "run-a", 1000)).unwrap();
    m.a.write(&take("live", "ma", "run-a", 1050)).unwrap();
    m.b.write(&take("new", "mb", "run-b", 1070)).unwrap();
    let tasks: Vec<String> = m.a.list().unwrap().into_iter().map(|c| c.task).collect();
    assert_eq!(tasks, ["live", "new"]);
}

/// Both machines build their claim on the same tip, which here is no ref at
/// all. A pushes first; B's push is rejected and its retry sees A's claim.
#[test]
fn two_machines_racing_to_create_the_ref_leave_one_winner() {
    let m = Machines::new();
    let b = m.b.prepare(&take("t", "mb", "run-b", 1000)).unwrap();
    let a = m.a.prepare(&take("t", "ma", "run-a", 1000)).unwrap();
    assert!(m.a.publish(&a).unwrap());
    assert!(!m.b.publish(&b).unwrap(), "a push onto a ref that moved is rejected");
    let held = holder(m.b.write(&take("t", "mb", "run-b", 1001)).unwrap_err());
    assert_eq!(held.run, "run-a");
    let claims = m.b.list().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].run, "run-a");
}

/// The same race on a ref that already exists, and one on different tasks,
/// where the loser's retry keeps the winner's claim beside its own.
#[test]
fn two_machines_racing_on_an_existing_ref_lose_no_claim() {
    let m = Machines::new();
    m.a.write(&take("other", "ma", "run-x", 1000)).unwrap();
    let b = m.b.prepare(&take("t", "mb", "run-b", 1000)).unwrap();
    assert!(
        m.a.publish(&m.a.prepare(&take("t", "ma", "run-a", 1000)).unwrap())
            .unwrap()
    );
    assert!(!m.b.publish(&b).unwrap());
    assert_eq!(
        holder(m.b.write(&take("t", "mb", "run-b", 1000)).unwrap_err()).run,
        "run-a"
    );

    let b = m.b.prepare(&take("u", "mb", "run-b", 1000)).unwrap();
    assert!(
        m.a.publish(&m.a.prepare(&take("v", "ma", "run-a", 1000)).unwrap())
            .unwrap()
    );
    assert!(!m.b.publish(&b).unwrap());
    assert!(matches!(
        m.b.write(&take("u", "mb", "run-b", 1000)).unwrap(),
        Outcome::Claimed(_)
    ));
    let tasks: Vec<String> = m.a.list().unwrap().into_iter().map(|c| c.task).collect();
    assert_eq!(tasks, ["other", "t", "u", "v"]);
}
