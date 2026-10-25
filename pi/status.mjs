import { execFile } from "node:child_process";

const SEPARATOR = " · ";
const STEPS = { reviewer: "review", verifier: "verify", "adversarial-reviewer": "adversarial review", reviser: "revise" };
const ACTIVE = ["prepared", "starting", "running"];
const FAILED = ["failed", "timed_out"];
const ACTIONS = {
  worker_started: "starting", turn_start: "turn started", tool_execution_start: "running a tool",
  tool_execution_end: "tool finished", worker_report: "reported", control_queued: "steering queued",
  control_delivered: "steering delivered", control_processed: "steering processed",
};
const MAX_WORKER_LINES = 4;

/**
 * The status area below Pi's editor and the worker widget above it. It reads
 * only `fabrial:pr` and `fabrial:worker` events and local Git, and starts no
 * timer, so headless sessions and dispatch never depend on it. Pi's own footer
 * stays below it with the worktree, context, cost and model.
 *
 * `fit(line, width)` trims a styled line to the terminal width, and
 * `measure(text)` counts the columns plain text takes; the extension passes
 * pi-tui's `truncateToWidth` and `visibleWidth`.
 */
export function statusArea(pi, { fit = (line) => line, measure = (text) => text.length, run = git } = {}) {
  let session;
  pi.on("session_start", (_event, ctx) => {
    session?.stop();
    session = ctx.hasUI ? new StatusSession(pi, ctx, { fit, measure, run }) : undefined;
  });
  pi.on("session_shutdown", () => { session?.stop(); session = undefined; });
  pi.on("turn_end", () => session?.readGit());
  pi.on("user_bash", () => session?.readGit());
}

/** One session's subscriptions and state; `stop()` releases all of them. */
class StatusSession {
  constructor(pi, ctx, { fit, measure, run }) {
    this.ctx = ctx;
    this.fit = fit;
    this.measure = measure;
    this.run = run;
    this.controller = new AbortController();
    this.state = { pr: undefined, git: undefined, workers: {} };
    this.unsubscribe = [
      pi.events.on("fabrial:pr", (snapshot) => { this.state.pr = snapshot; this.update(); this.readGit(); }),
      pi.events.on("fabrial:worker", (event) => { this.state = applyWorker(this.state, event); this.update(); }),
    ];
    if (ctx.mode === "tui") {
      ctx.ui.setWidget("fabrial-status", (tui, theme) => {
        this.tui = tui;
        return {
          render: (width) => renderStatus(this.state, width, style(theme, this.measure)).map((line) => this.fit(line, width)),
          invalidate() {},
        };
      }, { placement: "belowEditor" });
    }
    this.update();
    this.readGit();
  }

  update() {
    if (this.stopped) return;
    if (this.ctx.mode !== "tui") {
      // RPC clients get plain text; component widgets are ignored there.
      this.ctx.ui.setStatus("fabrial", renderStatus(this.state, 80, { ...PLAIN, width: this.measure }).join(SEPARATOR));
      return;
    }
    // The widget reads the state when it renders, so it is set again only to show or hide it.
    const shown = workerRows(this.state).length > 0;
    if (shown !== this.workersShown) {
      this.workersShown = shown;
      this.ctx.ui.setWidget("fabrial-workers", shown ? (_tui, theme) => ({
        render: (width) => renderWorkers(this.state, width, style(theme, this.measure)).map((line) => this.fit(line, width)),
        invalidate() {},
      }) : undefined);
    }
    this.tui?.requestRender();
  }

  /** Reads Git once at a time; a request during a read runs one more read after it. */
  async readGit() {
    if (this.stopped) return;
    if (this.reading) { this.again = true; return; }
    this.reading = true;
    try {
      do {
        this.again = false;
        const info = await readGit((command, args) => this.run(command, args, this.ctx.cwd, this.controller.signal));
        if (this.stopped) return;
        this.state = { ...this.state, git: info };
        this.update();
      } while (this.again);
    } finally { this.reading = false; }
  }

  stop() {
    if (this.stopped) return;
    this.stopped = true;
    this.controller.abort();
    for (const unsubscribe of this.unsubscribe) unsubscribe();
    if (this.ctx.mode === "tui") {
      this.ctx.ui.setWidget("fabrial-status", undefined);
      this.ctx.ui.setWidget("fabrial-workers", undefined);
    } else this.ctx.ui.setStatus("fabrial", undefined);
  }
}

const PLAIN = { fg: (_color, text) => text, bold: (text) => text, link: (text) => text };

/** Pi's theme plus OSC 8 links, which terminals without link support show as plain text, and the column measure. */
function style(theme, width) {
  return {
    width,
    fg: (color, text) => theme.fg(color, text),
    bold: (text) => theme.bold(text),
    link: (text, url) => `\x1b]8;;${url}\x1b\\${text}\x1b]8;;\x1b\\`,
  };
}

/** Records one `fabrial:worker` event: the worker's role, state and run, and its latest action. */
export function applyWorker(state, event) {
  const id = clean(event?.workerId);
  if (!id) return state;
  const known = state.workers[id] ?? {};
  const type = event.event?.type === "progress" ? event.event.event : event.event?.type;
  const worker = event.worker ?? {};
  return {
    ...state,
    workers: {
      ...state.workers,
      [id]: {
        role: clean(worker.role) || known.role,
        state: clean(worker.state) || known.state,
        runId: clean(worker.run_id) || known.runId,
        // Report text is untrusted and may hold anything; only its kind is shown.
        action: ACTIONS[type] ? (type === "worker_report" ? `reported ${clean(event.event.kind)}` : ACTIONS[type]) : known.action,
      },
    },
  };
}

/**
 * The two status rows for `width` columns. Row 1 is the repository, branch,
 * PR, checks and diff; row 2 is the run step, workers and run. When a row is
 * too wide, the lowest-priority fields go first; the PR, checks and step stay.
 */
export function renderStatus(state, width, s) {
  return [row(prFields(state, s), width, s), row(runFields(state, s), width, s)];
}

/** One line per worker that is active, blocked or failed, for the widget above the editor. */
export function renderWorkers(state, width, s) {
  const rows = workerRows(state);
  const lines = rows.slice(0, MAX_WORKER_LINES).map(([id, w]) => {
    const [label, color] = workerState(w.state);
    return row([
      { text: w.role ?? "worker", keep: true, paint: (t) => s.bold(t) },
      { text: label, keep: true, paint: (t) => s.fg(color, t) },
      ...(w.action ? [{ text: w.action, priority: 2, paint: (t) => s.fg("muted", t) }] : []),
      { text: id.slice(0, 8), priority: 1, paint: (t) => s.fg("dim", t) },
    ], width, s);
  });
  if (rows.length > MAX_WORKER_LINES) lines.push(row([{ text: `+${rows.length - MAX_WORKER_LINES} more workers`, keep: true, paint: (t) => s.fg("muted", t) }], width, s));
  return lines;
}

/** Reads the repository, branch, head, upstream counts and the diff against the fetched base. Unknown fields stay undefined. */
export async function readGit(run) {
  const read = (args) => run("git", args).then((out) => out.trim(), () => undefined);
  const [remote, branch, head, counts, base] = await Promise.all([
    read(["remote", "get-url", "origin"]), read(["rev-parse", "--abbrev-ref", "HEAD"]), read(["rev-parse", "HEAD"]),
    read(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]), read(["rev-parse", "--abbrev-ref", "origin/HEAD"]),
  ]);
  const fork = await read(["merge-base", base || "origin/main", "HEAD"]);
  const stat = fork === undefined ? undefined : await read(["diff", "--shortstat", fork]);
  const [behind, ahead] = counts?.split(/\s+/).map(Number) ?? [];
  const number = (pattern) => Number(pattern.exec(stat ?? "")?.[1] ?? 0);
  return {
    repository: repository(remote),
    branch: branch === "HEAD" ? "detached" : clean(branch) || undefined,
    head: /^[0-9a-f]{40}$/.test(head ?? "") ? head : undefined,
    ahead, behind,
    diff: stat === undefined ? undefined
      : { files: number(/(\d+) files? changed/), added: number(/(\d+) insertions?/), deleted: number(/(\d+) deletions?/) },
  };
}

/** `owner/name` of a GitHub remote. The URL itself never leaves here, since it can hold a token. */
export function repository(url) {
  const match = /github\.com[:/]([\w.-]+)\/([\w.-]+?)(?:\.git)?\/?$/.exec(url ?? "");
  return match ? `${match[1]}/${match[2]}` : undefined;
}

function git(command, args, cwd, signal) {
  return new Promise((resolve, reject) => {
    execFile(command, args, { cwd, signal, env: { ...process.env, GIT_OPTIONAL_LOCKS: "0" } },
      (error, stdout) => (error ? reject(new Error(`${command} failed`)) : resolve(stdout)));
  });
}

function prFields(state, s) {
  const { pr: snapshot, git: local } = state;
  const fields = [];
  const pr = snapshot?.pr && { ...snapshot.pr, url: clean(snapshot.pr.url) };
  if (!pr) {
    const known = snapshot?.observedAt !== undefined && !snapshot.error;
    const text = known ? "no PR" : `PR unknown${snapshot?.error ? ` (${problem(snapshot)})` : ""}`;
    fields.push({ text, keep: true, paint: (t) => s.fg(known ? "muted" : "warning", t) });
  } else {
    const [label, color] = { merged: ["merged", "success"], closed: ["closed", "error"] }[pr.state]
      ?? (pr.draft ? ["draft", "muted"] : ["ready", "accent"]);
    fields.push({ text: `PR #${pr.number}`, keep: true, paint: (t) => s.link(s.bold(t), pr.url) });
    fields.push({ text: pr.stopped ? `${label}, stopped` : label, keep: true, paint: (t) => s.fg(pr.stopped ? "warning" : color, t) });
    fields.push(...checks({ ...snapshot, pr }, local, s));
    fields.push({ text: pr.url, priority: 2, paint: (t) => s.link(s.fg("dim", t), pr.url) });
  }
  const repo = local?.repository ?? pr?.repository;
  if (repo) fields.push({ text: repo, priority: 4, paint: (t) => s.link(t, `https://github.com/${repo}`) });
  // Pi's footer already shows the branch name, so it goes first; the counts outlast it.
  if (local?.branch) fields.push({ text: local.branch, priority: 1, paint: (t) => s.fg("accent", t) });
  if (local?.ahead !== undefined) {
    fields.push({ text: `↑${local.ahead} ↓${local.behind}`, priority: 5, paint: (t) => s.fg("accent", t) });
  }
  if (local?.diff) {
    const { added, deleted, files } = local.diff;
    fields.push({ text: `+${added} -${deleted} in ${files} ${files === 1 ? "file" : "files"}`, priority: 3,
      paint: (t) => s.fg("muted", t) });
  }
  return fields;
}

/**
 * Checks belong to the head they were read for; anything else is stale or
 * unknown, never green. A stale result always keeps its cause and its passed,
 * failed and pending counts, dropping the word "checks" when the row is
 * narrow. The read time, then the retry time, then the head drop first.
 */
function checks(snapshot, local, s) {
  const { pr, checks: c } = snapshot;
  const counts = c ? [`${c.passed}/${c.total} passed`, c.failed ? `${c.failed} failed` : "", c.pending ? `${c.pending} pending` : ""]
    .filter(Boolean).join(", ") : "";
  const moved = local?.head && local.head !== pr.head;
  if (snapshot.status === "stale" || snapshot.error || moved) {
    const cause = snapshot.error === "GitHub rate limit" ? "rate limited" : snapshot.error ? "lookup failed"
      : moved ? "local head differs" : "head may have moved";
    const stale = [`stale, ${cause}`, counts].filter(Boolean).join(", ");
    const warn = (t) => s.fg("warning", t);
    return [
      { text: `checks ${stale}`, short: stale, keep: true, paint: warn },
      { text: `read ${clock(snapshot.observedAt)}`, priority: 8, paint: warn },
      ...(snapshot.retryAt ? [{ text: `retry ${clock(snapshot.retryAt)}`, priority: 7, paint: warn }] : []),
      { text: `head ${pr.head.slice(0, 7)}`, priority: 6, paint: (t) => s.fg("muted", t) },
    ];
  }
  if (!c?.total) return [{ text: "checks none", keep: true, paint: (t) => s.fg("muted", t) }];
  const color = c.failed ? "error" : c.pending ? "warning" : "success";
  return [{ text: `checks ${counts}`, keep: true, paint: (t) => s.fg(color, t) }];
}

function runFields(state, s) {
  const workers = Object.values(state.workers);
  const active = workers.filter((w) => ACTIVE.includes(w.state));
  const steps = [...new Set(active.map((w) => STEPS[w.role]).filter(Boolean))];
  const fields = [{ text: `step ${steps.length ? steps.join(", ") : "unknown"}`, keep: true,
    paint: (t) => s.fg(steps.length ? "accent" : "muted", t) }];
  if (workers.length) {
    const blocked = workers.filter((w) => w.state === "blocked").length;
    const failed = workers.filter((w) => FAILED.includes(w.state)).length;
    const parts = [`workers ${active.length} active of ${workers.length}`, blocked ? `${blocked} blocked` : "", failed ? `${failed} failed` : ""];
    fields.push({ text: parts.filter(Boolean).join(", "), priority: 2,
      paint: (t) => s.fg(failed ? "error" : blocked ? "warning" : "muted", t) });
  } else fields.push({ text: "workers none", priority: 2, paint: (t) => s.fg("muted", t) });
  const runs = [...new Set(active.map((w) => w.runId).filter(Boolean))];
  if (runs.length) fields.push({ text: `run ${runs.map((r) => r.slice(0, 8)).join(", ")}`, priority: 1, paint: (t) => s.fg("dim", t) });
  return fields;
}

function workerRows(state) {
  return Object.entries(state.workers).filter(([, w]) => !["succeeded", "cancelled"].includes(w.state));
}

function workerState(state) {
  if (state === "running") return ["working", "accent"];
  if (state === "blocked") return ["blocked", "warning"];
  if (FAILED.includes(state)) return ["failed", "error"];
  return ["starting", "muted"];
}

/**
 * Joins fields with separators. When the kept fields alone are too wide, each
 * field with a `short` text uses it. Then the lowest-priority fields drop until
 * the line fits, and any dropped field that still fits comes back. Kept fields
 * that still overflow are cut with an ellipsis. Widths are counted in columns
 * on plain text, before color and link codes.
 */
function row(fields, width, s) {
  const columns = s.width ?? ((text) => text.length);
  const size = (list) => list.reduce((n, f, i) => n + columns(f.text) + (i ? SEPARATOR.length : 0), 0);
  if (size(fields.filter((f) => f.keep)) > width) fields = fields.map((f) => (f.short ? { ...f, text: f.short } : f));
  let kept = fields;
  const dropped = [];
  while (size(kept) > width) {
    const drop = kept.filter((f) => !f.keep).sort((a, b) => a.priority - b.priority)[0];
    if (!drop) break;
    dropped.unshift(drop);
    kept = kept.filter((f) => f !== drop);
  }
  // A short field dropped early can still fit once a longer one has gone.
  for (const field of dropped) {
    const next = fields.filter((f) => kept.includes(f) || f === field);
    if (size(next) <= width) kept = next;
  }
  let room = width, line = "";
  for (const [i, field] of kept.entries()) {
    const gap = i ? SEPARATOR : "";
    if (room - gap.length <= 0) break;
    const text = columns(field.text) > room - gap.length ? cut(field.text, room - gap.length - 1, columns) : field.text;
    line += (gap && s.fg("dim", gap)) + field.paint(text);
    room -= gap.length + columns(text);
  }
  return line;
}

/** The longest start of `text` within `limit` columns, plus an ellipsis. */
function cut(text, limit, columns) {
  let out = "";
  for (const char of text) {
    if (columns(out + char) > limit) break;
    out += char;
  }
  return `${out}…`;
}

function problem(snapshot) {
  return snapshot.retryAt ? `${snapshot.error}, retry ${clock(snapshot.retryAt)}` : snapshot.error;
}

function clock(time) {
  return time === undefined ? "never" : new Date(time).toTimeString().slice(0, 5);
}

/**
 * Drops control characters, so event text cannot move the cursor or open a
 * link, and bidirectional and zero-width marks, so it cannot reorder or hide
 * the fields after it.
 */
function clean(value) {
  return typeof value === "string"
    ? value.replace(/[\x00-\x1f\x7f-\x9f\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]/g, "") : undefined;
}
