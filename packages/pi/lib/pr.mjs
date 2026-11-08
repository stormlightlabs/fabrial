import { execFile } from "node:child_process";

/** The shortest gap between two GitHub reads. */
export const MIN_INTERVAL = 30000;
const MAX_BACKOFF = 15 * 60000;
const RATE_LIMIT_BACKOFF = 5 * 60000;
const FIELDS = "number,url,state,isDraft,headRefName,headRefOid,statusCheckRollup,reviews";
const FAILED = ["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE"];
const PASSED = ["SUCCESS", "NEUTRAL", "SKIPPED"];
const BRANCH_MOVES = /\bgit\b[^\n]*\b(push|checkout|switch)\b|\bgh\s+pr\s+checkout\b/;

/**
 * Watches the pull request for the session's branch through the host's `gh`.
 * The status area reads `get()` and `subscribe()`; it never polls GitHub itself.
 * Snapshots carry no credentials and no PR text. A reading is for the person
 * to read: nothing here sends a message or starts a turn.
 */
export class PullRequestCollector {
  constructor({ cwd, progress = () => {}, env = process.env, clock = Date.now }) {
    this.cwd = cwd;
    this.progress = progress;
    this.env = env;
    this.clock = clock;
    this.controller = new AbortController();
    this.listeners = new Set();
    this.snapshot = { status: "unknown" };
    this.errors = 0;
    this.attempted = -Infinity;
    this.backoff = 0;
    this.generation = 0;
    this.disposed = false;
  }

  /** The latest snapshot. `status` is unknown, stale, pending, failed or success. */
  get() { return this.snapshot; }

  /** Calls `listener` with each new snapshot; returns the unsubscribe function. */
  subscribe(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  start() {
    if (this.timer || this.disposed) return;
    this.timer = setInterval(() => { void this.poll().catch(() => {}); }, 1000);
    this.timer.unref?.();
  }

  stop() {
    if (this.disposed) return;
    this.disposed = true;
    clearInterval(this.timer);
    this.controller.abort();
    this.listeners.clear();
  }

  /** After a push or branch change: the head may have moved, so stop reporting it as current. */
  refresh() {
    if (this.disposed) return;
    // A read already in flight saw the old head; poll() drops it.
    this.generation++;
    if (this.snapshot.pr) this.publish({ ...this.snapshot, status: "stale" });
  }

  /** Reads GitHub when due: 30 s after the last read, and after any backoff. */
  async poll() {
    if (this.disposed) return;
    const now = this.clock();
    if (this.busy || now < this.backoff) return;
    if (now - this.attempted < MIN_INTERVAL) return;
    this.busy = true;
    this.attempted = now;
    const generation = this.generation;
    try {
      const branch = (await this.run("git", ["rev-parse", "--abbrev-ref", "HEAD"])).trim();
      const raw = branch === "HEAD" ? null : await this.view(branch);
      if (this.disposed) return;
      // The head moved during the read, so it stays stale until the next read.
      if (generation !== this.generation) return;
      this.errors = 0;
      this.backoff = 0;
      const snapshot = raw ? observe(raw, this.clock()) : { status: "unknown", branch, observedAt: this.clock() };
      this.publish({ ...snapshot, branch });
    } catch (error) {
      if (this.disposed || generation !== this.generation) return;
      this.errors++;
      const limited = /rate limit|HTTP 429/i.test(error.stderr ?? "");
      const delay = Math.min(MAX_BACKOFF, Math.max(limited ? RATE_LIMIT_BACKOFF : 0, MIN_INTERVAL * 2 ** this.errors));
      this.backoff = this.clock() + delay;
      // A failed read never turns a known head green or clears it; it only ages it.
      this.publish({ ...this.snapshot, status: this.snapshot.pr ? "stale" : "unknown",
        error: limited ? "GitHub rate limit" : "GitHub lookup failed", retryAt: this.backoff });
    } finally { this.busy = false; }
  }

  /**
   * `gh pr view <branch>` matches any PR whose head has that name, including
   * one from a fork, so list the branch's PRs and keep the newest whose head
   * is in the base repository.
   */
  async view(branch) {
    const prs = JSON.parse(await this.run("gh",
      ["pr", "list", "--head", branch, "--state", "all", "--limit", "20", "--json", "number,isCrossRepository,createdAt"]));
    const own = prs.filter((pr) => !pr.isCrossRepository).sort((a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt));
    if (!own.length) return null;
    return JSON.parse(await this.run("gh", ["pr", "view", String(own[0].number), "--json", FIELDS]));
  }

  run(command, args) {
    return new Promise((resolve, reject) => {
      execFile(command, args, { cwd: this.cwd, env: this.env, signal: this.controller.signal, maxBuffer: 16 * 1024 * 1024 },
        (error, stdout, stderr) => {
          if (!error) return resolve(stdout);
          // Keep stderr for classification only; it never reaches a message or event.
          reject(Object.assign(new Error(`${command} failed`), { stderr: String(stderr) }));
        });
    });
  }

  publish(snapshot) {
    this.snapshot = snapshot;
    this.progress(snapshot);
    for (const listener of this.listeners) listener(snapshot);
  }
}

/** Whether a shell command may move the session's branch or head. */
export function movesBranch(command) {
  return BRANCH_MOVES.test(String(command ?? ""));
}

/** One read of a PR, keyed by repository, number and head; checks and reviews belong to that head only. */
export function observe(raw, observedAt) {
  const items = (raw.statusCheckRollup ?? []).map((check) => {
    const result = check.__typename === "StatusContext" ? check.state
      : check.status === "COMPLETED" ? check.conclusion : "PENDING";
    return { name: check.name ?? check.context,
      state: FAILED.includes(result) ? "failed" : PASSED.includes(result) ? "success" : "pending" };
  });
  const count = (state) => items.filter((item) => item.state === state).length;
  const checks = { total: items.length, passed: count("success"), pending: count("pending"), failed: count("failed"), items };
  const status = checks.failed ? "failed" : checks.pending ? "pending" : checks.total ? "success" : "unknown";
  const reviews = currentReviews(raw);
  const pr = {
    repository: new URL(raw.url).pathname.split("/").slice(1, 3).join("/"),
    number: raw.number, url: raw.url, head: raw.headRefOid, draft: raw.isDraft,
    state: raw.state.toLowerCase(),
  };
  return {
    status, observedAt, key: `${pr.repository}#${pr.number}@${pr.head}`, pr, checks,
    reviews: { approved: reviews.filter((r) => r.state === "APPROVED").length,
      changesRequested: reviews.filter((r) => r.state === "CHANGES_REQUESTED").length },
  };
}

/** Reviews of the current head; a push leaves earlier reviews behind. */
function currentReviews(raw) {
  return (raw.reviews ?? []).filter((r) => r.commit?.oid === raw.headRefOid);
}
