//! Queue admission is serialized across independent submitters.

use std::sync::{Arc, Barrier};
use std::thread;

use super::*;

#[test]
fn concurrent_submitters_cannot_exceed_the_outstanding_limit() {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let worker = WorkerRecord::create(
        temp.path(),
        "reviewer",
        "org/repo",
        temp.path(),
        "a".repeat(40),
        temp.path(),
    )
    .unwrap();
    let id = worker.snapshot().unwrap()["worker_id"].as_str().unwrap().to_owned();
    let directory = worker.directory().join("controls");
    fs::create_dir(&directory).unwrap();
    for _ in 0..63 {
        fs::write(directory.join(format!("{}.sent", uuid::Uuid::new_v4())), b"").unwrap();
    }
    let config = temp.path().join("host.toml");
    fs::write(&config, format!("state_dir = {:?}\n", temp.path().to_str().unwrap())).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let config = config.clone();
            let id = id.clone();
            thread::spawn(move || {
                barrier.wait();
                operate(Operation::Steer(Steer {
                    reference: Reference { worker_id: id, config },
                    head: "a".repeat(40),
                    message: "instruction".into(),
                }))
            })
        })
        .collect();
    barrier.wait();
    let successes = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|r| r.is_ok())
        .count();
    assert_eq!(successes, 1);
    assert_eq!(fs::read_dir(directory).unwrap().count(), 64);
}
