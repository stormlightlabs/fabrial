//! Run events, read from the records `fab dispatch` keeps under the host
//! configuration's `state_dir`. A host that serves runs over the network
//! calls these to relay them; fabrial runs no server.
//!
//! Each worker has a directory `records/<worker-id>/` holding `worker.json`,
//! its identity and last state, and `events.jsonl`, which the worker's
//! recorder appends one JSON object per line to and never rewrites. A
//! position in that file is a byte offset at a line boundary. A [`Cursor`]
//! holds one position per worker, so a reader that keeps the cursor of the
//! last event it handled resumes after it with no repeat and no gap.
//!
//! Pull request status is not a run event: hosts read it from GitHub.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The version of the [`Event`] line format. It changes when a field is
/// removed or changes meaning; new fields keep it.
pub const FORMAT: u32 = 1;
/// The worker record version this module reads.
pub const RECORD_VERSION: u32 = 1;
/// The longest event line read, in bytes: twice the 4 MiB limit on a line
/// the worker writes, since the recorder adds to it.
pub const EVENT_LIMIT: usize = 8 * 1024 * 1024;
/// How many events [`read_events`] returns at most for `fab worker events`.
pub const BATCH: usize = 64;

/// What can go wrong reading run events.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The host configuration is unreadable, or its `state_dir` is missing,
    /// relative or readable by others.
    #[error("{0}")]
    Config(&'static str),
    /// A worker record or event file is unreadable or invalid.
    #[error("{0}")]
    Record(&'static str),
    /// A cursor is malformed, past the end of a stream or not at a line
    /// boundary.
    #[error("{0}")]
    Cursor(&'static str),
}

/// A worker's identity, read from its `worker.json`. Only the host
/// supervisor writes the record; this view reads it.
#[derive(Debug, Clone, Deserialize)]
pub struct Worker {
    /// The worker's UUID, also the name of its record directory.
    pub worker_id: String,
    /// The UUID of the run that dispatched it.
    pub run_id: String,
    /// The role it runs, such as `reviewer`.
    pub role: String,
    /// The repository, as `owner/name`.
    pub repository: String,
    /// The commit the worker started from.
    pub task_head: String,
    /// When the record was created, in seconds since the Unix epoch.
    pub created_at: u64,
    version: u32,
    #[serde(skip)]
    directory: PathBuf,
}

impl Worker {
    /// Reads the record of worker `id` under the registry `root`. `id` must
    /// be a canonical lowercase UUID, so it cannot name another path.
    pub fn open(root: &Path, id: &str) -> Result<Self, Error> {
        if !canonical_uuid(id) {
            return Err(Error::Record("worker ID must be a canonical UUID"));
        }
        Self::load(&root.join("records").join(id))
    }

    /// Every worker under `root` whose record exists, oldest first. A
    /// directory whose record is not written yet is left out.
    pub fn list(root: &Path) -> Result<Vec<Self>, Error> {
        let directory = root.join("records");
        let mut workers = Vec::new();
        if !directory.exists() {
            return Ok(workers);
        }
        for entry in fs::read_dir(directory).map_err(|_| Error::Record("cannot list workers"))? {
            let entry = entry.map_err(|_| Error::Record("cannot read worker entry"))?;
            if !canonical_uuid(&entry.file_name().to_string_lossy()) || !entry.path().join("worker.json").exists() {
                continue;
            }
            workers.push(Self::load(&entry.path())?);
        }
        workers.sort_by(|a, b| (a.created_at, &a.worker_id).cmp(&(b.created_at, &b.worker_id)));
        Ok(workers)
    }

    /// The worker's record directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn load(directory: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(directory.join("worker.json"))
            .map_err(|_| Error::Record("cannot read worker record"))?;
        let mut worker: Self = serde_json::from_str(&text).map_err(|_| Error::Record("invalid worker record"))?;
        if worker.version != RECORD_VERSION {
            return Err(Error::Record("unsupported worker record version"));
        }
        worker.directory = directory.into();
        Ok(worker)
    }
}

/// Complete events read from one worker's `events.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Batch {
    /// The events, in the order the recorder wrote them.
    pub events: Vec<Recorded>,
    /// The offset after the last event read: the position to read from next.
    pub cursor: u64,
}

/// One event with its position in the worker's stream.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recorded {
    /// The byte offset just after this event's line.
    pub sequence: u64,
    /// The event as the recorder wrote it.
    pub event: Value,
}

/// A position in every worker's stream, as worker ID to byte offset. A
/// worker missing from it is at 0. It is written as `<worker-id>:<offset>`
/// pairs joined by commas, leaving out workers at 0, so the empty string is
/// the start of every stream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor(BTreeMap<String, u64>);

impl Cursor {
    /// The offset reached in `worker_id`'s stream.
    pub fn position(&self, worker_id: &str) -> u64 {
        self.0.get(worker_id).copied().unwrap_or(0)
    }

    /// Moves `worker_id`'s position to `offset`.
    pub fn advance(&mut self, worker_id: &str, offset: u64) {
        self.0.insert(worker_id.to_owned(), offset);
    }
}

impl FromStr for Cursor {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let mut positions = BTreeMap::new();
        for pair in text.split(',').filter(|p| !p.is_empty()) {
            let (id, offset) = pair
                .split_once(':')
                .ok_or(Error::Cursor("cursor pairs are <worker-id>:<offset>"))?;
            if !canonical_uuid(id) {
                return Err(Error::Cursor("cursor names a worker ID that is not a canonical UUID"));
            }
            let offset = offset
                .parse::<u64>()
                .map_err(|_| Error::Cursor("cursor offset is not a number"))?;
            if positions.insert(id.to_owned(), offset).is_some() {
                return Err(Error::Cursor("cursor names a worker twice"));
            }
        }
        Ok(Self(positions))
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (id, offset) in self.0.iter().filter(|(_, o)| **o > 0) {
            if !first {
                f.write_str(",")?;
            }
            write!(f, "{id}:{offset}")?;
            first = false;
        }
        Ok(())
    }
}

/// One line of `fab events`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    /// The line format, [`FORMAT`].
    pub format: u32,
    /// The cursor just after this event. Pass it to `--after`, or to
    /// [`poll`], to resume after this event.
    pub cursor: String,
    /// The worker that wrote the event.
    pub worker_id: String,
    /// The run that dispatched the worker.
    pub run_id: String,
    /// The worker's role.
    pub role: String,
    /// The byte offset just after this event in the worker's stream.
    pub sequence: u64,
    /// The event as the worker's recorder wrote it.
    pub event: Value,
}

/// The registry root: the absolute, private `state_dir` the host
/// configuration at `config` names. It reads no other setting, so it needs
/// no login or sandbox build.
pub fn registry(config: &Path) -> Result<PathBuf, Error> {
    let text = fs::read_to_string(config).map_err(|_| Error::Config("cannot read host configuration"))?;
    let config: toml::Value = toml::from_str(&text).map_err(|_| Error::Config("invalid host configuration"))?;
    let root = config
        .get("state_dir")
        .and_then(toml::Value::as_str)
        .ok_or(Error::Config("state_dir missing"))?;
    if !Path::new(root).is_absolute() {
        return Err(Error::Config("state_dir must be absolute"));
    }
    let root = Path::new(root)
        .canonicalize()
        .map_err(|_| Error::Config("missing local path"))?;
    let mode = fs::metadata(&root)
        .map_err(|_| Error::Config("cannot inspect registry"))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(Error::Config("state_dir must be private (mode 0700)"));
    }
    Ok(root)
}

/// Reads up to `limit` complete events from the `events.jsonl` in a worker's
/// record `directory`, starting at byte offset `after`. A line still being
/// written is left for the next read. `after` must be 0 or the offset just
/// after a line.
pub fn read_events(directory: &Path, after: u64, limit: usize) -> Result<Batch, Error> {
    let path = directory.join("events.jsonl");
    let mut batch = Batch { events: Vec::new(), cursor: after };
    if !path.exists() {
        if after != 0 {
            return Err(Error::Cursor("event stream missing for cursor"));
        }
        return Ok(batch);
    }
    let mut file = File::open(path).map_err(|_| Error::Record("cannot open worker events"))?;
    let length = file
        .metadata()
        .map_err(|_| Error::Record("cannot inspect events"))?
        .len();
    if after > length {
        return Err(Error::Cursor("event cursor exceeds stream length"));
    }
    if after > 0 {
        file.seek(SeekFrom::Start(after - 1))
            .map_err(|_| Error::Record("cannot seek event cursor"))?;
        let mut last = [0];
        file.read_exact(&mut last)
            .map_err(|_| Error::Record("cannot validate event cursor"))?;
        if last[0] != b'\n' {
            return Err(Error::Cursor("event cursor is not a record boundary"));
        }
    }
    file.seek(SeekFrom::Start(after))
        .map_err(|_| Error::Record("cannot seek events"))?;
    let mut reader = BufReader::new(file);
    for _ in 0..limit {
        let mut line = Vec::new();
        reader
            .by_ref()
            .take((EVENT_LIMIT + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|_| Error::Record("cannot read worker events"))?;
        if line.len() > EVENT_LIMIT {
            return Err(Error::Record("worker event exceeds limit"));
        }
        if !line.ends_with(b"\n") {
            break;
        }
        let event = serde_json::from_slice(&line).map_err(|_| Error::Record("invalid worker event"))?;
        batch.cursor += line.len() as u64;
        batch.events.push(Recorded { sequence: batch.cursor, event });
    }
    Ok(batch)
}

/// Reads every complete event after `cursor` from every worker under the
/// registry `root`, oldest worker first, and moves `cursor` past them. Each
/// event carries the cursor just after it, so a reader that stops partway
/// resumes from the last event it handled.
pub fn poll(root: &Path, cursor: &mut Cursor) -> Result<Vec<Event>, Error> {
    let mut events = Vec::new();
    for worker in Worker::list(root)? {
        loop {
            let batch = read_events(worker.directory(), cursor.position(&worker.worker_id), BATCH)?;
            if batch.events.is_empty() {
                break;
            }
            for recorded in batch.events {
                cursor.advance(&worker.worker_id, recorded.sequence);
                events.push(Event {
                    format: FORMAT,
                    cursor: cursor.to_string(),
                    worker_id: worker.worker_id.clone(),
                    run_id: worker.run_id.clone(),
                    role: worker.role.clone(),
                    sequence: recorded.sequence,
                    event: recorded.event,
                });
            }
        }
    }
    Ok(events)
}

/// Whether `id` is a UUID in canonical form: 36 lowercase hex digits and
/// dashes, grouped 8-4-4-4-12.
fn canonical_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_and_reject_malformed_text() {
        let id = "0b0c6a52-8f43-4f0e-9a51-2a1d3c4e5f60";
        let cursor: Cursor = format!("{id}:42").parse().unwrap();
        assert_eq!(cursor.position(id), 42);
        assert_eq!(cursor.to_string(), format!("{id}:42"));
        assert_eq!("".parse::<Cursor>().unwrap(), Cursor::default());
        for bad in [
            "42",
            "../x:1",
            &format!("{id}:x"),
            &format!("{id}:1,{id}:2"),
            &format!("{}:1", id.to_uppercase()),
        ] {
            assert!(bad.parse::<Cursor>().is_err(), "{bad}");
        }
    }

    #[test]
    fn uuids_must_be_canonical() {
        assert!(canonical_uuid("0b0c6a52-8f43-4f0e-9a51-2a1d3c4e5f60"));
        assert!(!canonical_uuid("0B0C6A52-8F43-4F0E-9A51-2A1D3C4E5F60"));
        assert!(!canonical_uuid("0b0c6a528f434f0e9a512a1d3c4e5f60"));
        assert!(!canonical_uuid("../../escape"));
    }
}
