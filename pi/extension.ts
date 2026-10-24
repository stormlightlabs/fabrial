import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { dispatch } from "./dispatch.mjs";

/** Register fresh review workers. Host paths are operator configuration. */
export default function (pi: ExtensionAPI) {
  pi.registerTool(defineTool({
    name: "agent",
    label: "Agent",
    description: "Run one fresh, isolated Pi review worker through host fab dispatch. No shared history.",
    parameters: Type.Object({
      role: Type.Union([
        Type.Literal("reviewer"), Type.Literal("verifier"),
        Type.Literal("adversarial-reviewer"), Type.Literal("reviser"),
      ]),
      taskFile: Type.String({ description: "Absolute path to a task file" }),
      worktree: Type.String({ description: "Absolute path to the worker's linked Git worktree" }),
    }),
    async execute(_id, params, signal, onUpdate) {
      const text = await dispatch(params, signal,
        (progress: string) => onUpdate?.({
          content: [{ type: "text", text: progress }], details: undefined,
        }), {
          config: process.env.FAB_DISPATCH_CONFIG,
          trustedCheckout: process.env.FAB_TRUSTED_CHECKOUT,
          binary: process.env.FAB_BINARY,
        });
      return { content: [{ type: "text", text }], details: { role: params.role } };
    },
  }));
}
