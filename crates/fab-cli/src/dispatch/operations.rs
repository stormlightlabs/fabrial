//! Host operations on persistent worker identities, independent of an open tool call.

use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use fab_core::runs;
use serde_json::json;

use super::state::{WorkerRecord, atomic_write};
use super::{Result, emit, read, signals};

/// Explicit local worker operations. None starts a replacement worker.
#[derive(Subcommand)]
pub enum Operation {
    /// List durable worker metadata, without result bodies or credentials.
    List {
        #[arg(long)]
        config: PathBuf,
    },
    /// Recover cleanup after the supervisor has stopped; never restart a stage.
    Recover(Reference),
    /// Read the last recorded state and any final result.
    Inspect(Reference),
    /// Request cancellation of one unfinished worker.
    Cancel(Reference),
    /// Explicitly wait for one worker; ending this wait does not cancel it.
    Wait(Wait),
    /// Read a bounded batch of events after a byte cursor.
    Events(Events),
    /// Send driver-authorized steering to this worker at its starting head.
    Steer(Steer),
}

impl Operation {
    /// Run this operation and emit credential-free JSON.
    pub fn run(self) -> u8 {
        match operate(self) {
            Ok(code) => code,
            Err(error) => {
                emit(&json!({"type": "error", "message": error}));
                1
            }
        }
    }
}

/// Worker identity and operator-selected configuration.
#[derive(Args)]
pub struct Reference {
    worker_id: String,
    #[arg(long)]
    config: PathBuf,
}

/// Replay cursor for one immutable worker identity.
#[derive(Args)]
pub struct Events {
    #[command(flatten)]
    reference: Reference,
    #[arg(long, default_value_t = 0)]
    after: u64,
}

/// Steering is an explicit host action, never authority granted by worker output.
#[derive(Args)]
pub struct Steer {
    #[command(flatten)]
    reference: Reference,
    #[arg(long)]
    head: String,
    #[arg(long)]
    message: String,
}

/// A bounded, optional observer of an independently supervised worker.
#[derive(Args)]
pub struct Wait {
    #[command(flatten)]
    reference: Reference,
    #[arg(long, default_value_t = 3000, value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout: u64,
}

/// Read only the registry setting: inspection does not require live OAuth or a sandbox build.
pub(super) fn registry(config: &Path) -> Result<PathBuf> {
    runs::registry(config).map_err(|e| e.to_string())
}

/// The metadata of every readable worker record under the host
/// configuration's `state_dir`, as `fab worker list` prints it. A record
/// directory without `worker.json` is a dispatch still creating it and is
/// skipped; a record that cannot be read is skipped with a line on stderr.
/// `fab worker list` fails on either instead, since it serves recovery.
pub(crate) fn workers(config: &Path) -> Result<Vec<serde_json::Value>> {
    let directory = registry(config)?.join("records");
    let mut workers = Vec::new();
    if !directory.exists() {
        return Ok(workers);
    }
    for entry in fs::read_dir(&directory).map_err(|_| "cannot list workers")? {
        let entry = entry.map_err(|_| "cannot read worker entry")?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path().join("worker.json");
        if uuid::Uuid::parse_str(&name).is_err() || !path.exists() {
            continue;
        }
        match WorkerRecord::open(&path).and_then(|record| record.summary()) {
            Ok(summary) => workers.push(summary),
            Err(e) => eprintln!("fab: skipping worker record {name}: {e}"),
        }
    }
    Ok(workers)
}

pub(super) fn lookup(root: &Path, id: &str) -> Result<WorkerRecord> {
    if uuid::Uuid::parse_str(id)
        .map_err(|_| "invalid worker UUID")?
        .to_string()
        != id
    {
        return Err("worker ID must be a canonical UUID".into());
    }
    WorkerRecord::open(&root.join("records").join(id).join("worker.json"))
}

pub(super) fn events(worker: &WorkerRecord, after: u64) -> Result<serde_json::Value> {
    let batch = runs::read_events(worker.directory(), after, runs::BATCH).map_err(|e| e.to_string())?;
    Ok(json!({"worker": worker.snapshot()?, "events": batch.events, "cursor": batch.cursor}))
}

fn operate(operation: Operation) -> Result<u8> {
    let (config, id) = match &operation {
        Operation::List { config } => (config, None),
        Operation::Inspect(r) | Operation::Cancel(r) | Operation::Recover(r) => (&r.config, Some(r.worker_id.as_str())),
        Operation::Wait(w) => (&w.reference.config, Some(w.reference.worker_id.as_str())),
        Operation::Events(e) => (&e.reference.config, Some(e.reference.worker_id.as_str())),
        Operation::Steer(s) => (&s.reference.config, Some(s.reference.worker_id.as_str())),
    };
    let root = registry(config)?;
    let Some(id) = id else {
        emit(&super::recovery::list(&root)?);
        return Ok(0);
    };
    let mut worker = lookup(&root, id)?;
    match operation {
        Operation::List { .. } => unreachable!("list handled before lookup"),
        Operation::Recover(_) => {
            super::recovery::recover(&root, &mut worker)?;
            emit(&worker.snapshot()?);
            Ok(0)
        }
        Operation::Inspect(_) => {
            emit(&worker.snapshot()?);
            Ok(0)
        }
        Operation::Cancel(_) => {
            worker.cancel()?;
            emit(&json!({"type": "cancel_requested", "worker": worker.snapshot()?}));
            Ok(0)
        }
        Operation::Events(options) => {
            let mut batch = events(&worker, options.after)?;
            batch["supervisor_alive"] = json!(super::pane(worker.session())?);
            emit(&batch);
            Ok(0)
        }
        Operation::Steer(options) => {
            let _submission =
                super::state::control_lock(worker.directory())?.ok_or("control submission busy; retry")?;
            let worker = WorkerRecord::open(worker.path())?;
            let identity = worker.snapshot()?;
            if worker.terminal() || identity["task_head"] != options.head {
                return Err("stale head or completed worker".into());
            }
            if options.message.is_empty() || options.message.len() > 16384 {
                return Err("steering must contain 1–16384 bytes".into());
            }
            let directory = worker.directory().join("controls");
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&directory)
                .map_err(|_| "cannot create controls")?;
            if fs::read_dir(&directory)
                .map_err(|_| "cannot inspect controls")?
                .take(65)
                .count()
                >= 64
            {
                return Err("control queue full".into());
            }
            let counter = worker.directory().join("control-sequence");
            let sequence = if counter.exists() {
                read(&counter)?.parse::<u64>().map_err(|_| "invalid control sequence")?
            } else {
                0
            };
            let sequence = sequence.checked_add(1).ok_or("control sequence exhausted")?;
            atomic_write(&counter, sequence.to_string().as_bytes())?;
            let id = uuid::Uuid::new_v4().to_string();
            let path = directory.join(format!("{id}.json"));
            atomic_write(
                &path,
                json!({"id": id, "sequence": sequence, "worker_id": identity["worker_id"], "run_id": identity["run_id"],
                "task_head": options.head, "message": options.message})
                .to_string()
                .as_bytes(),
            )?;
            if WorkerRecord::open(worker.path())?.terminal() {
                let _ = fs::remove_file(path);
                return Err("worker completed before steering was submitted".into());
            }
            emit(&json!({"type": "control_submitted", "id": id, "sequence": sequence}));
            Ok(0)
        }
        Operation::Wait(options) => {
            let interrupted = signals()?;
            let deadline = Instant::now() + Duration::from_secs(options.timeout);
            loop {
                let worker = lookup(&root, &options.reference.worker_id)?;
                if worker.terminal() {
                    emit(&worker.snapshot()?);
                    return Ok(worker.exit_code());
                }
                if interrupted.load(Ordering::Relaxed) || Instant::now() >= deadline {
                    return Err("wait ended; worker remains independently supervised".into());
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

#[cfg(test)]
mod tests;
