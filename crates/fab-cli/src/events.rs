//! `fab events`: run events from dispatch's records, as JSON lines a host
//! can relay.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use clap::Args;
use fab_core::runs::{self, Cursor};

/// How often `--follow` reads the records again.
const INTERVAL: Duration = Duration::from_millis(250);

/// What `fab events` takes.
#[derive(Args)]
pub struct Options {
    /// The host configuration whose `state_dir` holds dispatch's records.
    #[arg(long)]
    config: PathBuf,
    /// Print events after this cursor, the `cursor` of the last event seen.
    #[arg(long, default_value = "")]
    after: String,
    /// Keep reading until interrupted, printing events as workers write them.
    #[arg(long)]
    follow: bool,
}

/// Prints each event after `--after` as one JSON object per line. Exits 0
/// when done or interrupted, or when the reader closes the pipe, and 1 with
/// the error on standard error when the records cannot be read.
pub fn run(options: Options) -> u8 {
    match stream(&options) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("fab: {error}");
            1
        }
    }
}

fn stream(options: &Options) -> Result<(), String> {
    let mut cursor: Cursor = options.after.parse().map_err(|e: runs::Error| e.to_string())?;
    let root = runs::registry(&options.config).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
    ] {
        signal_hook::flag::register(signal, stop.clone()).map_err(|_| "cannot install the interrupt handler")?;
    }
    loop {
        let events = runs::poll(&root, &mut cursor).map_err(|e| e.to_string())?;
        let mut out = std::io::stdout().lock();
        for event in events {
            let line = serde_json::to_string(&event).map_err(|_| "cannot serialize an event")?;
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                return Ok(());
            }
        }
        drop(out);
        if !options.follow || stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        thread::sleep(INTERVAL);
    }
}
