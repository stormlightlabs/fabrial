//! The core of fabrial. It installs fabrial into a repository ([`install`]),
//! reads a repository's settings ([`settings`]) and claims tasks for runs
//! ([`claims`]). The other run operations every driver shares (start a step,
//! record its result and mark the pull request ready) are still to come.

pub mod claims;
pub mod install;
pub mod settings;

use std::path::PathBuf;

/// The release of this build, from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The commit this build was made from, or `unknown` outside a git checkout.
pub const COMMIT: &str = env!("FAB_COMMIT");
/// The release and the commit this build was made from, as `0.2.0+abc1234`,
/// for `fab --version`. Installs record [`VERSION`] alone.
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
    /// `AGENTS.md` holds a fabrial begin marker with no end marker after it,
    /// so the section's extent is unknown.
    #[error("AGENTS.md: {0}")]
    Agents(String),
    /// `fab init` found fabrial already installed.
    #[error("fabrial is installed here; run fab update to refresh it")]
    Installed,
    /// `fab update` or `fab check` found no install.
    #[error("fabrial is not installed here; run fab init first")]
    NotInstalled,
    /// `fab update` found a recorded version it cannot order against this
    /// build's.
    #[error("the installed version {0} is not a release like 0.2.0; pass --force to replace it")]
    UnknownVersion(String),
    /// `fab update` would replace a newer release's files with this build's.
    #[error(
        "fabrial {installed} is installed, newer than this build's {this}; install a newer fab, or pass --force to downgrade"
    )]
    Downgrade { installed: String, this: String },
    /// `.agents/skills` is a directory, which `init` never deletes.
    #[error(".agents/skills is a directory, not a link to ../.claude/skills; remove it and run fab again")]
    SkillsDir,
    /// A managed path, or a directory above one, is a symlink, or is not the
    /// kind of entry fabrial writes there. `init` never follows or deletes
    /// it, with or without `--force`.
    #[error(
        "these managed paths are symlinks or not the file or directory fabrial writes, and fab does not follow or delete them; remove them and run fab again: {}",
        list(.0)
    )]
    Unmanageable(Vec<PathBuf>),
    /// The directory given is not a repository root: it has no `.git` entry.
    #[error("{}: no .git here; --dir must be the repository root", .0.display())]
    NotRepoRoot(PathBuf),
    /// The platform cannot do something fabrial needs.
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
}

/// The release part of a version (`0.2.0` in `0.2.0+abc1234`) as numbers, for
/// ordering releases. `None` when it is not three dot-separated numbers.
pub fn release(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split('+').next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let release = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(release)
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
    fn release_orders_versions_by_their_release_part() {
        assert_eq!(release("0.2.0+abc1234"), Some((0, 2, 0)));
        assert_eq!(release("1.10.3"), Some((1, 10, 3)));
        assert!(release("0.10.0") > release("0.9.9"));
        for bad in ["", "0.2", "0.2.0.1", "v0.2.0", "0.x.0", "0.2.0-rc.1"] {
            assert_eq!(release(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_workspace_version_is_a_release() {
        assert!(release(VERSION).is_some(), "{VERSION}");
    }
}
