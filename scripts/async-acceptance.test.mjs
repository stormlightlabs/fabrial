import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

test("acceptance cleans up a journalled worker when Pi exits before tool_execution_end", () => {
  const root = mkdtempSync(join(tmpdir(), "fab-acceptance-test-"));
  const id = "11111111-1111-4111-8111-111111111111";
  const program = (name, body) => {
    const path = join(root, name);
    writeFileSync(path, `#!/usr/bin/env node\n${body}`, { mode: 0o700 });
    return path;
  };
  try {
    const pi = program("pi", `
const fs = require("node:fs"), readline = require("node:readline"), path = require("node:path");
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (request.type === "prompt") {
    fs.writeFileSync(path.join(process.env.FIXTURE_ROOT, "session.jsonl"), JSON.stringify({
      type: "custom", customType: "fabrial.worker", data: { worker: { worker_id: ${JSON.stringify(id)} }, cursor: 0, reported: false, done: false }
    }) + "\\n", { mode: 0o600 });
    process.exit(23);
  }
  const data = request.type === "get_state" ? { sessionFile: path.join(process.env.FIXTURE_ROOT, "session.jsonl") }
    : { commands: [{ name: "worker-watch" }] };
  process.stdout.write(JSON.stringify({ type: "response", id: request.id, success: true, data }) + "\\n");
});`);
    const binary = program("fab", `
const fs = require("node:fs"), path = require("node:path");
const root = process.env.FIXTURE_ROOT, action = process.argv[3], flag = path.join(root, "cancelled");
let record;
if (action === "list") record = { workers: [] };
else if (action === "cancel") { fs.writeFileSync(flag, "cancelled"); record = { type: "cancel_requested" }; }
else if (action === "inspect") record = { worker_id: ${JSON.stringify(id)}, state: fs.existsSync(flag) ? "cancelled" : "running",
  credential_cleanup: fs.existsSync(flag) ? "complete" : "pending", error: "cancelled",
  resources: { private_state: path.join(root, "absent-private-state"), tmux_session: "fixture-no-session" } };
else process.exit(91);
process.stdout.write(JSON.stringify(record) + "\\n");`);
    program("tmux", "process.exit(1);");
    const script = fileURLToPath(new URL("async-acceptance.mjs", import.meta.url));
    const output = spawnSync(process.execPath, [script, "--worktree", root, "--pi", pi], {
      env: { ...process.env, FIXTURE_ROOT: root, PATH: `${root}:${process.env.PATH}`,
        FAB_BINARY: binary, FAB_DISPATCH_CONFIG: join(root, "config"), FAB_TRUSTED_CHECKOUT: root },
      encoding: "utf8", timeout: 10000,
    });
    assert.equal(output.error, undefined);
    assert.notEqual(output.status, 0);
    assert.equal(existsSync(join(root, "cancelled")), true, output.stderr);
    assert(!output.stdout.includes("ASYNC-ACCEPTANCE-PASSED"));
    assert(!output.stderr.includes("cleanup unconfirmed"), output.stderr);
  } finally { rmSync(root, { recursive: true, force: false }); }
});
