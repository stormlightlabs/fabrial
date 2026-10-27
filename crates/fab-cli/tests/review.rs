//! Runs `fab pr-review` and `fab pr-answer` against a fake `gh` that logs
//! every call and answers the reads they make.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use serde_json::{Value, json};

const HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// The fake `gh`. It logs each call's arguments and the JSON of any
/// `--input` file to `TEST_GH_LOG`. It answers the pull request's base
/// branch, the compare with `TEST_COMPARE`, the review threads with
/// `TEST_THREADS`, and each mutation with the id its filter reads. With
/// `TEST_FAIL` set to a mutation's name, that mutation fails.
const FAKE_GH: &str = r#"#!/bin/bash
set -u
input=null filter= query=
for ((i = 1; i < $#; i++)); do
  j=$((i + 1))
  if [[ ${!i} == --input ]]; then input=$(cat "${!j}"); fi
  if [[ ${!i} == --jq ]]; then filter=${!j}; fi
  if [[ ${!i} == -f && ${!j} == query=* ]]; then query=${!j#query=}; fi
done
printf '%s\0' "$@" | jq -Rsc --argjson input "$input" '{args: split("\u0000")[:-1], input: $input}' >> "$TEST_GH_LOG"
reply() { if [[ -n $filter ]]; then jq -r "$filter" <<<"$1"; else printf '%s\n' "$1"; fi; }
if [[ $1 == api && $2 == graphql ]]; then
  name=$(grep -oE '(addPullRequestReviewThreadReply|addPullRequestReview|submitPullRequestReview|resolveReviewThread|deletePullRequestReview)' <<<"$query" | head -1)
  if [[ -n $name && $name == "${TEST_FAIL:-}" ]]; then echo "GraphQL: $name failed" >&2; exit 1; fi
  case "$name" in
    addPullRequestReview) reply '{"data": {"addPullRequestReview": {"pullRequestReview": {"id": "PRR_1"}}}}' ;;
    addPullRequestReviewThreadReply) reply '{"data": {"addPullRequestReviewThreadReply": {"comment": {"id": "C_1"}}}}' ;;
    submitPullRequestReview) reply '{"data": {"submitPullRequestReview": {"pullRequestReview": {"id": "PRR_1"}}}}' ;;
    resolveReviewThread) reply '{"data": {"resolveReviewThread": {"thread": {"id": "T"}}}}' ;;
    deletePullRequestReview) reply '{"data": {"deletePullRequestReview": {"clientMutationId": null}}}' ;;
    *) reply "$TEST_THREADS" ;;
  esac
  exit 0
fi
case "$*" in
  "api -X POST "*/reviews*) echo '{"id": 1}' ;;
  "api "*/pulls/7" "*) reply '{"base": {"ref": "main"}}' ;;
  "api "*/compare/main...*) printf '%s\n' "$TEST_COMPARE" ;;
  *) exit 91 ;;
esac
"#;

/// A run's exit code, standard error and `gh` calls.
struct Run {
    code: Option<i32>,
    stderr: String,
    calls: Vec<Value>,
}

impl Run {
    /// The GraphQL mutations the run sent, by name, with their variables.
    fn mutations(&self) -> Vec<(String, Vec<String>)> {
        self.calls
            .iter()
            .filter_map(|c| {
                let args: Vec<String> = c["args"]
                    .as_array()?
                    .iter()
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect();
                let query = args.iter().find_map(|a| a.strip_prefix("query="))?;
                let name = query
                    .strip_prefix("mutation")?
                    .split('{')
                    .nth(1)?
                    .split('(')
                    .next()?
                    .trim()
                    .to_string();
                let vars = args
                    .iter()
                    .filter(|a| !a.starts_with("query=") && a.contains('='))
                    .cloned()
                    .collect();
                Some((name, vars))
            })
            .collect()
    }
}

/// Runs `fab <command>` with `report` as its body file and `env` set.
fn fab(command: &str, report: &str, env: &[(&str, &str)]) -> Run {
    let temp = tempfile::tempdir().unwrap();
    let gh = temp.path().join("gh");
    fs::write(&gh, FAKE_GH).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
    let body = temp.path().join("body.md");
    fs::write(&body, report).unwrap();
    let log = temp.path().join("calls.jsonl");
    fs::write(&log, "").unwrap();
    let path = format!("{}:{}", temp.path().display(), std::env::var("PATH").unwrap());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fab"));
    cmd.arg(command)
        .arg("--body-file")
        .arg(&body)
        .env("PATH", path)
        .env("TEST_GH_LOG", &log)
        .env("REPO", "org/repo")
        .env("PR", "7")
        .env_remove("DRY_RUN")
        .env_remove("GITHUB_ACTIONS");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    let calls = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    Run { code: out.status.code(), stderr: String::from_utf8_lossy(&out.stderr).into(), calls }
}

fn compare() -> String {
    json!({"files": [
        {"filename": "src/a.rs", "patch": "@@ -1,3 +1,5 @@\n a\n+b\n+c\n d\n e"},
        {"filename": "big.bin"}
    ]})
    .to_string()
}

fn threads() -> String {
    json!({"data": {"repository": {"pullRequest": {"id": "PR_7", "reviewThreads": {
    "pageInfo": {"hasNextPage": false, "endCursor": null},
    "nodes": [
        {"id": "T_1", "comments": {"nodes": [{"fullDatabaseId": "1001"}]}},
        {"id": "T_2", "comments": {"nodes": [{"fullDatabaseId": "1002"}]}}
    ]}}}}})
    .to_string()
}

#[test]
fn a_review_puts_findings_on_the_diff_and_the_rest_in_its_body() {
    let report = "Standard pass on bbbbbbb · m\n\n- medium · src/a.rs:2 — on the diff\n- low · src/a.rs:40 — off it\n- nit · big.bin:1 — no patch";
    let compare = compare();
    let run = fab("pr-review", report, &[("HEAD_SHA", HEAD), ("TEST_COMPARE", &compare)]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let posts: Vec<&Value> = run.calls.iter().filter(|c| c["args"][1] == "-X").collect();
    assert_eq!(posts.len(), 1, "one review, so one notification");
    assert_eq!(posts[0]["args"][3], "repos/org/repo/pulls/7/reviews");
    let review = &posts[0]["input"];
    assert_eq!(review["commit_id"], HEAD);
    assert_eq!(review["event"], "COMMENT");
    assert_eq!(
        review["comments"],
        json!([{"path": "src/a.rs", "line": 2, "side": "RIGHT", "body": "medium — on the diff"}])
    );
    assert_eq!(
        review["body"],
        "Standard pass on bbbbbbb · m\n\n- low · src/a.rs:40 — off it\n- nit · big.bin:1 — no patch"
    );
    assert!(
        run.calls
            .iter()
            .any(|c| c["args"][1] == "repos/org/repo/compare/main...".to_string() + HEAD)
    );
}

#[test]
fn a_review_needs_the_full_head_sha() {
    let run = fab("pr-review", "x", &[("HEAD_SHA", "bbbbbbb")]);
    assert_eq!(run.code, Some(2));
    assert!(run.stderr.contains("not a full commit SHA"), "{}", run.stderr);
    assert!(run.calls.is_empty());
}

#[test]
fn a_dry_run_review_writes_nothing() {
    let compare = compare();
    let run = fab(
        "pr-review",
        "- low · src/a.rs:1 — x",
        &[("HEAD_SHA", HEAD), ("TEST_COMPARE", &compare), ("DRY_RUN", "1")],
    );
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.stderr.contains("DRY_RUN"));
    assert!(!run.calls.iter().any(|c| c["args"][1] == "-X"));
}

#[test]
fn answers_reply_in_one_review_and_resolve_what_they_close() {
    let report =
        "Answering the review at bbbbbbb · m\n\n- 1001 · Fixed in ccccccc.\n- 1002 · Not a defect: the test covers it.";
    let threads = threads();
    let run = fab("pr-answer", report, &[("TEST_THREADS", &threads)]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    let names: Vec<String> = run.mutations().into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        [
            "addPullRequestReview",
            "addPullRequestReviewThreadReply",
            "addPullRequestReviewThreadReply",
            "submitPullRequestReview",
            "resolveReviewThread"
        ]
    );
    let mutations = run.mutations();
    assert!(mutations[0].1.contains(&"pr=PR_7".to_string()));
    assert!(mutations[1].1.contains(&"thread=T_1".to_string()));
    assert!(mutations[1].1.contains(&"body=Fixed in ccccccc.".to_string()));
    assert!(
        mutations[3]
            .1
            .contains(&"body=Answering the review at bbbbbbb · m".to_string())
    );
    assert_eq!(
        mutations[4].1,
        ["thread=T_1"],
        "a declined finding stays open for the owner"
    );
}

#[test]
fn an_answer_to_an_unknown_comment_posts_nothing() {
    let threads = threads();
    let run = fab(
        "pr-answer",
        "- 1001 · Fixed.\n- 9999 · Fixed.",
        &[("TEST_THREADS", &threads)],
    );
    assert_eq!(run.code, Some(2));
    assert!(run.stderr.contains("9999"), "{}", run.stderr);
    assert!(run.mutations().is_empty());
}

#[test]
fn a_failed_answer_deletes_its_pending_review() {
    let threads = threads();
    let run = fab(
        "pr-answer",
        "- 1001 · Fixed.",
        &[("TEST_THREADS", &threads), ("TEST_FAIL", "submitPullRequestReview")],
    );
    assert_eq!(run.code, Some(2));
    let names: Vec<String> = run.mutations().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names.last().map(String::as_str), Some("deletePullRequestReview"));
    assert!(!names.contains(&"resolveReviewThread".to_string()));
}

#[test]
fn answers_with_no_thread_post_as_a_plain_review() {
    let body = "Answering the review at bbbbbbb · m\n\n- The PR body names one variable: fixed in ccccccc.";
    let run = fab("pr-answer", body, &[]);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.mutations().is_empty());
    let posts: Vec<&Value> = run.calls.iter().filter(|c| c["args"][1] == "-X").collect();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["input"], json!({"event": "COMMENT", "body": body}));
}

#[test]
fn a_failed_resolve_still_counts_the_answers_as_posted() {
    let threads = threads();
    let run = fab(
        "pr-answer",
        "- 1001 · Fixed.",
        &[("TEST_THREADS", &threads), ("TEST_FAIL", "resolveReviewThread")],
    );
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.stderr.contains("cannot resolve thread T_1"), "{}", run.stderr);
}
