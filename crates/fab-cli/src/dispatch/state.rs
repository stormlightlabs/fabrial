//! Credential-free worker records retained independently of temporary issuer state.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
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
    updated_at: u64,
    state: WorkerState,
    credential_cleanup: CredentialCleanup,
    result_file: Option<PathBuf>,
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
                supervisor_pid: None,
                worker_pid: None,
            },
            worker_id,
            task_head,
            role: role.into(),
            repository: repository.into(),
            worktree: worktree.into(),
            created_at: now(),
            updated_at: now(),
            state: WorkerState::Prepared,
            credential_cleanup: CredentialCleanup::Pending,
            result_file: None,
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

    /// Persist only the validated and redacted final role text.
    pub(super) fn result(&mut self, text: &str) -> Result<()> {
        let path = self.path.with_file_name("result.txt");
        atomic_write(&path, text.as_bytes())?;
        self.result_file = Some(path);
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
    supervisor_pid: Option<u32>,
    worker_pid: Option<u32>,
}

/// Replace a complete file atomically; readers never observe a partial record.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("worker record has no directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| "cannot create worker record file")?;
    file.write_all(bytes).map_err(|_| "cannot write worker record")?;
    file.as_file().sync_all().map_err(|_| "cannot sync worker record")?;
    file.persist(path).map_err(|_| "cannot replace worker record")?;
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "cannot sync worker record directory".into())
}

#[cfg(test)]
mod tests;
