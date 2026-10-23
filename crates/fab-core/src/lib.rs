//! The core of fabrial. It installs fabrial into a repository ([`install`])
//! and reads a repository's settings ([`settings`]). The run operations every
//! driver shares (claim a task, start a step, record its result, mark the
//! pull request ready, and release) are added here in phase 5 of `PLAN.md`.

pub mod install;
pub mod settings;

use std::path::PathBuf;

/// The release of this build, from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The commit this build was made from, or `unknown` outside a git checkout.
pub const COMMIT: &str = env!("FAB_COMMIT");
/// The version recorded in an installed repository's settings: the release
/// and the commit, as `0.1.0+abc1234`.
pub const FULL_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("FAB_COMMIT"));

/// What can go wrong installing or checking fabrial.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Reading or writing a file failed.
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: std::io::Error },
    /// The settings file is missing a value, or holds one the workflows
    /// cannot read.
    #[error(".github/fabrial.toml: {0}")]
    Settings(String),
    /// A first install would replace files fabrial did not write.
    #[error("these files exist and differ from fabrial's; pass --force to replace them: {}", list(.0))]
    Conflict(Vec<PathBuf>),
    /// The platform cannot do something fabrial needs.
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
}

/// [`FULL_VERSION`] as an owned string.
pub fn version() -> String {
    FULL_VERSION.to_owned()
}

fn list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_workspace_version_and_commit() {
        assert_eq!(VERSION, "0.1.0");
        assert!(version().starts_with("0.1.0+"));
    }
}
