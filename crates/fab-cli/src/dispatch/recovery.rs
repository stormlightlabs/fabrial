//! Conservative recovery of host-owned state; never signal a historical PID.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use nix::errno::Errno;
use nix::sys::signal::{kill, killpg};
use nix::unistd::Pid;
use serde_json::{Value, json};

use super::state::{WorkerRecord, boot_identity, discard_controls};
use super::{Request, Result, pane, read, read_json, revoke};

/// List metadata only: no credentials, result bodies, or live model dependencies.
pub(super) fn list(root: &Path) -> Result<Value> {
    let directory = root.join("records");
    let mut workers = Vec::new();
    if directory.exists() {
        for entry in fs::read_dir(directory).map_err(|_| "cannot list workers")? {
            let entry = entry.map_err(|_| "cannot read worker entry")?;
            if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok() {
                workers.push(WorkerRecord::open(&entry.path().join("worker.json"))?.summary()?);
            }
        }
    }
    Ok(json!({"workers": workers}))
}

/// Prove quiescence before revocation or deletion. Unknown ownership retains private state.
pub(super) fn recover(root: &Path, worker: &mut WorkerRecord) -> Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(worker.directory().join("recovery.lock"))
        .map_err(|_| "cannot open recovery lock")?;
    lock.try_lock().map_err(|_| "worker recovery already in progress")?;
    let identity = worker.summary()?;
    let private = Path::new(
        identity["resources"]["private_state"]
            .as_str()
            .ok_or("private state missing")?,
    );
    if private.parent() != Some(root)
        || !private
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("dispatch-"))
    {
        return Err("private state is outside this worker registry".into());
    }
    let private_exists = match fs::symlink_metadata(private) {
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => return Err("private state must be an owned directory, not a symlink".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err("cannot inspect private state".into()),
    };
    if !private_exists && worker.terminal() && identity["credential_cleanup"] == "complete" {
        discard_controls(worker.directory())?;
        super::state::sync_directory(root)?;
        return worker.recovered();
    }
    let rebooted =
        boot_identity().is_some_and(|current| identity["boot_id"].as_str().is_some_and(|old| old != current));
    if pane(worker.session())? {
        return Err("supervisor is active; request cancellation instead".into());
    }
    if !rebooted && alive(identity["resources"]["launcher_pid"].as_u64(), false)? {
        return Err("worker launcher is still active".into());
    }
    // Stop a prepared recorder from starting after the quiescence check.
    let claim = worker.directory().join("supervisor.claim");
    match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&claim) {
        Ok(mut file) => {
            write!(file, "{}", std::process::id()).map_err(|_| "cannot claim recovery")?;
            file.sync_all().map_err(|_| "cannot persist recovery claim")?;
            super::state::sync_directory(worker.directory())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !rebooted {
                let pid = read(&claim)?
                    .parse::<u64>()
                    .map_err(|_| "supervisor claim incomplete; retry recovery")?;
                if alive(Some(pid), false)? {
                    return Err("supervisor claim is still active".into());
                }
            }
        }
        Err(_) => return Err("cannot claim recovery".into()),
    }
    if !rebooted
        && (alive(identity["resources"]["supervisor_pid"].as_u64(), false)?
            || alive(identity["resources"]["worker_pid"].as_u64(), true)?)
    {
        return Err("worker processes may still be alive; private recovery state retained".into());
    }
    if private_exists {
        let owned = worker.directory().join("cleanup-owned");
        if private.join("request.json").exists() {
            let request: Request = read_json(&private.join("request.json"))?;
            if request.record != worker.path() {
                return Err("private state belongs to another worker".into());
            }
            super::state::atomic_write(&owned, private.as_os_str().as_encoded_bytes())?;
        } else if read(&owned)? != private.to_str().ok_or("invalid private path")? {
            return Err("private cleanup ownership unconfirmed".into());
        }
        if private.join("installation-token").exists() {
            revoke(private, &read(&private.join("installation-token"))?)?;
        }
        fs::remove_dir_all(private).map_err(|_| "private cleanup unconfirmed; recovery state retained")?;
        super::state::sync_directory(root)?;
    }
    discard_controls(worker.directory())?;
    worker.recovered()
}

fn alive(pid: Option<u64>, group: bool) -> Result<bool> {
    let Some(pid) = pid else {
        return Ok(false);
    };
    let pid = i32::try_from(pid)
        .ok()
        .filter(|p| *p > 0)
        .ok_or("invalid recorded PID")?;
    let result = if group { killpg(Pid::from_raw(pid), None) } else { kill(Pid::from_raw(pid), None) };
    match result {
        Ok(()) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(_) => Err("cannot confirm recorded process is stopped".into()),
    }
}
