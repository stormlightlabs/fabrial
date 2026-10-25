import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { WorkerJournal, dedupeNotifications } from "../lib/journal.mjs";
import { PullRequestCollector, movesBranch, prNotification } from "../lib/pr.mjs";

/** How long a stop waits for the workers extension to reply before it reports no outcome. */
const STOP_REPLY_MS = 1000;
/**
 * How long a stop waits for the outcome after the reply. The workers extension
 * bounds each cancel at 3 s; this backstop keeps a hung outcome from holding
 * the PR watch.
 */
const STOP_OUTCOME_MS = 10000;

/** A `fabrial:stop` request for the stop with this key. */
type StopRequest = { key: string };
/** The `fabrial:stop-outcome` reply: the cancel outcome sentence, which never rejects. */
type StopReply = { key: string; outcome: Promise<string> };

/**
 * Watches the session branch's pull request, publishes each reading as a
 * `fabrial:pr` event and resumes the orchestrator on the changes docs/pi.md
 * lists. On an authorized stop it emits `fabrial:stop` and waits for the
 * keyed `fabrial:stop-outcome` reply and its outcome before queuing one
 * message, so a reload before then repeats the stop. With no reply within
 * `STOP_REPLY_MS`, as without the workers extension, the message has no
 * outcome, and so it has when the outcome takes longer than `STOP_OUTCOME_MS`.
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
          const outcome = await stopOutcome(pi, event.key);
          if (journal !== current) return;
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

/** Emits `fabrial:stop` and resolves to the reply's outcome, or "" when the reply or outcome is late. */
function stopOutcome(pi: ExtensionAPI, key: string): Promise<string> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => { off(); resolve(""); }, STOP_REPLY_MS);
    const off = pi.events.on("fabrial:stop-outcome", (data) => {
      const reply = data as Partial<StopReply> | undefined;
      if (reply?.key !== key || !(reply.outcome instanceof Promise)) return;
      clearTimeout(timer); off();
      let late: ReturnType<typeof setTimeout> | undefined;
      const backstop = new Promise<string>((done) => { late = setTimeout(() => done(""), STOP_OUTCOME_MS); });
      resolve(Promise.race([reply.outcome.then(String, () => ""), backstop]).finally(() => clearTimeout(late)));
    });
    pi.events.emit("fabrial:stop", { key } satisfies StopRequest);
  });
}
