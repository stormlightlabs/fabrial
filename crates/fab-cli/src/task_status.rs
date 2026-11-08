//! `fab status`: the tasks runs hold. Each claimed task is a `storm/<task>`
//! branch on the remote ([`fab_core::claims`]); its claim commit names the
//! holder, run and expiry. One paginated `gh api` call gives the open pull
//! request on each branch, and with `--config` dispatch's worker records
//! give the step each task is at.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use fab_core::claims::{self, Listed, Store, utc};
use serde_json::{Value, json};

use crate::claim::NOW_VAR;
use crate::cleanup;
use crate::run::{self, Result};

/// What `fab status` reads.
#[derive(Args)]
pub struct Options {
    /// The remote that holds the tasks' branches.
    #[arg(long, default_value = "origin")]
    remote: String,
    /// The GitHub repository, as owner/name; read from the remote's URL by
    /// default.
    #[arg(long)]
    repo: Option<String>,
    /// The host's dispatch configuration, to add each task's latest worker
    /// role and state from the records under its `state_dir`.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Print one JSON object per task, one per line.
    #[arg(long)]
    json: bool,
}

/// An open pull request on a task's branch.
struct Pull {
    number: u64,
    url: String,
    draft: bool,
}

/// The latest worker dispatched for a task.
struct Step {
    worker_id: String,
    role: String,
    state: String,
}

/// Runs `fab status` in the current repository. Exits 2 when git, `gh` or
/// the worker records cannot be read.
pub fn run(options: Options) -> ExitCode {
    match status(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fab: {e}");
            ExitCode::from(2)
        }
    }
}

fn status(options: &Options) -> Result<()> {
    let now = match std::env::var(NOW_VAR) {
        Ok(v) => v
            .parse()
            .map_err(|_| format!("{NOW_VAR}={v} is not a number of seconds"))?,
        Err(_) => claims::now(),
    };
    let listed = Store::new(".", &options.remote).list().map_err(|e| e.to_string())?;
    let repo = match (&options.repo, listed.is_empty()) {
        (_, true) => String::new(),
        (Some(repo), false) => repo.clone(),
        (None, false) => cleanup::repo_from_url(&run::git(Path::new("."), &["remote", "get-url", &options.remote])?)
            .ok_or_else(|| format!("cannot read owner/name from {}'s URL; pass --repo", options.remote))?,
    };
    let pulls = if listed.is_empty() { Vec::new() } else { pulls(&repo)? };
    let workers = match &options.config {
        Some(config) => Some(workers(config)?),
        None => None,
    };
    if listed.is_empty() && !options.json {
        println!("no claimed tasks on {}", options.remote);
    }
    for task in &listed {
        let pull = pulls.iter().find(|(b, _)| *b == task.branch).map(|(_, p)| p);
        let step = workers.as_ref().map(|w| step(w, &repo, &task.task));
        if options.json {
            println!("{}", object(task, pull, step.as_ref(), now));
        } else {
            print!("{}", text(task, pull, step.as_ref(), now));
        }
    }
    Ok(())
}

/// The open pull requests from the repository's own branches, by head
/// branch, read with one paginated `gh api` call.
fn pulls(repo: &str) -> Result<Vec<(String, Pull)>> {
    let list = run::gh(&[
        "api",
        "--paginate",
        &format!("repos/{repo}/pulls?state=open&per_page=100"),
    ])?;
    Ok(cleanup::pages(&list)?
        .iter()
        .filter(|p| {
            p["head"]["repo"]["full_name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(repo))
        })
        .filter_map(|p| {
            let branch = p["head"]["ref"].as_str()?.to_string();
            let pull = Pull {
                number: p["number"].as_u64()?,
                url: p["html_url"].as_str().unwrap_or_default().to_string(),
                draft: p["draft"] == true,
            };
            Some((branch, pull))
        })
        .collect())
}

#[cfg(unix)]
fn workers(config: &Path) -> Result<Vec<Value>> {
    crate::dispatch::operations::workers(config).map_err(|e| format!("{}: {e}", config.display()))
}

#[cfg(not(unix))]
fn workers(_: &Path) -> Result<Vec<Value>> {
    Err("--config reads dispatch's worker records, which only Unix hosts keep".into())
}

/// The latest worker for `repo` whose worktree is the task's: `fab run`
/// gives the task worktree the task's name, and each review worktree
/// `<task>-<role>-<short head>`. Hosts keep one `state_dir` for every
/// repository, so a worker from another repository with a same-named task
/// is skipped.
fn step(workers: &[Value], repo: &str, task: &str) -> Option<Step> {
    workers
        .iter()
        .filter(|w| w["repository"].as_str().is_some_and(|r| r.eq_ignore_ascii_case(repo)))
        .filter(|w| {
            let role = w["role"].as_str().unwrap_or_default();
            let name = Path::new(w["worktree"].as_str().unwrap_or_default())
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            name == task
                || name
                    .strip_prefix(&format!("{task}-{role}-"))
                    .is_some_and(|head| !head.is_empty() && head.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        .max_by_key(|w| w["created_at"].as_u64().unwrap_or_default())
        .map(|w| Step {
            worker_id: w["worker_id"].as_str().unwrap_or_default().to_string(),
            role: w["role"].as_str().unwrap_or_default().to_string(),
            state: w["state"].as_str().unwrap_or_default().to_string(),
        })
}

/// One task as a JSON object. `step` is present only with `--config`, and
/// null there when no worker ran for the task.
fn object(task: &Listed, pull: Option<&Pull>, step: Option<&Option<Step>>, now: u64) -> Value {
    let claim = task.holder.as_ref().map(|c| {
        json!({
            "machine": c.machine,
            "run": c.run,
            "expires_at": c.expires_at,
            "expires": utc(c.expires_at),
            "live": c.is_live(now),
        })
    });
    let pull = pull.map(|p| json!({"number": p.number, "url": p.url, "draft": p.draft}));
    let mut value = json!({
        "task": task.task,
        "branch": task.branch,
        "tip": task.tip,
        "claim": claim,
        "work": task.work,
        "pull_request": pull,
    });
    if let Some(step) = step {
        value["step"] = match step {
            Some(s) => json!({"worker_id": s.worker_id, "role": s.role, "state": s.state}),
            None => Value::Null,
        };
    }
    value
}

/// One task as lines of text: the task and its claim, then the pull request
/// and the step when there are any.
fn text(task: &Listed, pull: Option<&Pull>, step: Option<&Option<Step>>, now: u64) -> String {
    let mut out = match &task.holder {
        Some(c) => {
            let state = if c.is_live(now) { "live until" } else { "lapsed at" };
            format!(
                "{}: {state} {}, run {} on {}\n",
                task.task,
                utc(c.expires_at),
                c.run,
                c.machine
            )
        }
        None => format!("{}: {} holds no claim\n", task.task, task.branch),
    };
    match pull {
        Some(p) => {
            let draft = if p.draft { "draft " } else { "" };
            out.push_str(&format!("  {draft}pull request #{} {}\n", p.number, p.url));
        }
        None if task.work => out.push_str("  work on the branch, no open pull request\n"),
        None => {}
    }
    match step {
        Some(Some(s)) => out.push_str(&format!("  step: {} {} (worker {})\n", s.role, s.state, s.worker_id)),
        Some(None) => out.push_str("  step: no worker recorded\n"),
        None => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(id: &str, role: &str, worktree: &str, created_at: u64) -> Value {
        json!({
            "worker_id": id, "role": role, "state": "running", "repository": "o/r",
            "worktree": worktree, "created_at": created_at,
        })
    }

    #[test]
    fn the_step_is_the_latest_worker_in_the_task_or_its_review_worktrees() {
        let workers = [
            worker("1", "implementer", "/w/r-worktrees/t1", 10),
            worker("2", "reviewer", "/w/r-worktrees/t1-reviewer-abc1234", 20),
            worker("3", "reviewer", "/w/r-worktrees/t1-2-reviewer-abc1234", 30),
            worker("4", "implementer", "/w/r-worktrees/t1-implementer-notahex", 40),
        ];
        let step = step(&workers, "o/r", "t1").unwrap();
        assert_eq!((step.worker_id.as_str(), step.role.as_str()), ("2", "reviewer"));
        assert!(super::step(&workers, "o/r", "t3").is_none());
    }

    #[test]
    fn the_step_skips_workers_from_another_repository() {
        let mut other = worker("2", "reviewer", "/w/q-worktrees/t1", 20);
        other["repository"] = json!("o/q");
        let workers = [worker("1", "implementer", "/w/r-worktrees/t1", 10), other];
        assert_eq!(step(&workers, "O/R", "t1").unwrap().worker_id, "1");
        assert!(step(&workers, "o/s", "t1").is_none());
    }
}
