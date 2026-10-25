import { StringEnum, Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

/**
 * The only extension inside a worker sandbox; `fab dispatch` embeds it, and the
 * package manifest does not list it. Reports travel over Pi's RPC stdout, never host files.
 */
export default function (pi: ExtensionAPI) {
  // The supervisor checks this command before sending any model prompt.
  pi.registerCommand("fabrial-worker-ready", {
    description: "Internal worker readiness marker",
    handler: async () => {},
  });
  pi.registerTool(defineTool({
    name: "worker_report",
    label: "Report to orchestrator",
    description: "Report progress, a blocker, or a question. A blocker or question stops this worker and asks the driver for input. Never include credentials.",
    parameters: Type.Object({
      kind: StringEnum(["progress", "blocked", "question"] as const),
      text: Type.String({ minLength: 1, maxLength: 4096 }),
    }),
    async execute(_id, report) {
      pi.appendEntry("fabrial.worker-report", report);
      return { content: [{ type: "text", text: "Report sent." }], details: undefined };
    },
  }));
}
