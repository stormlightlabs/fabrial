//! Fresh, isolated Pi workers. The trusted host launcher owns credentials and
//! tmux; the complete Pi process runs inside the pinned Nix sandbox.

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use std::process::Command;

/// Replaces this process with the shared host dispatcher, preserving signals.
/// Python 3.11+, tmux, BWS and the configured sandbox must exist on the host.
#[cfg(unix)]
pub fn exec(arguments: impl IntoIterator<Item = OsString>) -> std::io::Error {
    Command::new("python3")
        .arg("-c")
        .arg(include_str!("dispatch.py"))
        .args(arguments)
        .exec()
}
