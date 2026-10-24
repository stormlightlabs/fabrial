//! Host operations on persistent worker identities, independent of an open tool call.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use serde_json::json;

use super::state::WorkerRecord;
use super::{Result, canonical, emit, read, signals};

/// Explicit local worker operations. None starts a replacement worker.
#[derive(Subcommand)]
pub enum Operation {
    /// Read the last recorded state and any final result.
    Inspect(Reference),
    /// Request cancellation of one unfinished worker.
    Cancel(Reference),
    /// Explicitly wait for one worker; ending this wait does not cancel it.
    Wait(Wait),
}

/// Worker identity and operator-selected configuration.
#[derive(Args)]
pub struct Reference {
    worker_id: String,
    #[arg(long)]
    config: PathBuf,
}

/// A bounded, optional observer of an independently supervised worker.
#[derive(Args)]
pub struct Wait {
    #[command(flatten)]
    reference: Reference,
    #[arg(long, default_value_t = 3000, value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout: u64,
}

/// Run an operation and emit credential-free JSON.
pub fn run(operation: Operation) -> u8 {
    match operate(operation) {
        Ok(code) => code,
        Err(error) => {
            emit(&json!({"type": "error", "message": error}));
            1
        }
    }
}

/// Read only the registry setting: inspection does not require live OAuth or a sandbox build.
pub(super) fn registry(config: &Path) -> Result<PathBuf> {
    let config: toml::Value = toml::from_str(&read(config)?).map_err(|_| "invalid host configuration")?;
    let root = config
        .get("state_dir")
        .and_then(toml::Value::as_str)
        .ok_or("state_dir missing")?;
    if !Path::new(root).is_absolute() {
        return Err("state_dir must be absolute".into());
    }
    let root = canonical(Path::new(root))?;
    if fs::metadata(&root)
        .map_err(|_| "cannot inspect registry")?
        .permissions()
        .mode()
        & 0o077
        != 0
    {
        return Err("state_dir must be private (mode 0700)".into());
    }
    Ok(root)
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

fn operate(operation: Operation) -> Result<u8> {
    let reference = match &operation {
        Operation::Inspect(r) | Operation::Cancel(r) => r,
        Operation::Wait(w) => &w.reference,
    };
    let root = registry(&reference.config)?;
    let worker = lookup(&root, &reference.worker_id)?;
    match operation {
        Operation::Inspect(_) => {
            emit(&worker.snapshot()?);
            Ok(0)
        }
        Operation::Cancel(_) => {
            worker.cancel()?;
            emit(&json!({"type": "cancel_requested", "worker": worker.snapshot()?}));
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
