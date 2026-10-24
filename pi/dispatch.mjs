import { spawn } from "node:child_process";

/** Invoke the same host dispatcher as the CLI, with LF-framed progress. */
export async function dispatch(params, signal, onUpdate, host, launch = spawn) {
  if (!host.config || !host.trustedCheckout || !host.binary) {
    throw new Error("Set FAB_DISPATCH_CONFIG, FAB_TRUSTED_CHECKOUT, and FAB_BINARY on the trusted host");
  }
  signal?.throwIfAborted();
  const args = ["dispatch", params.role, "--task", params.taskFile,
    "--worktree", params.worktree, "--trusted-checkout", host.trustedCheckout,
    "--config", host.config];
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
      if (pending.length > 4 * 1024 * 1024) {
        failure = new Error("Dispatch record exceeds 4 MiB");
        abort();
        return;
      }
      let index;
      while ((index = pending.indexOf("\n")) >= 0) {
        const line = pending.slice(0, index);
        pending = pending.slice(index + 1);
        try {
          const event = JSON.parse(line);
          if (event.type === "progress") onUpdate?.(event.event);
          else if (event.type === "result") result = event.text;
          else if (event.type === "error") failure = new Error(event.message);
        } catch {
          failure = new Error("Invalid dispatch JSONL");
          abort();
        }
      }
    });
    // Consume diagnostics without copying possibly sensitive host output into Pi.
    child.stderr.resume();
    child.on("error", () => { failure = new Error("Cannot launch trusted fab binary"); });
    child.on("close", (code) => {
      signal?.removeEventListener("abort", abort);
      if (signal?.aborted) reject(new Error("Worker cancelled"));
      else if (failure) reject(failure);
      else if (code !== 0 || pending || typeof result !== "string" || !result.trim()) {
        reject(new Error("Dispatch failed or returned no final role result"));
      } else resolve(result);
    });
  });
}
