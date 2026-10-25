process.env.TZ = "UTC";
import assert from "node:assert/strict";
import test from "node:test";
import { applyWorker, readGit, renderStatus, renderWorkers, repository, statusArea } from "./status.mjs";

const head = "a".repeat(40), local = "b".repeat(40);
const observedAt = Date.parse("2026-10-25T12:03:00Z");
// Colors show as <color>text</color> and links as [text](url), so tests can read both.
const theme = { fg: (c, t) => `<${c}>${t}</${c}>`, bold: (t) => t, link: (t, url) => `[${t}](${url})` };
const plain = (line) => line.replace(/<\/?\w+>/g, "").replace(/\[([^\]]*)\]\([^)]*\)/g, "$1");
const pr = (overrides = {}) => ({
  status: "success", observedAt, key: `owner/repo#7@${head}`, branch: "storm/task",
  pr: { repository: "owner/repo", number: 7, url: "https://github.com/owner/repo/pull/7", head, draft: true, state: "open", stopped: false },
  checks: { total: 4, passed: 4, pending: 0, failed: 0, items: [] }, reviews: { approved: 0, changesRequested: 0 },
  ...overrides,
});
const git = { repository: "owner/repo", branch: "storm/task", head, ahead: 2, behind: 0, diff: { files: 5, added: 120, deleted: 8 } };
const state = (overrides = {}) => ({ pr: pr(), git, workers: {}, ...overrides });
const render = (s, width = 120) => {
  const lines = renderStatus(s, width, theme);
  for (const line of lines) assert.ok(plain(line).length <= width, `"${plain(line)}" fits ${width}`);
  return lines;
};

test("a wide terminal shows the repository, branch, PR, checks and diff", () => {
  const [first, second] = render(state(), 140);
  assert.equal(plain(render(state())[0]), "PR #7 · draft · checks 4/4 passed · owner/repo · storm/task ↑2 ↓0 · +120 -8 in 5 files");
  assert.equal(plain(first), "PR #7 · draft · checks 4/4 passed · https://github.com/owner/repo/pull/7 · owner/repo · storm/task ↑2 ↓0 · +120 -8 in 5 files");
  assert.match(first, /\[PR #7\]\(https:\/\/github.com\/owner\/repo\/pull\/7\)/);
  assert.match(first, /<success>checks 4\/4 passed<\/success>/);
  assert.equal(plain(second), "step unknown · workers none");
});

test("a narrow terminal keeps the PR, checks and step and drops the rest", () => {
  const [first, second] = render(state({ pr: pr({ status: "pending", checks: { total: 4, passed: 3, pending: 1, failed: 0, items: [] } }) }), 60);
  assert.equal(plain(first), "PR #7 · draft · checks 3/4 passed, 1 pending · owner/repo");
  assert.match(first, /<warning>checks 3\/4 passed, 1 pending<\/warning>/);
  assert.equal(plain(second), "step unknown · workers none");
  const [tiny] = render(state(), 20);
  assert.equal(plain(tiny), "PR #7 · draft · che…");
});

test("a branch with no PR says so, and an unread one is unknown", () => {
  assert.equal(plain(render(state({ pr: { status: "unknown", branch: "storm/task", observedAt } }))[0]),
    "no PR · owner/repo · storm/task ↑2 ↓0 · +120 -8 in 5 files");
  assert.equal(plain(render(state({ pr: undefined, git: undefined }))[0]), "PR unknown");
});

test("failed checks are red and labelled", () => {
  const [first] = render(state({ pr: pr({ status: "failed", checks: { total: 4, passed: 2, pending: 1, failed: 1, items: [] } }) }));
  assert.match(first, /<error>checks 2\/4 passed, 1 failed, 1 pending<\/error>/);
});

test("a moved head shows the old checks as stale with their head and read time, never green", () => {
  for (const s of [state({ pr: pr({ status: "stale" }) }), state({ git: { ...git, head: local } })]) {
    const [first] = render(s);
    assert.doesNotMatch(first, /<success>/);
    assert.match(plain(first), /checks stale \((head may have moved|local head differs); 4\/4 passed at aaaaaaa, read 12:03\)/);
  }
});

test("a rate limit or failed lookup is unknown or stale, never green", () => {
  const limited = { error: "GitHub rate limit", retryAt: observedAt + 5 * 60000 };
  assert.equal(plain(render(state({ pr: { status: "unknown", ...limited } }))[0]).split(" · ")[0],
    "PR unknown (GitHub rate limit, retry 12:08)");
  const [first] = render(state({ pr: pr({ status: "stale", ...limited }) }));
  assert.doesNotMatch(first, /<success>/);
  assert.match(plain(first), /checks stale \(GitHub rate limit, retry 12:08; 4\/4 passed at aaaaaaa, read 12:03\)/);
  assert.match(plain(render(state({ pr: pr({ checks: { total: 0, passed: 0, pending: 0, failed: 0, items: [] }, status: "unknown" }) }))[0]),
    /checks none/);
});

test("worker events set the step, the counts and the widget, and keep report text out", () => {
  let s = state();
  s = applyWorker(s, { workerId: "w1-0000000000", worker: { role: "reviewer", state: "running", run_id: "r1-run-000000" } });
  s = applyWorker(s, { workerId: "w1-0000000000", sequence: 3, event: { type: "progress", event: "tool_execution_start" } });
  s = applyWorker(s, { workerId: "w2-0000000000", worker: { role: "reviser", state: "blocked" },
    event: { type: "worker_report", kind: "blocked", text: "ghs_secret \x1b]8;;evil" } });
  s = applyWorker(s, { workerId: "w3-0000000000", worker: { role: "verifier", state: "succeeded" } });
  assert.equal(plain(render(s)[1]), "step review · workers 1 active of 3, 1 blocked · run r1-run-0");
  const lines = renderWorkers(s, 120, theme).map(plain);
  assert.deepEqual(lines, ["reviewer · working · running a tool · w1-00000", "reviser · blocked · reported blocked · w2-00000"]);
  assert.ok(!lines.join("").includes("secret"));
  assert.deepEqual(renderWorkers(s, 20, theme).map(plain), ["reviewer · working", "reviser · blocked"]);
});

test("Git reads keep the remote's token out and leave failed reads unknown", async () => {
  assert.equal(repository("https://x-access-token:ghs_secret@github.com/owner/repo.git"), "owner/repo");
  assert.equal(repository("git@github.com:owner/repo.git"), "owner/repo");
  assert.equal(repository("https://example.com/owner/repo"), undefined);
  const replies = {
    "remote get-url origin": "https://x-access-token:ghs_secret@github.com/owner/repo.git\n",
    "rev-parse --abbrev-ref HEAD": "storm/task\n", "rev-parse HEAD": `${head}\n`,
    "rev-parse --abbrev-ref origin/HEAD": "origin/main\n", [`merge-base origin/main HEAD`]: "c".repeat(40),
    [`diff --shortstat ${"c".repeat(40)}`]: " 1 file changed, 3 insertions(+)\n",
  };
  const run = async (_command, args) => {
    const reply = replies[args.join(" ")];
    if (reply === undefined) throw new Error("no upstream");
    return reply;
  };
  assert.deepEqual(await readGit(run), { repository: "owner/repo", branch: "storm/task", head,
    ahead: undefined, behind: undefined, diff: { files: 1, added: 3, deleted: 0 } });
  assert.equal(plain(render(state({ git: await readGit(run) }))[0]).split(" · ").slice(-2).join(" · "), "storm/task · +3 -0 in 1 file");
});

// A fake Pi: handlers, an event bus and a UI that records widgets and statuses.
function fakePi() {
  const handlers = {}, listeners = {};
  const pi = {
    on: (name, handler) => { handlers[name] = handler; },
    events: {
      on: (channel, handler) => {
        (listeners[channel] ??= new Set()).add(handler);
        return () => listeners[channel].delete(handler);
      },
      emit: (channel, data) => { for (const handler of listeners[channel] ?? []) handler(data); },
    },
  };
  const context = (mode) => {
    const ui = { widgets: {}, statuses: {}, renders: 0 };
    ui.setWidget = (key, factory) => {
      ui.widgets[key] = factory && factory({ requestRender: () => ui.renders++ }, { fg: (_c, t) => t, bold: (t) => t });
    };
    ui.setStatus = (key, text) => { ui.statuses[key] = text; };
    return { mode, hasUI: mode !== "print", cwd: "/work", ui };
  };
  const count = () => Object.values(listeners).reduce((n, set) => n + set.size, 0);
  return { pi, handlers, context, count };
}

test("the TUI status follows events, and a reload releases the old session", async () => {
  const fake = fakePi();
  const reads = [];
  statusArea(fake.pi, { run: async (_c, args) => { reads.push(args.join(" ")); throw new Error("no git"); } });
  const first = fake.context("tui");
  fake.handlers.session_start({}, first);
  assert.equal(fake.count(), 2);
  assert.match(first.ui.widgets["fabrial-status"].render(120)[0], /PR unknown/);
  fake.pi.events.emit("fabrial:pr", pr({ status: "failed", checks: { total: 1, passed: 0, pending: 0, failed: 1, items: [] } }));
  assert.match(plain(first.ui.widgets["fabrial-status"].render(120)[0]), /checks 0\/1 passed, 1 failed/);
  assert.equal(first.ui.widgets["fabrial-workers"], undefined);
  fake.pi.events.emit("fabrial:worker", { workerId: "w1", worker: { role: "reviewer", state: "running" } });
  assert.match(first.ui.widgets["fabrial-workers"].render(80)[0], /reviewer · working/);
  assert.ok(first.ui.renders > 0);

  // /reload shuts the session down and starts it again with a new context.
  fake.handlers.session_shutdown({ reason: "reload" }, first);
  assert.equal(fake.count(), 0);
  assert.equal(first.ui.widgets["fabrial-status"], undefined);
  assert.equal(first.ui.widgets["fabrial-workers"], undefined);
  const second = fake.context("tui");
  fake.handlers.session_start({ reason: "reload" }, second);
  assert.equal(fake.count(), 2);
  fake.pi.events.emit("fabrial:pr", pr());
  assert.match(plain(second.ui.widgets["fabrial-status"].render(120)[0]), /checks 4\/4 passed/);
  fake.handlers.session_shutdown({ reason: "quit" }, second);
  assert.equal(fake.count(), 0);
});

test("RPC gets plain status text, and headless sessions get nothing and read nothing", async () => {
  const fake = fakePi();
  const reads = [];
  statusArea(fake.pi, { run: async (_c, args) => { reads.push(args); throw new Error("no git"); } });
  const rpc = fake.context("rpc");
  fake.handlers.session_start({}, rpc);
  fake.pi.events.emit("fabrial:pr", pr());
  assert.equal(rpc.ui.statuses.fabrial, "PR #7 · draft · checks 4/4 passed · owner/repo · step unknown · workers none");
  assert.deepEqual(rpc.ui.widgets, {});
  fake.handlers.session_shutdown({}, rpc);
  assert.equal(rpc.ui.statuses.fabrial, undefined);

  reads.length = 0;
  const print = fake.context("print");
  fake.handlers.session_start({}, print);
  fake.pi.events.emit("fabrial:pr", pr());
  fake.handlers.turn_end?.({}, print);
  assert.equal(fake.count(), 0);
  assert.deepEqual(print.ui.widgets, {});
  assert.deepEqual(print.ui.statuses, {});
  assert.equal(reads.length, 0);
});
