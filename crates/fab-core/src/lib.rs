//! The core of fabrial. The run operations every driver shares (claim a
//! task, start a step, record its result, mark the pull request ready, and
//! release) are added here in phase 5 of `PLAN.md`.

/// The version of this build, from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_workspace_version() {
        assert_eq!(VERSION, "0.1.0");
    }
}
