import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import { StatusSession } from "../lib/status.mjs";

/**
 * The status rows below the editor and the worker widget above it. It shows
 * what the PR status and workers extensions publish as `fabrial:pr` and
 * `fabrial:worker` events, and works without either: the rows then say the
 * PR is unknown and no workers run.
 */
export default function (pi: ExtensionAPI) {
  const options = { fit: (line: string, width: number) => truncateToWidth(line, width), measure: visibleWidth };
  let session: StatusSession | undefined;
  pi.on("session_start", (_event, ctx) => {
    session?.stop();
    session = ctx.hasUI ? new StatusSession(pi.events, ctx, options) : undefined;
  });
  pi.on("session_shutdown", () => { session?.stop(); session = undefined; });
  pi.on("turn_end", () => session?.readGit());
  pi.on("user_bash", () => session?.readGit());
}
