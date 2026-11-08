import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { PullRequestCollector, MIN_INTERVAL, movesBranch } from "./pr.mjs";

const worktree = fileURLToPath(new URL("../../../", import.meta.url));
const oldHead = "a".repeat(40), newHead = "b".repeat(40);
const base = { number: 7, url: "https://github.com/owner/repo/pull/7", state: "OPEN", isDraft: true,
  headRefName: "storm/test", headRefOid: oldHead, reviews: [],
  statusCheckRollup: [{ __typename: "CheckRun", name: "check", status: "COMPLETED", conclusion: "SUCCESS" }] };

// A fake `gh` and `git` on PATH. `gh pr view` answers with the JSON in
// reply.json, and `gh pr list` with its `list` field or else that PR as the
// branch's only one. Either fails as {"exit": n, "stderr": "..."} says; both log each call.
function host(t) {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, "pr-test-"));
  t.after(() => rmSync(root, { recursive: true, force: false }));
  const reply = join(root, "reply.json"), log = join(root, "calls.jsonl");
  for (const name of ["gh", "git"]) {
    writeFileSync(join(root, name), `#!/usr/bin/env node
import fs from "node:fs";
fs.appendFileSync(${JSON.stringify(log)}, JSON.stringify(["${name}", ...process.argv.slice(2)]) + "\\n");
if ("${name}" === "git") { console.log(process.env.TEST_BRANCH ?? "storm/test"); process.exit(0); }
const reply = JSON.parse(fs.readFileSync(${JSON.stringify(reply)}, "utf8"));
if (reply.exit) { process.stderr.write(reply.stderr); process.exit(reply.exit); }
if (process.argv[3] === "list") {
  console.log(JSON.stringify(reply.list ?? [{ number: reply.number, isCrossRepository: false, createdAt: "2026-10-25T09:00:00Z" }]));
} else console.log(JSON.stringify(reply.views?.[process.argv[4]] ?? reply));
`, { mode: 0o700 });
  }
  let now = Date.parse("2026-10-25T12:00:00Z");
  const snapshots = [];
  const make = (options = {}) => new PullRequestCollector({ cwd: worktree, clock: () => now,
    env: { ...process.env, PATH: `${root}:${process.env.PATH}`, GH_TOKEN: "ghs_secret" },
    progress: (s) => snapshots.push(s), ...options });
  return {
    make, snapshots, log,
    set: (pr) => writeFileSync(reply, JSON.stringify(pr)),
    advance: (ms) => { now += ms; },
    // Each read starts with one `gh pr list`.
    calls: () => readFileSync(log, "utf8").trim().split("\n").map((line) => JSON.parse(line)).filter((c) => c[0] === "gh" && c[2] === "list"),
  };
}

test("a new head replaces the old head's checks and reviews", async (t) => {
  const h = host(t);
  h.set({ ...base, reviews: [{ id: "R1", state: "APPROVED", authorAssociation: "OWNER", submittedAt: "2026-10-25T11:00:00Z", commit: { oid: oldHead } }] });
  const collector = h.make();
  await collector.poll();
  assert.equal(collector.get().status, "success");
  assert.equal(collector.get().key, `owner/repo#7@${oldHead}`);
  assert.equal(collector.get().reviews.approved, 1);
  assert.deepEqual(h.calls()[0].slice(0, 5), ["gh", "pr", "list", "--head", "storm/test"]);

  collector.refresh(); // The session pushed.
  assert.equal(collector.get().status, "stale");
  h.set({ ...base, headRefOid: newHead,
    statusCheckRollup: [{ __typename: "CheckRun", name: "check", status: "IN_PROGRESS", conclusion: "" }] });
  h.advance(MIN_INTERVAL);
  await collector.poll();
  assert.equal(collector.get().status, "pending");
  assert.equal(collector.get().pr.head, newHead);
  assert.equal(collector.get().reviews.approved, 0);
  assert.ok(h.snapshots.length >= 3);
  collector.stop();
});

test("reads are 30 s apart, errors back off and leave the last head stale, never green", async (t) => {
  const h = host(t);
  h.set(base);
  const collector = h.make();
  await collector.poll();
  h.advance(MIN_INTERVAL - 1); await collector.poll();
  assert.equal(h.calls().length, 1);

  h.set({ exit: 1, stderr: "HTTP 502 for token ghs_secret" });
  h.advance(1); await collector.poll();
  assert.equal(collector.get().status, "stale");
  assert.equal(collector.get().checks.passed, 1);
  assert.equal(collector.get().error, "GitHub lookup failed");
  assert.doesNotMatch(JSON.stringify(h.snapshots), /ghs_secret|502/);
  h.advance(MIN_INTERVAL); await collector.poll();
  assert.equal(h.calls().length, 2); // Backed off to 60 s.
  h.advance(MIN_INTERVAL); await collector.poll();
  assert.equal(h.calls().length, 3);

  h.set({ exit: 1, stderr: "API rate limit exceeded" });
  h.advance(4 * MIN_INTERVAL); await collector.poll();
  assert.equal(collector.get().error, "GitHub rate limit");
  h.advance(4 * 60000); await collector.poll();
  assert.equal(h.calls().length, 4); // A rate limit waits at least 5 minutes.
  h.set(base);
  h.advance(60000); await collector.poll();
  assert.equal(collector.get().status, "success");
  collector.stop();
});

test("a failed first read and a branch with no PR are unknown", async (t) => {
  const h = host(t);
  h.set({ exit: 1, stderr: "connection refused" });
  const collector = h.make();
  await collector.poll();
  assert.equal(collector.get().status, "unknown");
  assert.equal(collector.get().pr, undefined);
  h.set({ list: [] });
  h.advance(10 * MIN_INTERVAL); await collector.poll();
  assert.equal(collector.get().status, "unknown");
  assert.equal(collector.get().error, undefined);
  collector.stop();
});

test("only each author's latest review on the head counts", async (t) => {
  const h = host(t);
  const review = (id, state, login, submittedAt, authorAssociation = "MEMBER") =>
    ({ id, state, author: { login }, authorAssociation, submittedAt, commit: { oid: oldHead } });
  h.set({ ...base, reviews: [
    review("R1", "CHANGES_REQUESTED", "owner", "2026-10-25T11:00:00Z"),
    review("R2", "APPROVED", "owner", "2026-10-25T12:00:00Z"),
    review("R3", "APPROVED", "member", "2026-10-25T11:00:00Z"),
    review("R4", "COMMENTED", "member", "2026-10-25T12:00:00Z"),
  ] });
  const collector = h.make();
  await collector.poll();
  assert.deepEqual(collector.get().reviews, { approved: 2, changesRequested: 0 });

  h.set({ ...base, reviews: [
    review("R1", "APPROVED", "owner", "2026-10-25T11:00:00Z"),
    review("R2", "CHANGES_REQUESTED", "owner", "2026-10-25T12:00:00Z"),
  ] });
  h.advance(MIN_INTERVAL); await collector.poll();
  assert.deepEqual(collector.get().reviews, { approved: 0, changesRequested: 1 });

  // A drive-by review from outside the repository counts for nothing.
  h.set({ ...base, reviews: [
    review("R1", "CHANGES_REQUESTED", "stranger", "2026-10-25T11:00:00Z", "NONE"),
    review("R2", "APPROVED", "contributor", "2026-10-25T11:00:00Z", "CONTRIBUTOR"),
  ] });
  h.advance(MIN_INTERVAL); await collector.poll();
  assert.deepEqual(collector.get().reviews, { approved: 0, changesRequested: 0 });
  collector.stop();
});

test("merges, closures, failed checks and requested changes are readings only", async (t) => {
  const h = host(t);
  h.set({ ...base,
    statusCheckRollup: [{ __typename: "StatusContext", context: "ci", state: "FAILURE" }],
    reviews: [
      { id: "R1", state: "CHANGES_REQUESTED", authorAssociation: "MEMBER", body: "Fix it", submittedAt: "2026-10-25T11:00:00Z", commit: { oid: oldHead } },
      { id: "R2", state: "CHANGES_REQUESTED", authorAssociation: "OWNER", body: "Other head", submittedAt: "2026-10-25T11:00:00Z", commit: { oid: newHead } },
    ] });
  // A collector handed the callbacks the earlier PR status took must never call them.
  let called = 0;
  const collector = h.make({ notify: () => called++, replay: () => called++ });
  await collector.poll();
  assert.equal(collector.get().status, "failed");
  assert.deepEqual(collector.get().checks.items, [{ name: "ci", state: "failed" }]);
  assert.equal(collector.get().reviews.changesRequested, 1);
  assert.doesNotMatch(JSON.stringify(collector.get()), /Fix it/); // Snapshots carry no PR text.

  for (const state of ["MERGED", "CLOSED"]) {
    h.set({ ...base, state });
    h.advance(MIN_INTERVAL); await collector.poll();
    assert.equal(collector.get().pr.state, state.toLowerCase());
  }
  // Labels and comments, where `storm:stopped` and `/storm:stop` lived, are never read.
  const view = readFileSync(h.log, "utf8").trim().split("\n").map((line) => JSON.parse(line)).find((c) => c[2] === "view");
  assert.doesNotMatch(view.at(-1), /labels|comments/);
  assert.equal(called, 0);
  collector.stop();
});

test("dispose stops reads, drops late results and listeners", async (t) => {
  const h = host(t);
  h.set({ ...base, state: "MERGED" });
  const collector = h.make();
  const seen = [];
  const unsubscribe = collector.subscribe((s) => seen.push(s));
  const pending = collector.poll();
  collector.stop();
  await pending;
  h.advance(MIN_INTERVAL); await collector.poll(); collector.refresh();
  assert.equal(seen.length, 0);
  assert.equal(collector.get().status, "unknown");
  unsubscribe();
});

test("a read in flight across a push leaves the head stale", async (t) => {
  const h = host(t);
  h.set(base);
  const collector = h.make();
  await collector.poll();
  const run = collector.run.bind(collector);
  let release;
  collector.run = async (command, args) => {
    const out = await run(command, args);
    if (command === "gh" && args[1] === "view") await new Promise((resolve) => { release = resolve; });
    return out;
  };
  h.advance(MIN_INTERVAL);
  const pending = collector.poll();
  while (!release) await new Promise((resolve) => setTimeout(resolve, 5));
  collector.refresh(); // The session pushed while the old head's read was in flight.
  release();
  await pending;
  assert.equal(collector.get().status, "stale");
  assert.equal(collector.get().pr.head, oldHead);
  collector.stop();
});

test("the lookup takes the newest PR from the base repository, never a fork's", async (t) => {
  const h = host(t);
  h.set({ ...base, list: [
    { number: 9, isCrossRepository: true, createdAt: "2026-10-25T11:30:00Z" },
    { number: 5, isCrossRepository: false, createdAt: "2026-10-20T09:00:00Z" },
    { number: 7, isCrossRepository: false, createdAt: "2026-10-25T09:00:00Z" },
  ], views: { 5: { ...base, number: 5, url: "https://github.com/owner/repo/pull/5" },
    9: { ...base, number: 9, url: "https://github.com/owner/repo/pull/9" } } });
  const collector = h.make();
  await collector.poll();
  assert.equal(collector.get().pr.number, 7);

  h.set({ ...base, list: [{ number: 9, isCrossRepository: true, createdAt: "2026-10-25T11:30:00Z" }] });
  h.advance(MIN_INTERVAL); await collector.poll();
  assert.equal(collector.get().status, "unknown");
  assert.equal(collector.get().pr, undefined);
  collector.stop();
});

test("branch moves refresh", () => {
  for (const command of ["git push", "git checkout -b x", "cd w && git switch main", "gh pr checkout 7"]) assert.ok(movesBranch(command), command);
  for (const command of ["git status", "echo push", "gh pr view 7", undefined]) assert.ok(!movesBranch(command), String(command));
});
