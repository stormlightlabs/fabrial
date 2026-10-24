import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const worktree = fileURLToPath(new URL("../", import.meta.url));
const head = "a".repeat(40);

// Runs gate.sh against a fake gh that logs every call and answers `pr view`
// with `pr`, and returns the exit status and the logged calls.
function gate(env, pr = {}) {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, "gate-test-"));
  try {
    const log = join(root, "calls.jsonl");
    const view = { state: "OPEN", isDraft: true, title: "Test change", body: "A change.", headRefName: "storm/test",
      labels: [], comments: [], ...pr };
    writeFileSync(join(root, "gh"), `#!/usr/bin/env node
import fs from "node:fs";
const args = process.argv.slice(2), index = args.indexOf("--body-file");
fs.appendFileSync(process.env.TEST_GH_LOG, JSON.stringify({args, body:index < 0 ? "" : fs.readFileSync(args[index + 1], "utf8")}) + "\\n");
if (args[0] === "pr" && args[1] === "view") console.log(process.env.TEST_PR);
else if (args[0] === "api" && args[1] !== "-X") console.log(process.env.TEST_API ?? "");
else if (args[0] === "api" || args[0] === "label" || (args[0] === "pr" && ["comment", "ready", "edit", "merge"].includes(args[1]))) console.log("{}");
else process.exit(91);
`, { mode: 0o700 });
    const output = spawnSync("bash", [".github/storm/gate.sh"], { cwd: worktree, encoding: "utf8", timeout: 10000,
      env: { ...process.env, PATH: `${root}:${process.env.PATH}`, REPO: "owner/repo", PR: "1", HEAD_SHA: head,
        HEAD_DIR: join(root, "absent"), TEST_GH_LOG: log, TEST_PR: JSON.stringify(view), ...env } });
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

for (const autoMergeRequest of [null, { mergeMethod: "MERGE", commitHeadline: "Owner's choice", commitBody: "" }]) {
  test(`Review finished marks the draft ready and posts the squash message, ${autoMergeRequest ? "keeping the owner's auto-merge" : "with no auto-merge"}`, () => {
    const { status, stderr, calls } = gate({ EVENT: "comment", AUTHOR: "stormlight-thunderbot[bot]", BODY: `Review finished at ${head}` },
      { autoMergeRequest });
    assert.equal(status, 0, stderr);
    assert(calls.some(c => c.args[0] === "pr" && c.args[1] === "ready"));
    const message = calls.find(c => c.args[1] === "comment")?.body;
    assert(message?.includes("Test change") && message.includes("Merged from storm/test.") && message.includes("Co-authored-by:"));
    assert(calls.some(c => c.args.includes("conclusion=success")));
    assert(!calls.some(c => c.args[1] === "merge"), "the gate must not touch auto-merge");
  });
}

test("a repeated Review finished posts the same squash message once", () => {
  const first = gate({ EVENT: "comment", AUTHOR: "stormlight-thunderbot[bot]", BODY: `Review finished at ${head}` });
  const posted = first.calls.find(c => c.args[1] === "comment").body;
  const again = gate({ EVENT: "comment", AUTHOR: "stormlight-thunderbot[bot]", BODY: `Review finished at ${head}` },
    { isDraft: false, comments: [{ body: posted }] });
  assert.equal(again.status, 0, again.stderr);
  assert(!again.calls.some(c => c.args[1] === "comment"));
});

test("a stopped pull request gets the success and no merge settings", () => {
  const { status, calls } = gate({ EVENT: "comment", AUTHOR: "stormlight-thunderbot[bot]", BODY: `Review finished at ${head}` },
    { labels: [{ name: "storm:stopped" }] });
  assert.equal(status, 0);
  assert(!calls.some(c => c.args[1] === "ready" || c.args[1] === "merge" || c.args[1] === "comment"));
});

test("/storm:stop still turns auto-merge off", () => {
  const { status, calls } = gate({ EVENT: "comment", AUTHOR: "desertthunder", BODY: "/storm:stop" });
  assert.equal(status, 0);
  assert(calls.some(c => c.args[1] === "merge" && c.args.includes("--disable-auto")));
});
