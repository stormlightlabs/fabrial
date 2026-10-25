import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { dispatch, invoke, startWorker } from "./dispatch.mjs";
import { WorkerMonitor, cancelWorkers } from "./workers.mjs";
import { WorkerJournal, dedupeNotifications } from "./journal.mjs";

const host = { binary: "/trusted/fab", config: "/private/issuer.toml", trustedCheckout: "/trusted/main" };
const params = { role: "reviewer", taskFile: "/private/task", worktree: "/work/pr" };
function fakeLaunch(action, inspect = () => {}) {
  return (binary, args, options) => {
    inspect(binary, args, options);
    const child = new EventEmitter();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    child.kill = (signal) => { child.emit("killed", signal); return true; };
    queueMicrotask(() => action(child));
    return child;
  };
}

test("dispatch streams LF progress and returns only final role result", async () => {
  const updates = [];
  const result = await dispatch(params, undefined, (text) => updates.push(text), host,
    fakeLaunch((child) => {
      child.stdout.write('{"type":"progress","event":"turn_start"}\n{"type":"res');
      child.stdout.write('ult","text":"one two"}\n');
      child.emit("close", 0);
    }, (binary, args, options) => {
      assert.equal(binary, host.binary);
      assert.deepEqual(args, ["dispatch", "reviewer", "--task", params.taskFile,
        "--worktree", params.worktree, "--trusted-checkout", host.trustedCheckout,
        "--config", host.config]);
      assert.deepEqual(options.stdio, ["ignore", "pipe", "pipe"]);
    }));
  assert.equal(result, "one two");
  assert.deepEqual(updates, ["turn_start"]);
});

test("failure, missing result, malformed stream and spawn error reject", async () => {
  for (const action of [
    (c) => { c.stdout.write('{"type":"result","text":"partial"}\n'); c.emit("close", 1); },
    (c) => { c.emit("close", 0); },
    (c) => { c.stdout.write("bad\n"); c.emit("close", 0); },
    (c) => { c.emit("error", new Error("private host diagnostics")); c.emit("close", -1); },
    (c) => { c.stdout.write('{"type":"error","message":"worker failed"}\n'); c.emit("close", 0); },
  ]) {
    await assert.rejects(dispatch(params, undefined, undefined, host, fakeLaunch(action)),
      (error) => !error.message.includes("private host diagnostics"));
  }
});

test("cancellation reaches the dispatcher and cannot return success", async () => {
  const controller = new AbortController();
  let killed;
  const promise = dispatch(params, controller.signal, undefined, host, fakeLaunch((child) => {
    child.on("killed", (sig) => { killed = sig; child.emit("close", 0); });
    controller.abort();
  }));
  await assert.rejects(promise, /cancelled/);
  assert.equal(killed, "SIGTERM");
});

test("host configuration is required; pre-cancelled calls never spawn", async () => {
  const never = () => { throw new Error("must not spawn"); };
  await assert.rejects(dispatch(params, undefined, undefined, {}, never), /FAB_DISPATCH_CONFIG/);
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(dispatch(params, controller.signal, undefined, host, never), /abort/i);
});

function sessionJournal() {
  const entries = [], sent = [];
  return { entries, sent, options: {
    entries: () => entries,
    append: (customType, data) => entries.push({ type: "custom", customType, data }),
    send: (message) => sent.push(message),
  } };
}

test("undelivered outbox survives restart and duplicate queued messages are deduped", () => {
  const session = sessionJournal();
  const journal = new WorkerJournal(session.options);
  journal.enqueue({ key: "one:terminal", worker: { worker_id: "one", state: "succeeded", result: "done" } });
  journal.flush(); assert.equal(session.sent.length, 1);
  const restored = new WorkerJournal(session.options); restored.flush();
  assert.equal(session.sent.length, 2); // A new process cannot assume the old queue survived.
  const messages = session.sent.map((message) => ({ ...message, role: "custom" }));
  assert.equal(dedupeNotifications(messages).length, 1);
});

test("watching an existing worker cannot rewind a checkpoint or reopen completion", () => {
  const session = sessionJournal();
  const journal = new WorkerJournal(session.options);
  const worker = { worker_id: "watched", state: "succeeded" };
  journal.checkpoint({ worker, cursor: 250, reported: true, done: true });
  const state = journal.checkpoint({ worker, cursor: 0, reported: false, done: false });
  assert.equal(state.cursor, 250); assert.equal(state.reported, true); assert.equal(state.done, true);
  assert.equal(session.entries.length, 1);
  assert.equal(new WorkerJournal(session.options).states.get("watched").cursor, 250);
});

test("committed custom-message receipt prevents delivery after restart", () => {
  const session = sessionJournal();
  const journal = new WorkerJournal(session.options);
  journal.enqueue({ key: "one:terminal", worker: { worker_id: "one", state: "succeeded", result: "done" } });
  session.entries.push({ ...session.sent[0], type: "custom_message" });
  new WorkerJournal(session.options).flush();
  assert.equal(session.sent.length, 1);
});

test("completed checkpoints still replay an undelivered notification without relaunching", async () => {
  const session = sessionJournal();
  const journal = new WorkerJournal(session.options);
  const worker = { worker_id: "one", state: "succeeded", result: "full result" };
  journal.checkpoint({ worker, cursor: 20, reported: false, done: true });
  journal.checkpoint({ worker, cursor: 20, reported: false, done: true });
  assert.equal(session.entries.length, 1);
  assert.equal(session.entries[0].data.worker.result, undefined);
  journal.enqueue({ key: "one:terminal", worker });
  const restored = new WorkerJournal(session.options);
  assert.equal(restored.states.get("one").cursor, 20);
  const monitor = new WorkerMonitor({ fetch: () => { throw new Error("must not fetch completed work"); },
    notify: () => {}, replay: () => restored.flush() });
  await monitor.poll(); monitor.stop();
  assert.equal(session.sent.length, 2);
});

test("worker and PR journals share a session but each delivers only its own messages", () => {
  const session = sessionJournal();
  const workers = new WorkerJournal(session.options);
  const prs = new WorkerJournal({ ...session.options, customType: "fabrial.pr-event" });
  workers.enqueue({ key: "one:terminal", worker: { worker_id: "one", state: "succeeded", result: "done" } });
  prs.enqueue({ key: "pr:owner/repo#7:merged", content: "PR merged" });
  assert.deepEqual(session.sent.map((m) => m.customType), ["fabrial.worker-event", "fabrial.pr-event"]);
  // After a reload each journal replays only its own undelivered item.
  const restored = new WorkerJournal(session.options);
  const restoredPrs = new WorkerJournal({ ...session.options, customType: "fabrial.pr-event" });
  assert.deepEqual([...restored.outbox.keys()], ["one:terminal"]);
  assert.deepEqual([...restoredPrs.outbox.keys()], ["pr:owner/repo#7:merged"]);
});

test("a stop cancels each worker and says which were asked and which failed", async () => {
  const asked = [];
  const cancel = async (id) => {
    asked.push(id);
    if (id === "w2") throw Object.assign(new Error("fab failed"), { stderr: "token ghs_secret" });
  };
  assert.equal(await cancelWorkers([], cancel), "No workers were active.");
  const outcome = await cancelWorkers(["w1", "w2"], cancel);
  assert.deepEqual(asked, ["w1", "w2"]);
  assert.match(outcome, /^Asked worker w1 to cancel\. Could not cancel worker w2\. Inspect each/);
  assert.doesNotMatch(outcome, /ghs_secret|fab failed/);
  assert.match(await cancelWorkers(["w3"], () => { throw new Error("sync"); }), /^Could not cancel worker w3\./);
});

test("a cancel that never settles, or outlives the session, counts as unconfirmed", async () => {
  const signals = [];
  const hang = (id, signal) => { signals.push(signal); return new Promise(() => {}); };
  const outcome = await cancelWorkers(["w1", "w2"], hang, { timeout: 20 });
  assert.equal(outcome, "Workers w1, w2 did not confirm cancellation. Inspect each with agent_control, cancel it again, and start nothing new until it is stopped.");
  assert.ok(signals.every((signal) => signal.aborted));
  const session = new AbortController();
  const pending = cancelWorkers(["w3"], hang, { timeout: 60000, signal: session.signal });
  session.abort();
  assert.match(await pending, /^Worker w3 did not confirm cancellation\./);
});

const worker = { worker_id: "worker-one", run_id: "run-one", task_head: "a".repeat(40), role: "reviewer", state: "running", credential_cleanup: "pending" };

test("native launch identity is captured even when dispatch exits nonzero afterward", async () => {
  const accepted = [];
  await assert.rejects(invoke(["dispatch"], undefined, undefined, host, fakeLaunch((child) => {
    child.stdout.write(JSON.stringify({ type: "started", worker }) + "\n");
    child.emit("close", 1);
  }), (record) => { if (record.type === "started") accepted.push(record.worker.worker_id); }));
  assert.deepEqual(accepted, [worker.worker_id]);
});

test("launch returns an accepted identity; abort after return does not cancel the worker", async () => {
  const controller = new AbortController();
  const accepted = [];
  let kills = 0;
  const result = await startWorker(params, controller.signal, host, (w) => accepted.push(w),
    fakeLaunch((child) => {
      child.on("killed", () => { kills++; });
      child.stdout.write(JSON.stringify({ type: "started", worker }) + "\n");
      child.emit("close", 0);
    }, (_binary, args) => assert.equal(args.at(-1), "--detach")));
  assert.equal(result.worker_id, worker.worker_id);
  assert.equal(accepted.length, 1);
  controller.abort();
  assert.equal(kills, 0);
});

test("aborted launch retains the accepted identity and requests explicit cancellation", async () => {
  const controller = new AbortController();
  const accepted = [];
  const commands = [];
  await assert.rejects(startWorker(params, controller.signal, host, (w) => accepted.push(w),
    fakeLaunch((child) => {
      if (commands.length === 1) {
        child.on("killed", () => child.emit("close", 0));
        child.stdout.write(JSON.stringify({ type: "started", worker }) + "\n");
        controller.abort();
      } else {
        child.stdout.write('{"type":"cancel_requested"}\n'); child.emit("close", 0);
      }
    }, (_binary, args) => commands.push(args))), /cancelled/);
  assert.equal(accepted.length, 1);
  assert.deepEqual(commands[1].slice(0, 3), ["worker", "cancel", worker.worker_id]);
});

test("monitor progress never wakes a model and completion is delivered once", async () => {
  const notifications = [];
  let calls = 0;
  const monitor = new WorkerMonitor({
    fetch: async () => ({ worker: calls++ === 0 ? worker : { ...worker, state: "succeeded", credential_cleanup: "complete", result: "done" },
      cursor: 10, events: [{ sequence: 10, event: { type: "progress" } }] }),
    notify: (event) => notifications.push(event),
  });
  monitor.track(worker);
  await monitor.poll(); assert.equal(notifications.length, 0);
  await monitor.poll(); await monitor.poll();
  assert.equal(notifications.length, 1);
  assert.equal(calls, 2);
  monitor.stop(); monitor.stop();
});

test("monitor progress names each worker's role, state and run, and nothing else from its record", async () => {
  const progress = [];
  let calls = 0;
  const monitor = new WorkerMonitor({
    fetch: async () => ({ worker: { ...worker, state: calls++ === 0 ? "running" : "succeeded", credential_cleanup: "complete", result_file: "/private/result.txt" },
      cursor: calls, events: calls === 1 ? [{ sequence: 1, event: { type: "progress", event: "turn_start" } }] : [] }),
    notify: () => {}, progress: (event) => progress.push(event),
  });
  monitor.track(worker);
  await monitor.poll(); await monitor.poll();
  const summary = (state) => ({ role: worker.role, state, run_id: worker.run_id });
  assert.deepEqual(progress, [
    { workerId: worker.worker_id, worker: summary("running") },
    { workerId: worker.worker_id, worker: summary("running"), sequence: 1, event: { type: "progress", event: "turn_start" } },
    { workerId: worker.worker_id, worker: summary("succeeded") },
  ]);
  monitor.stop();
});

test("blocker notification is immediate and is not repeated on clean completion", async () => {
  let calls = 0;
  const notifications = [];
  const monitor = new WorkerMonitor({
    fetch: async () => ({ worker: calls++ === 0 ? worker : { ...worker, state: "blocked", credential_cleanup: "complete" },
      events: [{ sequence: 20, event: { type: "worker_report", kind: "blocked", text: "need input" } }], cursor: 20 }),
    notify: (event) => notifications.push(event),
  });
  monitor.track(worker); await monitor.poll();
  assert.equal(notifications.length, 1);
  await monitor.poll(); assert.equal(notifications.length, 1);
  monitor.stop();
});

test("shutdown disposes observers, not workers, and rejects late notifications", async () => {
  let finish;
  const notifications = [];
  const monitor = new WorkerMonitor({ fetch: () => new Promise((resolve) => { finish = resolve; }),
    notify: (event) => notifications.push(event) });
  monitor.track(worker);
  const pending = monitor.poll(); await Promise.resolve(); monitor.stop();
  finish({ worker: { ...worker, state: "succeeded" }, events: [], cursor: 0 });
  await pending; assert.equal(notifications.length, 0);
});

test("terminal state does not discard events beyond the first replay batch", async () => {
  let calls = 0;
  const notifications = [];
  const monitor = new WorkerMonitor({
    fetch: async (_id, cursor) => {
      assert.equal(cursor, calls === 0 ? 0 : 64);
      return { worker: { ...worker, state: "blocked", credential_cleanup: "complete" },
        events: calls++ === 0 ? Array.from({ length: 64 }, (_, i) => ({ sequence: i + 1, event: { type: "progress" } }))
          : [{ sequence: 65, event: { type: "worker_report", kind: "blocked", text: "need input" } }], cursor: 65 };
    }, notify: (event) => notifications.push(event),
  });
  monitor.track(worker); await monitor.poll(); assert.equal(notifications.length, 0);
  await monitor.poll(); assert.equal(notifications.length, 1);
  assert.equal(notifications[0].event.text, "need input");
  monitor.stop();
});

test("observer errors notify once and back off without launching replacement work", async () => {
  let calls = 0;
  const notifications = [];
  const monitor = new WorkerMonitor({ fetch: async () => { calls++; throw new Error("offline"); },
    notify: (event) => notifications.push(event) });
  monitor.track(worker); await monitor.poll(); await monitor.poll();
  assert.equal(calls, 1); assert.equal(notifications.length, 1);
  assert.equal(monitor.workers.size, 1);
  monitor.stop();
});
