#!/usr/bin/env node
// Opt-in integration proof: real subscription calls, scoped tokens, and the pinned sandbox.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { EventEmitter } from "node:events";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { invoke, workerOperation } from "../pi/dispatch.mjs";
import { WorkerJournal } from "../pi/journal.mjs";

const { values } = parseArgs({ options: { worktree: { type: "string" }, pi: { type: "string", default: "pi" } } });
assert(values.worktree, "--worktree must name a linked task worktree, never the primary checkout");
const worktree = resolve(values.worktree);
const extension = resolve(dirname(fileURLToPath(import.meta.url)), "../pi/extension.ts");
const host = { binary: process.env.FAB_BINARY, config: process.env.FAB_DISPATCH_CONFIG, trustedCheckout: process.env.FAB_TRUSTED_CHECKOUT };
assert(host.binary && host.config && host.trustedCheckout, "Set the three FAB host variables first");
const root = mkdtempSync(join(tmpdir(), "fab-async-proof-"));
const suffix = randomUUID().slice(0, 8);
const owned = [], workers = [], clients = [], facts = [];
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const git = (...args) => execFileSync("git", ["-C", worktree, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
const op = (action, id, extra = []) => workerOperation(action, id, extra, undefined, host);
const terminal = (w) => ["succeeded", "failed", "blocked", "timed_out", "cancelled"].includes(w.state);

class Rpc {
  constructor(session) {
    const args = ["--mode", "rpc", "--no-extensions", "--no-skills", "--no-prompt-templates", "--no-context-files", "--no-mcp",
      "--extension", extension, "--model", "openai/gpt-6-sol", "--thinking", "medium", "--tools", "agent,agent_control"];
    if (session) args.push("--session", session);
    else args.push("--session-dir", join(root, "sessions"));
    this.events = []; this.changed = new EventEmitter();
    this.child = spawn(values.pi, args, { cwd: worktree, env: process.env, stdio: ["pipe", "pipe", "pipe"] });
    this.child.stderr.resume();
    this.child.stdin.on("error", () => {});
    this.child.on("error", () => { this.failure = "Pi could not start"; this.changed.emit("change"); });
    this.closed = new Promise((resolve) => this.child.once("close", (code) => {
      this.exited = true; this.failure ||= `Pi exited (${code})`; this.changed.emit("change"); resolve();
    }));
    createInterface({ input: this.child.stdout }).on("line", (line) => {
      try { this.events.push(JSON.parse(line)); }
      catch { this.failure = "Pi emitted invalid JSONL"; }
      this.changed.emit("change");
    });
    clients.push(this);
  }
  wait(predicate, after = 0, timeout = 180000) {
    return new Promise((resolve, reject) => {
      const finish = () => {
        const event = this.events.slice(after).find(predicate);
        if (!event && !this.failure) return;
        clearTimeout(timer); this.changed.off("change", finish);
        if (event) resolve(event); else reject(new Error(this.failure));
      };
      const timer = setTimeout(() => { this.changed.off("change", finish); reject(new Error("Pi RPC acceptance deadline exceeded")); }, timeout);
      this.changed.on("change", finish); finish();
    });
  }
  request(type, extra = {}) {
    const id = randomUUID();
    const reply = this.wait((e) => e.type === "response" && e.id === id);
    this.child.stdin.write(JSON.stringify({ id, type, ...extra }) + "\n");
    return reply.then((r) => { assert.equal(r.success, true, `RPC ${type} rejected`); return r.data; });
  }
  answer(text, after = 0) {
    return this.wait((e) => e.type === "message_end" && e.message?.role === "assistant" && e.message.stopReason === "stop"
      && e.message.content?.filter((c) => c.type === "text").map((c) => c.text).join("\n").trim() === text, after);
  }
  async close() {
    if (this.exited) return;
    this.child.stdin.end();
    await Promise.race([this.closed, pause(5000)]);
    if (!this.exited) { this.child.kill("SIGTERM"); await Promise.race([this.closed, pause(5000)]); }
    if (!this.exited) { this.child.kill("SIGKILL"); await this.closed; }
  }
}

function task(name, seconds, result) {
  const path = join(root, `${name}.txt`);
  writeFileSync(path, `This is an integration acceptance case, not a code review. Do not edit, push, comment, dispatch agents, or inspect unrelated files. Run the Bash tool with exactly 'sleep ${seconds}'. Then call worker_report with kind progress and text LIVE-REPORT. Finally return exactly ${result}.\n`, { mode: 0o600 });
  return path;
}

async function inspectUntil(id, predicate, seconds = 90) {
  const deadline = Date.now() + seconds * 1000;
  while (Date.now() < deadline) {
    const worker = await op("inspect", id);
    if (predicate(worker)) return worker;
    await pause(500);
  }
  throw new Error(`Worker ${id} did not reach the required state`);
}

async function cleaned(id) {
  const worker = await inspectUntil(id, terminal);
  assert.equal(worker.credential_cleanup, "complete", `Unconfirmed credentials: ${id}`);
  assert.equal(existsSync(worker.resources.private_state), false, `Private state retained: ${id}`);
  const probe = spawn("tmux", ["has-session", "-t", worker.resources.tmux_session], { stdio: "ignore" });
  const code = await new Promise((resolve) => probe.once("close", resolve));
  assert.equal(code, 1, `Pane still present: ${id}`);
  if (worker.resources.worker_pid) {
    let alive = false;
    try { process.kill(-worker.resources.worker_pid, 0); alive = true; }
    catch (error) { assert.equal(error.code, "ESRCH"); }
    assert.equal(alive, false, `Worker process group remains: ${id}`);
  }
  assert(worker.result_file || worker.error, "Result or failure evidence must survive cleanup");
  return worker;
}

let complete = false;
let sessionFile;
let conversationLaunched = false;
let nativeLaunchUnconfirmed = false;
try {
  await invoke(["worker", "list", "--config", host.config], undefined, undefined, host);
  mkdirSync(join(root, "sessions"), { mode: 0o700 });
  const first = new Rpc();
  const commands = await first.request("get_commands");
  assert(commands.commands.some((c) => c.name === "worker-watch"), "Fabrial extension did not load; no model prompt sent");
  const state = await first.request("get_state");
  assert(state.sessionFile, "Acceptance needs a persistent Pi session");
  sessionFile = state.sessionFile;
  const taskFile = task("conversation", 45, "ASYNC-WORKER-PASSED");
  const accepted = first.wait((e) => e.type === "tool_execution_end" && e.toolName === "agent" && e.result?.details?.workerId);
  // Startup can fail before we reach the await below; cleanup must still run.
  void accepted.catch(() => {});
  conversationLaunched = true;
  await first.request("prompt", { message: `Call agent exactly once with role reviewer, taskFile ${JSON.stringify(taskFile)}, worktree ${JSON.stringify(worktree)}. Do not poll or wait. After launch reply exactly LAUNCHED. If the driver asks a question, answer it while the worker runs. When a later completion notification arrives, reply exactly ASYNC-PROOF-DONE. Never launch another worker.` });
  const event = await accepted;
  const id = event.result.details.workerId; workers.push(id);
  await first.answer("LAUNCHED");
  await inspectUntil(id, (w) => w.last_progress?.event === "tool_execution_start");
  const worker = await op("inspect", id);
  assert.equal(worker.state, "running");
  await assert.rejects(op("steer", id, ["--head", "0".repeat(40), "--message", "Reject this stale control"]));
  await op("steer", id, ["--head", worker.task_head, "--message", "Keep the requested final phrase ASYNC-WORKER-PASSED. Do not start agents."]);
  const after = first.events.length;
  await first.request("prompt", { message: "Driver question: reply exactly DRIVER-ANSWER now. Do not call tools or wait for the worker." });
  await first.answer("DRIVER-ANSWER", after);
  assert.equal((await op("inspect", id)).state, "running", "Worker must still run while Pi answers the driver");
  await first.close();
  assert.equal((await op("inspect", id)).state, "running", "Pi disconnect must not cancel its worker");
  const resumed = new Rpc(state.sessionFile);
  await resumed.request("get_commands");
  await resumed.answer("ASYNC-PROOF-DONE"); // No prompt sent to the resumed Pi.
  const result = await cleaned(id);
  assert.equal(result.result, "ASYNC-WORKER-PASSED");
  await pause(1500);
  await resumed.close();
  const entries = readFileSync(state.sessionFile, "utf8").trim().split("\n").map((line) => JSON.parse(line));
  const notifications = entries.filter((e) => e.type === "custom_message" && e.customType === "fabrial.worker-event" && e.details?.workerId === id);
  assert.equal(notifications.length, 1, "Completion must not be delivered twice");
  assert.equal(first.events.filter((e) => e.type === "tool_execution_start" && e.toolName === "agent").length, 1);
  assert.equal(resumed.events.filter((e) => e.type === "tool_execution_start" && e.toolName === "agent").length, 0);
  facts.push({ case: "conversation-restart-replay-steering", worker: id, result: result.result, notifications: notifications.length });

  for (const mode of ["cancel", "timeout", "pane-exit"]) {
    const path = join(root, `${mode}-${suffix}`), branch = `storm/proof-${mode}-${suffix}`;
    git("worktree", "add", path, "-b", branch, "HEAD"); owned.push({ path, branch });
    const taskFile = task(mode, 120, "UNEXPECTED-SUCCESS");
    // Native dispatch timeout is intentionally bounded separately from the observer.
    const args = ["dispatch", "reviewer", "--task", taskFile, "--worktree", path, "--trusted-checkout", host.trustedCheckout,
      "--config", host.config, "--timeout", mode === "timeout" ? "60" : "180", "--detach"];
    nativeLaunchUnconfirmed = true;
    const started = await invoke(args, undefined, undefined, host, undefined, (event) => {
      if (event.type === "started") {
        assert(event.worker?.worker_id, "Native launch acceptance has no worker identity");
        if (!workers.includes(event.worker.worker_id)) workers.push(event.worker.worker_id);
        nativeLaunchUnconfirmed = false;
      }
    });
    const id = started.worker.worker_id;
    await inspectUntil(id, (w) => w.last_progress?.event === "tool_execution_start");
    if (mode === "cancel") await op("cancel", id);
    if (mode === "pane-exit") {
      const worker = await op("inspect", id);
      execFileSync("tmux", ["kill-session", "-t", worker.resources.tmux_session], { stdio: "ignore" });
      await pause(3000); // Allow RPC EOF to dispose the worker before recovery verifies quiescence.
      await op("recover", id);
    }
    const result = await cleaned(id);
    assert.equal(result.state, { cancel: "cancelled", timeout: "timed_out", "pane-exit": "failed" }[mode]);
    assert.equal(git("-C", path, "status", "--porcelain"), "");
    facts.push({ case: mode, worker: id, state: result.state, cleanup: result.credential_cleanup });
  }
  complete = true;
} finally {
  for (const client of clients) await client.close();
  let uncertain = nativeLaunchUnconfirmed;
  if (uncertain) console.error("YOUR ATTENTION IS REQUIRED: native launch ownership is unconfirmed; scratch evidence is retained.");
  if (sessionFile) {
    try {
      const entries = readFileSync(sessionFile, "utf8").trim().split("\n").map((line) => JSON.parse(line));
      const journal = new WorkerJournal({ entries: () => entries });
      if (conversationLaunched && journal.states.size === 0) throw new Error("Pi accepted identity missing");
      for (const id of journal.states.keys()) if (!workers.includes(id)) workers.push(id);
    } catch {
      uncertain = true;
      console.error("YOUR ATTENTION IS REQUIRED: Pi acceptance journal could not confirm worker ownership; scratch evidence is retained.");
    }
  }
  for (const id of workers) {
    try {
      const worker = await op("inspect", id);
      if (!terminal(worker)) await op("cancel", id);
      await cleaned(id);
    } catch { uncertain = true; console.error(`YOUR ATTENTION IS REQUIRED: cleanup unconfirmed for worker ${id}; recover its retained record.`); }
  }
  for (const item of owned) {
    if (!uncertain) { git("worktree", "remove", item.path); git("branch", "-D", item.branch); }
  }
  if (!uncertain) rmSync(root, { recursive: true, force: false });
  else console.error(`Recovery evidence retained at ${root}`);
  if (!complete) console.error("Acceptance failed; no success or readiness is claimed.");
  if (uncertain) throw new Error("Acceptance cleanup unconfirmed; retained state requires attention");
}
console.log(JSON.stringify({ head: git("rev-parse", "HEAD"), facts, modelWorkers: workers.length, result: "ASYNC-ACCEPTANCE-PASSED" }, null, 2));
