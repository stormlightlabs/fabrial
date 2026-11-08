use super::*;
use std::cell::RefCell;

use crate::depth::Depth;
use crate::sequence::Step;

/// A machine that records every command and answers from its state: how
/// many times the branch was pushed, whether the worktree has changes, and
/// the results each role's workers return, in order.
#[derive(Default)]
struct Fake {
    calls: RefCell<Vec<String>>,
    pushes: RefCell<usize>,
    dirty: RefCell<bool>,
    /// How many times a tracker command wrote the task file.
    writes: RefCell<usize>,
    /// Each role's results, first first; a result starting `push:` pushes.
    results: RefCell<Vec<(String, String)>>,
    /// Pull request list reads that come back empty after the first push.
    pr_delay: usize,
    pr_reads: RefCell<usize>,
    open_pr: bool,
    never_pr: bool,
    gates_fail: bool,
    /// The tracker refuses to start the task.
    start_fails: bool,
    /// The worktree's head is not origin's when the run finishes.
    stale: bool,
    /// The `check` job's `status conclusion` at the head; `None` is a pass.
    check: Option<&'static str>,
    /// When the plan is full; `None` is always, and otherwise it is standard.
    full_when: Option<fn(&Fake) -> bool>,
}

impl Fake {
    fn with(results: &[(&str, &str)]) -> Self {
        Fake {
            results: RefCell::new(results.iter().map(|(r, t)| (r.to_string(), t.to_string())).collect()),
            ..Fake::default()
        }
    }

    fn head(&self) -> String {
        format!("{:040}", *self.pushes.borrow() + 1)
    }

    fn called(&self, needle: &str) -> bool {
        self.calls.borrow().iter().any(|c| c.contains(needle))
    }

    /// Where the first call containing `needle` falls among all calls.
    fn order(&self, needle: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .position(|c| c.contains(needle))
            .unwrap_or_else(|| panic!("no call contains {needle}"))
    }

    fn roles(&self) -> Vec<String> {
        self.calls
            .borrow()
            .iter()
            .filter_map(|c| {
                c.strip_prefix("dispatch ")
                    .map(|r| r.split(' ').next().unwrap().to_string())
            })
            .collect()
    }
}

impl Host for Fake {
    fn run(&self, dir: &Path, program: &str, args: &[String]) -> Result<String> {
        // Every Git command turns hooks and fsmonitor off; the rest of the
        // fake reads the command after those settings.
        let args = if program == "git" {
            assert!(
                args.starts_with(&strings(GIT_HARDENING)),
                "git without hooks and fsmonitor off: {args:?}"
            );
            &args[GIT_HARDENING.len()..]
        } else {
            args
        };
        let line = format!("{program} {}", args.join(" "));
        self.calls.borrow_mut().push(line.clone());
        let arg = |n: usize| args.get(n).map(String::as_str).unwrap_or("");
        match program {
            "hostname" => Ok("box".into()),
            "gh" if line.starts_with("gh pr list") => {
                if self.open_pr {
                    return Ok("3".into());
                }
                if *self.pushes.borrow() == 0 || self.never_pr {
                    return Ok(String::new());
                }
                *self.pr_reads.borrow_mut() += 1;
                Ok(if *self.pr_reads.borrow() > self.pr_delay { "7".into() } else { String::new() })
            }
            "gh" if line.starts_with("gh pr view") && line.contains("statusCheckRollup") => Ok(format!(
                "{}\n{}",
                self.head(),
                self.check.unwrap_or("COMPLETED SUCCESS")
            )),
            "gh" if line.starts_with("gh pr view") => Ok("MERGEABLE".into()),
            "gh" if line.starts_with("gh issue view") => Ok("# Fix it\n\nThe body.".into()),
            "gh" | "fab" => Ok(String::new()),
            "git" => match arg(0) {
                "rev-parse" if arg(1) == "HEAD" && self.stale => Ok("f".repeat(40)),
                "rev-parse" => Ok(self.head()),
                "status" => Ok(if *self.dirty.borrow() { " M task.md\0".into() } else { String::new() }),
                "hash-object" => Ok(format!("hash-{}", self.writes.borrow())),
                "commit" => {
                    *self.dirty.borrow_mut() = false;
                    Ok(String::new())
                }
                "reset" => {
                    *self.writes.borrow_mut() -= 1;
                    Ok(String::new())
                }
                "push" | "pull" => panic!("fab run pushes and pulls only through dispatch: {line}"),
                "worktree" if arg(1) == "add" && arg(2) != "--detach" => {
                    fs::create_dir_all(arg(2)).unwrap();
                    Ok(String::new())
                }
                _ => Ok(String::new()),
            },
            "arcl" => {
                // The task is read before it has a worktree; every other
                // step runs in the worktree, where its file changes land.
                if arg(1) == "show" {
                    return Ok("Do the thing.".into());
                }
                assert!(
                    dir.ends_with("t-1"),
                    "tracker commands run in the task worktree: {}",
                    dir.display()
                );
                if arg(0) == "init" {
                    return Ok(String::new());
                }
                if arg(1) == "start" && self.start_fails {
                    return Err("arcl task start failed: not ready".into());
                }
                *self.dirty.borrow_mut() = true;
                *self.writes.borrow_mut() += 1;
                Ok(String::new())
            }
            "just" if self.gates_fail => Err("just check failed".into()),
            "just" => Ok(String::new()),
            other => Err(format!("unexpected program {other}")),
        }
    }

    fn dispatch(&self, role: &str, task: &Path, worktree: &Path) -> Result<String> {
        let text = fs::read_to_string(task).unwrap();
        self.calls.borrow_mut().push(format!(
            "dispatch {role} {} {}",
            worktree.display(),
            text.replace('\n', " ")
        ));
        let mut results = self.results.borrow_mut();
        let at = results
            .iter()
            .position(|(r, _)| r == role)
            .ok_or(format!("no result for {role}"))?;
        let (_, result) = results.remove(at);
        if let Some(rest) = result.strip_prefix("push:") {
            // The worker commits what it found changed, the task file too.
            *self.dirty.borrow_mut() = false;
            *self.pushes.borrow_mut() += 1;
            return Ok(rest.to_string());
        }
        Ok(result)
    }

    fn push(&self, worktree: &Path) -> Result<String> {
        self.calls.borrow_mut().push(format!("push {}", worktree.display()));
        *self.pushes.borrow_mut() += 1;
        Ok(self.head())
    }

    fn plan(&self, _head: &str) -> Result<Plan> {
        let step = |name: &str, role: &str, conditional| Step {
            name: name.into(),
            role: role.into(),
            conditional,
            posts: format!("{name} posts"),
        };
        let mut steps = vec![
            step("standard review", "reviewer", false),
            step("standard revise", "reviser", true),
        ];
        if self.full_when.is_none_or(|full| full(self)) {
            steps.push(step("adversarial review", "adversarial-reviewer", false));
            steps.push(step("adversarial revise", "reviser", true));
            return Ok(Plan { depth: Depth::Full, reason: "src/x.rs is code".into(), steps });
        }
        Ok(Plan { depth: Depth::Standard, reason: "docs only".into(), steps })
    }

    fn pause(&self) {}
}

fn settings(tracked: bool) -> Settings {
    let tasks = "[tasks]\ninit = \"arcl init\"\nshow = \"arcl task show {id}\"\nstart = \"arcl task start {id}\"\n\
                 complete = \"arcl task complete {id} --evidence {evidence}\"\n\
                 handoff = \"arcl task handoff {id} --note-file {file}\"\n";
    let text = format!(
        "owner = \"o\"\ntrailer = \"O <o@example.com>\"\ngates = \"just check\"\n{}",
        if tracked { tasks } else { "" }
    );
    Settings::parse(&text).unwrap()
}

fn root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir(&root).unwrap();
    (temp, root)
}

#[test]
fn a_tracked_task_runs_implement_each_stage_and_the_finish() {
    let (_temp, root) = root();
    let host = Fake {
        pr_delay: 2,
        ..Fake::with(&[
            ("implementer", "push:Implemented."),
            (
                "reviewer",
                "Standard pass on 0000002\n- medium · a.rs:1 · wrong\nAbove nit: yes",
            ),
            ("reviser", "push:Fixed in 0000003."),
            (
                "adversarial-reviewer",
                "Adversarial pass on 0000003\nNothing found.\nAbove nit: no",
            ),
        ])
    };
    let report = drive(&host, &root, settings(true), "t-1", false).unwrap();
    assert_eq!(
        host.roles(),
        ["implementer", "reviewer", "reviser", "adversarial-reviewer"]
    );
    assert!(host.called("fab claim take t-1 --run fab-run-"), "{:?}", host.calls);
    let worktree = root.parent().unwrap().join("repo-worktrees/t-1");
    assert!(host.called(&format!(
        "git worktree add {} -b storm/t-1 origin/storm/t-1",
        worktree.display()
    )));
    assert!(host.called("arcl task start t-1"));
    assert!(
        host.order("arcl init") < host.order("arcl task start t-1"),
        "the worktree is initialized before the task starts"
    );
    assert!(
        host.called("which holds the claim for run fab-run-"),
        "the implementer is told the run ID"
    );
    assert!(
        host.called("Post your review through the Bot comment workflow as this says: standard review posts"),
        "{:?}",
        host.calls
    );
    assert!(host.called("Then post your answers through the Bot comment workflow as this says: standard revise posts"));
    // Workers cannot push: dispatch pushes a writer's branch after it exits.
    assert!(host.called("dispatch implementer"));
    assert!(host.called("Commit your work and do not push: dispatch pushes storm/t-1 to origin after you exit"));
    assert!(host.called("commit to storm/t-1 without pushing: dispatch pushes it after you exit"));
    // Reviewers work in a detached worktree at the pinned head; revisers in
    // the task's.
    assert!(host.called("git worktree add --detach"));
    assert!(host.called(&format!("dispatch reviser {}", worktree.display())));
    assert!(host.called("Above nit: yes"), "the reviewer is told how to end");
    assert!(
        host.called("Standard pass on 0000002"),
        "later stages see earlier results"
    );
    assert!(
        host.called("arcl task complete t-1 --evidence Done in #7 at 0000000: fab run took it through full review")
    );
    assert!(host.called("git commit -m Complete the task -- task.md"));
    assert!(
        host.order("git commit -m Complete the task") < host.order(&format!("push {}", worktree.display())),
        "the completion leaves through dispatch's push"
    );
    assert!(!host.called("just"), "the gates do not run on the host by default");
    assert!(
        host.order("statusCheckRollup") < host.order("arcl task complete"),
        "the check job is read before the task is completed"
    );
    assert!(
        host.order("--json mergeable") < host.order("arcl task complete"),
        "the merge is checked before the task is completed"
    );
    let finished = host.head();
    assert!(
        host.called(&format!(
            "body=Review finished at {finished}\n\nDepth: full (src/x.rs is code). Gates: the check job at 0000000 passed."
        )),
        "{:?}",
        host.calls
    );
    assert!(
        report.contains("adversarial revise: skipped, nothing above nit"),
        "{report}"
    );
    assert!(
        report.starts_with(&format!("#7 ready for review at {finished}")),
        "{report}"
    );
    assert!(
        !host.called("branch-pr.yml"),
        "the pull request appeared within the reads"
    );
}

#[test]
fn an_implementer_that_commits_nothing_ends_in_a_handoff() {
    let (_temp, root) = root();
    let host = Fake::with(&[("implementer", "Blocked: the gates need the host.")]);
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(err.contains("the implementer committed nothing"), "{err}");
    assert!(host.called("arcl task handoff t-1 --note-file"));
    assert!(host.called("git commit -m Hand off the task -- task.md"));
    assert!(err.contains("Handoff recorded on t-1."), "{err}");
    assert_eq!(host.roles(), ["implementer"]);
}

#[test]
fn a_missing_pull_request_asks_branch_pr_once_then_stops() {
    let (_temp, root) = root();
    let host = Fake { never_pr: true, ..Fake::with(&[("implementer", "push:Done.")]) };
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(err.contains("no pull request opened for storm/t-1"), "{err}");
    assert_eq!(
        host.calls
            .borrow()
            .iter()
            .filter(|c| c.contains("branch-pr.yml"))
            .count(),
        1
    );
}

#[test]
fn a_task_that_already_has_a_pull_request_is_not_claimed() {
    let (_temp, root) = root();
    let host = Fake { open_pr: true, ..Fake::default() };
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(err.contains("#3 already has storm/t-1"), "{err}");
    assert!(!host.called("claim take"));
}

fn reviewed(host: Fake) -> Fake {
    Fake {
        results: RefCell::new(
            [
                ("implementer", "push:Done."),
                ("reviewer", "Above nit: no"),
                ("adversarial-reviewer", "Above nit: no"),
            ]
            .iter()
            .map(|(r, t)| (r.to_string(), t.to_string()))
            .collect(),
        ),
        ..host
    }
}

#[test]
fn failing_host_gates_stop_before_review_finished() {
    let (_temp, root) = root();
    let host = reviewed(Fake { gates_fail: true, ..Fake::default() });
    let err = drive(&host, &root, settings(true), "t-1", true).unwrap_err();
    assert!(err.contains("the gates failed"), "{err}");
    assert!(!host.called("Review finished at"));
    assert!(!host.called("statusCheckRollup"), "host gates replace the check read");
    assert!(
        !host.called("arcl task complete"),
        "a task whose gates fail stays started"
    );
}

#[test]
fn passing_host_gates_are_named_in_review_finished() {
    let (_temp, root) = root();
    let host = reviewed(Fake::default());
    drive(&host, &root, settings(true), "t-1", true).unwrap();
    assert!(host.order("just check") < host.order("arcl task complete"));
    assert!(host.called("Gates: `just check` passed on the host at 0000000."));
}

#[test]
fn a_failed_check_job_stops_before_completion() {
    let (_temp, root) = root();
    let host = reviewed(Fake { check: Some("COMPLETED FAILURE"), ..Fake::default() });
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(
        err.contains("the check job failed at 0000000: COMPLETED FAILURE"),
        "{err}"
    );
    assert!(!host.called("arcl task complete"));
    assert!(!host.called("Review finished at"));
}

#[test]
fn a_running_check_job_is_read_once_and_reported() {
    let (_temp, root) = root();
    let host = reviewed(Fake { check: Some("IN_PROGRESS "), ..Fake::default() });
    let report = drive(&host, &root, settings(true), "t-1", false).unwrap();
    assert!(report.contains("Gates: the check job at 0000000 running"), "{report}");
    assert_eq!(
        host.calls
            .borrow()
            .iter()
            .filter(|c| c.contains("statusCheckRollup"))
            .count(),
        1
    );
}

#[test]
fn stages_added_by_a_rise_in_depth_run_once() {
    let (_temp, root) = root();
    // Standard until the reviser's push, the second; full after it.
    let host = Fake {
        full_when: Some(|fake| *fake.pushes.borrow() >= 2),
        ..Fake::with(&[
            ("implementer", "push:Done."),
            ("reviewer", "Above nit: yes"),
            ("reviser", "push:Fixed, and touched code."),
            ("adversarial-reviewer", "Above nit: yes"),
            ("reviser", "push:Fixed again."),
        ])
    };
    let report = drive(&host, &root, settings(true), "t-1", false).unwrap();
    assert_eq!(
        host.roles(),
        ["implementer", "reviewer", "reviser", "adversarial-reviewer", "reviser"]
    );
    assert!(report.contains("depth rose to full (src/x.rs is code)"), "{report}");
    assert!(host.called("through full review"), "{:?}", host.calls);
}

#[test]
fn a_completion_that_raises_the_depth_is_undone_before_the_push() {
    let (_temp, root) = root();
    // The start is the first write; the completion, the second, raises it.
    let host = Fake {
        full_when: Some(|fake| *fake.writes.borrow() >= 2),
        ..Fake::with(&[("implementer", "push:Done."), ("reviewer", "Above nit: no")])
    };
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(err.contains("would raise the review depth"), "{err}");
    assert!(host.called("git reset --keep HEAD~1"));
    assert!(!host.called("Review finished at"));
    let calls = host.calls.borrow();
    let completed = host.order("arcl task complete");
    let pushed = completed + calls[completed..].iter().position(|c| c.starts_with("push ")).unwrap();
    assert!(
        host.order("git reset") < pushed,
        "only the handoff pushes, after the undo: {calls:?}"
    );
}

#[test]
fn a_worktree_off_origins_head_stops_before_completion() {
    let (_temp, root) = root();
    let host = reviewed(Fake { stale: true, ..Fake::default() });
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(
        err.contains("the worktree is at fffffff but origin has storm/t-1"),
        "{err}"
    );
    assert!(!host.called("arcl task complete"));
    assert!(!host.called("Review finished at"));
}

#[test]
fn porcelain_paths_keep_their_bytes_and_skip_rename_sources() {
    let status = " M plain.md\0?? with space \"quoted\".md\0R  new.md\0old.md\0 D gone.md\0";
    assert_eq!(
        changed_paths(status),
        ["plain.md", "with space \"quoted\".md", "new.md", "gone.md"]
    );
}

#[test]
fn a_failed_start_removes_the_worktree_and_branch_and_releases_the_claim() {
    let (_temp, root) = root();
    let host = Fake { start_fails: true, ..Fake::default() };
    let err = drive(&host, &root, settings(true), "t-1", false).unwrap_err();
    assert!(err.contains("not ready"), "{err}");
    assert!(err.contains("the worktree and branch were removed"), "{err}");
    let worktree = root.parent().unwrap().join("repo-worktrees/t-1");
    assert!(host.called(&format!("git worktree remove --force {}", worktree.display())));
    assert!(host.called("git branch -D storm/t-1"));
    assert!(host.called("fab claim release t-1 --run fab-run-"));
    assert!(host.roles().is_empty());
}

#[test]
fn an_issue_runs_without_task_commands() {
    let (_temp, root) = root();
    let host = Fake::with(&[
        ("implementer", "push:Done."),
        ("reviewer", "Above nit: no"),
        ("adversarial-reviewer", "Above nit: no"),
    ]);
    drive(&host, &root, settings(false), "#12", false).unwrap();
    assert!(host.called("fab claim take issue-12"));
    assert!(host.called("# Fix it"), "the issue's text reaches the worker");
    assert!(!host.called("arcl"));
    assert_eq!(host.roles(), ["implementer", "reviewer", "adversarial-reviewer"]);
}

#[test]
fn the_argument_names_its_source() {
    let tasks = settings(true).tasks;
    assert_eq!(Source::parse("#4", None).unwrap(), Source::Issue(4));
    assert_eq!(
        Source::parse("t-9", tasks.as_ref()).unwrap(),
        Source::Tracked("t-9".into())
    );
    let err = Source::parse("t-9", None).unwrap_err();
    assert!(err.contains("no [tasks] commands"), "{err}");
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("Fix the Login page.md");
    fs::write(&file, "x").unwrap();
    let source = Source::parse(file.to_str().unwrap(), None).unwrap();
    assert_eq!(source.name(), "fix-the-login-page");
    assert_eq!(Source::Issue(4).name(), "issue-4");
}

#[test]
fn a_review_without_the_line_counts_as_finding_something() {
    assert!(above_nit("- high · x"));
    assert!(above_nit("Above nit: yes"));
    assert!(!above_nit("Nothing.\nAbove nit: no\n"));
    assert!(!above_nit("Above nit: NO"));
}

#[test]
fn the_worker_result_is_the_last_result_event() {
    let out = "{\"type\":\"progress\"}\n{\"type\":\"result\",\"text\":\"done\"}\n";
    assert_eq!(worker_result(out, true).unwrap(), "done");
    let failed = "{\"type\":\"error\",\"message\":\"token refused\"}\n";
    assert_eq!(worker_result(failed, false).unwrap_err(), "token refused");
    assert_eq!(worker_result(out, false).unwrap_err(), "no result");
}
