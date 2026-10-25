import { notification } from "./workers.mjs";

/** Messages the journal delivers; worker events are the default. */
const EVENT_TYPES = ["fabrial.worker-event", "fabrial.pr-event"];

/**
 * Session-local checkpoints and a durable outbox; queued messages are not delivery receipts.
 * Each extension keeps its own journal: it loads and delivers only outbox items of its
 * `customType`, `fabrial.worker-event` for workers and `fabrial.pr-event` for PR status.
 */
export class WorkerJournal {
  constructor({ entries, append, send, customType = EVENT_TYPES[0] }) {
    this.entries = entries;
    this.append = append;
    this.send = send;
    this.states = new Map();
    this.outbox = new Map();
    this.seen = new Set();
    this.sent = new Set();
    for (const entry of entries()) {
      if (entry.type === "custom" && entry.customType === "fabrial.worker" && entry.data?.worker?.worker_id) {
        this.states.set(entry.data.worker.worker_id, entry.data);
      } else if (entry.type === "custom" && entry.customType === "fabrial.worker-outbox" && entry.data?.key &&
        (entry.data.customType ?? EVENT_TYPES[0]) === customType) {
        this.outbox.set(entry.data.key, entry.data);
      }
    }
  }

  checkpoint(value) {
    const worker = { ...value.worker };
    delete worker.result; // The host retains the full result; do not duplicate it in checkpoints.
    const state = { worker, cursor: value.cursor, reported: value.reported, done: value.done };
    const previous = this.states.get(worker.worker_id);
    if (previous) {
      state.cursor = Math.max(previous.cursor, state.cursor);
      state.reported ||= previous.reported;
      state.done ||= previous.done;
    }
    if (JSON.stringify(previous) === JSON.stringify(state)) return previous;
    this.append("fabrial.worker", state);
    this.states.set(worker.worker_id, state);
    return state;
  }

  /** Queue a worker event, or a PR event that carries its own `content`. */
  enqueue(event) {
    if (!this.outbox.has(event.key)) {
      const message = event.worker
        ? { key: event.key, workerId: event.worker.worker_id, content: notification(event) }
        : { key: event.key, customType: "fabrial.pr-event", content: event.content };
      this.append("fabrial.worker-outbox", message);
      this.outbox.set(event.key, message);
    }
    this.flush();
  }

  flush() {
    // A follow-up may have been committed since the preceding poll or during reload.
    for (const entry of this.entries()) {
      if (entry.type === "custom_message" && EVENT_TYPES.includes(entry.customType) && entry.details?.eventKey) {
        this.seen.add(entry.details.eventKey);
      }
    }
    for (const [key, message] of this.outbox) {
      if (this.seen.has(key) || this.sent.has(key)) continue;
      this.send({ customType: message.customType ?? "fabrial.worker-event", content: message.content, display: true,
        details: { eventKey: key, workerId: message.workerId } });
      this.sent.add(key);
    }
  }
}

/** Reload may replay an undelivered outbox item still in Pi's in-memory queue. */
export function dedupeNotifications(messages) {
  const seen = new Set();
  return messages.filter((message) => {
    if (message.role !== "custom" || !EVENT_TYPES.includes(message.customType)) return true;
    const key = message.details?.eventKey;
    if (!key) return true;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
