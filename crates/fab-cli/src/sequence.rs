//! The review sequence: the stages the `storm` skill runs on a pull request,
//! read from `.github/fabrial-sequence.toml`, cut to the pull request's
//! review depth. The file is the one place the stages, their roles and what
//! each posts are written down; the skills point to `fab sequence` instead of
//! repeating them.

use std::path::Path;

use clap::Args;
use serde::Deserialize;

use crate::depth::{self, Depth};
use crate::run::Result;

/// Where the sequence lives, relative to the repository root.
pub const SEQUENCE_PATH: &str = ".github/fabrial-sequence.toml";

/// What `fab sequence` reads.
#[derive(Args)]
pub struct Options {
    /// The diff whose review depth cuts the sequence.
    #[command(flatten)]
    diff: depth::Options,
    /// Use this depth instead of the diff's.
    #[arg(long, value_enum)]
    depth: Option<Depth>,
}

/// The sequence file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sequence {
    stage: Vec<Stage>,
    finish: Finish,
}

/// One stage: a fresh agent in a role, run once.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage {
    name: String,
    /// The agent role, a file in `.claude/agents/`.
    role: String,
    /// The lowest depth that runs the stage.
    depth: Depth,
    /// The condition the stage needs to run; without one it always runs.
    when: Option<String>,
    /// What the stage leaves on the pull request.
    posts: String,
}

/// The last step, after every stage the depth gives.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Finish {
    posts: String,
}

/// Prints the depth and the stages it gives, numbered, then the finish.
pub fn run(options: Options) -> Result<()> {
    let root = Path::new(".");
    let text =
        std::fs::read_to_string(root.join(SEQUENCE_PATH)).map_err(|e| format!("cannot read {SEQUENCE_PATH}: {e}"))?;
    let sequence = parse(&text, |role| {
        root.join(".claude/agents").join(format!("{role}.md")).is_file()
    })?;
    let (depth, reason) = match options.depth {
        Some(depth) => (depth, "given with --depth".to_string()),
        None => depth::of_diff(&options.diff)?,
    };
    print!("{}", render(&sequence, depth, &reason));
    Ok(())
}

/// Reads a sequence and checks that every stage names a role that exists.
fn parse(text: &str, role_exists: impl Fn(&str) -> bool) -> Result<Sequence> {
    let sequence: Sequence = toml::from_str(text).map_err(|e| format!("{SEQUENCE_PATH}: {e}"))?;
    if let Some(stage) = sequence.stage.iter().find(|s| !role_exists(&s.role)) {
        return Err(format!(
            "{SEQUENCE_PATH}: stage `{}` names role `{}`, which has no file in .claude/agents/.",
            stage.name, stage.role
        ));
    }
    Ok(sequence)
}

/// The steps for a depth, as `fab sequence` prints them.
fn render(sequence: &Sequence, depth: Depth, reason: &str) -> String {
    let mut out = format!("Depth: {} ({reason})\n\n", depth.name());
    let stages = sequence
        .stage
        .iter()
        .filter(|s| s.depth <= depth && depth != Depth::None);
    for (n, stage) in stages.enumerate() {
        out.push_str(&format!("{}. {}: the `{}` agent", n + 1, stage.name, stage.role));
        if let Some(when) = &stage.when {
            out.push_str(&format!(", only when {when}"));
        }
        out.push_str(&format!(".\n   Posts: {}\n", stage.posts));
    }
    out.push_str(&format!("Finish. Posts: {}\n", sequence.finish.posts));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = include_str!("../../../.github/fabrial-sequence.toml");

    #[test]
    fn the_installed_sequence_parses_and_its_roles_exist() {
        let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/agents");
        let sequence = parse(FILE, |role| agents.join(format!("{role}.md")).is_file()).unwrap();
        assert_eq!(sequence.stage.len(), 4);
    }

    #[test]
    fn each_depth_runs_its_stages_in_order() {
        let sequence = parse(FILE, |_| true).unwrap();
        let steps = |depth| {
            render(&sequence, depth, "r")
                .lines()
                .filter(|l| l.starts_with(char::is_numeric) || l.starts_with("Finish"))
                .map(|l| l.split(':').next().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(steps(Depth::None), ["Finish. Posts"]);
        assert_eq!(
            steps(Depth::Standard),
            ["1. standard review", "2. standard revise", "Finish. Posts"]
        );
        assert_eq!(
            steps(Depth::Full),
            [
                "1. standard review",
                "2. standard revise",
                "3. adversarial review",
                "4. adversarial revise",
                "Finish. Posts"
            ]
        );
        assert!(render(&sequence, Depth::Full, "r").starts_with("Depth: full (r)\n"));
    }

    #[test]
    fn a_missing_role_or_an_unknown_key_is_refused() {
        let err = parse(FILE, |role| role != "reviser").unwrap_err();
        assert!(err.contains("role `reviser`"), "{err}");
        let extra = FILE.replacen("role = \"reviser\"", "role = \"reviser\"\nmodel = \"x\"", 1);
        let err = parse(&extra, |_| true).unwrap_err();
        assert!(err.contains("unknown field `model`"), "{err}");
        let bad_depth = FILE.replacen("depth = \"full\"", "depth = \"deep\"", 1);
        assert!(parse(&bad_depth, |_| true).is_err());
    }
}
