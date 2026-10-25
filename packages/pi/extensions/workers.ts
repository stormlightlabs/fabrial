import { StringEnum, Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { startWorker, workerOperation } from "../lib/dispatch.mjs";
import { WorkerMonitor, cancelWorkers, isTerminal } from "../lib/workers.mjs";
import { WorkerJournal, dedupeNotifications } from "../lib/journal.mjs";

/**
 * The `agent` and `agent_control` tools and the `/worker-*` commands. Fresh
 * workers run independently; the extension observes without owning their
 * lifetime, publishes progress as `fabrial:worker` events and answers
 * `fabrial:stop` by cancelling every worker it observes.
 */
export default function (pi: ExtensionAPI) {
  const host = {
    config: process.env.FAB_DISPATCH_CONFIG,
    trustedCheckout: process.env.FAB_TRUSTED_CHECKOUT,
    binary: process.env.FAB_BINARY,
  };
  let monitor: WorkerMonitor | undefined;
  let journal: WorkerJournal | undefined;

  pi.on("context", (event) => ({ messages: dedupeNotifications(event.messages) }));
  // An authorized PR stop from the PR status extension. It queues its message once
  // `respond`'s outcome settles, so answer synchronously, before any await.
  pi.events.on("fabrial:stop", (request) => {
    const respond = (request as { respond?: unknown } | undefined)?.respond;
    if (typeof respond !== "function") return;
    respond(cancelWorkers([...(monitor?.workers.keys() ?? [])],
      (id: string) => workerOperation("cancel", id, [], undefined, host)));
  });
  pi.on("session_start", (_event, ctx) => {
    monitor?.stop();
    const sessionId = ctx.sessionManager.getSessionId();
    journal = new WorkerJournal({
      entries: () => ctx.sessionManager.getBranch(),
      append: (type: string, data: unknown) => {
        if (ctx.sessionManager.getSessionId() !== sessionId) throw new Error("Worker session changed");
        pi.appendEntry(type, data);
      },
      send: (message: Parameters<ExtensionAPI["sendMessage"]>[0]) =>
        pi.sendMessage(message, { triggerTurn: true, deliverAs: "followUp" }),
    });
    const current = journal;
    monitor = new WorkerMonitor({
      fetch: async (id: string, cursor: number, signal: AbortSignal) => {
        let batch = await workerOperation("events", id, ["--after", String(cursor)], signal, host);
        if (batch.supervisor_alive === false && (!isTerminal(batch.worker) || batch.worker.credential_cleanup !== "complete")) {
          await workerOperation("recover", id, [], signal, host);
          batch = await workerOperation("events", id, ["--after", String(cursor)], signal, host);
        }
        return batch;
      },
      notify: (event: unknown) => current.enqueue(event),
      replay: () => current.flush(),
      checkpoint: (state: unknown) => current.checkpoint(state),
      progress: (event: unknown) => pi.events.emit("fabrial:worker", event),
    });
    for (const state of current.states.values()) {
      if (!state.done) monitor.track(state.worker, { cursor: state.cursor, reported: state.reported });
    }
    monitor.start();
  });
  pi.on("session_shutdown", () => {
    monitor?.stop();
    monitor = undefined; journal = undefined;
  });

  pi.registerTool(defineTool({
    name: "agent", label: "Agent",
    description: "Start one fresh isolated Pi worker and return its ID promptly. Completion or a blocker arrives later. Do not poll with model calls or repeat stages.",
    parameters: Type.Object({
      role: StringEnum(["reviewer", "verifier", "adversarial-reviewer", "reviser"] as const),
      taskFile: Type.String({ description: "Absolute path to a task file" }),
      worktree: Type.String({ description: "Absolute path to the worker's linked Git worktree" }),
    }),
    async execute(_id, params, signal) {
      const observer = monitor;
      const current = journal;
      if (!observer || !current) throw new Error("Worker monitoring has not started");
      const worker = await startWorker(params, signal, host, (accepted: { worker_id: string }) => {
        // Persist acceptance before returning; cancellation during startup cannot lose the identity.
        if (observer.disposed) throw new Error("Worker session changed during launch");
        current.checkpoint({ worker: accepted, cursor: 0, reported: false, done: false });
        observer.track(accepted);
      });
      return {
        content: [{ type: "text", text: `Worker ${worker.worker_id} started. Continue the conversation; completion will arrive automatically.` }],
        details: { workerId: worker.worker_id, runId: worker.run_id, head: worker.task_head },
      };
    },
  }));

  pi.registerTool(defineTool({
    name: "agent_control", label: "Worker control",
    description: "Inspect or explicitly cancel one worker. Cancellation requests are not cleanup confirmation. Never start a replacement until this worker is stopped.",
    parameters: Type.Object({ action: StringEnum(["inspect", "cancel"] as const), workerId: Type.String() }),
    async execute(_id, params, signal) {
      const result = await workerOperation(params.action, params.workerId, [], signal, host);
      return { content: [{ type: "text", text: JSON.stringify(result) }], details: result };
    },
  }));

  pi.registerCommand("worker-watch", {
    description: "Reconnect this session to an existing worker UUID",
    handler: async (args) => {
      if (!monitor || !journal) throw new Error("Worker monitoring has not started");
      const worker = await workerOperation("inspect", args.trim(), [], undefined, host);
      const state = journal.checkpoint({ worker, cursor: 0, reported: false, done: false });
      if (!state.done) monitor.track(worker, { cursor: state.cursor, reported: state.reported });
    },
  });

  pi.registerCommand("worker-steer", {
    description: 'Send driver steering: {"workerId":"…","head":"…","message":"…"}',
    handler: async (args) => {
      const value = JSON.parse(args);
      if (![value.workerId, value.head, value.message].every((v) => typeof v === "string" && v.length)) {
        throw new Error("Expected workerId, head, and message strings");
      }
      const receipt = await workerOperation("steer", value.workerId,
        ["--head", value.head, "--message", value.message], undefined, host);
      pi.sendMessage({ customType: "fabrial.worker-control", content: JSON.stringify(receipt), display: true }, { triggerTurn: false });
    },
  });
}
