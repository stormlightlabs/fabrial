import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { PullRequestCollector, movesBranch } from "../lib/pr.mjs";

/**
 * Watches the session branch's pull request and publishes each reading as a
 * `fabrial:pr` event for the status area. It sends no message and starts no
 * turn: the person reads the status, and acting on it is theirs or a host's.
 */
export default function (pi: ExtensionAPI) {
  let collector: PullRequestCollector | undefined;

  pi.on("tool_result", (event) => {
    if (event.toolName === "bash" && movesBranch(event.input.command)) collector?.refresh();
  });
  pi.on("user_bash", (event) => { if (movesBranch(event.command)) collector?.refresh(); });
  pi.on("session_start", (_event, ctx) => {
    collector?.stop();
    collector = new PullRequestCollector({
      cwd: ctx.sessionManager.getCwd(),
      progress: (snapshot: unknown) => pi.events.emit("fabrial:pr", snapshot),
    });
    collector.start();
  });
  pi.on("session_shutdown", () => {
    collector?.stop();
    collector = undefined;
  });
}
