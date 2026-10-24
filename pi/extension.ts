import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { startWorker, workerOperation } from "./dispatch.mjs";
import { WorkerMonitor, notification } from "./workers.mjs";

/** Fresh workers run independently; the extension observes without owning their lifetime. */
export default function (pi: ExtensionAPI) {
  const host = {
    config: process.env.FAB_DISPATCH_CONFIG,
    trustedCheckout: process.env.FAB_TRUSTED_CHECKOUT,
    binary: process.env.FAB_BINARY,
  };
  let monitor: WorkerMonitor | undefined;

  pi.on("session_start", () => {
    monitor = new WorkerMonitor({
      fetch: (id: string, cursor: number, signal: AbortSignal) =>
        workerOperation("events", id, ["--after", String(cursor)], signal, host),
      notify: (event: { key: string; worker: { worker_id: string } }) => {
        pi.sendMessage({
          customType: "fabrial.worker-event", content: notification(event), display: true,
          details: { eventKey: event.key, workerId: event.worker.worker_id },
        }, { triggerTurn: true, deliverAs: "followUp" });
      },
      progress: (event: unknown) => pi.events.emit("fabrial:worker", event),
    });
    monitor.start();
  });
  pi.on("session_shutdown", () => { monitor?.stop(); monitor = undefined; });

  pi.registerTool(defineTool({
    name: "agent", label: "Agent",
    description: "Start one fresh isolated Pi worker and return its ID promptly. Completion or a blocker arrives later. Do not poll with model calls or repeat stages.",
    parameters: Type.Object({
      role: Type.Union([
        Type.Literal("reviewer"), Type.Literal("verifier"),
        Type.Literal("adversarial-reviewer"), Type.Literal("reviser"),
      ]),
      taskFile: Type.String({ description: "Absolute path to a task file" }),
      worktree: Type.String({ description: "Absolute path to the worker's linked Git worktree" }),
    }),
    async execute(_id, params, signal) {
      if (!monitor) throw new Error("Worker monitoring has not started");
      const worker = await startWorker(params, signal, host, (accepted: { worker_id: string }) => {
        // Persist acceptance before returning; cancellation during startup cannot lose the identity.
        pi.appendEntry("fabrial.worker", { worker: accepted, cursor: 0, reported: false, done: false });
        monitor?.track(accepted);
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
    parameters: Type.Object({ action: Type.Union([Type.Literal("inspect"), Type.Literal("cancel")]), workerId: Type.String() }),
    async execute(_id, params, signal) {
      const result = await workerOperation(params.action, params.workerId, [], signal, host);
      return { content: [{ type: "text", text: JSON.stringify(result) }], details: result };
    },
  }));

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
