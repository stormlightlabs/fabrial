//! RPC delivery and replay boundaries without model or credential calls.

use super::*;
use crate::dispatch::{operations, quote, state::WorkerRecord};

#[test]
fn event_cursor_replays_complete_records_only() {
    let temp = tempfile::tempdir().unwrap();
    let worker = WorkerRecord::create(
        temp.path(),
        "reviewer",
        "org/repo",
        temp.path(),
        "a".repeat(40),
        temp.path(),
    )
    .unwrap();
    let first = json!({"type": "worker_report", "text": "one\u{2028}two"}).to_string() + "\n";
    fs::write(worker.directory().join("events.jsonl"), first.clone() + "{\"partial\"").unwrap();
    let batch = operations::events(&worker, 0).unwrap();
    assert_eq!(batch["events"].as_array().unwrap().len(), 1);
    assert_eq!(batch["cursor"], first.len());
    assert_eq!(batch["events"][0]["event"]["text"], "one\u{2028}two");
    assert!(
        operations::events(&worker, first.len() as u64).unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(operations::events(&worker, 1).is_err());
    assert!(operations::events(&worker, 99999).is_err());
}

#[test]
fn mailbox_is_claimed_once_and_checks_the_task_head() {
    let temp = tempfile::tempdir().unwrap();
    let worker = WorkerRecord::create(
        temp.path(),
        "reviewer",
        "org/repo",
        temp.path(),
        "a".repeat(40),
        temp.path(),
    )
    .unwrap();
    let identity = worker.snapshot().unwrap();
    let mailbox = worker.directory().join("controls");
    fs::create_dir(&mailbox).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut request = identity.clone();
    request["id"] = json!(id);
    request["message"] = json!("driver instruction");
    let path = mailbox.join(format!("{id}.json"));
    fs::write(&path, request.to_string()).unwrap();
    let batch = controls(worker.directory(), &identity).unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0]["type"], "steer");
    assert!(controls(worker.directory(), &identity).unwrap().is_empty());
    request["task_head"] = json!("b".repeat(40));
    fs::write(&path, request.to_string()).unwrap();
    assert!(controls(worker.directory(), &identity).is_err());
}

#[test]
fn rpc_ack_delivery_and_processing_are_distinct() {
    let id = uuid::Uuid::new_v4().to_string();
    let wire = [
        json!({"type":"response", "id":"initial", "success":true, "data":{"disposition":"started"}}),
        json!({"type":"response", "id":id, "success":true, "data":{"disposition":"queued"}}),
        json!({"type":"message_end", "message":{"role":"user", "content":format!("[fabrial-control:{id}] instruction")}}),
        json!({"type":"message_end", "message":{"role":"assistant", "stopReason":"stop", "content":[{"type":"text", "text":"done"}]}}),
        json!({"type":"agent_settled", "aborted":false}),
    ].map(|e| e.to_string()).join("\n");
    let ready =
        json!({"type":"response", "id":"ready", "success":true, "data":{"commands":[{"name":"fabrial-worker-ready"}]}})
            .to_string();
    let initial =
        json!({"type":"response", "id":"initial", "success":true, "data":{"disposition":"started"}}).to_string();
    let mut command = Command::new("bash");
    command.args([
        "-c",
        &format!(
            "read -r ready; printf '%s\\n' {}; read -r initial; printf '%s\\n' {}; read -r steer; printf '%s\\n' {}",
            quote(&ready),
            quote(&initial),
            quote(&wire)
        ),
    ]);
    let mut pending = Some(json!({"type":"steer", "id":id, "message":"instruction"}));
    let mut events = Vec::new();
    let result = collect(
        command,
        Duration::from_secs(5),
        || false,
        |event| {
            events.push(event);
            Ok(())
        },
        || Ok(pending.take().into_iter().collect()),
        "task",
    )
    .unwrap();
    assert_eq!(result, "done");
    for kind in ["control_ack", "control_delivered", "control_processed"] {
        assert!(events.iter().any(|e| e["type"] == kind && e["id"] == id));
    }
}
