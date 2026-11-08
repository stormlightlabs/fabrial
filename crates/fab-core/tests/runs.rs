//! Run events read from outside fab-core, from records laid out as
//! `fab dispatch` writes them.
#![cfg(unix)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::ops::ControlFlow;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use fab_core::runs::{self, Cursor, Error, Item, Skipped, Worker};
use serde_json::json;

const A: &str = "11111111-1111-4111-8111-111111111111";
const B: &str = "22222222-2222-4222-8222-222222222222";

/// A private `state_dir` and the host configuration that names it.
struct Registry {
    _temp: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
}

impl Registry {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("state");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let config = temp.path().join("host.toml");
        fs::write(&config, format!("state_dir = {:?}\n", root.to_str().unwrap())).unwrap();
        let root = root.canonicalize().unwrap();
        Self { _temp: temp, root, config }
    }

    /// Writes a worker record as dispatch does, created at `created_at`.
    fn worker(&self, id: &str, role: &str, created_at: u64) -> PathBuf {
        let directory = self.root.join("records").join(id);
        fs::create_dir_all(&directory).unwrap();
        let record = json!({
            "version": 1, "run_id": format!("run-{id}"), "worker_id": id, "task_head": "a".repeat(40),
            "role": role, "repository": "org/repo", "worktree": "/w", "created_at": created_at,
            "updated_at": created_at, "state": "running", "credential_cleanup": "pending",
            "result_file": null, "error": null,
            "resources": {"private_state": "/p", "tmux_session": "fab-x", "supervisor_pid": null, "worker_pid": null},
        });
        fs::write(directory.join("worker.json"), record.to_string()).unwrap();
        directory
    }
}

/// Appends raw text to a worker's event file, as the recorder appends lines.
fn append(directory: &Path, text: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("events.jsonl"))
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

fn line(n: u32) -> String {
    json!({"type": "progress", "n": n}).to_string() + "\n"
}

/// Polls every worker from `cursor`, collecting events and skipped workers.
fn poll(root: &Path, cursor: &mut Cursor) -> Result<(Vec<runs::Event>, Vec<Skipped>), Error> {
    let mut events = Vec::new();
    let mut skipped = Vec::new();
    let flow = runs::poll(root, cursor, |item| {
        match item {
            Item::Event(event) => events.push(event),
            Item::Skipped(worker) => skipped.push(worker),
        }
        ControlFlow::Continue(())
    })?;
    assert!(flow.is_continue());
    Ok((events, skipped))
}

/// Polls every worker from `cursor`, expecting none to be skipped.
fn events(root: &Path, cursor: &mut Cursor) -> Result<Vec<runs::Event>, Error> {
    let (events, skipped) = poll(root, cursor)?;
    assert!(skipped.is_empty(), "{skipped:?}");
    Ok(events)
}

/// The cursor a reader holds after handling `events`: the latest pair per
/// worker.
fn fold(events: &[runs::Event]) -> Cursor {
    let mut cursor = Cursor::default();
    for event in events {
        let pair: Cursor = event.cursor.parse().unwrap();
        cursor.advance(&event.worker_id, pair.position(&event.worker_id));
    }
    cursor
}

fn numbers(events: &[runs::Event]) -> Vec<(String, u64)> {
    events
        .iter()
        .map(|e| (e.worker_id.clone(), e.event["n"].as_u64().unwrap()))
        .collect()
}

fn expect(pairs: &[(&str, u64)]) -> Vec<(String, u64)> {
    pairs.iter().map(|(w, n)| ((*w).to_owned(), *n)).collect()
}

#[test]
fn registry_reads_state_dir_from_the_host_configuration() {
    let registry = Registry::new();
    assert_eq!(runs::registry(&registry.config).unwrap(), registry.root);
    fs::set_permissions(&registry.root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(runs::registry(&registry.config), Err(Error::Config(_))));
    fs::write(&registry.config, "state_dir = \"relative\"\n").unwrap();
    assert!(matches!(runs::registry(&registry.config), Err(Error::Config(_))));
}

#[test]
fn workers_are_listed_oldest_first_and_opened_by_canonical_id() {
    let registry = Registry::new();
    registry.worker(B, "reviewer", 1);
    registry.worker(A, "implementer", 2);
    fs::create_dir_all(
        registry
            .root
            .join("records")
            .join("33333333-3333-4333-8333-333333333333"),
    )
    .unwrap();
    let ids: Vec<_> = Worker::list(&registry.root)
        .unwrap()
        .into_iter()
        .map(|w| w.worker_id)
        .collect();
    assert_eq!(ids, [B, A]);
    assert_eq!(Worker::open(&registry.root, A).unwrap().role, "implementer");
    assert!(Worker::open(&registry.root, "../../escape").is_err());
}

#[test]
fn a_reader_restarted_from_any_cursor_sees_each_event_once() {
    let registry = Registry::new();
    let a = registry.worker(A, "implementer", 1);
    let b = registry.worker(B, "reviewer", 2);
    append(&a, &(line(1) + &line(2)));
    append(&b, &line(3));
    let mut cursor = Cursor::default();
    let first = events(&registry.root, &mut cursor).unwrap();
    assert_eq!(numbers(&first), expect(&[(A, 1), (A, 2), (B, 3)]));
    assert!(first.iter().all(|e| e.format == runs::FORMAT));
    assert_eq!(first[2].run_id, format!("run-{B}"));
    assert_eq!(first[2].cursor, format!("{B}:{}", first[2].sequence));
    assert_eq!(fold(&first), cursor);

    // More events arrive, one of them still half written, while the reader
    // is down. Every cursor it may have saved resumes with what follows.
    append(&a, &(line(4) + "{\"type\":"));
    append(&b, &line(5));
    for seen in 0..first.len() {
        let mut resumed = fold(&first[..=seen]);
        let rest = events(&registry.root, &mut resumed).unwrap();
        let mut all = numbers(&first[..=seen]);
        all.extend(numbers(&rest));
        all.sort_by_key(|(_, n)| *n);
        assert_eq!(
            all,
            expect(&[(A, 1), (A, 2), (B, 3), (A, 4), (B, 5)]),
            "resumed after event {seen}"
        );
    }

    // The half-written line appears once it is complete, and only once.
    let mut later: Cursor = cursor.to_string().parse().unwrap();
    assert_eq!(
        numbers(&events(&registry.root, &mut later).unwrap()),
        expect(&[(A, 4), (B, 5)])
    );
    append(&a, "\"progress\",\"n\":6}\n");
    assert_eq!(numbers(&events(&registry.root, &mut later).unwrap()), expect(&[(A, 6)]));
    assert!(events(&registry.root, &mut later).unwrap().is_empty());
}

#[test]
fn a_cursor_off_a_line_boundary_or_past_the_end_is_refused() {
    let registry = Registry::new();
    let a = registry.worker(A, "implementer", 1);
    append(&a, &line(1));
    for position in [1, 9999] {
        let mut cursor: Cursor = format!("{A}:{position}").parse().unwrap();
        assert!(matches!(poll(&registry.root, &mut cursor), Err(Error::Cursor(_))));
    }
    let batch = runs::read_events(&a, 0, runs::BATCH).unwrap();
    assert_eq!(batch.cursor, line(1).len() as u64);
    assert_eq!(batch.events[0].sequence, batch.cursor);
}

#[test]
fn an_unreadable_worker_is_skipped_and_the_others_go_on() {
    let registry = Registry::new();
    let a = registry.worker(A, "implementer", 1);
    let b = registry.worker(B, "reviewer", 2);
    let c = registry.worker("33333333-3333-4333-8333-333333333333", "reviewer", 3);
    fs::write(c.join("worker.json"), "{").unwrap();
    append(&a, &(line(1) + "not json\n" + &line(2)));
    append(&b, &line(3));
    let mut cursor = Cursor::default();
    let (events, skipped) = poll(&registry.root, &mut cursor).unwrap();
    assert_eq!(numbers(&events), expect(&[(A, 1), (B, 3)]));
    let skipped: Vec<_> = skipped
        .iter()
        .map(|s| (s.worker_id.as_str(), s.error.to_string()))
        .collect();
    assert_eq!(
        skipped,
        [
            (
                "33333333-3333-4333-8333-333333333333",
                "invalid worker record".to_owned()
            ),
            (A, "invalid worker event".to_owned()),
        ]
    );
    // A stays before its bad line, and the next poll skips it again.
    assert_eq!(cursor.position(A), line(1).len() as u64);
    let (events, skipped) = poll(&registry.root, &mut cursor).unwrap();
    assert!(events.is_empty());
    assert_eq!(skipped.len(), 2);
    assert!(Worker::list(&registry.root).is_err());
}

#[test]
fn a_reader_that_breaks_stops_the_poll_after_that_event() {
    let registry = Registry::new();
    let a = registry.worker(A, "implementer", 1);
    append(&a, &(line(1) + &line(2)));
    let mut cursor = Cursor::default();
    let mut seen = Vec::new();
    let flow = runs::poll(&registry.root, &mut cursor, |item| {
        if let Item::Event(event) = item {
            seen.push(event);
        }
        ControlFlow::Break(())
    })
    .unwrap();
    assert!(flow.is_break());
    assert_eq!(numbers(&seen), expect(&[(A, 1)]));
    assert_eq!(cursor, fold(&seen));
}
