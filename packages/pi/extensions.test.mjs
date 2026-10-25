// Loads the package's three extensions into a fake Pi runtime, in either
// order or alone, with fake `fab`, `gh` and `git` on PATH. Node strips the
// TypeScript; Pi's own packages are peer dependencies the gates cannot
// install, so they resolve to the small stubs below.
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const STUBS = {
  "@earendil-works/pi-ai": "export const Type = new Proxy({}, { get: () => (...args) => ({ args }) });" +
    "export const StringEnum = (values) => ({ type: 'string', enum: values });",
  "@earendil-works/pi-coding-agent": "export const defineTool = (tool) => tool;",
  "@earendil-works/pi-tui": "export const truncateToWidth = (line) => line; export const visibleWidth = (text) => text.length;",
};
registerHooks({
  resolve(specifier, context, next) {
    if (STUBS[specifier]) return { url: `data:text/javascript,${encodeURIComponent(STUBS[specifier])}`, shortCircuit: true };
    return next(specifier, context);
  },
});
const EXTENSIONS = {
  workers: (await import("./extensions/workers.ts")).default,
  pr: (await import("./extensions/pr-status.ts")).default,
  status: (await import("./extensions/status-area.ts")).default,
};

const worktree = fileURLToPath(new URL("../../", import.meta.url));
const head = "a".repeat(40);

// Fake host tools. `fab worker cancel` waits while the `hold` file exists.
function host(t) {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, "extensions-test-"));
  t.after(() => rmSync(root, { recursive: true, force: false }));
  const log = join(root, "fab.log"), hold = join(root, "hold");
  const program = (name, body) => writeFileSync(join(root, name), `#!/usr/bin/env node\n${body}`, { mode: 0o700 });
  program("fab", `
import fs from "node:fs";
const [, action, id] = process.argv.slice(2);
fs.appendFileSync(${JSON.stringify(log)}, action + " " + id + "\\n");
const reply = (record) => process.stdout.write(JSON.stringify(record) + "\\n");
if (action === "events") reply({ worker: { worker_id: id, role: "reviewer", state: "running", run_id: "run-1" }, events: [], supervisor_alive: true });
else if (action === "cancel") {
  const wait = () => fs.existsSync(${JSON.stringify(hold)}) ? setTimeout(wait, 20) : reply({ type: "cancel_requested" });
  wait();
} else process.exit(91);`);
  const pr = { number: 7, url: "https://github.com/owner/repo/pull/7", state: "OPEN", isDraft: true, headRefName: "storm/test",
    headRefOid: head, labels: [{ name: "storm:stopped" }], reviews: [], comments: [],
    statusCheckRollup: [{ __typename: "CheckRun", name: "check", status: "COMPLETED", conclusion: "SUCCESS" }] };
  program("gh", `
if (process.argv[3] === "list") console.log(JSON.stringify([{ number: 7, isCrossRepository: false, createdAt: "2026-10-25T09:00:00Z" }]));
else console.log(${JSON.stringify(JSON.stringify(pr))});`);
  program("git", `if (process.argv.includes("--abbrev-ref")) console.log("storm/test"); else process.exit(1);`);
  const env = { PATH: process.env.PATH, FAB_BINARY: process.env.FAB_BINARY, FAB_DISPATCH_CONFIG: process.env.FAB_DISPATCH_CONFIG,
    FAB_TRUSTED_CHECKOUT: process.env.FAB_TRUSTED_CHECKOUT };
  Object.assign(process.env, { PATH: `${root}:${env.PATH}`, FAB_BINARY: join(root, "fab"),
    FAB_DISPATCH_CONFIG: join(root, "config"), FAB_TRUSTED_CHECKOUT: root });
  t.after(() => { for (const [key, value] of Object.entries(env)) if (value === undefined) delete process.env[key]; else process.env[key] = value; });
  return {
    root, hold: () => writeFileSync(hold, ""), release: () => rmSync(hold),
    calls: () => (existsSync(log) ? readFileSync(log, "utf8").trim().split("\n") : []),
  };
}

// A session that outlives reloads; each `load` is one extension runtime, as Pi
// creates on startup and on every reload. Bus errors are collected. With
// `defer`, the bus delivers each event on a later task; Pi does not promise
// synchronous delivery.
function session(root, mode = "tui", defer = false) {
  const bus = new EventEmitter();
  const entries = [{ type: "custom", customType: "fabrial.worker",
    data: { worker: { worker_id: "w1", role: "reviewer" }, cursor: 0, reported: false, done: false } }];
  const sent = [], errors = [];
  const ui = { widgets: {}, statuses: {} };
  ui.setWidget = (key, factory) => {
    ui.widgets[key] = factory && factory({ requestRender() {} }, { fg: (_c, text) => text, bold: (text) => text });
  };
  ui.setStatus = (key, text) => { ui.statuses[key] = text; };
  const ctx = { mode, hasUI: mode !== "print", cwd: root, ui, sessionManager: {
    getSessionId: () => "session-1", getBranch: () => entries, getCwd: () => root,
    getHeader: () => ({ timestamp: "2026-10-25T10:00:00Z" }) } };
  const load = (names) => {
    const handlers = [], tools = new Map(), commands = new Map(), subscriptions = [];
    const pi = {
      on: (name, handler) => handlers.push([name, handler]),
      events: {
        emit: (channel, data) => (defer ? setTimeout(() => bus.emit(channel, data)) : bus.emit(channel, data)),
        on: (channel, handler) => {
          const safe = async (data) => { try { await handler(data); } catch (error) { errors.push(error); } };
          bus.on(channel, safe);
          const off = () => bus.off(channel, safe);
          subscriptions.push(off);
          return off;
        },
      },
      registerTool: (tool) => tools.set(tool.name, tool),
      registerCommand: (name, command) => commands.set(name, command),
      appendEntry: (customType, data) => entries.push({ type: "custom", customType, data }),
      sendMessage: (message) => sent.push(message),
    };
    for (const name of names) EXTENSIONS[name](pi);
    const fire = async (name, event = {}) => {
      for (const [n, handler] of handlers) if (n === name) await handler(event, ctx);
    };
    return {
      tools, commands, fire,
      start: () => fire("session_start", { reason: "startup" }),
      // Pi drops a runtime's bus subscriptions when it replaces the runtime.
      shutdown: async () => { await fire("session_shutdown", { reason: "reload" }); for (const off of subscriptions) off(); },
    };
  };
  const stops = () => sent.filter((m) => m.customType === "fabrial.pr-event");
  return { load, sent, stops, errors, ui };
}

async function until(condition, what) {
  const deadline = Date.now() + 8000;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(`Timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

for (const [order, defer] of [[["workers", "pr", "status"], false], [["status", "pr", "workers"], false],
  [["workers", "pr", "status"], true]]) {
  test(`loaded as ${order.join(", ")}${defer ? " with deferred events" : ""}: a stop cancels the workers and the status area renders`, async (t) => {
    const h = host(t);
    const s = session(h.root, "tui", defer);
    const runtime = s.load(order);
    assert.deepEqual([...runtime.tools.keys()], ["agent", "agent_control"]);
    assert.deepEqual([...runtime.commands.keys()], ["worker-watch", "worker-steer"]);
    await runtime.start();
    await until(() => s.stops().length && s.ui.widgets["fabrial-workers"], "the stop message and the worker widget");
    assert.ok(h.calls().includes("cancel w1"));
    assert.equal(s.stops().length, 1);
    assert.match(s.stops()[0].content, /Start nothing new\. Asked worker w1 to cancel\./);
    const links = /\x1b\]8;;[^\x1b]*\x1b\\/g; // OSC 8 hyperlinks around the PR number and repository.
    assert.match(s.ui.widgets["fabrial-status"].render(200)[0].replace(links, ""), /^PR #7 · draft, stopped · checks 1\/1 passed/);
    assert.match(s.ui.widgets["fabrial-workers"].render(80)[0], /reviewer · working/);
    await runtime.shutdown();
    assert.equal(s.ui.widgets["fabrial-status"], undefined);
    assert.deepEqual(s.errors, []);
  });
}

test("a reload before the cancels settle cancels again and queues one stop message", async (t) => {
  const h = host(t);
  const s = session(h.root);
  h.hold();
  const first = s.load(["workers", "pr", "status"]);
  await first.start();
  await until(() => h.calls().includes("cancel w1"), "the first cancel");
  await first.shutdown(); // Reload while the cancel is in flight.
  h.release();
  await new Promise((resolve) => setTimeout(resolve, 200));
  assert.equal(s.stops().length, 0);

  const second = s.load(["workers", "pr", "status"]);
  await second.start();
  await until(() => s.stops().length, "the stop message");
  assert.equal(h.calls().filter((call) => call === "cancel w1").length, 2);
  assert.match(s.stops()[0].content, /Asked worker w1 to cancel\./);
  await second.shutdown();
  assert.deepEqual(s.errors, []);
});

test("PR status alone reports a stop without a worker outcome", async (t) => {
  const h = host(t);
  const s = session(h.root);
  const runtime = s.load(["pr"]);
  await runtime.start();
  await until(() => s.stops().length, "the stop message");
  assert.match(s.stops()[0].content, /Start nothing new\.$/m);
  assert.doesNotMatch(s.stops()[0].content, /worker/i);
  assert.deepEqual(h.calls(), []);
  await runtime.shutdown();
  assert.deepEqual(s.errors, []);
});

test("the status area alone shows an unknown PR and no workers; print mode shows nothing", async (t) => {
  const h = host(t);
  const s = session(h.root);
  const runtime = s.load(["status"]);
  await runtime.start();
  assert.match(s.ui.widgets["fabrial-status"].render(200)[0], /PR unknown/);
  assert.equal(s.ui.widgets["fabrial-workers"], undefined);
  await runtime.shutdown();

  const print = session(h.root, "print");
  const headless = print.load(["status"]);
  await headless.start();
  await headless.fire("turn_end");
  assert.deepEqual(print.ui.widgets, {});
  assert.deepEqual(print.ui.statuses, {});
  await headless.shutdown();
});

test("workers alone observe their workers and answer no PR event", async (t) => {
  const h = host(t);
  const s = session(h.root);
  const runtime = s.load(["workers"]);
  await runtime.start();
  await until(() => h.calls().includes("events w1"), "the first worker read");
  await runtime.shutdown();
  assert.deepEqual(s.sent, []);
  assert.deepEqual(s.errors, []);
});
