//! Durable record registration and single-supervisor ownership.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::*;

#[test]
fn registration_is_private_and_survives_temporary_state_removal() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("private");
    fs::create_dir(&private).unwrap();
    let record = WorkerRecord::create(
        temp.path(),
        "reviewer",
        "org/repo",
        temp.path(),
        "a".repeat(40),
        &private,
    )
    .unwrap();
    let stored: serde_json::Value = read_json(record.path()).unwrap();
    for field in ["run_id", "worker_id"] {
        uuid::Uuid::parse_str(stored[field].as_str().unwrap()).unwrap();
    }
    assert_ne!(stored["run_id"], stored["worker_id"]);
    assert_eq!(stored["state"], "prepared");
    assert_eq!(stored["resources"]["private_state"], private.to_str().unwrap());
    assert!(stored["resources"]["supervisor_pid"].is_null());
    assert_eq!(fs::metadata(record.path()).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(record.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    fs::remove_dir(&private).unwrap();
    assert!(WorkerRecord::open(record.path()).is_ok());
}

#[test]
fn only_one_recorder_can_claim_a_prepared_worker() {
    let temp = tempfile::tempdir().unwrap();
    let mut first = WorkerRecord::create(
        temp.path(),
        "reviewer",
        "org/repo",
        temp.path(),
        "a".repeat(40),
        temp.path(),
    )
    .unwrap();
    let mut second = WorkerRecord::open(first.path()).unwrap();
    first.start(temp.path()).unwrap();
    assert!(second.start(temp.path()).is_err());
    assert!(WorkerRecord::open(first.path()).unwrap().start(temp.path()).is_err());
    let stored: serde_json::Value = read_json(first.path()).unwrap();
    assert_eq!(stored["state"], "starting");
    assert_eq!(stored["resources"]["supervisor_pid"], std::process::id());
}
