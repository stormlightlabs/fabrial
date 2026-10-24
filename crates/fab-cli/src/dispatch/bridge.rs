//! Pi RPC over anonymous pipes. No worker access to host queues or sockets.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{Process, Result, final_message, read_json, records};

/// Supervise one fresh RPC session until settlement, cancellation, or deadline.
pub(super) fn collect(
    mut command: Command, timeout: Duration, cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(Value) -> Result<()>, mut controls: impl FnMut() -> Result<Vec<Value>>, task: &str,
) -> Result<String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = Process(command.spawn().map_err(|_| "worker could not start")?);
    progress(json!({"type": "worker_started", "pid": child.0.id()}))?;
    let mut stdin = child.0.stdin.take().ok_or("worker stdin missing")?;
    let (input, requests) = mpsc::sync_channel::<Value>(16);
    thread::spawn(move || {
        for request in requests {
            if writeln!(stdin, "{request}").and_then(|_| stdin.flush()).is_err() {
                break;
            }
        }
    });
    input
        .send(json!({"id": "ready", "type": "get_commands"}))
        .map_err(|_| "worker input closed")?;
    let (tx, rx) = mpsc::sync_channel(16);
    let stdout = child.0.stdout.take().ok_or("worker stdout missing")?;
    thread::spawn(move || records(stdout, tx));
    let deadline = Instant::now() + timeout;
    let mut message = Value::Null;
    let mut accepted = false;
    let mut settled = false;
    let mut submitted = BTreeSet::new();
    let mut delivered = BTreeSet::new();
    loop {
        if cancelled() {
            return Err("worker cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("worker timed out".into());
        }
        // One batch is bounded by the host mailbox reader. Never block the stdout reader.
        for control in if settled || !accepted { Vec::new() } else { controls()? } {
            let id = control["id"].as_str().ok_or("control ID missing")?.to_string();
            match input.try_send(control) {
                Ok(()) => {
                    submitted.insert(id);
                }
                Err(_) => return Err("worker control backpressure or closed input".into()),
            }
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                let event = event?;
                match event["type"].as_str().unwrap_or("") {
                    "response" if event["id"] == "ready" => {
                        let ready = event["data"]["commands"]
                            .as_array()
                            .is_some_and(|commands| commands.iter().any(|c| c["name"] == "fabrial-worker-ready"));
                        if event["success"] != true || !ready {
                            return Err("trusted worker extension unavailable; no model prompt sent".into());
                        }
                        input
                            .try_send(json!({"id": "initial", "type": "prompt", "message": task}))
                            .map_err(|_| "cannot send initial worker prompt")?;
                    }
                    "extension_error" => return Err("trusted worker extension failed".into()),
                    "response" if event["id"] == "initial" => {
                        if event["success"] != true || event["data"]["disposition"] == "handled" {
                            return Err("worker rejected or handled initial prompt without a run".into());
                        }
                        accepted = true;
                    }
                    "response" => {
                        if let Some(id) = event["id"].as_str().filter(|id| submitted.contains(*id)) {
                            progress(
                                json!({"type": "control_ack", "id": id, "accepted": event["success"] == true,
                                "disposition": event["data"]["disposition"]}),
                            )?;
                        }
                    }
                    "message_end" if event["message"]["role"] == "assistant" => {
                        message = event["message"].clone();
                        for id in std::mem::take(&mut delivered) {
                            progress(json!({"type": "control_processed", "id": id}))?;
                        }
                    }
                    "message_end" if event["message"]["role"] == "user" => {
                        let content = &event["message"]["content"];
                        let text = content.as_str().or_else(|| content[0]["text"].as_str()).unwrap_or("");
                        if let Some(id) = text
                            .strip_prefix("[fabrial-control:")
                            .and_then(|s| s.split_once(']').map(|p| p.0))
                            .filter(|id| submitted.contains(*id))
                        {
                            delivered.insert(id.to_string());
                            progress(json!({"type": "control_delivered", "id": id}))?;
                        }
                    }
                    "entry_appended" if event["entry"]["customType"] == "fabrial.worker-report" => {
                        let report = &event["entry"]["data"];
                        let kind = report["kind"].as_str().unwrap_or("");
                        let text = report["text"].as_str().unwrap_or("");
                        if !["progress", "blocked", "question"].contains(&kind) || text.is_empty() || text.len() > 16384
                        {
                            return Err("invalid worker report".into());
                        }
                        progress(json!({"type": "worker_report", "kind": kind, "text": text}))?;
                        if kind != "progress" {
                            return Err("worker reported a blocker or question; driver input required".into());
                        }
                    }
                    "turn_start" | "tool_execution_start" | "tool_execution_end" => {
                        progress(json!({"type": "progress", "event": event["type"]}))?;
                    }
                    "agent_settled" => {
                        if event["aborted"] == true {
                            return Err("worker aborted".into());
                        }
                        settled = true;
                    }
                    _ => (),
                }
                if accepted && settled {
                    // Stop the entire process group at settlement; never run work queued after it.
                    return final_message(&message);
                }
            }
            Err(RecvTimeoutError::Timeout) => (),
            Err(RecvTimeoutError::Disconnected) => return Err("worker exited without settlement".into()),
        }
    }
}

/// Claim at most eight host-authored messages per pass. A restart never replays claimed requests.
pub(super) fn controls(directory: &Path, identity: &Value) -> Result<Vec<Value>> {
    let Some(_submission) = super::state::control_lock(directory)? else {
        return Ok(Vec::new());
    };
    let mailbox = directory.join("controls");
    if !mailbox.exists() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(&mailbox)
        .map_err(|_| "cannot read controls")?
        .map(|e| e.map(|e| e.path()).map_err(|_| "cannot read control entry".to_string()))
        .collect::<Result<Vec<_>>>()?;
    let mut ordered = Vec::new();
    for path in paths.drain(..).filter(|p| p.extension().is_some_and(|s| s == "json")) {
        let request: Value = read_json(&path)?;
        let sequence = request["sequence"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("control sequence missing")?;
        ordered.push((sequence, path, request));
    }
    ordered.sort_by_key(|(sequence, _, _)| *sequence);
    if ordered.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("duplicate control sequence".into());
    }
    let mut requests = Vec::new();
    for (_, path, request) in ordered.into_iter().take(8) {
        if request["worker_id"] != identity["worker_id"]
            || request["run_id"] != identity["run_id"]
            || request["task_head"] != identity["task_head"]
        {
            return Err("stale control identity".into());
        }
        let id = request["id"].as_str().ok_or("control ID missing")?;
        uuid::Uuid::parse_str(id).map_err(|_| "invalid control UUID")?;
        let text = request["message"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 16384)
            .ok_or("control message missing or too large")?;
        fs::rename(&path, path.with_extension("sent")).map_err(|_| "cannot claim control")?;
        requests.push(json!({"id": id, "type": "steer", "message": format!("[fabrial-control:{id}] {text}")}));
    }
    Ok(requests)
}

#[cfg(test)]
mod tests;
