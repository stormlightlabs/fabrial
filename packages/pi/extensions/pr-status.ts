import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { WorkerJournal, dedupeNotifications } from "../lib/journal.mjs";
import { PullRequestCollector, movesBranch, prNotification } from "../lib/pr.mjs";

/** A `fabrial:stop` request: each listener passes `respond` one outcome sentence that never rejects. */
type StopRequest = { key: string; respond: (outcome: Promise<string>) => void };

/**
 * Watches the session branch's pull request, publishes each reading as a
 * `fabrial:pr` event and resumes the orchestrator on the changes docs/pi.md
 * lists. On an authorized stop it emits `fabrial:stop` and waits for the
 * outcomes before queuing one message, so a reload before then repeats the
 * stop. Without the workers extension nothing answers and the message has no
 * outcome.
 */
export default function (pi: ExtensionAPI) {
  let journal: WorkerJournal | undefined;
  let collector: PullRequestCollector | undefined;

  pi.on("context", (event) => ({ messages: dedupeNotifications(event.messages) }));
  pi.on("tool_result", (event) => {
    if (event.toolName === "bash" && movesBranch(event.input.command)) collector?.refresh();
  });
  pi.on("user_bash", (event) => { if (movesBranch(event.command)) collector?.refresh(); });
  pi.on("session_start", (_event, ctx) => {
    collector?.stop();
    const sessionId = ctx.sessionManager.getSessionId();
    const current = journal = new WorkerJournal({
      customType: "fabrial.pr-event",
      entries: () => ctx.sessionManager.getBranch(),
      append: (type: string, data: unknown) => {
        if (ctx.sessionManager.getSessionId() !== sessionId) throw new Error("PR status session changed");
        pi.appendEntry(type, data);
      },
      send: (message: Parameters<ExtensionAPI["sendMessage"]>[0]) =>
        pi.sendMessage(message, { triggerTurn: true, deliverAs: "followUp" }),
    });
    collector = new PullRequestCollector({
      cwd: ctx.sessionManager.getCwd(),
      since: ctx.sessionManager.getHeader()?.timestamp,
      replay: () => current.flush(),
      notify: async (event: { key: string; kind: string; content: string; snapshot: unknown }) => {
        // The owner's stop is trusted: it came from an OWNER, MEMBER or COLLABORATOR or set the label.
        if (event.kind === "stop" && !current.outbox.has(event.key)) {
          const outcomes: Promise<string>[] = [];
          // Pi's event bus calls listeners synchronously, so every answer is in before emit returns.
          pi.events.emit("fabrial:stop", { key: event.key, respond: (outcome) => { outcomes.push(outcome); } } satisfies StopRequest);
          const settled = await Promise.allSettled(outcomes);
          if (journal !== current) return;
          const outcome = settled.map((result) => (result.status === "fulfilled" ? result.value : "")).filter(Boolean).join(" ");
          event = { ...event, content: prNotification({ ...event, outcome }, event.snapshot) };
        }
        current.enqueue(event);
      },
      progress: (snapshot: unknown) => pi.events.emit("fabrial:pr", snapshot),
    });
    collector.start();
  });
  pi.on("session_shutdown", () => {
    collector?.stop();
    collector = undefined; journal = undefined;
  });
}
