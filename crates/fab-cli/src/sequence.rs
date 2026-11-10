//! The review sequence: the stages the `fab-storm` skill runs on a pull request,
//! read from `.github/fabrial-sequence.toml`, cut to the pull request's
//! review depth. The file is the one place the stages, their roles and what
//! each posts are written down; the skills point to `fab sequence` instead of
//! repeating them.

use std::path::Path;

use clap::Args;
use fab_core::install;
use fab_core::keys::Key;
use serde::Deserialize;

use crate::depth::{self, Depth};
use crate::run::{self, Result};

/// Where the sequence lives, relative to the repository root.
pub const SEQUENCE_PATH: &str = ".github/fabrial-sequence.toml";
/// The top-level keys of [`Sequence`], as the reference pages describe them.
pub const KEYS: &[Key] = &[
    Key {
        name: "stage",
        kind: "array of tables",
        default: "required",
        about: "Stages, as `[[stage]]` tables, in the order they run. At least one.",
    },
    Key { name: "finish", kind: "table", default: "required", about: "Last step, after every stage." },
];
/// The keys of each `[[stage]]` table, [`Stage`], as the reference pages
/// describe them.
pub const STAGE_KEYS: &[Key] = &[
    Key {
        name: "name",
        kind: "string",
        default: "required",
        about: "Stage's name, such as `standard review`. Unique in the file, since `fab run` tells stages \
                apart by name.",
    },
    Key {
        name: "role",
        kind: "string",
        default: "required",
        about: "Agent role that runs the stage: a file in `.claude/agents/`, named in lowercase letters and \
                hyphens.",
    },
    Key {
        name: "depth",
        kind: "`standard` or `full`",
        default: "required",
        about: "Lowest review depth that runs the stage. A depth of `none` runs no stage.",
    },
    Key {
        name: "when",
        kind: "string",
        default: "unset",
        about: "Condition the stage needs to run, such as the stage before it finding anything above nit. \
                Unset, the stage always runs.",
    },
    Key {
        name: "posts",
        kind: "string",
        default: "required",
        about: "What the stage leaves on the pull request.",
    },
];
/// The keys of the `[finish]` table, [`Finish`], as the reference pages
/// describe them.
pub const FINISH_KEYS: &[Key] = &[Key {
    name: "posts",
    kind: "string",
    default: "required",
    about: "What the last step leaves on the pull request: the `Review finished` comment.",
}];

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
pub struct Sequence {
    /// The stages, in the order they run.
    pub stage: Vec<Stage>,
    /// The last step.
    pub finish: Finish,
}

/// One stage: a fresh agent in a role, run once.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// The stage's name, unique in the file.
    pub name: String,
    /// The agent role, a file in `.claude/agents/`.
    pub role: String,
    /// The lowest depth that runs the stage.
    pub depth: Depth,
    /// The condition the stage needs to run; without one it always runs.
    pub when: Option<String>,
    /// What the stage leaves on the pull request.
    pub posts: String,
}

/// The last step, after every stage the depth gives.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    /// What the last step leaves on the pull request.
    pub posts: String,
}

/// The stages a depth gives, in order, as `fab run` drives them.
pub struct Plan {
    /// The diff's depth, or the one given.
    pub depth: Depth,
    /// Why the diff has that depth.
    pub reason: String,
    /// The stages to run, in order.
    pub steps: Vec<Step>,
}

/// One stage of a [`Plan`].
pub struct Step {
    /// The stage's name, such as `standard review`.
    pub name: String,
    /// The agent role that runs it.
    pub role: String,
    /// Whether it runs only when the stage before it found anything above
    /// nit.
    pub conditional: bool,
    /// What it posts.
    pub posts: String,
}

/// Prints the depth and the stages it gives, numbered, then the finish.
pub fn run(options: Options) -> Result<()> {
    let (sequence, note) = load(&options.diff)?;
    let (depth, reason) = match options.depth {
        Some(depth) => (depth, "given with --depth".to_string()),
        None => depth::of_diff(&options.diff)?,
    };
    print!("{note}{}", render(&sequence, depth, &reason));
    Ok(())
}

/// The plan for a diff: its depth and the stages that depth gives.
pub fn plan(diff: &depth::Options) -> Result<Plan> {
    let (sequence, _) = load(diff)?;
    let (depth, reason) = depth::of_diff(diff)?;
    let steps = sequence
        .stage
        .into_iter()
        .filter(|s| s.depth <= depth && depth != Depth::None)
        .map(|s| Step { name: s.name, role: s.role, conditional: s.when.is_some(), posts: s.posts })
        .collect();
    Ok(Plan { depth, reason, steps })
}

/// The sequence this release installs.
pub fn installed() -> Result<Sequence> {
    let text = install::embedded(SEQUENCE_PATH).ok_or(format!("fab installs no {SEQUENCE_PATH}."))?;
    parse(text, |role| {
        install::embedded(&format!(".claude/agents/{role}.md")).is_some()
    })
}

/// Reads the sequence from the merge base, or the one this release installs
/// when the base has none, with a note saying which.
fn load(diff: &depth::Options) -> Result<(Sequence, String)> {
    let here = Path::new(".");
    // The file and the roles come from the merge base, as the gate comes
    // from main: a pull request's change to them applies to later pull
    // requests, never to its own review.
    let base = run::git(here, &["merge-base", &diff.base, &diff.head])?;
    let at_base = |path: &str| run::git(here, &["cat-file", "-e", &format!("{base}:{path}")]).is_ok();
    let agent = |role: &str| format!(".claude/agents/{role}.md");
    // A pull request that installs fabrial, or updates it to the release
    // that adds the file, has none at the base: it gets the sequence and
    // roles this release installs, and the output says so.
    if at_base(SEQUENCE_PATH) {
        let text = run::git(here, &["show", &format!("{base}:{SEQUENCE_PATH}")])
            .map_err(|e| format!("cannot read {SEQUENCE_PATH} at the base {base}: {e}"))?;
        Ok((parse(&text, |role| at_base(&agent(role)))?, String::new()))
    } else {
        let text = install::embedded(SEQUENCE_PATH).ok_or(format!("fab installs no {SEQUENCE_PATH}."))?;
        let note = format!(
            "Sequence: the one fabrial {} installs, since the base {} has no {SEQUENCE_PATH}.\n\n",
            fab_core::VERSION,
            &base[..base.len().min(7)]
        );
        Ok((parse(text, |role| install::embedded(&agent(role)).is_some())?, note))
    }
}

/// Reads a sequence and checks it: at least one stage, no two stages with
/// one name, since `fab run` tells stages apart by name, and every stage's
/// role a plain name that exists.
fn parse(text: &str, role_exists: impl Fn(&str) -> bool) -> Result<Sequence> {
    let sequence: Sequence = toml::from_str(text).map_err(|e| format!("{SEQUENCE_PATH}: {e}"))?;
    if sequence.stage.is_empty() {
        return Err(format!("{SEQUENCE_PATH} has no stages."));
    }
    for (n, stage) in sequence.stage.iter().enumerate() {
        if sequence.stage[..n].iter().any(|s| s.name == stage.name) {
            return Err(format!("{SEQUENCE_PATH}: two stages are named `{}`.", stage.name));
        }
    }
    let plain = |role: &str| !role.is_empty() && role.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    if let Some(stage) = sequence.stage.iter().find(|s| !plain(&s.role) || !role_exists(&s.role)) {
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
    use fab_core::keys::assert_keys_match;

    use super::*;

    const FILE: &str = include_str!("../../../.github/fabrial-sequence.toml");

    #[test]
    fn the_installed_sequence_parses_and_its_roles_exist() {
        let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/agents");
        let sequence = parse(FILE, |role| agents.join(format!("{role}.md")).is_file()).unwrap();
        assert_eq!(sequence.stage.len(), 4);
    }

    #[test]
    fn the_key_tables_match_the_sequence_structs() {
        assert_keys_match::<Sequence>(KEYS, |k| match k {
            "stage" => "[[stage]]\nname = \"a\"\nrole = \"r\"\ndepth = \"standard\"\nposts = \"p\"\n".into(),
            _ => format!("[{k}]\nposts = \"p\"\n"),
        });
        assert_keys_match::<Stage>(STAGE_KEYS, |k| match k {
            "depth" => "depth = \"full\"\n".into(),
            _ => format!("{k} = \"x\"\n"),
        });
        assert_keys_match::<Finish>(FINISH_KEYS, |k| format!("{k} = \"x\"\n"));
        assert_eq!(installed().unwrap().stage.len(), 4);
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
        let outside = FILE.replacen("role = \"reviewer\"", "role = \"../skills/fab-review/SKILL\"", 1);
        assert!(parse(&outside, |_| true).unwrap_err().contains("../skills"));
        let twice = FILE.replacen("name = \"adversarial review\"", "name = \"standard review\"", 1);
        let err = parse(&twice, |_| true).unwrap_err();
        assert!(err.contains("two stages are named `standard review`"), "{err}");
        let finish = &FILE[FILE.find("[finish]").unwrap()..];
        assert!(
            parse(&format!("stage = []\n{finish}"), |_| true)
                .unwrap_err()
                .contains("no stages")
        );
    }
}
