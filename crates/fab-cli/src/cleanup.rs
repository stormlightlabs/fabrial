//! `fab cleanup`: what follows a merge. It removes the worktrees and local
//! branches of merged agent pull requests, fast-forwards a clean primary
//! `main`, and lists the open agent pull requests that need a resync.
//!
//! Everything it acts on comes from git and from GitHub's REST API through
//! `gh api`, so it needs no record from the run that made the worktrees and
//! works after a restart. A worktree is removed only when its pull request
//! merged, `git status --porcelain --ignored` lists nothing but Cargo's
//! `target/`, and its branch tip equals the pull request's head. The branch
//! is deleted only at that tip. Anything that fails stops that item, which
//! stays as it was, and is reported. It never forces a removal, rebases,
//! resets or stashes; resyncing is the caller's, with the `resync` skill.
//!
//! Exit codes: 0 when everything was handled or needs nothing, 1 when
//! something was kept or needs a resync, 2 when something failed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use serde_json::Value;

use crate::run::{self, Result};

/// The branch prefixes agents push, as AGENTS.md names them.
const AGENT_PREFIXES: [&str; 3] = ["claude/", "codex/", "storm/"];

/// The ignored paths `git worktree remove` may delete with the worktree:
/// Cargo's build output at the worktree's root, which a build recreates.
/// Every other ignored file, such as an `.env` or a Nix `result` link, keeps
/// the worktree.
const DISPOSABLE: [&str; 1] = ["target/"];

/// What `cleanup` does with one worktree, decided from its pull requests.
#[derive(Debug, PartialEq)]
enum Plan {
    /// Merged, clean and at the merged head: remove it and its branch.
    Remove(u64),
    /// Merged or closed, but not removable; the reason says why.
    Keep(String),
    /// Still in progress; the reason says why.
    Leave(String),
}

/// The options of `fab cleanup`.
#[derive(Args)]
pub struct Options {
    /// The GitHub repository, as owner/name; without it, read from the
    /// `origin` remote's URL.
    #[arg(long)]
    repo: Option<String>,
    /// Print what would be done and change nothing, not even
    /// `origin/main`.
    #[arg(long)]
    dry_run: bool,
}

/// One worktree from `git worktree list --porcelain`.
struct Worktree {
    path: PathBuf,
    /// The branch it has checked out, without `refs/heads/`.
    branch: Option<String>,
    /// Whether git marks it prunable: its directory is gone.
    prunable: bool,
}

/// One pull request from the REST API's list of pulls for a head branch.
struct Pull {
    number: u64,
    open: bool,
    merged: bool,
    head: String,
}

/// What one run found, for its exit code.
#[derive(Default)]
struct Tally {
    kept: bool,
    failed: bool,
}

impl Tally {
    fn code(&self) -> ExitCode {
        ExitCode::from(if self.failed {
            2
        } else if self.kept {
            1
        } else {
            0
        })
    }
}

/// Runs `fab cleanup` from any checkout of the repository.
pub fn run(options: Options) -> ExitCode {
    match cleanup(&options) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fab: {e}");
            ExitCode::from(2)
        }
    }
}

fn cleanup(options: &Options) -> Result<ExitCode> {
    let here = std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    let repo = match &options.repo {
        Some(repo) => repo.clone(),
        None => repo_from_url(&run::git(&here, &["remote", "get-url", "origin"])?)
            .ok_or("cannot read owner/name from origin's URL; pass --repo")?,
    };
    let worktrees = worktrees(&run::git(&here, &["worktree", "list", "--porcelain"])?);
    let Some(primary) = worktrees.first() else {
        return Err("git lists no worktrees".into());
    };
    let primary_dir = primary.path.clone();
    let here = here.canonicalize().unwrap_or(here);
    let mut tally = Tally::default();
    for worktree in &worktrees[1..] {
        let Some(branch) = worktree.branch.as_deref().filter(|b| is_agent(b)) else {
            continue;
        };
        if worktree.prunable {
            println!(
                "stale {branch} at {at}: the directory is gone; run `git worktree remove {at}`",
                at = worktree.path.display()
            );
            tally.kept = true;
            continue;
        }
        if let Err(e) = one_worktree(
            &repo,
            &primary_dir,
            &here,
            worktree,
            branch,
            options.dry_run,
            &mut tally,
        ) {
            println!("error {branch} at {}: {e}", worktree.path.display());
            tally.failed = true;
        }
    }
    if let Err(e) = refresh_main(primary, options.dry_run, &mut tally) {
        println!("main: error: {e}");
        tally.failed = true;
    }
    if let Err(e) = needs_resync(&repo, &mut tally) {
        println!("resync: error: {e}");
        tally.failed = true;
    }
    Ok(tally.code())
}

/// Removes one agent worktree and its branch when its pull request merged
/// at its tip, and reports what it did or why not.
fn one_worktree(
    repo: &str, primary: &Path, here: &Path, worktree: &Worktree, branch: &str, dry_run: bool, tally: &mut Tally,
) -> Result<()> {
    let at = worktree.path.display();
    let owner = repo.split('/').next().unwrap_or_default();
    let path = format!(
        "repos/{repo}/pulls?head={}&state=all&per_page=100",
        encode(&format!("{owner}:{branch}"))
    );
    let pulls = pulls(&run::gh(&["api", &path])?)?;
    let tip = run::git(primary, &["rev-parse", &format!("refs/heads/{branch}")])?;
    let status = run::git(&worktree.path, &["status", "--porcelain", "--ignored"])?;
    let (ignored, changed): (Vec<&str>, Vec<&str>) = status.lines().partition(|l| l.starts_with("!! "));
    let mut plan = plan(&pulls, &tip, !changed.is_empty());
    let kept: Vec<&str> = ignored
        .iter()
        .map(|l| &l[3..])
        .filter(|p| !DISPOSABLE.contains(p))
        .collect();
    let inside = here.starts_with(worktree.path.canonicalize().unwrap_or(worktree.path.clone()));
    if matches!(plan, Plan::Remove(_)) {
        if inside {
            plan = Plan::Keep("it holds the current directory; run from another checkout".into());
        } else if !kept.is_empty() {
            plan = Plan::Keep(format!("removal would delete ignored files: {}", kept.join(", ")));
        }
    }
    match plan {
        Plan::Remove(number) if dry_run => println!("would remove {branch} at {at}: #{number} merged"),
        Plan::Remove(number) => {
            remove(primary, &worktree.path, branch, &tip)?;
            println!("removed {branch} at {at}: #{number} merged");
        }
        Plan::Keep(why) => {
            println!("kept {branch} at {at}: {why}");
            tally.kept = true;
        }
        Plan::Leave(why) => println!("left {branch} at {at}: {why}"),
    }
    Ok(())
}

/// Removes a worktree and its branch, deleting the branch first and only at
/// `tip`, so a commit added since it was judged is kept. The worktree is
/// detached for the deletion and put back on its branch if that fails, and
/// the branch is recreated if the removal then fails, so a failure leaves
/// both for the next run to see.
fn remove(primary: &Path, dir: &Path, branch: &str, tip: &str) -> Result<()> {
    let reference = format!("refs/heads/{branch}");
    run::git(dir, &["checkout", "-q", "--detach"])?;
    if let Err(e) = run::git(primary, &["update-ref", "-d", &reference, tip]) {
        return Err(match run::git(dir, &["checkout", "-q", branch]) {
            Ok(_) => format!("cannot delete the branch: {e}"),
            Err(back) => format!("cannot delete the branch ({e}), and the worktree stays detached: {back}"),
        });
    }
    if let Err(e) = run::git(primary, &["worktree", "remove", &dir.to_string_lossy()]) {
        let back = run::git(primary, &["update-ref", &reference, tip, ""])
            .and_then(|_| run::git(dir, &["checkout", "-q", branch]));
        return Err(match back {
            Ok(_) => format!("cannot remove the worktree: {e}"),
            Err(back) => format!("cannot remove the worktree ({e}) or put back {branch} at {tip}: {back}"),
        });
    }
    Ok(())
}

/// Decides what to do with a worktree from its branch's pull requests, its
/// branch tip and whether it has changes.
fn plan(pulls: &[Pull], tip: &str, dirty: bool) -> Plan {
    let short = |s: &str| s.chars().take(7).collect::<String>();
    if let Some(open) = pulls.iter().find(|p| p.open) {
        return Plan::Leave(format!("#{} is open", open.number));
    }
    let Some(merged) = pulls
        .iter()
        .find(|p| p.merged && p.head == tip)
        .or_else(|| pulls.iter().find(|p| p.merged))
    else {
        return match pulls.first() {
            Some(closed) => Plan::Keep(format!("#{} closed without merging", closed.number)),
            None => Plan::Leave("no pull request".into()),
        };
    };
    if merged.head != tip {
        Plan::Keep(format!(
            "tip {} differs from #{}'s merged head {}",
            short(tip),
            merged.number,
            short(&merged.head)
        ))
    } else if dirty {
        Plan::Keep(format!("uncommitted changes; #{} merged", merged.number))
    } else {
        Plan::Remove(merged.number)
    }
}

/// Fast-forwards the primary checkout's `main` to `origin/main` when it is
/// on `main`, clean and behind, and reports why not otherwise.
fn refresh_main(primary: &Worktree, dry_run: bool, tally: &mut Tally) -> Result<()> {
    let dir = &primary.path;
    if !dry_run {
        run::git(dir, &["fetch", "-q", "origin", "main"])?;
    }
    let mut leave = |why: String| -> Result<()> {
        println!("main: left: {why}");
        tally.kept = true;
        Ok(())
    };
    if primary.branch.as_deref() != Some("main") {
        let on = primary.branch.as_deref().unwrap_or("a detached head");
        return leave(format!("{} is on {on}", dir.display()));
    }
    if !run::git(dir, &["status", "--porcelain"])?.is_empty() {
        return leave(format!("{} has uncommitted changes", dir.display()));
    }
    let counts = run::git(dir, &["rev-list", "--left-right", "--count", "main...origin/main"])?;
    let (ahead, behind) = counts.split_once('\t').ok_or("cannot read rev-list counts")?;
    if ahead != "0" {
        return leave(format!("main has {} origin/main lacks", commits(ahead)));
    }
    if behind == "0" {
        println!("main: up to date");
    } else if dry_run {
        println!("main: would fast-forward {}", commits(behind));
    } else {
        run::git(dir, &["merge", "-q", "--ff-only", "origin/main"])?;
        println!("main: fast-forwarded {}", commits(behind));
    }
    Ok(())
}

/// Lists the repository's open agent pull requests that conflict with
/// their base or are behind it, and those whose mergeability GitHub has not
/// worked out yet. GitHub reports `behind` only when the base requires
/// up-to-date branches, so the compare API decides that instead.
fn needs_resync(repo: &str, tally: &mut Tally) -> Result<()> {
    let list = run::gh(&[
        "api",
        "--paginate",
        &format!("repos/{repo}/pulls?state=open&per_page=100"),
    ])?;
    for pull in pages(&list)? {
        let branch = pull["head"]["ref"].as_str().unwrap_or_default();
        let same_repo = pull["head"]["repo"]["full_name"].as_str() == Some(repo);
        let Some(number) = pull["number"].as_u64().filter(|_| same_repo && is_agent(branch)) else {
            continue;
        };
        let one: Value = serde_json::from_str(&run::gh(&["api", &format!("repos/{repo}/pulls/{number}")])?)
            .map_err(|e| format!("cannot read #{number}: {e}"))?;
        let base = one["base"]["ref"].as_str().unwrap_or("main");
        let why = match one["mergeable_state"].as_str() {
            Some("dirty") => format!("conflicts with {base}"),
            Some("unknown") | None => {
                println!("unknown: #{number} {branch}: GitHub has not worked out whether it merges; run again");
                tally.kept = true;
                continue;
            }
            Some(_) => {
                let head = one["head"]["sha"].as_str().ok_or(format!("#{number} has no head"))?;
                let path = format!("repos/{repo}/compare/{}...{head}", encode(base));
                let compare: Value = serde_json::from_str(&run::gh(&["api", &path])?)
                    .map_err(|e| format!("cannot compare #{number} with {base}: {e}"))?;
                match compare["behind_by"].as_u64() {
                    Some(0) => continue,
                    Some(n) => format!("behind {base} by {}", commits(&n.to_string())),
                    None => return Err(format!("cannot read how far #{number} is behind {base}")),
                }
            }
        };
        println!("needs resync: #{number} {branch}: {why}");
        tally.kept = true;
    }
    Ok(())
}

/// Whether a branch is one agents push.
fn is_agent(branch: &str) -> bool {
    AGENT_PREFIXES.iter().any(|p| branch.starts_with(p))
}

/// A count of commits, as words.
fn commits(count: &str) -> String {
    if count == "1" { "1 commit".into() } else { format!("{count} commits") }
}

/// Reads `git worktree list --porcelain`; the primary checkout comes first.
fn worktrees(porcelain: &str) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    for line in porcelain.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            list.push(Worktree { path: PathBuf::from(path), branch: None, prunable: false });
        } else if let (Some(branch), Some(last)) = (line.strip_prefix("branch refs/heads/"), list.last_mut()) {
            last.branch = Some(branch.to_string());
        } else if let (true, Some(last)) = (line == "prunable" || line.starts_with("prunable "), list.last_mut()) {
            last.prunable = true;
        }
    }
    list
}

/// Reads the REST API's list of pulls.
fn pulls(json: &str) -> Result<Vec<Pull>> {
    let list: Vec<Value> = serde_json::from_str(json).map_err(|e| format!("cannot read the pulls: {e}"))?;
    Ok(list
        .iter()
        .map(|p| Pull {
            number: p["number"].as_u64().unwrap_or_default(),
            open: p["state"] == "open",
            merged: !p["merged_at"].is_null(),
            head: p["head"]["sha"].as_str().unwrap_or_default().to_string(),
        })
        .collect())
}

/// Reads `gh api --paginate` output, one JSON array per page.
fn pages(json: &str) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    for page in serde_json::Deserializer::from_str(json).into_iter::<Vec<Value>>() {
        all.extend(page.map_err(|e| format!("cannot read the open pulls: {e}"))?);
    }
    Ok(all)
}

/// Reads owner/name from a GitHub remote URL, in its HTTPS, SSH or scp
/// form, or a proxy URL that ends with them.
fn repo_from_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let mut parts = url.rsplit(['/', ':']);
    let name = parts.next().filter(|s| !s.is_empty())?;
    let owner = parts.next().filter(|s| !s.is_empty() && !s.contains('@'))?;
    Some(format!("{owner}/{name}"))
}

/// Percent-encodes a query value, keeping what branch names usually hold.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pull(number: u64, open: bool, merged: bool, head: &str) -> Pull {
        Pull { number, open, merged, head: head.into() }
    }

    #[test]
    fn a_clean_branch_at_the_merged_head_is_removed() {
        assert_eq!(plan(&[pull(3, false, true, "abc")], "abc", false), Plan::Remove(3));
    }

    #[test]
    fn a_merged_branch_with_changes_or_a_new_tip_is_kept() {
        assert!(matches!(
            plan(&[pull(3, false, true, "abc")], "abc", true),
            Plan::Keep(_)
        ));
        assert!(matches!(
            plan(&[pull(3, false, true, "abc")], "def", false),
            Plan::Keep(_)
        ));
    }

    #[test]
    fn an_open_or_missing_pull_request_leaves_the_branch() {
        let reopened = [pull(3, false, true, "abc"), pull(4, true, false, "def")];
        assert!(matches!(plan(&reopened, "abc", false), Plan::Leave(_)));
        assert!(matches!(plan(&[], "abc", false), Plan::Leave(_)));
        assert!(matches!(
            plan(&[pull(5, false, false, "abc")], "abc", false),
            Plan::Keep(_)
        ));
    }

    #[test]
    fn owner_and_name_come_from_any_github_url() {
        for url in [
            "https://github.com/o/r",
            "https://github.com/o/r.git",
            "git@github.com:o/r.git",
            "ssh://git@github.com/o/r",
            "http://proxy@127.0.0.1:1234/git/o/r",
        ] {
            assert_eq!(repo_from_url(url).as_deref(), Some("o/r"), "{url}");
        }
        assert_eq!(repo_from_url("git@github.com:r"), None);
    }

    #[test]
    fn a_prunable_worktree_is_marked() {
        let list = worktrees(
            "worktree /p\nHEAD a\nbranch refs/heads/main\n\n\
             worktree /w\nHEAD b\nbranch refs/heads/storm/x\nprunable gitdir file points to non-existent location\n",
        );
        assert_eq!(list.len(), 2);
        assert!(!list[0].prunable);
        assert!(list[1].prunable);
        assert_eq!(list[1].branch.as_deref(), Some("storm/x"));
    }

    #[test]
    fn paginated_pages_are_read_in_order() {
        let all = pages("[{\"number\":1}]\n[{\"number\":2}]").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1]["number"], 2);
    }

    #[test]
    fn a_query_value_keeps_branch_characters() {
        assert_eq!(encode("o:storm/a-b_c.d"), "o:storm/a-b_c.d");
        assert_eq!(encode("o:a+b c"), "o:a%2Bb%20c");
    }
}
