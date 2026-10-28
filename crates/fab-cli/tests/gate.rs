//! Runs `fab gate` against a fake `gh` that logs every call and answers
//! `pr view` with a pull request the test sets, plus the comments the run
//! has posted as the bot.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BOT: &str = "stormlight-thunderbot[bot]";

/// The fake `gh`. It fails `pr <TEST_FAIL>`, and with `TEST_FAIL=comment`
/// also a write to a comment. It keeps the pull request's comments, in the
/// REST API's shape, in the file `TEST_COMMENTS`: a POST adds a comment from
/// the bot, a PATCH edits one, and a read lists them. It answers `pr view`
/// with `TEST_PR`, or `TEST_PR_LATER` once the run has written a comment; a
/// collaborator's permission with `TEST_PERMISSION`; and a commit's check
/// runs with `TEST_CHECK_RUNS`. It applies `--jq` as gh does and accepts the
/// other writes the gate makes.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -u
body= filter=
for ((i = 1; i < $#; i++)); do
  j=$((i + 1))
  # The x keeps trailing newlines, which command substitution strips.
  if [[ ${!i} == --body-file ]]; then body=$(cat "${!j}"; echo x); body=${body%x}; fi
  if [[ ${!i} == -F && ${!j} == body=@* ]]; then f=${!j#body=@}; body=$(cat "$f"; echo x); body=${body%x}; fi
  if [[ ${!i} == --jq ]]; then filter=${!j}; fi
done
printf '%s\0' "$@" | jq -Rsc --arg body "$body" '{args: split("\u0000")[:-1], body: $body}' >> "$TEST_GH_LOG"
reply() { if [[ -n $filter ]]; then jq -r "$filter" <<<"$1"; else printf '%s\n' "$1"; fi; }
if [[ $1 == pr && ${2:-} == "${TEST_FAIL:-}" ]]; then exit 1; fi
if [[ $1 == api && ${2:-} == -X && ${4:-} == */comments* && ${TEST_FAIL:-} == comment ]]; then exit 1; fi
case "$1 ${2:-}" in
  "pr view")
    view=$TEST_PR
    wrote=$(jq -s 'any(.[]; .args[0] == "api" and .args[1] == "-X" and (.args[3] | test("/comments")))' "$TEST_GH_LOG")
    if [[ $wrote == true && -n ${TEST_PR_LATER:-} ]]; then view=$TEST_PR_LATER; fi
    printf '%s\n' "$view" ;;
  "api -X")
    # TEST_LOSE_WRITE loses the first write to a comment, as a writer racing
    # this one would.
    if [[ -n ${TEST_LOSE_WRITE:-} && $4 == */comments* && ! -e $TEST_COMMENTS.lost ]]; then
      touch "$TEST_COMMENTS.lost"; reply '{"id": 0}'; exit 0
    fi
    case "$4" in
      */issues/*/comments)
        id=$((100 + $(jq length "$TEST_COMMENTS")))
        jq -c --arg body "$body" --argjson id "$id" \
          '. + [{id: $id, user: {login: "stormlight-thunderbot[bot]"}, body: $body}]' "$TEST_COMMENTS" > "$TEST_COMMENTS.new"
        mv "$TEST_COMMENTS.new" "$TEST_COMMENTS"
        reply "{\"id\": $id}" ;;
      */issues/comments/*)
        id=${4##*/}
        if [[ $3 == DELETE ]]; then edit='map(select(.id != $id))'; else edit='map(if .id == $id then .body = $body else . end)'; fi
        jq -c --arg body "$body" --argjson id "$id" "$edit" "$TEST_COMMENTS" > "$TEST_COMMENTS.new"
        mv "$TEST_COMMENTS.new" "$TEST_COMMENTS"
        reply "{\"id\": $id}" ;;
      *) echo '{}' ;;
    esac ;;
  "api "*)
    path=$2
    if [[ $path == --paginate ]]; then path=$3; fi
    case "$path" in
      */issues/*/comments*) out=$(cat "$TEST_COMMENTS") ;;
      */collaborators/*/permission) out="{\"permission\": \"${TEST_PERMISSION:-none}\"}" ;;
      */check-runs\?*) out=${TEST_CHECK_RUNS:-"{\"check_runs\": []}"} ;;
      *) exit 92 ;;
    esac
    reply "$out" ;;
  "label "* | "pr comment" | "pr ready" | "pr edit" | "pr merge") echo '{}' ;;
  *) exit 91 ;;
esac
"#;

/// One call the fake `gh` saw, with the text of its `--body-file`.
struct Call {
    args: Vec<String>,
    body: String,
}

impl Call {
    /// Whether this is `gh <first> <second> ...`.
    fn is(&self, first: &str, second: &str) -> bool {
        self.args.first().is_some_and(|a| a == first) && self.args.get(1).is_some_and(|a| a == second)
    }

    fn has(&self, arg: &str) -> bool {
        self.args.iter().any(|a| a == arg)
    }
}

/// A gate run's exit code, standard error and `gh` calls.
struct Run {
    code: Option<i32>,
    stderr: String,
    calls: Vec<Call>,
}

impl Run {
    /// The bodies the run wrote to the status comment, by a POST or a PATCH.
    fn posts(&self) -> Vec<&str> {
        self.status_writes().map(|c| c.body.as_str()).collect()
    }

    fn status_writes(&self) -> impl Iterator<Item = &Call> {
        self.calls
            .iter()
            .filter(|c| c.is("api", "-X") && c.args.get(3).is_some_and(|p| p.contains("/comments")))
    }

    fn succeeded_check(&self) -> bool {
        self.calls.iter().any(|c| c.has("conclusion=success"))
    }
}

/// The repository root, whose `.github/fabrial.toml` the gate reads.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Runs `fab gate` with `env` against a pull request: a draft with a title
/// and body, changed by `pr`, and by `later` too once the run has posted.
fn gate(env: &[(&str, &str)], pr: Value, later: Option<Value>) -> Run {
    let target = root().join("target");
    fs::create_dir_all(&target).unwrap();
    let temp = tempfile::tempdir_in(target).unwrap();
    let gh = temp.path().join("gh");
    fs::write(&gh, FAKE_GH).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
    let mut view = json!({"state": "OPEN", "isDraft": true, "title": "Test change", "body": "A change.",
                          "headRefName": "storm/test", "labels": []});
    merge(&mut view, &pr);
    // The comments live in their own file, in the REST API's shape.
    let comments = temp.path().join("comments.json");
    let existing = view.as_object_mut().unwrap().remove("comments").unwrap_or(json!([]));
    fs::write(&comments, existing.to_string()).unwrap();
    let later = later.map(|l| {
        let mut v = view.clone();
        merge(&mut v, &l);
        v.to_string()
    });
    let log = temp.path().join("calls.jsonl");
    let path = format!("{}:{}", temp.path().display(), std::env::var("PATH").unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_fab"))
        .arg("gate")
        .current_dir(root())
        .env_clear()
        .env("PATH", path)
        .env("HOME", temp.path())
        .envs([("REPO", "owner/repo"), ("PR", "1"), ("HEAD_SHA", HEAD)])
        .env("HEAD_DIR", temp.path().join("absent"))
        .env("TEST_GH_LOG", &log)
        .env("TEST_COMMENTS", &comments)
        .env("TEST_PR", view.to_string())
        .env("TEST_PR_LATER", later.unwrap_or_default())
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let calls = fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let call: Value = serde_json::from_str(line).unwrap();
            Call {
                args: call["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect(),
                body: call["body"].as_str().unwrap().to_string(),
            }
        })
        .collect();
    Run { code: output.status.code(), stderr: String::from_utf8_lossy(&output.stderr).to_string(), calls }
}

/// Sets each of `changes`' fields on `view`.
fn merge(view: &mut Value, changes: &Value) {
    for (key, value) in changes.as_object().into_iter().flatten() {
        view[key] = value.clone();
    }
}

fn finished() -> Vec<(&'static str, String)> {
    vec![
        ("EVENT", "comment".into()),
        ("AUTHOR", BOT.into()),
        ("BODY", format!("Review finished at {HEAD}")),
    ]
}

/// `finished()` with more variables.
fn with<'a>(base: &'a [(&'a str, String)], more: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    base.iter()
        .map(|(k, v)| (*k, v.as_str()))
        .chain(more.iter().copied())
        .collect()
}

/// The message the gate posts for a title, read back from a first run.
fn posted(title: &str) -> String {
    gate(&with(&finished(), &[]), json!({"title": title}), None).posts()[0].to_string()
}

/// The bot's comment as the REST API lists it.
fn bot_comment(id: u64, body: &str) -> Value {
    json!({"id": id, "user": {"login": "stormlight-thunderbot[bot]"}, "body": body})
}

/// A commit's check runs as the API lists them: one completed run per
/// `(app, conclusion, summary)`, each finishing after the one before.
fn check_runs(runs: &[(&str, &str, &str)]) -> String {
    let runs: Vec<Value> = runs
        .iter()
        .enumerate()
        .map(|(i, (app, conclusion, summary))| {
            json!({"status": "completed", "conclusion": conclusion, "completed_at": format!("2026-10-26T00:00:0{i}Z"),
                   "app": {"slug": app}, "output": {"summary": summary}})
        })
        .collect();
    json!({"check_runs": runs}).to_string()
}

/// The bot's passed review with `summary`.
fn passed_review(summary: &str) -> String {
    check_runs(&[("stormlight-thunderbot", "success", summary)])
}

#[test]
fn branch_pr_never_turns_on_auto_merge() {
    let workflow = fs::read_to_string(root().join(".github/workflows/branch-pr.yml")).unwrap();
    assert!(
        !workflow
            .lines()
            .any(|l| l.contains("gh pr merge") && l.contains("--auto"))
    );
    assert!(
        !fs::read_to_string(root().join("crates/fab-cli/src/branch_pr.rs"))
            .unwrap()
            .contains("\"merge\"")
    );
}

#[test]
fn review_finished_marks_the_draft_ready_and_posts_the_squash_message() {
    let owners_choice = json!({"mergeMethod": "SQUASH", "commitHeadline": "Owner's choice", "commitBody": ""});
    for auto_merge in [Value::Null, owners_choice] {
        let run = gate(&with(&finished(), &[]), json!({"autoMergeRequest": auto_merge}), None);
        assert_eq!(run.code, Some(0), "{}", run.stderr);
        assert!(run.calls.iter().any(|c| c.is("pr", "ready")));
        let message = run.posts()[0];
        assert!(message.contains("Test change (#1)\n"), "{message}");
        assert!(message.contains("Merged from storm/test.") && message.contains("Co-authored-by:"));
        assert_eq!(message.contains("Auto-merge is on"), !auto_merge.is_null());
        assert!(run.succeeded_check());
        assert!(
            !run.calls.iter().any(|c| c.is("pr", "merge")),
            "the gate must not touch auto-merge"
        );
    }
}

#[test]
fn a_repeated_review_finished_posts_the_same_squash_message_once() {
    let comments = json!([bot_comment(5, &posted("Test change"))]);
    let run = gate(
        &with(&finished(), &[]),
        json!({"isDraft": false, "comments": comments}),
        None,
    );
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.posts().is_empty());
}

#[test]
fn a_changed_message_edits_the_status_comment_in_place() {
    let comments = json!([bot_comment(5, &posted("Other"))]);
    let run = gate(
        &with(&finished(), &[]),
        json!({"isDraft": false, "comments": comments}),
        None,
    );
    assert_eq!(run.posts().len(), 1);
    let write = run.status_writes().next().unwrap();
    assert!(write.is("api", "-X") && write.has("PATCH") && write.has("repos/owner/repo/issues/comments/5"));
    assert!(write.body.contains("Test change (#1)\n") && !write.body.contains("Other"));
}

#[test]
fn a_copy_of_the_message_from_someone_else_does_not_stop_the_bots() {
    let comments = json!([{"id": 6, "user": {"login": "eve"}, "body": posted("Test change")}]);
    let run = gate(
        &with(&finished(), &[]),
        json!({"isDraft": false, "comments": comments}),
        None,
    );
    assert_eq!(run.posts().len(), 1);
}

#[test]
fn a_stopped_pull_request_gets_the_success_and_no_merge_settings() {
    let run = gate(
        &with(&finished(), &[]),
        json!({"labels": [{"name": "storm:stopped"}]}),
        None,
    );
    assert_eq!(run.code, Some(0));
    assert!(run.succeeded_check());
    assert!(!run.calls.iter().any(|c| c.is("pr", "ready")
        || c.is("pr", "merge")
        || c.is("pr", "comment")
        || c.is("api", "-X") && c.has("PATCH")));
}

#[test]
fn storm_stop_turns_auto_merge_off() {
    let env = [
        ("EVENT", "comment"),
        ("AUTHOR", "desertthunder"),
        ("BODY", "/storm:stop"),
    ];
    let run = gate(&env, json!({}), None);
    assert_eq!(run.code, Some(0));
    assert!(run.calls.iter().any(|c| c.is("pr", "merge") && c.has("--disable-auto")));
}

#[test]
fn an_edit_from_the_bot_posts_only_for_a_reviewed_ready_pull_request() {
    let passed = passed_review("Reviewed");
    let other_app = check_runs(&[("eve-app", "success", "Reviewed")]);
    let failed_since = check_runs(&[
        ("stormlight-thunderbot", "success", "Reviewed"),
        ("stormlight-thunderbot", "failure", "Changed"),
    ]);
    for (name, pr, runs, posts) in [
        (
            "reviewed and ready",
            json!({"isDraft": false, "title": "New title"}),
            passed.as_str(),
            true,
        ),
        ("a draft", json!({"isDraft": true}), passed.as_str(), false),
        ("an unreviewed head", json!({"isDraft": false}), "", false),
        (
            "passed by another app",
            json!({"isDraft": false}),
            other_app.as_str(),
            false,
        ),
        (
            "failed since its review",
            json!({"isDraft": false}),
            failed_since.as_str(),
            false,
        ),
    ] {
        let mut env = vec![("EVENT", "edit"), ("SENDER", BOT)];
        if !runs.is_empty() {
            env.push(("TEST_CHECK_RUNS", runs));
        }
        let run = gate(&env, pr, None);
        assert_eq!(run.code, Some(0), "{name}: {}", run.stderr);
        assert_eq!(run.posts().len(), usize::from(posts), "{name}");
        if posts {
            assert!(run.posts()[0].contains("New title"));
        }
        assert!(
            !run.calls.iter().any(|c| c.is("pr", "merge") || c.is("pr", "ready")),
            "{name}"
        );
    }
}

#[test]
fn an_edit_posts_only_for_a_sender_with_write_access() {
    let passed = passed_review("Reviewed");
    for (permission, posts) in [("write", true), ("maintain", true), ("read", false)] {
        let env = [
            ("EVENT", "edit"),
            ("SENDER", "someone"),
            ("TEST_PERMISSION", permission),
            ("TEST_CHECK_RUNS", &passed),
        ];
        let run = gate(&env, json!({"isDraft": false}), None);
        assert_eq!(run.code, Some(0));
        assert_eq!(!run.posts().is_empty(), posts, "{permission}");
        assert!(
            run.calls
                .iter()
                .any(|c| c.args.iter().any(|a| a.contains("collaborators/someone/permission")))
        );
    }
}

#[test]
fn the_subject_ends_with_the_pull_request_number_once() {
    let run = gate(&with(&finished(), &[]), json!({"title": "Fix (#12)"}), None);
    assert!(run.posts()[0].contains("Fix (#12) (#1)\n"));
    let run = gate(&with(&finished(), &[]), json!({"title": "Test change (#1)"}), None);
    let message = run.posts()[0];
    assert!(message.contains("Test change (#1)\n") && !message.contains("(#1) (#1)"));
}

#[test]
fn a_run_posts_once_then_sees_its_own_message_and_stops() {
    let run = gate(&with(&finished(), &[]), json!({}), None);
    assert_eq!(run.posts().len(), 1);
}

#[test]
fn an_edit_during_the_run_posts_the_edited_text_last() {
    let run = gate(&with(&finished(), &[]), json!({}), Some(json!({"title": "Edited"})));
    let posts = run.posts();
    assert_eq!(posts.len(), 2);
    assert!(posts[1].contains("Edited (#1)\n"));
}

#[test]
fn a_failed_ready_posts_no_success() {
    let run = gate(&with(&finished(), &[("TEST_FAIL", "ready")]), json!({}), None);
    assert_ne!(run.code, Some(0));
    assert!(!run.succeeded_check());
}

#[test]
fn a_failed_squash_message_fails_the_run() {
    let run = gate(&with(&finished(), &[("TEST_FAIL", "comment")]), json!({}), None);
    assert_ne!(run.code, Some(0));
}

#[test]
fn dry_run_reads_but_writes_nothing() {
    let run = gate(&with(&finished(), &[("DRY_RUN", "1")]), json!({}), None);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    // Reads only: the pull request and its comments.
    assert!(
        run.calls
            .iter()
            .all(|c| c.is("pr", "view") || c.is("api", "--paginate"))
    );
    assert!(run.stderr.contains(r#"DRY_RUN gh "pr" "ready""#), "{}", run.stderr);
}

#[test]
fn a_rebase_that_only_moves_the_base_keeps_the_review_and_a_new_change_does_not() {
    let target = root().join("target");
    let repo = tempfile::tempdir_in(target).unwrap();
    let git = |args: &[&str]| {
        // A clear environment, so a GIT_DIR from a hook cannot point these
        // commands at another repository.
        let output = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", repo.path())
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    let commit = |text: &str, message: &str| {
        fs::write(repo.path().join("a.txt"), text).unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-qm", message]);
        git(&["rev-parse", "HEAD"])
    };
    git(&["init", "-q", "-b", "main"]);
    commit("  one\n  two\n  three\n", "base");
    git(&["checkout", "-qb", "topic"]);
    let first = commit("  one\n  two\n  three\n  four\n", "change");
    git(&["checkout", "-q", "main"]);
    // Moves the change's hunk down a line, so only the cut ranges match.
    // Indented lines give git no context label for the hunk header.
    commit("  zero\n  one\n  two\n  three\n", "main moves");
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(&["checkout", "-q", "topic"]);
    git(&["rebase", "-q", "main"]);
    let second = git(&["rev-parse", "HEAD"]);
    let third = commit("  zero\n  one\n  two\n  three\n  five\n", "another change");
    let dir = repo.path().to_str().unwrap();

    let body = format!("Review finished at {first}");
    let reviewed = gate(
        &[
            ("EVENT", "comment"),
            ("AUTHOR", BOT),
            ("BODY", &body),
            ("HEAD_SHA", &first),
            ("HEAD_DIR", dir),
        ],
        json!({}),
        None,
    );
    let success = reviewed.calls.iter().find(|c| c.has("conclusion=success")).unwrap();
    let summary = success
        .args
        .iter()
        .find_map(|a| a.strip_prefix("output[summary]="))
        .unwrap()
        .to_string();
    let hash = summary
        .split("<!-- storm-review diff=")
        .nth(1)
        .and_then(|rest| rest.split_once(" -->"))
        .map(|(hash, _)| hash)
        .unwrap_or_default();
    assert!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{summary}"
    );

    let rebased = gate(
        &[
            ("EVENT", "push"),
            ("HEAD_SHA", &second),
            ("BEFORE", &first),
            ("HEAD_DIR", dir),
            ("TEST_CHECK_RUNS", &passed_review(&summary)),
        ],
        json!({}),
        None,
    );
    let title = format!("output[title]=Same change as {}", &first[..7]);
    assert!(
        rebased
            .calls
            .iter()
            .any(|c| c.has("conclusion=success") && c.has(&title))
    );

    let changed = gate(
        &[
            ("EVENT", "push"),
            ("HEAD_SHA", &third),
            ("BEFORE", &second),
            ("HEAD_DIR", dir),
            ("TEST_CHECK_RUNS", &passed_review(&summary)),
        ],
        json!({}),
        None,
    );
    assert!(changed.calls.iter().any(|c| c.has("status=in_progress")));
    assert!(!changed.succeeded_check());
}

#[test]
fn storm_ready_accepts_the_head_and_posts_the_squash_message() {
    let body = format!("/storm:ready {}", &HEAD[..7]);
    let run = gate(
        &[("EVENT", "comment"), ("AUTHOR", "desertthunder"), ("BODY", &body)],
        json!({}),
        None,
    );
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.calls.iter().any(|c| c.is("pr", "ready")));
    assert!(
        run.calls
            .iter()
            .any(|c| c.has("conclusion=success") && c.has("output[title]=Marked ready by @desertthunder"))
    );
    assert_eq!(run.posts().len(), 1);
    assert!(run.posts()[0].contains("Test change (#1)\n"));
}

#[test]
fn a_command_naming_another_commit_asks_again_and_accepts_nothing() {
    for body in [
        "Review finished at bbbbbbb",
        "/storm:ready bbbbbbb",
        "/storm:ready",
        "Review finished at aaaa",
    ] {
        let author = if body.starts_with('/') { "desertthunder" } else { BOT };
        let run = gate(
            &[("EVENT", "comment"), ("AUTHOR", author), ("BODY", body)],
            json!({}),
            None,
        );
        assert_eq!(run.code, Some(0), "{body}: {}", run.stderr);
        assert!(!run.succeeded_check(), "{body}");
        assert!(!run.calls.iter().any(|c| c.is("pr", "ready")), "{body}");
        let asks = run.calls.iter().filter(|c| c.is("pr", "comment")).collect::<Vec<_>>();
        assert_eq!(asks.len(), 1, "{body}");
        assert!(
            asks[0].args.iter().any(|a| a.contains("but the head is `aaaaaaa`")),
            "{body}"
        );
    }
}

#[test]
fn a_closed_pull_request_is_not_marked_ready_or_sent_a_message() {
    let run = gate(&with(&finished(), &[]), json!({"state": "CLOSED"}), None);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(!run.calls.iter().any(|c| c.is("pr", "ready")));
    assert!(run.posts().is_empty());
}

#[test]
fn a_reopened_head_keeps_its_own_success_and_otherwise_waits() {
    let passed = passed_review("Reviewed");
    let run = gate(
        &[("EVENT", "push"), ("ACTION", "reopened"), ("TEST_CHECK_RUNS", &passed)],
        json!({}),
        None,
    );
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(!run.calls.iter().any(|c| c.is("api", "-X")));

    let run = gate(&[("EVENT", "push"), ("ACTION", "reopened")], json!({}), None);
    assert!(run.calls.iter().any(|c| c.has("status=in_progress")));
}

#[test]
fn storm_stop_labels_the_pull_request() {
    let env = [
        ("EVENT", "comment"),
        ("AUTHOR", "desertthunder"),
        ("BODY", "/storm:stop now"),
    ];
    let run = gate(&env, json!({}), None);
    assert_eq!(run.code, Some(0));
    assert!(
        run.calls
            .iter()
            .any(|c| c.is("label", "create") && c.has("storm:stopped"))
    );
    assert!(
        run.calls
            .iter()
            .any(|c| c.is("pr", "edit") && c.has("--add-label") && c.has("storm:stopped"))
    );
}

#[test]
fn pr_status_keeps_one_comment_and_edits_its_sections() {
    let target = root().join("target");
    let temp = tempfile::tempdir_in(target).unwrap();
    fs::write(temp.path().join("gh"), FAKE_GH).unwrap();
    fs::set_permissions(temp.path().join("gh"), fs::Permissions::from_mode(0o700)).unwrap();
    let comments = temp.path().join("comments.json");
    fs::write(
        &comments,
        json!([{"id": 6, "user": {"login": "eve"}, "body": "<!-- fabrial:status -->"}]).to_string(),
    )
    .unwrap();
    let path = format!("{}:{}", temp.path().display(), std::env::var("PATH").unwrap());
    let set = |section: &str, text: &str| {
        let file = temp.path().join("section.md");
        fs::write(&file, text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fab"))
            .args(["pr-status", "--section", section, "--body-file"])
            .arg(&file)
            .env_clear()
            .env("PATH", &path)
            .env("HOME", temp.path())
            .envs([("REPO", "owner/repo"), ("PR", "1")])
            .env("TEST_GH_LOG", temp.path().join("calls.jsonl"))
            .env("TEST_COMMENTS", &comments)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    };
    set("Standard pass", "Standard pass on abc · m\n\nNo findings\n");
    set("Squash message", "When you merge, squash with this message:");
    set("Standard pass", "Standard pass on def · m\n\n- low · a.rs:1\n");
    let comments: Value = serde_json::from_str(&fs::read_to_string(&comments).unwrap()).unwrap();
    let ours: Vec<&Value> = comments
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["user"]["login"] == "stormlight-thunderbot[bot]")
        .collect();
    assert_eq!(ours.len(), 1, "{comments}");
    let body = ours[0]["body"].as_str().unwrap();
    assert!(
        body.starts_with("<!-- fabrial:status -->\n<!-- fabrial:section Standard pass -->\nStandard pass on def"),
        "{body}"
    );
    assert!(
        body.contains("<!-- fabrial:section Squash message -->\nWhen you merge") && !body.contains("abc"),
        "{body}"
    );
}

#[test]
fn a_second_status_comment_is_folded_into_the_first() {
    let first = "<!-- fabrial:status -->\n<!-- fabrial:section Standard pass -->\nStandard pass on abc";
    let second = "<!-- fabrial:status -->\n<!-- fabrial:section Other -->\nkept";
    let comments = json!([bot_comment(5, first), bot_comment(7, second)]);
    let run = gate(&with(&finished(), &[]), json!({"comments": comments}), None);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(
        run.calls
            .iter()
            .any(|c| c.has("DELETE") && c.has("repos/owner/repo/issues/comments/7"))
    );
    let write = run.status_writes().find(|c| c.has("PATCH")).unwrap();
    assert!(write.has("repos/owner/repo/issues/comments/5"));
    for section in ["Standard pass on abc", "kept", "Test change (#1)"] {
        assert!(write.body.contains(section), "{section}: {}", write.body);
    }
}

#[test]
fn a_write_another_writer_overwrote_is_made_again() {
    let comments = json!([bot_comment(5, "<!-- fabrial:status -->")]);
    let run = gate(
        &with(&finished(), &[("TEST_LOSE_WRITE", "1")]),
        json!({"comments": comments}),
        None,
    );
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(run.status_writes().filter(|c| c.has("PATCH")).count(), 2);
}
