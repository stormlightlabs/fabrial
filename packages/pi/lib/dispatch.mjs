import { spawn } from "node:child_process";

/** Run a host operation. JSONL framing is LF-only, including across UTF-8 chunks. */
export async function invoke(args, signal, onUpdate, host, launch = spawn, onRecord) {
  if (!host.config || !host.trustedCheckout || !host.binary) {
    throw new Error("Set FAB_DISPATCH_CONFIG, FAB_TRUSTED_CHECKOUT, and FAB_BINARY on the trusted host");
  }
  signal?.throwIfAborted();
  return new Promise((resolve, reject) => {
    const child = launch(host.binary, args, { stdio: ["ignore", "pipe", "pipe"] });
    let pending = "";
    let result;
    let failure;
    const abort = () => child.kill("SIGTERM");
    signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) abort();
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      pending += chunk;
      if (Buffer.byteLength(pending) > 16 * 1024 * 1024) {
        failure = new Error("Host record exceeds 16 MiB"); abort(); return;
      }
      let index;
      while ((index = pending.indexOf("\n")) >= 0) {
        const line = pending.slice(0, index);
        pending = pending.slice(index + 1);
        try {
          const record = JSON.parse(line);
          if (!record || typeof record !== "object" || Array.isArray(record)) throw new Error();
          if (record.type === "progress") onUpdate?.(record.event);
          else if (record.type === "error") failure = new Error(record.message || "Host operation failed");
          else result = record;
          onRecord?.(record);
        } catch {
          failure = new Error("Invalid host JSONL or rejected record"); abort();
        }
      }
    });
    child.stderr.resume();
    child.on("error", () => { failure = new Error("Cannot launch trusted fab binary"); });
    child.on("close", (code) => {
      signal?.removeEventListener("abort", abort);
      if (signal?.aborted) reject(new Error("Worker operation cancelled"));
      else if (failure) reject(failure);
      else if (code !== 0 || pending || !result) reject(new Error("Host operation failed or returned no record"));
      else resolve(result);
    });
  });
}

/** Compatibility client for explicitly synchronous callers. */
export async function dispatch(params, signal, onUpdate, host, launch = spawn) {
  const record = await invoke(["dispatch", params.role, "--task", params.taskFile,
    "--worktree", params.worktree, "--trusted-checkout", host.trustedCheckout,
    "--config", host.config], signal, onUpdate, host, launch);
  if (record.type !== "result" || typeof record.text !== "string" || !record.text.trim()) {
    throw new Error("Dispatch returned no final role result");
  }
  return record.text;
}

/** Launch acceptance returns promptly. A later abort cannot cancel an accepted tool result. */
export async function startWorker(params, signal, host, onAccepted, launch = spawn) {
  let worker;
  try {
    const record = await invoke(["dispatch", params.role, "--task", params.taskFile,
      "--worktree", params.worktree, "--trusted-checkout", host.trustedCheckout,
      "--config", host.config, "--detach"], signal, undefined, host, launch, (event) => {
      if (event.type === "started") {
        worker = event.worker;
        if (!worker?.worker_id || !worker?.task_head) throw new Error("Invalid launch identity");
        onAccepted(worker);
      }
    });
    if (record.type !== "started" || !worker) throw new Error("No worker launch acceptance");
    return worker;
  } catch (error) {
    if (worker?.worker_id) {
      try { await workerOperation("cancel", worker.worker_id, [], undefined, host, launch); }
      catch { /* The accepted identity was persisted; monitoring/recovery can confirm cleanup. */ }
    }
    throw error;
  }
}

/** Operations use only operator-selected host paths, not model-selected configuration. */
export function workerOperation(action, id, extra, signal, host, launch = spawn) {
  return invoke(["worker", action, id, "--config", host.config, ...extra], signal, undefined, host, launch);
}
