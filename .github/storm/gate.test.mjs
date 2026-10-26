import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const worktree = fileURLToPath(new URL("../../", import.meta.url));
// `cargo test`, which `just check` runs first, builds the binary.
const fab = process.env.FAB_BIN ?? join(worktree, "target/debug/fab");
const head = "a".repeat(40);

// Runs `fab gate` against a fake gh that logs every call and answers `pr view`
// with `pr`, plus the comments the run has posted as the bot. After the first
// post it answers with `later` instead, as if the pull request was edited
// while the run worked. Returns the exit status and the logged calls.
function gate(env, pr = {}, later = null) {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, "gate-test-"));
  try {
    const log = join(root, "calls.jsonl");
    const base = { state: "OPEN", isDraft: true, title: "Test change", body: "A change.", headRefName: "storm/test",
      labels: [], comments: [] };
    writeFileSync(join(root, "gh"), `#!/usr/bin/env node
import fs from "node:fs";
const args = process.argv.slice(2), index = args.indexOf("--body-file");
const body = index < 0 ? "" : fs.readFileSync(args[index + 1], "utf8");
fs.appendFileSync(process.env.TEST_GH_LOG, JSON.stringify({args, body}) + "\\n");
const posted = fs.readFileSync(process.env.TEST_GH_LOG, "utf8").trim().split("\\n").map(l => JSON.parse(l))
  .filter(c => c.args[0] === "pr" && c.args[1] === "comment").map(c => ({ author: { login: "stormlight-thunderbot" }, body: c.body }));
if (args[0] === "pr" && args[1] === process.env.TEST_FAIL) process.exit(1);
if (args[0] === "pr" && args[1] === "view") {
  const view = JSON.parse(posted.length && process.env.TEST_PR_LATER ? process.env.TEST_PR_LATER : process.env.TEST_PR);
  view.comments = [...view.comments, ...posted];
  console.log(JSON.stringify(view));
}
else if (args[0] === "api" && args[1] !== "-X") console.log(process.env.TEST_API ?? "");
else if (args[0] === "api" || args[0] === "label" || (args[0] === "pr" && ["comment", "ready", "edit", "merge"].includes(args[1]))) console.log("{}");
else process.exit(91);
`, { mode: 0o700 });
    const output = spawnSync(fab, ["gate"], { cwd: worktree, encoding: "utf8", timeout: 10000,
      env: { ...process.env, PATH: `${root}:${process.env.PATH}`, REPO: "owner/repo", PR: "1", HEAD_SHA: head,
        HEAD_DIR: join(root, "absent"), TEST_GH_LOG: log, TEST_PR: JSON.stringify({ ...base, ...pr }),
        TEST_PR_LATER: later ? JSON.stringify({ ...base, ...pr, ...later }) : "", ...env } });
    assert.equal(output.error, undefined);
    let calls = [];
    try { calls = readFileSync(log, "utf8").trim().split("\n").map(line => JSON.parse(line)); } catch {}
    return { status: output.status, stderr: output.stderr, calls };
  } finally { rmSync(root, { recursive: true, force: false }); }
}

test("Branch PR never turns on auto-merge", () => {
  const workflow = readFileSync(join(worktree, ".github/workflows/branch-pr.yml"), "utf8");
  assert.doesNotMatch(workflow, /gh pr merge[^\n]*--auto/);
});

const finished = { EVENT: "comment", AUTHOR: "stormlight-thunderbot[bot]", BODY: `Review finished at ${head}` };
const bot = { login: "stormlight-thunderbot" };

for (const autoMergeRequest of [null, { mergeMethod: "SQUASH", commitHeadline: "Owner's choice", commitBody: "" }]) {
  test(`Review finished marks the draft ready and posts the squash message, ${autoMergeRequest ? "noting the owner's auto-merge" : "with no auto-merge"}`, () => {
    const { status, stderr, calls } = gate(finished, { autoMergeRequest });
    assert.equal(status, 0, stderr);
    assert(calls.some(c => c.args[0] === "pr" && c.args[1] === "ready"));
    const message = calls.find(c => c.args[1] === "comment")?.body;
    assert(message?.includes("Test change (#1)\n") && message.includes("Merged from storm/test.") && message.includes("Co-authored-by:"));
    assert.equal(message.includes("Auto-merge is on"), Boolean(autoMergeRequest));
    assert(calls.some(c => c.args.includes("conclusion=success")));
    assert(!calls.some(c => c.args[1] === "merge"), "the gate must not touch auto-merge");
  });
}

// The message the gate posts for a title, read back from a first run.
function posted(title) {
  return gate(finished, { title }).calls.find(c => c.args[1] === "comment").body;
}

test("a repeated Review finished posts the same squash message once", () => {
  const again = gate(finished, { isDraft: false, comments: [{ author: bot, body: posted("Test change") }] });
  assert.equal(again.status, 0, again.stderr);
  assert(!again.calls.some(c => c.args[1] === "comment"));
});

test("a title changed back posts again, since the newest message differs", () => {
  const comments = [{ author: bot, body: posted("Test change") }, { author: bot, body: posted("Other") }];
  const { calls } = gate(finished, { isDraft: false, comments });
  assert(calls.some(c => c.args[1] === "comment"));
});

test("a copy of the message from someone else does not stop the bot's", () => {
  const { calls } = gate(finished, { isDraft: false, comments: [{ author: { login: "eve" }, body: posted("Test change") }] });
  assert(calls.some(c => c.args[1] === "comment"));
});

test("a stopped pull request gets the success and no merge settings", () => {
  const { status, calls } = gate(finished, { labels: [{ name: "storm:stopped" }] });
  assert.equal(status, 0);
  assert(!calls.some(c => c.args[1] === "ready" || c.args[1] === "merge" || c.args[1] === "comment"));
});

test("/storm:stop still turns auto-merge off", () => {
  const { status, calls } = gate({ EVENT: "comment", AUTHOR: "desertthunder", BODY: "/storm:stop" });
  assert.equal(status, 0);
  assert(calls.some(c => c.args[1] === "merge" && c.args.includes("--disable-auto")));
});

for (const [name, pr, api, posts] of [
  ["a reviewed, ready pull request posts the new squash message", { isDraft: false, title: "New title" }, "Reviewed", true],
  ["a draft posts nothing", { isDraft: true }, "Reviewed", false],
  ["an unreviewed head posts nothing", { isDraft: false }, "", false],
]) {
  test(`an edit from the bot: ${name}`, () => {
    const { status, stderr, calls } = gate({ EVENT: "edit", SENDER: "stormlight-thunderbot[bot]", TEST_API: api }, pr);
    assert.equal(status, 0, stderr);
    const message = calls.find(c => c.args[1] === "comment")?.body;
    assert.equal(Boolean(message), posts);
    if (posts) assert(message.includes("New title"));
    assert(!calls.some(c => c.args[1] === "merge" || c.args[1] === "ready"));
  });
}

// The fake answers every `gh api` read with TEST_API, so "write" is both the
// sender's permission and a passed review.
for (const [permission, posts] of [["write", true], ["read", false]]) {
  test(`an edit from a sender with ${permission} access ${posts ? "posts" : "posts nothing"}`, () => {
    const { status, calls } = gate({ EVENT: "edit", SENDER: "someone", TEST_API: permission }, { isDraft: false });
    assert.equal(status, 0);
    assert.equal(calls.some(c => c.args[1] === "comment"), posts);
    assert.equal(calls.some(c => c.args.some(a => a.includes("collaborators/someone/permission"))), true);
  });
}

test("a title ending with another number still gets its own", () => {
  const { calls } = gate(finished, { title: "Fix (#12)" });
  assert(calls.find(c => c.args[1] === "comment").body.includes("Fix (#12) (#1)\n"));
});

test("a title that already ends with its number keeps one", () => {
  const { calls } = gate(finished, { title: "Test change (#1)" });
  const message = calls.find(c => c.args[1] === "comment").body;
  assert(message.includes("Test change (#1)\n") && !message.includes("(#1) (#1)"));
});

test("a run posts once, then sees its own message and stops", () => {
  const { calls } = gate(finished);
  assert.equal(calls.filter(c => c.args[1] === "comment").length, 1);
});

test("an edit during the run posts the edited text last", () => {
  const { calls } = gate(finished, {}, { title: "Edited" });
  const posts = calls.filter(c => c.args[1] === "comment");
  assert.equal(posts.length, 2);
  assert(posts[1].body.includes("Edited (#1)\n"));
});

// squash-body.sh with only the kept text.
function kept(description) {
  return spawnSync("bash", [".github/storm/squash-body.sh", "--text"], { cwd: worktree, encoding: "utf8", input: description }).stdout;
}

test("squash body keeps a Session line in prose and drops one naming a link", () => {
  assert.equal(kept("Keep.\nSession: tokens now expire.\nSession: https://pi.dev/s/abc\n> Session: <https://pi.dev/s/abc>\nSession: [run](https://x.io/s)\n"),
    "Keep.\nSession: tokens now expire.\n");
});

test("squash body treats code blocks like any other text", () => {
  assert.equal(kept("Keep.\n````\n```\nCo-authored-by: A <a@b>\n````\nAlso kept.\n<!-- fabrial:footer -->\nGone.\n"),
    "Keep.\n````\n```\n````\nAlso kept.\n");
});

test("a failed ready posts no success", () => {
  const { status, calls } = gate({ ...finished, TEST_FAIL: "ready" });
  assert.notEqual(status, 0);
  assert(!calls.some(c => c.args.includes("conclusion=success")));
});

test("a failed squash message fails the run", () => {
  const { status } = gate({ ...finished, TEST_FAIL: "comment" });
  assert.notEqual(status, 0);
});

test("DRY_RUN=1 reads but writes nothing", () => {
  const { status, stderr, calls } = gate({ ...finished, DRY_RUN: "1" });
  assert.equal(status, 0, stderr);
  assert(calls.every(c => c.args[1] === "view"), JSON.stringify(calls.map(c => c.args.slice(0, 2))));
  assert.match(stderr, /DRY_RUN gh "pr" "ready"/);
});

test("a rebase that only moves the base keeps the review, and a new change does not", () => {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const repo = mkdtempSync(join(target, "gate-repo-"));
  try {
    const git = (...args) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", ...args],
      { cwd: repo, encoding: "utf8" }).trim();
    const commit = (file, text, message) => { writeFileSync(join(repo, file), text); git("add", file); git("commit", "-qm", message); return git("rev-parse", "HEAD"); };
    git("init", "-q", "-b", "main");
    commit("a.txt", "  one\n  two\n  three\n", "base");
    git("checkout", "-qb", "topic");
    const first = commit("a.txt", "  one\n  two\n  three\n  four\n", "change");
    git("checkout", "-q", "main");
    // Moves the change's hunk down a line, so only the cut ranges match.
    // Indented lines give git no context label for the hunk header.
    commit("a.txt", "  zero\n  one\n  two\n  three\n", "main moves");
    git("update-ref", "refs/remotes/origin/main", "HEAD");
    git("checkout", "-q", "topic");
    git("rebase", "-q", "main");
    const second = git("rev-parse", "HEAD");
    const third = commit("a.txt", "  zero\n  one\n  two\n  three\n  five\n", "another change");

    const reviewed = gate({ ...finished, BODY: `Review finished at ${first}`, HEAD_SHA: first, HEAD_DIR: repo });
    const success = reviewed.calls.find(c => c.args.includes("conclusion=success"));
    const summary = success.args.find(a => a.startsWith("output[summary]=")).slice("output[summary]=".length);
    assert.match(summary, /<!-- storm-review diff=[0-9a-f]{64} -->/);

    const rebased = gate({ EVENT: "push", HEAD_SHA: second, BEFORE: first, HEAD_DIR: repo, TEST_API: summary });
    assert(rebased.calls.some(c => c.args.includes("conclusion=success") && c.args.includes(`output[title]=Same change as ${first.slice(0, 7)}`)),
      JSON.stringify(rebased.calls));

    const changed = gate({ EVENT: "push", HEAD_SHA: third, BEFORE: second, HEAD_DIR: repo, TEST_API: summary });
    assert(changed.calls.some(c => c.args.includes("status=in_progress")));
    assert(!changed.calls.some(c => c.args.includes("conclusion=success")));
  } finally { rmSync(repo, { recursive: true, force: false }); }
});
