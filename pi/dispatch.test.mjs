import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import { dispatch } from "./dispatch.mjs";

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
