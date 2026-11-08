//! `fab run`: take one task through the fixed sequence in code. It claims the
//! task, gives it a worktree, dispatches one Pi worker per step through
//! `fab dispatch` (the implementer, then each stage `fab sequence` gives),
//! and ends with `Review finished at <sha>`, or a handoff when a step fails.
//!
//! The task comes from the repository's task source: the `[tasks]` commands
//! in `.github/fabrial.toml`, run on the host, or else a GitHub issue `#<n>`
//! or a description file. Nothing here names a tracker.
//!
//! The run never waits on a GitHub event. Where it needs GitHub to have done
//! something, such as Branch PR opening the pull request or GitHub settling
//! whether it merges, it reads a bounded number of times and then stops.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use clap::Args;
use fab_core::claims;
use fab_core::settings::{Settings, Tasks};
use serde_json::Value;

use crate::depth;
use crate::run::Result;
use crate::sequence::{self, Plan};

/// How many times a run reads GitHub for a result before it stops.
const READS: usize = 10;
/// The pause between two reads.
const PAUSE: Duration = Duration::from_secs(6);
/// Minutes a run's claim lasts; a run takes longer than the default hour.
const CLAIM_MINUTES: &str = "240";
/// The line a review worker ends with, so the run knows whether the revise
/// after it runs.
const ABOVE_NIT: &str = "Above nit:";

/// What `fab run` takes.
#[derive(Args)]
pub struct Options {
    /// A task ID for the `[tasks]` commands, an issue as `#<n>`, or a file
    /// holding a description.
    task: String,
    /// The trusted checkout `fab dispatch` reads roles and skills from.
    #[arg(long)]
    trusted_checkout: PathBuf,
    /// The host's dispatch configuration.
    #[arg(long)]
    config: PathBuf,
    /// Seconds each worker may run.
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..=3000))]
    timeout: u64,
}

/// Where a task comes from.
#[derive(Debug, PartialEq)]
enum Source {
    /// A task the `[tasks]` commands reach, by ID.
    Tracked(String),
    /// A GitHub issue.
    Issue(u64),
    /// A file holding a description.
    Description(PathBuf),
}

impl Source {
    /// Reads the argument: `#<n>` is an issue, an existing file a
    /// description, and anything else an ID, which needs `[tasks]`.
    fn parse(task: &str, tasks: Option<&Tasks>) -> Result<Self> {
        if let Some(n) = task.strip_prefix('#').and_then(|n| n.parse().ok()) {
            return Ok(Source::Issue(n));
        }
        if Path::new(task).is_file() {
            return Ok(Source::Description(task.into()));
        }
        if tasks.is_none() {
            return Err(format!(
                "{task} is not an issue (#<n>) or a file, and .github/fabrial.toml has no [tasks] commands to read it with."
            ));
        }
        claims::branch(task).map_err(|e| e.to_string())?;
        Ok(Source::Tracked(task.into()))
    }

    /// The name the task's branch and worktree take.
    fn name(&self) -> String {
        match self {
            Source::Tracked(id) => id.clone(),
            Source::Issue(n) => format!("issue-{n}"),
            Source::Description(path) => {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
                let slug: String = stem
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect();
                let slug = slug.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
                if slug.is_empty() { "task".into() } else { slug }
            }
        }
    }
}

/// What the run needs from the machine. [`System`] runs real commands; tests
/// put a fake in its place.
pub trait Host {
    /// Runs a program in `dir` and returns its standard output. `fab` names
    /// this binary.
    fn run(&self, dir: &Path, program: &str, args: &[String]) -> Result<String>;
    /// Runs one worker to its end and returns its final message.
    fn dispatch(&self, role: &str, task: &Path, worktree: &Path) -> Result<String>;
    /// The sequence for the diff from `origin/main` to `head`.
    fn plan(&self, head: &str) -> Result<Plan>;
    /// Waits between two reads of GitHub.
    fn pause(&self);
}

/// The real machine.
struct System {
    trusted_checkout: PathBuf,
    config: PathBuf,
    timeout: u64,
}

impl Host for System {
    fn run(&self, dir: &Path, program: &str, args: &[String]) -> Result<String> {
        let program = if program == "fab" {
            std::env::current_exe().map_err(|e| format!("cannot find fab: {e}"))?
        } else {
            program.into()
        };
        let output = Command::new(&program)
            .current_dir(dir)
            .args(args)
            .output()
            .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
        if !output.status.success() {
            let what = args.iter().take(4).cloned().collect::<Vec<_>>().join(" ");
            return Err(format!(
                "{} {what} failed: {}{}",
                program.display(),
                String::from_utf8_lossy(&output.stderr).trim(),
                String::from_utf8_lossy(&output.stdout).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
    }

    fn dispatch(&self, role: &str, task: &Path, worktree: &Path) -> Result<String> {
        let fab = std::env::current_exe().map_err(|e| format!("cannot find fab: {e}"))?;
        let output = Command::new(fab)
            .arg("dispatch")
            .arg(role)
            .arg("--task")
            .arg(task)
            .arg("--worktree")
            .arg(worktree)
            .arg("--trusted-checkout")
            .arg(&self.trusted_checkout)
            .arg("--config")
            .arg(&self.config)
            .arg("--timeout")
            .arg(self.timeout.to_string())
            .output()
            .map_err(|e| format!("cannot run fab dispatch: {e}"))?;
        worker_result(&String::from_utf8_lossy(&output.stdout), output.status.success())
            .map_err(|e| format!("the {role} worker failed: {e}"))
    }

    fn plan(&self, head: &str) -> Result<Plan> {
        sequence::plan(&depth::Options { base: "origin/main".into(), head: head.into() })
    }

    fn pause(&self) {
        std::thread::sleep(PAUSE);
    }
}

/// One run of one task.
struct Run<'a, H: Host> {
    host: &'a H,
    /// The primary checkout the run started in.
    root: PathBuf,
    settings: Settings,
    source: Source,
    /// The task's text, as the worker reads it.
    text: String,
    run_id: String,
    branch: String,
    worktree: PathBuf,
    /// A private directory for the task files the workers read.
    scratch: PathBuf,
}

/// Takes the task through the sequence, from the repository root.
pub fn run(options: Options) -> Result<()> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let text = fs::read_to_string(root.join(fab_core::settings::SETTINGS_PATH))
        .map_err(|e| format!("cannot read .github/fabrial.toml: {e}"))?;
    let settings = Settings::parse(&text).map_err(|e| e.to_string())?;
    let host = System { trusted_checkout: options.trusted_checkout, config: options.config, timeout: options.timeout };
    let report = drive(&host, &root, settings, &options.task)?;
    println!("{report}");
    Ok(())
}

/// Runs the whole sequence for one task and returns the report.
fn drive<H: Host>(host: &H, root: &Path, settings: Settings, task: &str) -> Result<String> {
    let source = Source::parse(task, settings.tasks.as_ref())?;
    let name = source.name();
    let branch = claims::branch(&name).map_err(|e| e.to_string())?;
    let machine = host.run(root, "hostname", &[]).unwrap_or_else(|_| "host".into());
    let run_id = format!("fab-run-{}@{machine}", claims::now());
    let repo = root.file_name().unwrap_or_default().to_string_lossy().to_string();
    let worktree = root
        .parent()
        .unwrap_or(root)
        .join(format!("{repo}-worktrees"))
        .join(&name);
    let scratch = std::env::temp_dir().join(format!("fab-run-{name}-{}", uuid::Uuid::new_v4()));
    let mut run =
        Run { host, root: root.into(), settings, source, text: String::new(), run_id, branch, worktree, scratch };
    run.text = run.read_task()?;
    run.start()?;
    let outcome = run.sequence();
    let _ = fs::remove_dir_all(&run.scratch);
    outcome
}

impl<H: Host> Run<'_, H> {
    fn git(&self, dir: &Path, args: &[&str]) -> Result<String> {
        self.host.run(dir, "git", &strings(args))
    }

    fn gh(&self, args: &[&str]) -> Result<String> {
        self.host.run(&self.root, "gh", &strings(args))
    }

    /// Runs one `[tasks]` command in the task's worktree.
    fn task_command(&self, step: &str, values: &[(&str, &str)]) -> Result<String> {
        let tasks = self.settings.tasks.as_ref().ok_or("no [tasks] commands")?;
        let command = match step {
            "init" => tasks.init.as_deref().unwrap_or_default(),
            "show" => &tasks.show,
            "start" => &tasks.start,
            "complete" => &tasks.complete,
            _ => &tasks.handoff,
        };
        let argv = Tasks::argv(command, values);
        if argv.is_empty() {
            return Ok(String::new());
        }
        let dir = if self.worktree.exists() { &self.worktree } else { &self.root };
        self.host.run(dir, &argv[0], &argv[1..])
    }

    fn read_task(&self) -> Result<String> {
        match &self.source {
            Source::Tracked(id) => self.task_command("show", &[("{id}", id)]),
            Source::Issue(n) => self.gh(&[
                "issue",
                "view",
                &n.to_string(),
                "--json",
                "title,body",
                "--jq",
                "\"# \" + .title + \"\\n\\n\" + .body",
            ]),
            Source::Description(path) => {
                fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
            }
        }
    }

    /// Checks no pull request has the task, claims it, makes its worktree
    /// and starts it in the tracker. A failure removes the worktree and the
    /// local branch it made and releases the claim, so a rerun starts clean.
    fn start(&self) -> Result<()> {
        let open = self.gh(&[
            "pr",
            "list",
            "--head",
            &self.branch,
            "--state",
            "open",
            "--json",
            "number",
            "--jq",
            ".[].number",
        ])?;
        if !open.trim().is_empty() {
            return Err(format!(
                "pull request #{} already has {}; fab run starts only new tasks.",
                open.trim(),
                self.branch
            ));
        }
        let name = self.source.name();
        self.host.run(
            &self.root,
            "fab",
            &strings(&["claim", "take", &name, "--run", &self.run_id, "--ttl", CLAIM_MINUTES]),
        )?;
        let path = self.worktree.to_string_lossy().to_string();
        let added = std::cell::Cell::new(false);
        let made = (|| {
            self.git(&self.root, &["fetch", "origin", &self.branch])?;
            self.git(
                &self.root,
                &[
                    "worktree",
                    "add",
                    &path,
                    "-b",
                    &self.branch,
                    &format!("origin/{}", self.branch),
                ],
            )?;
            added.set(true);
            self.git(
                &self.worktree,
                &["branch", "--set-upstream-to", &format!("origin/{}", self.branch)],
            )?;
            if let Source::Tracked(id) = &self.source {
                self.task_command("init", &[])?;
                self.task_command("start", &[("{id}", id)])?;
            }
            fs::create_dir_all(&self.scratch).map_err(|e| format!("cannot make {}: {e}", self.scratch.display()))
        })();
        if let Err(error) = made {
            if added.get() {
                let _ = self.git(&self.root, &["worktree", "remove", "--force", &path]);
                let _ = self.git(&self.root, &["branch", "-D", &self.branch]);
            }
            let _ = self.host.run(
                &self.root,
                "fab",
                &strings(&["claim", "release", &name, "--run", &self.run_id]),
            );
            let undone = if added.get() { "the worktree and branch were removed and " } else { "" };
            return Err(format!("{error}; {undone}the claim was released"));
        }
        Ok(())
    }

    /// The head of the task's branch on origin, fetched.
    fn head(&self) -> Result<String> {
        self.git(&self.root, &["fetch", "origin", &self.branch])?;
        self.git(&self.root, &["rev-parse", &format!("origin/{}", self.branch)])
    }

    /// Implement, then every stage, then the finish. A failure after the
    /// claim records a handoff.
    fn sequence(&self) -> Result<String> {
        let mut log = Vec::new();
        let result = self.stages(&mut log);
        match result {
            Ok(report) => Ok(report),
            Err(error) => {
                log.push(format!("Stopped: {error}"));
                let note = self.handoff(&log);
                Err(format!("{error}\n{note}"))
            }
        }
    }

    fn stages(&self, log: &mut Vec<String>) -> Result<String> {
        let claimed = self.head()?;
        let result = self.dispatch("implementer", &self.implement_task(), &self.worktree)?;
        log.push(format!("implement: {}", first_line(&result)));
        let mut head = self.head()?;
        if head == claimed {
            return Err("the implementer pushed nothing".into());
        }
        let pr = self.pull_request()?;
        log.push(format!("pull request #{pr} at {}", short(&head)));
        let mut plan = self.host.plan(&format!("origin/{}", self.branch))?;
        let mut done: Vec<String> = Vec::new();
        let mut found = true;
        let mut notes: Vec<String> = Vec::new();
        while let Some(step) = plan.steps.iter().find(|s| !done.contains(&s.name)) {
            done.push(step.name.clone());
            if step.conditional && !found {
                log.push(format!("{}: skipped, nothing above nit", step.name));
                continue;
            }
            let task = self.stage_task(pr, &head, step, &plan, &notes);
            let result = if step.role == "reviser" {
                self.dispatch(&step.role, &task, &self.worktree)?
            } else {
                self.review(&step.role, &task, &head)?
            };
            found = above_nit(&result);
            notes.push(format!("## {} at {}\n\n{}", step.name, short(&head), result.trim()));
            log.push(format!("{}: {}", step.name, first_line(&result)));
            let now = self.head()?;
            if now != head {
                head = now;
                let next = self.host.plan(&format!("origin/{}", self.branch))?;
                if next.depth > plan.depth {
                    log.push(format!("depth rose to {} ({})", next.depth.name(), next.reason));
                    plan = next;
                }
            }
        }
        self.finish(pr, &plan, log)
    }

    /// Runs the gates, then completes a tracked task, pushes, checks the
    /// pull request merges and posts `Review finished`. Failing gates stop
    /// the run with the task still started.
    fn finish(&self, pr: u64, plan: &Plan, log: &mut Vec<String>) -> Result<String> {
        self.git(&self.worktree, &["pull", "--ff-only"])?;
        let gates: Vec<String> = self.settings.gates.split_whitespace().map(String::from).collect();
        self.host
            .run(&self.worktree, &gates[0], &gates[1..])
            .map_err(|e| format!("the gates failed: {e}"))?;
        if let Source::Tracked(id) = &self.source {
            let head = self.git(&self.worktree, &["rev-parse", "HEAD"])?;
            let evidence = format!(
                "Done in #{pr} at {}: fab run took it through {} review",
                short(&head),
                plan.depth.name()
            );
            self.record(
                "complete",
                &[("{id}", id), ("{evidence}", &evidence)],
                "Complete the task",
            )?;
        }
        self.git(&self.worktree, &["push", "origin", &self.branch])?;
        let head = self.head()?;
        if self.host.plan(&format!("origin/{}", self.branch))?.depth > plan.depth {
            return Err("completing the task raised the review depth; run the added stages by hand".into());
        }
        self.mergeable(pr)?;
        let body = format!(
            "Review finished at {head}\n\nDepth: {} ({}). Run by fab run.",
            plan.depth.name(),
            plan.reason
        );
        self.gh(&[
            "workflow",
            "run",
            "comment.yml",
            "-f",
            &format!("pr={pr}"),
            "-f",
            &format!("body={body}"),
        ])?;
        log.push(format!("Review finished at {}", short(&head)));
        Ok(format!("#{pr} ready for review at {head}\n{}", log.join("\n")))
    }

    /// Runs a `[tasks]` step that changes the task, then commits the paths it
    /// changed, and only those, so a worker's leftovers stay out. A path
    /// counts as changed when it is newly dirty or its content moved.
    fn record(&self, step: &str, values: &[(&str, &str)], message: &str) -> Result<()> {
        let before = self.dirty()?;
        self.task_command(step, values)?;
        let paths: Vec<String> = self
            .dirty()?
            .into_iter()
            .filter(|now| !before.contains(now))
            .map(|(path, _)| path)
            .collect();
        if paths.is_empty() {
            return Ok(());
        }
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let mut add = vec!["add", "--"];
        add.extend(&paths);
        self.git(&self.worktree, &add)?;
        let mut commit = vec!["commit", "-m", message, "--"];
        commit.extend(&paths);
        self.git(&self.worktree, &commit).map(drop)
    }

    /// The worktree's changed paths, each with its content's hash, or an
    /// empty hash when it is gone.
    fn dirty(&self) -> Result<Vec<(String, String)>> {
        let status = self.git(&self.worktree, &["status", "--porcelain"])?;
        Ok(status
            .lines()
            .filter_map(|l| l.get(3..))
            .map(|path| {
                let hash = self
                    .git(&self.worktree, &["hash-object", "--", path])
                    .unwrap_or_default();
                (path.to_string(), hash)
            })
            .collect())
    }

    /// Runs a review role in its own detached worktree at the pinned head.
    fn review(&self, role: &str, task: &str, head: &str) -> Result<String> {
        let path = self
            .worktree
            .with_file_name(format!("{}-{role}-{}", self.source.name(), short(head)));
        let text = path.to_string_lossy().to_string();
        self.git(&self.root, &["worktree", "add", "--detach", &text, head])?;
        let result = self.dispatch(role, task, &path);
        let _ = self.git(&self.root, &["worktree", "remove", "--force", &text]);
        result
    }

    fn dispatch(&self, role: &str, text: &str, worktree: &Path) -> Result<String> {
        let file = self.scratch.join(format!("{role}-{}.txt", claims::now()));
        fs::write(&file, text).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        self.host.dispatch(role, &file, worktree)
    }

    /// The pull request Branch PR opened for the branch, read a bounded
    /// number of times. Halfway, the run asks Branch PR to open it once.
    fn pull_request(&self) -> Result<u64> {
        for n in 0..READS {
            let number = self.gh(&[
                "pr",
                "list",
                "--head",
                &self.branch,
                "--state",
                "open",
                "--json",
                "number",
                "--jq",
                ".[].number",
            ])?;
            if let Ok(number) = number.trim().parse() {
                return Ok(number);
            }
            if n == READS / 2 {
                self.gh(&["workflow", "run", "branch-pr.yml", "--ref", &self.branch])?;
            }
            self.host.pause();
        }
        Err(format!(
            "no pull request opened for {} after {READS} reads",
            self.branch
        ))
    }

    /// Reads whether the pull request merges until GitHub knows, a bounded
    /// number of times.
    fn mergeable(&self, pr: u64) -> Result<()> {
        for _ in 0..READS {
            match self
                .gh(&[
                    "pr",
                    "view",
                    &pr.to_string(),
                    "--json",
                    "mergeable",
                    "--jq",
                    ".mergeable",
                ])?
                .trim()
            {
                "MERGEABLE" => return Ok(()),
                "CONFLICTING" => {
                    return Err(format!(
                        "#{pr} conflicts with main; resolve it, then run the review again"
                    ));
                }
                _ => self.host.pause(),
            }
        }
        Err(format!("GitHub did not say whether #{pr} merges after {READS} reads"))
    }

    /// Records where the run stopped: a tracker handoff, committed and
    /// pushed, or the note itself when there is no tracker.
    fn handoff(&self, log: &[String]) -> String {
        let note = format!("fab run {} stopped.\n\n{}\n", self.run_id, log.join("\n"));
        let Source::Tracked(id) = &self.source else {
            return note;
        };
        let file = self.scratch.join("handoff.md");
        let recorded = fs::write(&file, &note)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                self.record(
                    "handoff",
                    &[("{id}", id), ("{file}", &file.to_string_lossy())],
                    "Hand off the task",
                )
            })
            .and_then(|_| self.git(&self.worktree, &["push", "origin", &self.branch]).map(drop));
        match recorded {
            Ok(()) => format!("Handoff recorded on {id}."),
            Err(e) => format!("The handoff could not be recorded ({e}):\n{note}"),
        }
    }

    fn implement_task(&self) -> String {
        let tracked = match &self.source {
            Source::Tracked(id) => format!(
                "\nThe task is {id}. The run started it in the tracker and completes it after review; do neither. \
                 Commit the task file the start changed with your work.\n"
            ),
            _ => String::new(),
        };
        format!(
            "Implement this task in the worktree {}, on the branch {}, which holds the claim for run {}. \
             Push your work to origin {}; Branch PR opens the pull request. Post nothing.\n{tracked}\n{}\n",
            self.worktree.display(),
            self.branch,
            self.run_id,
            self.branch,
            self.text.trim()
        )
    }

    fn stage_task(&self, pr: u64, head: &str, step: &sequence::Step, plan: &Plan, notes: &[String]) -> String {
        let ask = if step.role == "reviser" {
            format!(
                "Address the findings of the review before you on pull request #{pr}, in the worktree {}, and push to {}. \
                 Then post your answers through the Bot comment workflow as this says: {}",
                self.worktree.display(),
                self.branch,
                step.posts
            )
        } else {
            format!(
                "Run the {} on pull request #{pr} at the pinned head {head}, in the worktree you start in. \
                 Post your review through the Bot comment workflow as this says: {} \
                 End your final message with a line `{ABOVE_NIT} yes` when you found anything above nit, else `{ABOVE_NIT} no`.",
                step.name, step.posts
            )
        };
        format!(
            "{ask}\nDepth: {} ({}).\n\n# Task\n\n{}\n\n# Earlier stages\n\n{}\n",
            plan.depth.name(),
            plan.reason,
            self.text.trim(),
            if notes.is_empty() { "None.".to_string() } else { notes.join("\n\n") }
        )
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn first_line(text: &str) -> &str {
    text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
}

/// Whether a review found anything above nit, from its last `Above nit:`
/// line. A review without one counts as finding something, so its revise
/// runs and reads the findings itself.
fn above_nit(result: &str) -> bool {
    result
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(ABOVE_NIT))
        .is_none_or(|v| !v.trim().eq_ignore_ascii_case("no"))
}

/// The final message from `fab dispatch`'s events, one JSON object a line.
fn worker_result(stdout: &str, success: bool) -> Result<String> {
    let events: Vec<Value> = stdout.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let text = events
        .iter()
        .rev()
        .find(|e| e["type"] == "result")
        .and_then(|e| e["text"].as_str());
    match (success, text) {
        (true, Some(text)) => Ok(text.to_string()),
        _ => {
            let error = events
                .iter()
                .rev()
                .find(|e| e["type"] == "error")
                .and_then(|e| e["message"].as_str())
                .unwrap_or("no result");
            Err(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests;
