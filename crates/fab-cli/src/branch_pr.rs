//! Branch PR: opens a draft pull request for an agent's work branch as the
//! bot, so the owner can approve it under the ruleset on main.
//!
//! The title is the subject of the branch's first commit that is not a claim
//! commit ([`fab_core::claims`]), and the body is that commit's body as the
//! squash keeps it, above the footer line, so it needs no
//! model. A pull request that is already open keeps the title and body set
//! later by hand or by an agent. The pull request stays a draft until its
//! review finishes; the Review gate then marks it ready and posts the squash
//! message, and the owner merges it. Nothing here turns auto-merge on.
//!
//! It runs with the branch checked out in `HEAD_DIR`, `origin/main` fetched
//! there, and talks to GitHub through `gh` with `GH_TOKEN`. With `DRY_RUN=1`,
//! the pull request is printed instead of opened.

use std::path::PathBuf;

use clap::Args;
use clap::builder::FalseyValueParser;
use fab_core::claims::Claim;

use crate::run::{self, Result};
use crate::squash;

/// The note below the footer line of a new pull request's body.
const NOTE: &str = "Draft until the review finishes; the owner merges it.";

/// What the Branch PR workflow passes, each also read from its environment
/// variable.
#[derive(Args)]
pub struct Options {
    /// The repository, as owner/name.
    #[arg(long, env = "REPO")]
    repo: String,
    /// The branch to open a pull request for.
    #[arg(long, env = "BRANCH")]
    branch: String,
    /// A checkout of the branch, with origin/main fetched.
    #[arg(long, env = "HEAD_DIR", default_value = "head")]
    head_dir: PathBuf,
    /// Print the pull request instead of opening it. `DRY_RUN=1` sets it.
    #[arg(long, env = "DRY_RUN", action = clap::ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    dry_run: bool,
}

/// Opens the branch's draft pull request, unless one is open or the branch
/// has no commits ahead of main besides claim commits.
pub fn run(options: Options) -> Result<()> {
    let git = |args: &[&str]| run::git(&options.head_dir, args);
    let commits = git(&["rev-list", "--reverse", "origin/main..HEAD"])?;
    if commits.is_empty() {
        println!("No commits ahead of main.");
        return Ok(());
    }
    let mut first = None;
    for commit in commits.lines() {
        if Claim::parse(&git(&["show", "-s", "--format=%B", commit])?).is_none() {
            first = Some(commit);
            break;
        }
    }
    let Some(first) = first else {
        println!("Only a claim on the branch; no work to open a pull request for.");
        return Ok(());
    };
    let open = run::gh(&[
        "pr",
        "list",
        "--repo",
        &options.repo,
        "--head",
        &options.branch,
        "--state",
        "open",
        "--json",
        "number",
        "--jq",
        ".[0].number // empty",
    ])?;
    if !open.is_empty() {
        println!("#{open} is open; its title and body stay as they are.");
        return Ok(());
    }
    let title = git(&["show", "-s", "--format=%s", first])?;
    let body = body(&git(&["show", "-s", "--format=%b", first])?, &options.branch);
    let scratch = tempfile::tempdir().map_err(|e| format!("cannot make a scratch directory: {e}"))?;
    let file = scratch.path().join("body.md");
    std::fs::write(&file, body).map_err(|e| format!("cannot write the body: {e}"))?;
    let file = file.to_string_lossy();
    // The `=` form keeps a title that starts with a dash from reading as a
    // flag. GitHub skips CODEOWNERS on a draft and requests the owner's
    // review when the Review gate marks it ready; asking with --reviewer
    // fails with "Could not add requested reviewers".
    let title = format!("--title={title}");
    let args = [
        "pr",
        "create",
        "--repo",
        &options.repo,
        "--draft",
        "--base",
        "main",
        "--head",
        &options.branch,
        &title,
        "--body-file",
        &file,
    ];
    if options.dry_run {
        run::print_write(&args);
        return Ok(());
    }
    run::gh(&args)?;
    println!("Opened a draft for {}; it waits for the review.", options.branch);
    Ok(())
}

/// A new pull request's body: the kept text of the first commit's body, then
/// the footer line and the note.
fn body(commit_body: &str, branch: &str) -> String {
    let text = squash::kept_text(commit_body);
    let footer = format!(
        "{}: the text above becomes the squash commit body. -->\nFrom `{branch}`. {NOTE}\n",
        squash::FOOTER
    );
    if text.is_empty() { footer } else { format!("{text}\n\n{footer}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_keeps_the_commit_text_above_the_footer() {
        let commit = "Why it changed.\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n";
        assert_eq!(
            body(commit, "storm/x"),
            "Why it changed.\n\n<!-- fabrial:footer: the text above becomes the squash commit body. -->\n\
             From `storm/x`. Draft until the review finishes; the owner merges it.\n"
        );
    }

    #[test]
    fn a_commit_with_no_body_gives_the_footer_alone() {
        assert!(body("", "storm/x").starts_with("<!-- fabrial:footer: "));
    }

    #[test]
    fn the_squash_keeps_what_branch_pr_writes_above_the_footer() {
        let written = body("Why it changed.\n", "storm/x");
        assert_eq!(
            squash::body(&written, "storm/x", "A <a@b>"),
            "Why it changed.\n\nMerged from storm/x.\n\nCo-authored-by: A <a@b>"
        );
    }
}
