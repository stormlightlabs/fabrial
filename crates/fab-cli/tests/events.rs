//! `fab events` against records laid out as `fab dispatch` writes them.
#![cfg(unix)]

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const WORKER: &str = "11111111-1111-4111-8111-111111111111";

fn append(directory: &Path, n: u32) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("events.jsonl"))
        .unwrap();
    writeln!(file, "{}", json!({"type": "progress", "n": n})).unwrap();
}

/// Starts `fab events --follow`, reads `count` lines, then stops it.
fn follow(config: &Path, after: &str, count: usize) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args([
            "events",
            "--follow",
            "--config",
            config.to_str().unwrap(),
            "--after",
            after,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let events = (0..count)
        .map(|_| serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap())
        .collect();
    child.kill().unwrap();
    child.wait().unwrap();
    events
}

#[test]
fn a_restarted_follow_resumes_after_the_last_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let directory = root.join("records").join(WORKER);
    fs::create_dir_all(&directory).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let config = temp.path().join("host.toml");
    fs::write(&config, format!("state_dir = {:?}\n", root.to_str().unwrap())).unwrap();
    let record = json!({"version": 1, "worker_id": WORKER, "run_id": "r", "role": "reviewer",
        "repository": "org/repo", "task_head": "a".repeat(40), "created_at": 1});
    fs::write(directory.join("worker.json"), record.to_string()).unwrap();
    for n in 1..=3 {
        append(&directory, n);
    }

    let first = follow(&config, "", 2);
    assert_eq!(first[0]["format"], 1);
    assert_eq!(first[0]["worker_id"], WORKER);
    assert_eq!(first[0]["role"], "reviewer");
    assert_eq!(first[1]["event"]["n"], 2);

    append(&directory, 4);
    let cursor = first[1]["cursor"].as_str().unwrap();
    let rest = follow(&config, cursor, 2);
    let numbers: Vec<_> = rest.iter().map(|e| e["event"]["n"].as_u64().unwrap()).collect();
    assert_eq!(numbers, [3, 4]);

    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(["events", "--config", config.to_str().unwrap()])
        .args(["--after", rest[1]["cursor"].as_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "no event after the last cursor");

    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(["events", "--config", config.to_str().unwrap(), "--after", "bad"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn an_unreadable_worker_is_named_on_stderr_and_the_others_print() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let good = root.join("records").join(WORKER);
    let bad = root.join("records").join("22222222-2222-4222-8222-222222222222");
    fs::create_dir_all(&good).unwrap();
    fs::create_dir_all(&bad).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let config = temp.path().join("host.toml");
    fs::write(&config, format!("state_dir = {:?}\n", root.to_str().unwrap())).unwrap();
    let record = json!({"version": 1, "worker_id": WORKER, "run_id": "r", "role": "reviewer",
        "repository": "org/repo", "task_head": "a".repeat(40), "created_at": 1});
    fs::write(good.join("worker.json"), record.to_string()).unwrap();
    fs::write(bad.join("worker.json"), "{").unwrap();
    append(&good, 1);

    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .args(["events", "--config", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let line: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(line["event"]["n"], 1);
    assert_eq!(line["cursor"], format!("{WORKER}:{}", line["sequence"]));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("skipping worker 22222222-2222-4222-8222-222222222222: invalid worker record"),
        "{stderr}"
    );
}
