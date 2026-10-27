//! Review passes as GitHub reviews. `fab pr-review` posts a pass's report as
//! one bot review: each finding whose `<path>:<line>` falls in the diff sits
//! on that line, and the rest of the report is the review's body.
//! `fab pr-answer` posts a revise pass's answers as one bot review whose
//! comments are replies on the findings' threads, then resolves the threads
//! the answers close. One review is one notification, however many findings
//! it holds.

use std::path::{Path, PathBuf};

use clap::Args;
use clap::builder::FalseyValueParser;
use serde_json::{Value, json};

use crate::run::{self, Result};

/// The severities a finding line opens with: the `review` skill's five,
/// plus `major` and `minor`, which reviewers also write.
const SEVERITIES: &[&str] = &["blocker", "high", "major", "medium", "minor", "low", "nit"];
/// The outcomes that close a thread. A finding declined as not a defect
/// stays open for the owner to judge.
const CLOSING: &[&str] = &["Fixed", "Deferred"];
/// The pull request's pending reviews. GitHub shows a pending review only
/// to its author, so these are the bot's own.
const PENDING: &str = "query($owner: String!, $name: String!, $pr: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $pr) { reviews(states: [PENDING], first: 10) { nodes { id } } }
  }
}";
const THREADS: &str = "query($owner: String!, $name: String!, $pr: Int!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $pr) {
      id
      reviewThreads(first: 100, after: $endCursor) {
        pageInfo { hasNextPage endCursor }
        nodes { id comments(first: 1) { nodes { fullDatabaseId } } }
      }
    }
  }
}";

/// What `fab pr-review` posts, each also read from its environment variable.
#[derive(Args)]
pub struct ReviewOptions {
    /// The repository, as owner/name.
    #[arg(long, env = "REPO")]
    repo: String,
    /// The pull request number.
    #[arg(long, env = "PR")]
    pr: String,
    /// The full SHA of the head the pass reviewed.
    #[arg(long, env = "HEAD_SHA")]
    head: String,
    /// A file holding the pass's report.
    #[arg(long)]
    body_file: PathBuf,
    /// Print the write instead of sending it. `DRY_RUN=1` sets it.
    #[arg(long, env = "DRY_RUN", action = clap::ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    dry_run: bool,
}

/// What `fab pr-answer` posts, each also read from its environment variable.
#[derive(Args)]
pub struct AnswerOptions {
    /// The repository, as owner/name.
    #[arg(long, env = "REPO")]
    repo: String,
    /// The pull request number.
    #[arg(long, env = "PR")]
    pr: String,
    /// A file holding the answers: a line `<comment id> · <outcome>` per
    /// finding, and any other lines for the review's body.
    #[arg(long)]
    body_file: PathBuf,
    /// Print the writes instead of sending them. `DRY_RUN=1` sets it.
    #[arg(long, env = "DRY_RUN", action = clap::ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    dry_run: bool,
}

/// A finding line that names a place in a file.
#[derive(Debug, PartialEq)]
struct Finding {
    /// The report line it came from.
    line_no: usize,
    path: String,
    /// The first line of a range, when the finding names one.
    start: Option<u64>,
    line: u64,
    /// The comment's text: the severity, then the finding after its place.
    text: String,
}

/// One answer: the review comment that opened the thread, and the outcome.
#[derive(Debug, PartialEq)]
struct Answer {
    comment: String,
    text: String,
}

/// Posts a pass's report as one review on the head it reviewed, for the Bot
/// comment workflow.
pub fn review(options: ReviewOptions) -> Result<()> {
    let report = read(&options.body_file)?;
    if options.head.len() != 40 || !options.head.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("`{}` is not a full commit SHA.", options.head));
    }
    let base = run::gh(&[
        "api",
        &format!("repos/{}/pulls/{}", options.repo, options.pr),
        "--jq",
        ".base.ref",
    ])?;
    let compare = run::gh(&[
        "api",
        &format!("repos/{}/compare/{base}...{}", options.repo, options.head),
    ])?;
    let compare: Value = serde_json::from_str(&compare).map_err(|e| format!("cannot read the diff: {e}"))?;
    let files: Vec<(String, Vec<(u64, u64)>)> = compare["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let name = f["filename"].as_str().unwrap_or_default().to_string();
            (name, hunks(f["patch"].as_str().unwrap_or_default()))
        })
        .collect();
    let (placed, body) = place(&report, &files);
    let comments: Vec<Value> = placed
        .iter()
        .map(|f| {
            let mut c = json!({"path": f.path, "line": f.line, "side": "RIGHT", "body": f.text});
            if let Some(start) = f.start {
                c["start_line"] = json!(start);
                c["start_side"] = json!("RIGHT");
            }
            c
        })
        .collect();
    let request = json!({"commit_id": options.head, "event": "COMMENT", "body": body, "comments": comments});
    if post(&options.repo, &options.pr, &request, options.dry_run)? {
        println!("Posted the review with {} of its findings on the diff.", placed.len());
    }
    Ok(())
}

/// Posts a revise pass's answers as replies in one review, then resolves
/// the threads a fix or a deferral closes, for the Bot comment workflow.
pub fn answer(options: AnswerOptions) -> Result<()> {
    let text = read(&options.body_file)?;
    let (answers, body) = answers(&text);
    if answers.is_empty() {
        // Every finding was in the review's body, so the answers are too.
        if body.is_empty() {
            return Err("the answers are empty.".into());
        }
        if post(
            &options.repo,
            &options.pr,
            &json!({"event": "COMMENT", "body": body}),
            options.dry_run,
        )? {
            println!("Posted the answers as a review, with no thread to reply on.");
        }
        return Ok(());
    }
    let (owner, name) = options
        .repo
        .split_once('/')
        .ok_or_else(|| format!("`{}` is not owner/name.", options.repo))?;
    let filter = ".data.repository.pullRequest | .id as $pr | .reviewThreads.nodes[] \
                  | \"\\($pr) \\(.id) \\(.comments.nodes[0].fullDatabaseId)\"";
    let found = run::gh(&[
        "api",
        "graphql",
        "--paginate",
        "-f",
        &format!("query={THREADS}"),
        "-f",
        &format!("owner={owner}"),
        "-f",
        &format!("name={name}"),
        "-F",
        &format!("pr={}", options.pr),
        "--jq",
        filter,
    ])?;
    let threads: Vec<Vec<&str>> = found
        .lines()
        .map(|l| l.split(' ').collect())
        .filter(|t: &Vec<_>| t.len() == 3)
        .collect();
    let pr_id = threads
        .first()
        .map(|t| t[0])
        .ok_or("the pull request has no review threads to answer.")?;
    let mut targets = Vec::new();
    let mut unknown = Vec::new();
    for a in &answers {
        match threads.iter().find(|t| t[2] == a.comment) {
            Some(t) => targets.push((t[1], a)),
            None => unknown.push(a.comment.as_str()),
        }
    }
    if !unknown.is_empty() {
        return Err(format!("no review thread starts with comment {}.", unknown.join(", ")));
    }
    if options.dry_run {
        for (thread, a) in &targets {
            eprintln!(
                "DRY_RUN reply on {thread}: {:?}{}",
                a.text,
                if closes(&a.text) { ", then resolve" } else { "" }
            );
        }
        eprintln!("DRY_RUN review body: {body:?}");
        return Ok(());
    }
    // GitHub allows one pending review per author, so one a cancelled run
    // left would block this one.
    let pending = run::gh(&[
        "api",
        "graphql",
        "-f",
        &format!("query={PENDING}"),
        "-f",
        &format!("owner={owner}"),
        "-f",
        &format!("name={name}"),
        "-F",
        &format!("pr={}", options.pr),
        "--jq",
        ".data.repository.pullRequest.reviews.nodes[].id",
    ])?;
    for old in pending.lines().filter(|l| !l.trim().is_empty()) {
        delete_review(old)?;
    }
    let review = graphql(
        "mutation($pr: ID!) { addPullRequestReview(input: {pullRequestId: $pr}) { pullRequestReview { id } } }",
        &[("pr", pr_id)],
        ".data.addPullRequestReview.pullRequestReview.id",
    )?;
    let posted = (|| {
        for (thread, a) in &targets {
            graphql(
                "mutation($review: ID!, $thread: ID!, $body: String!) { addPullRequestReviewThreadReply(input: \
                 {pullRequestReviewId: $review, pullRequestReviewThreadId: $thread, body: $body}) { comment { id } } }",
                &[("review", &review), ("thread", thread), ("body", &a.text)],
                ".data.addPullRequestReviewThreadReply.comment.id",
            )?;
        }
        graphql(
            "mutation($review: ID!, $body: String!) { submitPullRequestReview(input: \
             {pullRequestReviewId: $review, event: COMMENT, body: $body}) { pullRequestReview { id } } }",
            &[("review", &review), ("body", &body)],
            ".data.submitPullRequestReview.pullRequestReview.id",
        )
    })();
    if let Err(e) = posted {
        // The next run deletes a pending review left behind, so a failed
        // delete here only needs saying.
        if let Err(d) = delete_review(&review) {
            eprintln!("warning: cannot delete the pending review {review}: {d}");
        }
        return Err(e);
    }
    // The replies are posted, so a failed resolve is a warning: running
    // again would post them twice.
    let mut resolved = 0;
    for (thread, _) in targets.iter().filter(|(_, a)| closes(&a.text)) {
        match graphql(
            "mutation($thread: ID!) { resolveReviewThread(input: {threadId: $thread}) { thread { id } } }",
            &[("thread", thread)],
            ".data.resolveReviewThread.thread.id",
        ) {
            Ok(_) => resolved += 1,
            Err(e) => eprintln!("warning: cannot resolve thread {thread}; resolve it by hand: {e}"),
        }
    }
    println!("Answered {} findings and resolved {resolved} threads.", targets.len());
    Ok(())
}

/// Posts a review through the REST API, or prints it with `dry_run`.
/// Returns whether it posted.
fn post(repo: &str, pr: &str, request: &Value, dry_run: bool) -> Result<bool> {
    let scratch = tempfile::tempdir().map_err(|e| format!("cannot make a scratch directory: {e}"))?;
    let file = scratch.path().join("review.json");
    std::fs::write(&file, request.to_string()).map_err(|e| format!("cannot write the review: {e}"))?;
    let path = format!("repos/{repo}/pulls/{pr}/reviews");
    let args = [
        "api",
        "-X",
        "POST",
        &path,
        "--input",
        file.to_str().ok_or("invalid scratch path")?,
    ];
    if dry_run {
        run::print_write(&args);
        eprintln!("DRY_RUN review: {request}");
        return Ok(false);
    }
    run::gh(&args)?;
    Ok(true)
}

/// Deletes a pending review.
fn delete_review(id: &str) -> Result<String> {
    graphql(
        "mutation($review: ID!) { deletePullRequestReview(input: {pullRequestReviewId: $review}) { clientMutationId } }",
        &[("review", id)],
        ".data",
    )
}

/// Runs one GraphQL request through `gh` and returns what `filter` picks,
/// failing when it picks nothing.
fn graphql(query: &str, vars: &[(&str, &str)], filter: &str) -> Result<String> {
    let mut args = vec![
        "api".to_string(),
        "graphql".into(),
        "-f".into(),
        format!("query={query}"),
    ];
    for (name, value) in vars {
        args.extend(["-f".into(), format!("{name}={value}")]);
    }
    args.extend(["--jq".into(), filter.into()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run::gh(&args)?;
    if out.trim().is_empty() || out.trim() == "null" {
        return Err(format!("GitHub returned nothing for `{filter}`."));
    }
    Ok(out.trim().to_string())
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Whether an answer closes its thread.
fn closes(text: &str) -> bool {
    CLOSING.iter().any(|c| text.starts_with(c))
}

/// The new-side line ranges, first and last, of a file's patch hunks.
fn hunks(patch: &str) -> Vec<(u64, u64)> {
    patch
        .lines()
        .filter_map(|l| {
            let new = l.strip_prefix("@@ -")?.split(' ').nth(1)?.strip_prefix('+')?;
            let (start, count) = match new.split_once(',') {
                Some((s, c)) => (s.parse::<u64>().ok()?, c.parse::<u64>().ok()?),
                None => (new.parse::<u64>().ok()?, 1),
            };
            (count > 0).then(|| (start, start + count - 1))
        })
        .collect()
}

/// A report line read as a finding with a place, such as
/// `medium · src/a.rs:12 — ...` or `- low · src/a.rs:3-5 — ...`.
fn finding(line_no: usize, line: &str) -> Option<Finding> {
    let rest = line.trim_start().strip_prefix("- ").unwrap_or(line.trim_start());
    // The severity may carry a mark after it, as in `minor, unverified`.
    let (severity, rest) = rest.split_once(" · ")?;
    let word = severity.split([',', ' ']).next().unwrap_or_default();
    if !SEVERITIES.contains(&word) {
        return None;
    }
    // The place runs to the dash before the problem, or else to the first
    // space, so a path with a space in it needs the dash.
    let (place, text) = rest
        .split_once(" — ")
        .or_else(|| rest.split_once(' '))
        .unwrap_or((rest, ""));
    let place = place.trim().trim_end_matches([':', ',']);
    let (path, lines) = place.rsplit_once(':')?;
    let (start, end) = match lines.split_once('-') {
        Some((a, b)) => (Some(a.parse().ok()?), b.parse().ok()?),
        None => (None, lines.parse().ok()?),
    };
    if path.is_empty() || start.is_some_and(|s| s >= end) {
        return None;
    }
    let text = text.trim_start().trim_start_matches(['—', '-', ':']).trim_start();
    Some(Finding { line_no, path: path.to_string(), start, line: end, text: format!("{severity} — {text}") })
}

/// Splits a report into the findings that sit on the diff, each inside one
/// hunk of its file, and the review body: every other line.
fn place(report: &str, files: &[(String, Vec<(u64, u64)>)]) -> (Vec<Finding>, String) {
    let mut placed = Vec::new();
    let mut body: Vec<&str> = Vec::new();
    for (no, line) in report.lines().enumerate() {
        let on_diff = finding(no, line).filter(|f| {
            let first = f.start.unwrap_or(f.line);
            files
                .iter()
                .find(|(name, _)| *name == f.path)
                .is_some_and(|(_, hunks)| hunks.iter().any(|(a, b)| *a <= first && f.line <= *b))
        });
        match on_diff {
            Some(f) => placed.push(f),
            None => {
                // A finding moved inline leaves no run of blank lines behind.
                if !(line.trim().is_empty() && body.last().is_some_and(|l| l.trim().is_empty())) {
                    body.push(line);
                }
            }
        }
    }
    (placed, body.join("\n").trim().to_string())
}

/// Splits answers into those naming a review comment, as `<id> · <outcome>`
/// or `- <id> · <outcome>`, and the review body: every other line.
fn answers(text: &str) -> (Vec<Answer>, String) {
    let mut found = Vec::new();
    let mut body = Vec::new();
    for line in text.lines() {
        let rest = line.trim_start().strip_prefix("- ").unwrap_or(line.trim_start());
        match rest.split_once(" · ") {
            Some((id, outcome)) if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) => {
                found.push(Answer { comment: id.to_string(), text: outcome.trim().to_string() })
            }
            _ => body.push(line),
        }
    }
    (found, body.join("\n").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunks_give_the_new_side_ranges() {
        let patch = "@@ -1,3 +1,4 @@ fn a\n a\n+b\n c\n@@ -20 +21 @@\n-x\n+y\n@@ -30,2 +32,0 @@\n-gone\n-gone";
        assert_eq!(hunks(patch), vec![(1, 4), (21, 21)]);
        assert_eq!(hunks(""), vec![]);
    }

    #[test]
    fn finding_lines_name_their_place() {
        let f = finding(0, "medium · PLAN.md:54 — the plan says Keychain → fix").unwrap();
        assert_eq!((f.path.as_str(), f.start, f.line), ("PLAN.md", None, 54));
        assert_eq!(f.text, "medium — the plan says Keychain → fix");
        let f = finding(3, "- low · crates/a b/x.rs:3-5 — wrong").unwrap();
        assert_eq!(
            (f.path.as_str(), f.start, f.line, f.line_no),
            ("crates/a b/x.rs", Some(3), 5, 3)
        );
        assert_eq!(f.text, "low — wrong");
        let f = finding(0, "minor, unverified · x.rs:9 — maybe").unwrap();
        assert_eq!(f.text, "minor, unverified — maybe");
        let f = finding(0, "  - high · x.rs:7: no dash").unwrap();
        assert_eq!(
            (f.path.as_str(), f.line, f.text.as_str()),
            ("x.rs", 7, "high — no dash")
        );
    }

    #[test]
    fn lines_without_a_place_are_not_findings() {
        for line in [
            "Standard pass on abc · model",
            "medium · PLAN.md says the wrong thing",
            "praise · x.rs:3 — nice",
            "low · commit 8a96450 / PR body — names one variable",
            "low · x.rs:5-3 — backwards",
            "low · :3 — no path",
            "",
        ] {
            assert_eq!(finding(0, line), None, "{line:?}");
        }
    }

    #[test]
    fn findings_on_the_diff_leave_the_body() {
        let report = "Standard pass on abc · m\n\n- medium · a.rs:3 — on a hunk\n\n- low · a.rs:40 — off it\n- nit · b.rs:2 — no such file\n- low · a.rs:9-11 — spans two hunks";
        let files = vec![("a.rs".to_string(), vec![(1, 10), (11, 20)])];
        let (placed, body) = place(report, &files);
        assert_eq!(placed.iter().map(|f| f.line).collect::<Vec<_>>(), vec![3]);
        assert_eq!(
            body,
            "Standard pass on abc · m\n\n- low · a.rs:40 — off it\n- nit · b.rs:2 — no such file\n- low · a.rs:9-11 — spans two hunks"
        );
    }

    #[test]
    fn answers_name_their_comment_and_closing_outcomes_resolve() {
        let (found, body) = answers(
            "Answering the review at abc · m\n\n- 123 · Fixed in def.\n456 · Not a defect: the test shows it.\n- x7 · stays",
        );
        assert_eq!(
            found,
            vec![
                Answer { comment: "123".into(), text: "Fixed in def.".into() },
                Answer { comment: "456".into(), text: "Not a defect: the test shows it.".into() },
            ]
        );
        assert_eq!(body, "Answering the review at abc · m\n\n- x7 · stays");
        assert!(closes("Fixed in def.") && closes("Deferred to a capture."));
        assert!(!closes("Not a defect.") && !closes("fixed"));
    }
}
