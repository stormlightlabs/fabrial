//! Credential-free worker records retained independently of temporary issuer state.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Result, now, read_json};

/// The supervisor's last observed worker state, not a claim that its PID is alive.
#[derive(Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum WorkerState {
    Prepared,
    Starting,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Blocked,
}

/// Credential cleanup is separate from the task worktree and retained evidence.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CredentialCleanup {
    Pending,
    Complete,
    Unconfirmed,
}

/// One durable, versioned record. Only the host supervisor updates it after launch.
#[derive(Deserialize, Serialize)]
pub(super) struct WorkerRecord {
    #[serde(skip)]
    path: PathBuf,
    version: u32,
    run_id: String,
    worker_id: String,
    task_head: String,
    role: String,
    repository: String,
    worktree: PathBuf,
    created_at: u64,
    #[serde(default)]
    boot_id: Option<String>,
    updated_at: u64,
    state: WorkerState,
    credential_cleanup: CredentialCleanup,
    result_file: Option<PathBuf>,
    #[serde(default)]
    last_progress: Option<serde_json::Value>,
    #[serde(default)]
    report: Option<serde_json::Value>,
    error: Option<String>,
    resources: Resources,
}

impl WorkerRecord {
    /// Register resource ownership before the recorder can issue credentials.
    pub(super) fn create(
        root: &Path, role: &str, repository: &str, worktree: &Path, task_head: String, private_state: &Path,
    ) -> Result<Self> {
        let worker_id = uuid::Uuid::new_v4().to_string();
        let records = root.join("records");
        let directory = records.join(&worker_id);
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(|_| "cannot create worker record directory")?;
        let mut record = Self {
            path: directory.join("worker.json"),
            version: 1,
            run_id: uuid::Uuid::new_v4().to_string(),
            resources: Resources {
                private_state: private_state.into(),
                tmux_session: format!("fab-{worker_id}"),
                launcher_pid: Some(std::process::id()),
                supervisor_pid: None,
                worker_pid: None,
            },
            worker_id,
            task_head,
            role: role.into(),
            repository: repository.into(),
            worktree: worktree.into(),
            created_at: now(),
            boot_id: boot_identity(),
            updated_at: now(),
            state: WorkerState::Prepared,
            credential_cleanup: CredentialCleanup::Pending,
            result_file: None,
            last_progress: None,
            report: None,
            error: None,
        };
        record.save()?;
        // The record's directory is synced by save; persist its ancestors too.
        // Otherwise a first dispatch can lose the entire registry after power loss.
        sync_directory(&records)?;
        sync_directory(root)?;
        Ok(record)
    }

    /// Load a record without treating its contents as executable configuration.
    pub(super) fn open(path: &Path) -> Result<Self> {
        let mut record: Self = read_json(path)?;
        if record.version != 1 {
            return Err("unsupported worker record version".into());
        }
        record.path = path.into();
        Ok(record)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn session(&self) -> &str {
        &self.resources.tmux_session
    }

    /// Claim a prepared record once; duplicate recorder starts must not issue tokens.
    pub(super) fn start(&mut self, private: &Path) -> Result<()> {
        if self.state != WorkerState::Prepared || self.resources.private_state != private {
            return Err("worker record is not prepared for this recorder".into());
        }
        let mut claim = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.with_file_name("supervisor.claim"))
            .map_err(|_| "worker supervisor already claimed or cannot be recorded")?;
        write!(claim, "{}", std::process::id()).map_err(|_| "cannot record supervisor claim")?;
        claim.sync_all().map_err(|_| "cannot sync supervisor claim")?;
        self.resources.supervisor_pid = Some(std::process::id());
        self.state = WorkerState::Starting;
        self.save()
    }

    pub(super) fn worker_started(&mut self, pid: u32) -> Result<()> {
        self.resources.worker_pid = Some(pid);
        self.state = WorkerState::Running;
        self.save()
    }

    /// Retain the latest credential-free progress for detached inspection.
    pub(super) fn progress(&mut self, event: &serde_json::Value) -> Result<()> {
        self.last_progress = Some(event.clone());
        self.save()
    }

    /// Persist only the validated and redacted final role text.
    pub(super) fn result(&mut self, text: &str) -> Result<()> {
        let path = self.path.with_file_name("result.txt");
        atomic_write(&path, text.as_bytes())?;
        self.result_file = Some(path);
        self.save()
    }

    pub(super) fn report(&mut self, report: &serde_json::Value) -> Result<()> {
        self.report = Some(report.clone());
        self.save()
    }

    pub(super) fn finish(
        &mut self, result: &Result<()>, cleaned: bool, cancelled: bool, timed_out: bool,
    ) -> Result<()> {
        self.credential_cleanup = if cleaned { CredentialCleanup::Complete } else { CredentialCleanup::Unconfirmed };
        self.state = if !cleaned {
            WorkerState::Failed
        } else if result.is_ok() {
            WorkerState::Succeeded
        } else if self
            .report
            .as_ref()
            .is_some_and(|r| r["kind"] == "blocked" || r["kind"] == "question")
        {
            WorkerState::Blocked
        } else if cancelled {
            WorkerState::Cancelled
        } else if timed_out {
            WorkerState::TimedOut
        } else {
            WorkerState::Failed
        };
        self.error = if cleaned {
            result.as_ref().err().cloned()
        } else {
            Some("credential cleanup unconfirmed; private state retained".into())
        };
        self.save()
    }

    pub(super) fn directory(&self) -> &Path {
        self.path.parent().expect("record path has a parent")
    }

    pub(super) fn terminal(&self) -> bool {
        matches!(
            self.state,
            WorkerState::Succeeded
                | WorkerState::Failed
                | WorkerState::Cancelled
                | WorkerState::TimedOut
                | WorkerState::Blocked
        )
    }

    pub(super) fn exit_code(&self) -> u8 {
        u8::from(self.state != WorkerState::Succeeded)
    }

    pub(super) fn cancel(&self) -> Result<()> {
        if self.terminal() {
            return Err("worker has already completed".into());
        }
        atomic_write(&self.directory().join("cancel"), b"")
    }

    pub(super) fn summary(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self).map_err(|_| "cannot serialize worker record".into())
    }

    /// A stopped supervisor never resumes or re-runs a stage during recovery.
    pub(super) fn recovered(&mut self) -> Result<()> {
        if !self.terminal() {
            self.state = WorkerState::Failed;
            self.error = Some("supervisor exited before recording an outcome; cleanup recovered".into());
        } else if matches!(self.credential_cleanup, CredentialCleanup::Unconfirmed) {
            self.error = Some("cleanup recovered; previous worker outcome retained".into());
        }
        self.credential_cleanup = CredentialCleanup::Complete;
        self.save()
    }

    pub(super) fn snapshot(&self) -> Result<serde_json::Value> {
        let mut value = self.summary()?;
        if let Some(path) = &self.result_file {
            value["result"] = serde_json::json!(super::read(path)?);
        }
        Ok(value)
    }

    fn save(&mut self) -> Result<()> {
        self.updated_at = now();
        let bytes = serde_json::to_vec(self).map_err(|_| "cannot serialize worker record")?;
        atomic_write(&self.path, &bytes)
    }
}

/// Resources owned by this worker. The worktree and shared Nix root are not owned.
#[derive(Deserialize, Serialize)]
struct Resources {
    private_state: PathBuf,
    tmux_session: String,
    #[serde(default)]
    launcher_pid: Option<u32>,
    supervisor_pid: Option<u32>,
    worker_pid: Option<u32>,
}

/// Serialize submission and claiming so capacity and order share one boundary.
pub(super) fn control_lock(directory: &Path) -> Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("control-submission.lock"))
        .map_err(|_| "cannot open control submission lock")?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(_) => Err("cannot lock control submissions".into()),
    }
}

/// Identify a boot without trusting recorded PIDs across a host restart.
pub(super) fn boot_identity() -> Option<String> {
    let bytes = if cfg!(target_os = "macos") {
        super::tool("sysctl", &["-n", "kern.boottime"], None).ok()?
    } else {
        std::fs::read("/proc/sys/kernel/random/boot_id").ok()?
    };
    String::from_utf8(bytes)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Discard control contents, never retained evidence or another worker's files.
pub(super) fn discard_controls(directory: &Path) -> Result<()> {
    let controls = directory.join("controls");
    if controls.exists() {
        std::fs::remove_dir_all(controls).map_err(|_| "cannot remove worker controls")?;
        sync_directory(directory)?;
    }
    Ok(())
}

/// Replace a complete file atomically; readers never observe a partial record.
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("worker record has no directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| "cannot create worker record file")?;
    file.write_all(bytes).map_err(|_| "cannot write worker record")?;
    file.as_file().sync_all().map_err(|_| "cannot sync worker record")?;
    file.persist(path).map_err(|_| "cannot replace worker record")?;
    sync_directory(parent)
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "cannot sync worker record directory".into())
}

#[cfg(test)]
mod tests;
