import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** The only worker extension. Reports travel over Pi's RPC stdout, never host files. */
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
      kind: Type.Union([Type.Literal("progress"), Type.Literal("blocked"), Type.Literal("question")]),
      text: Type.String({ minLength: 1, maxLength: 4096 }),
    }),
    async execute(_id, report) {
      pi.appendEntry("fabrial.worker-report", report);
      return { content: [{ type: "text", text: "Report sent." }], details: undefined };
    },
  }));
}
