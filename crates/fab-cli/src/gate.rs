//! The Review gate: sets the `storm/review` check that the ruleset on main
//! requires. No model runs here: the agent that opened the pull request
//! reviews it in its own session (the `storm` skill) and ends by commenting
//! `Review finished at <sha>`.
//!
//! - `push`: a new head is pending, unless it is the change last reviewed with
//!   only the base moved (`BEFORE` is the previous head). With
//!   `ACTION=reopened` there is no `BEFORE`: the head stays as it is when its
//!   own latest run is a success, else pending.
//! - `comment`: `BODY` from `AUTHOR`. `Review finished at <sha>` or
//!   `/storm:ready <sha>` naming the head sets it to success, marks a draft
//!   ready and posts the squash message; the owner merges, by hand or with
//!   auto-merge. `/storm:stop` labels the pull request and turns off
//!   auto-merge.
//! - `edit`: `SENDER` changed the title or body. On a ready pull request whose
//!   head passed review, it posts the new squash message. Edits from anyone
//!   without write access are ignored.
//!
//! A command word ends at whitespace or the end of the line, so
//! `/storm:readyish` and `/storm:stopwatch` do nothing.
//!
//! It runs from a checkout of main with the pull request's head in
//! `HEAD_DIR` and `origin/main` fetched there, and talks to GitHub through
//! `gh` with `GH_TOKEN`. With `DRY_RUN=1`, writes are printed instead of sent.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use clap::builder::FalseyValueParser;
use clap::{ArgAction, Args, ValueEnum};
use fab_core::settings::Settings;
use serde_json::Value;
use sha2::{Digest, Sha256};

const CHECK_NAME: &str = "storm/review";
const BOT_SLUG: &str = "stormlight-thunderbot";
const STOP_LABEL: &str = "storm:stopped";
const INTRO: &str = "When you merge, by hand or with auto-merge, squash with this message:";
const AUTO_MERGE_NOTE: &str = "Auto-merge is on and squashes with the message it was turned on with. \
                               To use this one, turn auto-merge off and on again with it.";
const MARKER: &str = "<!-- storm-review diff=";

type Result<T> = std::result::Result<T, String>;

/// The pull request event the gate answers.
#[derive(Clone, Copy, PartialEq, ValueEnum)]
pub enum Event {
    /// A new or reopened head.
    Push,
    /// A comment that may be a review result or a `/storm:` command.
    Comment,
    /// An edit to the title or body.
    Edit,
}

/// What the Review gate workflow passes, each also read from its environment
/// variable.
#[derive(Args)]
pub struct Options {
    /// The repository, as owner/name.
    #[arg(long, env = "REPO")]
    repo: String,
    /// The pull request number.
    #[arg(long, env = "PR")]
    pr: String,
    /// The event to answer.
    #[arg(long, env = "EVENT", value_enum)]
    event: Event,
    /// The head to act on; without it, the pull request's current head.
    #[arg(long, env = "HEAD_SHA")]
    head_sha: Option<String>,
    /// A checkout of the head, with origin/main fetched.
    #[arg(long, env = "HEAD_DIR", default_value = "head")]
    head_dir: PathBuf,
    /// The pull request event's action, such as `reopened`.
    #[arg(long, env = "ACTION")]
    action: Option<String>,
    /// The head before a push.
    #[arg(long, env = "BEFORE")]
    before: Option<String>,
    /// The comment's text.
    #[arg(long, env = "BODY")]
    body: Option<String>,
    /// Who wrote the comment.
    #[arg(long, env = "AUTHOR")]
    author: Option<String>,
    /// Who edited the pull request.
    #[arg(long, env = "SENDER")]
    sender: Option<String>,
    /// Print writes to GitHub instead of sending them. `DRY_RUN=1` sets it.
    #[arg(long, env = "DRY_RUN", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    dry_run: bool,
}

/// One gate run: the pull request, its head, and how to reach GitHub.
struct Gate {
    repo: String,
    pr: String,
    head: String,
    head_dir: PathBuf,
    trailer: String,
    dry_run: bool,
    scratch: tempfile::TempDir,
}

impl Gate {
    fn short(&self) -> &str {
        &self.head[..self.head.len().min(7)]
    }

    /// Runs `gh` and returns its standard output.
    fn gh(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("gh")
            .args(args)
            .output()
            .map_err(|e| format!("cannot run gh: {e}"))?;
        if !output.status.success() {
            let what = args.iter().take(4).copied().collect::<Vec<_>>().join(" ");
            return Err(format!(
                "gh {what} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string())
    }

    /// Runs `gh` for a write, or prints it under `DRY_RUN`, with the text of
    /// any `--body-file` it names.
    fn gh_write(&self, args: &[&str]) -> Result<()> {
        if !self.dry_run {
            return self.gh(args).map(drop);
        }
        eprintln!(
            "DRY_RUN gh {}",
            args.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(" ")
        );
        for pair in args.windows(2) {
            if pair[0] == "--body-file" {
                eprintln!(
                    "DRY_RUN body: {:?}",
                    std::fs::read_to_string(pair[1]).unwrap_or_default()
                );
            }
        }
        Ok(())
    }

    fn view(&self, fields: &str) -> Result<Value> {
        let text = self.gh(&["pr", "view", &self.pr, "--repo", &self.repo, "--json", fields])?;
        serde_json::from_str(&text).map_err(|e| format!("cannot read gh pr view: {e}"))
    }

    /// Records the check on a commit, `in_progress` or a conclusion.
    fn check(&self, sha: &str, state: &str, title: &str, summary: &str) -> Result<()> {
        let path = format!("repos/{}/check-runs", self.repo);
        let fields = [
            format!("name={CHECK_NAME}"),
            format!("head_sha={sha}"),
            format!("output[title]={title}"),
            format!("output[summary]={summary}"),
        ];
        let mut args = vec!["api", "-X", "POST", &path];
        for field in &fields {
            args.extend(["-f", field]);
        }
        let conclusion = format!("conclusion={state}");
        if state == "in_progress" {
            args.extend(["-f", "status=in_progress"]);
        } else {
            args.extend(["-f", "status=completed", "-f", &conclusion]);
        }
        self.gh_write(&args)
    }

    /// The summary of the bot's latest completed `storm/review` run on a
    /// commit when that run succeeded, and empty otherwise. Runs from other
    /// apps are ignored, since any app with checks:write can post one under
    /// this name. A failed call reads as no success.
    fn last_success(&self, sha: &str) -> String {
        let path = format!("repos/{}/commits/{sha}/check-runs?check_name={CHECK_NAME}", self.repo);
        let filter = format!(
            "[.check_runs[] | select(.status == \"completed\" and .app.slug == \"{BOT_SLUG}\")] \
             | sort_by(.completed_at) | last | select(.conclusion == \"success\") | .output.summary"
        );
        self.gh(&["api", &path, "--jq", &filter]).unwrap_or_default()
    }

    /// A hash of a commit's diff against its merge base with main, or empty
    /// when the commit is not in the clone or adds nothing. The hash leaves
    /// out where the change sits: `index` lines, and the line ranges in hunk
    /// headers, so a rebase over an earlier line of the same file keeps it.
    /// The context label git adds after the ranges stays, so a change moved
    /// into another function does not. Every other byte counts, whitespace
    /// included.
    fn diff_hash(&self, sha: &str) -> String {
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&self.head_dir)
                .args(args)
                .output()
                .ok()
        };
        let ok = |out: Option<std::process::Output>| out.filter(|o| o.status.success());
        if ok(git(&["cat-file", "-e", &format!("{sha}^{{commit}}")])).is_none()
            && ok(git(&["fetch", "-q", "origin", sha])).is_none()
        {
            return String::new();
        }
        let Some(base) = ok(git(&["merge-base", "origin/main", sha])) else {
            return String::new();
        };
        let base = String::from_utf8_lossy(&base.stdout).trim().to_string();
        if git(&["diff", "--quiet", &base, sha]).is_some_and(|o| o.status.success()) {
            return String::new();
        }
        let Some(diff) = ok(git(&["diff", "--binary", "--no-color", "--no-ext-diff", &base, sha])) else {
            return String::new();
        };
        Sha256::digest(normalize_diff(&diff.stdout))
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The marker a success leaves in its summary, so a later push can tell
    /// whether only the base moved.
    fn marker(&self, sha: &str) -> String {
        format!("\n\n{MARKER}{} -->", self.diff_hash(sha))
    }

    /// Marks a draft pull request ready once its head has passed review.
    /// Merging, by hand or with auto-merge, is the owner's choice: the gate
    /// never turns auto-merge on or changes a request the owner set. A
    /// stopped or closed pull request is left alone. A failed call stops the
    /// run before the success is posted.
    fn mark_ready(&self) -> Result<()> {
        let info = self.view("state,isDraft,labels")?;
        if info["state"] != "OPEN" || stopped(&info) {
            return Ok(());
        }
        if info["isDraft"] == true {
            self.gh_write(&["pr", "ready", &self.pr, "--repo", &self.repo])?;
        }
        Ok(())
    }

    /// Posts the squash message Branch PR's flow builds, for the owner. Runs
    /// can overlap, such as an edit and a `Review finished`, and each reads
    /// the text when it starts. A run reads again after each post until a
    /// read finds nothing new, reading at most six times, so the run that
    /// posts last also posts the pull request's current text unless edits
    /// keep landing. Two runs that read the same text can both post it.
    fn post_squash_message(&self) -> Result<()> {
        for _ in 0..6 {
            if !self.squash_message()? || self.dry_run {
                break;
            }
        }
        Ok(())
    }

    /// Posts the squash message built from the pull request as it is now,
    /// and says whether it posted. Posts nothing when the pull request is
    /// closed or stopped, or when the bot's newest squash message already
    /// says the same.
    fn squash_message(&self) -> Result<bool> {
        let info = self.view("state,title,body,headRefName,labels,autoMergeRequest,comments")?;
        if info["state"] != "OPEN" || stopped(&info) {
            return Ok(false);
        }
        // The subject carries the PR number, as GitHub's own squash title
        // does, so `git log` on main leads back to the pull request.
        let mut title = info["title"].as_str().unwrap_or_default().to_string();
        let number = format!("(#{})", self.pr);
        if !title.ends_with(&number) {
            title = format!("{title} {number}");
        }
        let branch = info["headRefName"].as_str().unwrap_or_default();
        let body = self.squash_body(info["body"].as_str().unwrap_or_default(), branch)?;
        // The fence outgrows any backtick run in the message, so a title
        // cannot close it early.
        let fence = "`".repeat(longest_backtick_run(&format!("{title}{body}")).max(2) + 1);
        let mut text = format!("{INTRO}\n\n{fence}text\n{title}\n\n{body}\n{fence}");
        // GitHub keeps the message auto-merge was turned on with, whatever
        // the pull request says later.
        if !info["autoMergeRequest"].is_null() {
            text = format!("{text}\n\n{AUTO_MERGE_NOTE}");
        }
        let newest = info["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| is_bot(c["author"]["login"].as_str().unwrap_or_default()))
            .filter_map(|c| c["body"].as_str())
            .rfind(|b| b.starts_with(INTRO))
            .unwrap_or_default();
        if newest == text {
            return Ok(false);
        }
        let file = self.scratch.path().join("comment");
        std::fs::write(&file, &text).map_err(|e| format!("cannot write the comment: {e}"))?;
        let file = file.to_string_lossy();
        self.gh_write(&["pr", "comment", &self.pr, "--repo", &self.repo, "--body-file", &file])?;
        Ok(true)
    }

    /// The squash body from `.github/storm/squash-body.sh`, as Branch PR
    /// builds it.
    fn squash_body(&self, description: &str, branch: &str) -> Result<String> {
        let mut child = Command::new(".github/storm/squash-body.sh")
            .args([branch, &self.trailer])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run squash-body.sh: {e}"))?;
        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(description.as_bytes())
            .map_err(|e| format!("cannot write to squash-body.sh: {e}"))?;
        let output = child
            .wait_with_output()
            .map_err(|e| format!("squash-body.sh failed: {e}"))?;
        if !output.status.success() {
            return Err("squash-body.sh failed".into());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches('\n')
            .to_string())
    }

    /// Asks for a command again when the SHA it names is not the head.
    fn wrong_sha(&self, command: &str, named: &str) -> Result<()> {
        let named = if named.is_empty() { "nothing" } else { named };
        let body = format!(
            "`{command}` names `{named}`, but the head is `{}`. Check the head and post it again.",
            self.short()
        );
        self.gh_write(&["pr", "comment", &self.pr, "--repo", &self.repo, "--body", &body])
    }

    fn edit(&self, sender: &str) -> Result<()> {
        // Only the bot, or someone who could push to the branch, changes the
        // message a reviewed pull request merges with. A fork's author cannot.
        if sender != format!("{BOT_SLUG}[bot]") {
            let sender = if sender.is_empty() { "none" } else { sender };
            let path = format!("repos/{}/collaborators/{sender}/permission", self.repo);
            let permission = self.gh(&["api", &path, "--jq", ".permission"]).unwrap_or_default();
            if !matches!(permission.as_str(), "admin" | "maintain" | "write") {
                return Ok(());
            }
        }
        let draft = self.view("isDraft")?["isDraft"] == true;
        if !draft && !self.last_success(&self.head).is_empty() {
            self.post_squash_message()?;
        }
        Ok(())
    }

    fn push(&self, action: &str, before: &str) -> Result<()> {
        if action == "reopened" && !self.last_success(&self.head).is_empty() {
            return Ok(());
        }
        if !before.is_empty() && !before.chars().all(|c| c == '0') {
            let reviewed = marked_hash(&self.last_success(before));
            if !reviewed.is_empty() && reviewed == self.diff_hash(&self.head) {
                let old = &before[..before.len().min(7)];
                let summary = format!(
                    "Only the base moved since {old}, so its review stands.{}",
                    self.marker(&self.head)
                );
                return self.check(&self.head, "success", &format!("Same change as {old}"), &summary);
            }
        }
        let short = self.short();
        let summary = format!(
            "The agent that opened this pull request reviews it and comments `Review finished at {short}`. \
             A person can accept it as it is with `/storm:ready {short}`."
        );
        self.check(&self.head, "in_progress", "Waiting for review", &summary)
    }

    fn comment(&self, body: &str, author: &str) -> Result<()> {
        let line = body.lines().next().unwrap_or_default().replace('\r', "");
        let short = self.short().to_string();
        if line.starts_with("Review finished at ") {
            let named = named_sha(&line, 4);
            if !self.names_head(&named) {
                return self.wrong_sha("Review finished", &named);
            }
            self.mark_ready()?;
            let summary = format!("@{author} finished the review of {short}.{}", self.marker(&self.head));
            self.check(&self.head, "success", &format!("Reviewed at {short}"), &summary)?;
            // After the success, so an edit from here on finds the head
            // reviewed and posts its own message.
            self.post_squash_message()
        } else if is_command(&line, "/storm:ready") {
            let named = named_sha(&line, 2);
            if !self.names_head(&named) {
                return self.wrong_sha("/storm:ready", &named);
            }
            self.mark_ready()?;
            let summary = format!(
                "@{author} accepted {short} with `/storm:ready`.{}",
                self.marker(&self.head)
            );
            self.check(&self.head, "success", &format!("Marked ready by @{author}"), &summary)?;
            self.post_squash_message()
        } else if is_command(&line, "/storm:stop") {
            self.gh_write(&[
                "label",
                "create",
                STOP_LABEL,
                "--repo",
                &self.repo,
                "--force",
                "--description",
                "No auto-merge or agent work until a person removes this label",
            ])?;
            self.gh_write(&["pr", "edit", &self.pr, "--repo", &self.repo, "--add-label", STOP_LABEL])?;
            // Turning off auto-merge that is already off fails; that is fine.
            let _ = self.gh_write(&["pr", "merge", &self.pr, "--repo", &self.repo, "--disable-auto"]);
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Whether a named SHA is at least 7 hex characters and a prefix of the
    /// head.
    fn names_head(&self, named: &str) -> bool {
        (7..=40).contains(&named.len())
            && named.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && self.head.starts_with(named)
    }
}

/// Runs the gate for one event. The settings come from main's
/// `.github/fabrial.toml`, in the working directory.
pub fn run(options: Options) -> Result<()> {
    let text = std::fs::read_to_string(".github/fabrial.toml")
        .map_err(|e| format!("cannot read .github/fabrial.toml: {e}"))?;
    let trailer = Settings::parse(&text).map_err(|e| e.to_string())?.trailer;
    let given = |v: Option<String>| v.filter(|s| !s.is_empty());
    let mut gate = Gate {
        repo: options.repo,
        pr: options.pr,
        head: String::new(),
        head_dir: options.head_dir,
        trailer,
        dry_run: options.dry_run,
        scratch: tempfile::tempdir().map_err(|e| format!("cannot make a scratch directory: {e}"))?,
    };
    gate.head = match given(options.head_sha) {
        Some(sha) => sha,
        None => gate.gh(&[
            "pr",
            "view",
            &gate.pr,
            "--repo",
            &gate.repo,
            "--json",
            "headRefOid",
            "--jq",
            ".headRefOid",
        ])?,
    };
    match options.event {
        Event::Edit => gate.edit(&given(options.sender).unwrap_or_default()),
        Event::Push => gate.push(
            &given(options.action).unwrap_or_default(),
            &given(options.before).unwrap_or_default(),
        ),
        Event::Comment => gate.comment(
            &options.body.unwrap_or_default(),
            &given(options.author).unwrap_or_else(|| "someone".into()),
        ),
    }
}

/// Whether a pull request carries the stop label.
fn stopped(info: &Value) -> bool {
    info["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|l| l["name"] == STOP_LABEL)
}

/// Whether a comment author is the bot, as `gh` or the API names it.
fn is_bot(login: &str) -> bool {
    let login = login.strip_prefix("app/").unwrap_or(login);
    login.strip_suffix("[bot]").unwrap_or(login) == BOT_SLUG
}

/// Whether a line is a command word, alone or followed by whitespace.
fn is_command(line: &str, word: &str) -> bool {
    line.strip_prefix(word)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// The SHA a command line names as its `n`th word, without backticks or full
/// stops.
fn named_sha(line: &str, n: usize) -> String {
    line.split_whitespace()
        .nth(n - 1)
        .unwrap_or_default()
        .replace(['`', '.'], "")
}

/// The diff hash a success's summary records, or empty.
fn marked_hash(summary: &str) -> String {
    summary
        .lines()
        .find_map(|line| {
            let rest = &line[line.find(MARKER)? + MARKER.len()..];
            let hash = rest
                .strip_suffix(" -->")
                .or_else(|| rest.split_once(" -->").map(|p| p.0))?;
            hash.bytes().all(|b| b.is_ascii_hexdigit()).then(|| hash.to_string())
        })
        .unwrap_or_default()
}

/// The longest run of backticks in a text.
fn longest_backtick_run(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// Drops a diff's `index` and similarity lines and cuts hunk headers' line
/// ranges, keeping every other byte, the context label included.
fn normalize_diff(diff: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(diff.len());
    let mut lines = diff.split(|&b| b == b'\n').peekable();
    while let Some(line) = lines.next() {
        let last = lines.peek().is_none();
        let dropped = [&b"index "[..], b"similarity index ", b"dissimilarity index "]
            .iter()
            .any(|p| line.starts_with(p));
        if !dropped {
            out.extend_from_slice(&cut_hunk_header(line));
        }
        if !last && !dropped {
            out.push(b'\n');
        }
    }
    out
}

/// `@@ -a,b +c,d @@ label` becomes `@@ label`; any other line is unchanged.
fn cut_hunk_header(line: &[u8]) -> Vec<u8> {
    let range = |s: &[u8]| s.iter().take_while(|b| b.is_ascii_digit() || **b == b',').count();
    let Some(rest) = line.strip_prefix(b"@@ -") else {
        return line.to_vec();
    };
    let old = range(rest);
    let Some(rest) = rest[old..].strip_prefix(b" +").filter(|_| old > 0) else {
        return line.to_vec();
    };
    let new = range(rest);
    match rest[new..].strip_prefix(b" @@").filter(|_| new > 0) {
        Some(label) => [&b"@@"[..], label].concat(),
        None => line.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunk_headers_lose_their_ranges_and_index_lines_go() {
        let diff = b"diff --git a/x b/x\nindex 1..2 100644\n@@ -1,3 +1,4 @@ fn x\n-a\n+b\n";
        assert_eq!(normalize_diff(diff), b"diff --git a/x b/x\n@@ fn x\n-a\n+b\n");
        assert_eq!(cut_hunk_header(b"@@ -1 +1 @@"), b"@@");
        assert_eq!(cut_hunk_header(b"@@ not a header"), b"@@ not a header");
    }

    #[test]
    fn commands_end_at_whitespace() {
        assert!(is_command("/storm:ready abc1234", "/storm:ready"));
        assert!(is_command("/storm:stop", "/storm:stop"));
        assert!(!is_command("/storm:readyish abc1234", "/storm:ready"));
        assert!(!is_command("/storm:stopwatch", "/storm:stop"));
    }

    #[test]
    fn named_sha_drops_backticks_and_full_stops() {
        assert_eq!(named_sha("Review finished at `abc1234`.", 4), "abc1234");
        assert_eq!(named_sha("/storm:ready", 2), "");
    }

    #[test]
    fn the_bot_is_known_by_every_login_form() {
        for login in [
            "stormlight-thunderbot",
            "stormlight-thunderbot[bot]",
            "app/stormlight-thunderbot",
        ] {
            assert!(is_bot(login), "{login}");
        }
        assert!(!is_bot("eve"));
        assert!(!is_bot("stormlight-thunderbot-impostor"));
    }

    #[test]
    fn the_marker_reads_back() {
        let summary = "@a finished.\n\n<!-- storm-review diff=abc123 -->";
        assert_eq!(marked_hash(summary), "abc123");
        assert_eq!(marked_hash("no marker"), "");
        assert_eq!(marked_hash("<!-- storm-review diff= -->"), "");
    }

    #[test]
    fn the_fence_outgrows_backtick_runs() {
        assert_eq!(longest_backtick_run("a ``` b ````"), 4);
        assert_eq!(longest_backtick_run("none"), 0);
    }
}
