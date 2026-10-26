import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("changelog.sh", import.meta.url));
const worktree = fileURLToPath(new URL("../../", import.meta.url));

// Runs changelog.sh in a scratch repository: an initial commit with
// `changelog` as CHANGELOG.md, tagged v0.1.0 unless `tag` is false, then one
// empty commit per subject. Returns the exit status, stderr and the
// CHANGELOG.md that results.
function release(changelog, subjects, { tag = true, candidate = false } = {}) {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, "changelog-test-"));
  try {
    const git = (...args) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", ...args], { cwd: root });
    git("init", "-q");
    writeFileSync(join(root, "CHANGELOG.md"), changelog);
    git("add", "CHANGELOG.md");
    git("commit", "-qm", "Start");
    if (tag) git("tag", "v0.1.0");
    for (const subject of subjects) git("commit", "-q", "--allow-empty", "-m", subject);
    if (candidate) git("tag", "v0.2.0-rc.1");
    const output = spawnSync("bash", [script, "0.2.0", "2026-10-25"], { cwd: root, encoding: "utf8" });
    return { status: output.status, stderr: output.stderr, changelog: readFileSync(join(root, "CHANGELOG.md"), "utf8") };
  } finally { rmSync(root, { recursive: true, force: false }); }
}

const top = "# Changelog\n\nWhat each release changes.\n";
const older = "## 0.1.0 - 2026-10-01\n\n- Old.\n";

test("each commit since the last tag becomes a line above the newest release", () => {
  const { status, changelog } = release(`${top}\n${older}`, ["Add a (#3)", "Release 0.1.1 (#4)", "Fix b (#5)"]);
  assert.equal(status, 0);
  assert.equal(changelog, `${top}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n- Add a (#3)\n\n${older}`);
});

test("a changelog with no release yet gets the section at the end", () => {
  const { changelog } = release(top, ["Fix b (#5)"]);
  assert.equal(changelog, `${top}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n`);
});

test("a heading inside the intro is not a release", () => {
  const intro = "# Changelog\n\n```md\n## Example\n```\n";
  const { changelog } = release(`${intro}\n${older}`, ["Fix b (#5)"]);
  assert.equal(changelog, `${intro}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n${older}`);
});

test("without a tag, every commit is listed", () => {
  const { changelog } = release(`${top}\n${older}`, ["Fix b (#5)"], { tag: false });
  assert.equal(changelog, `${top}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n- Start\n\n${older}`);
});

test("a pre-release tag is not the base", () => {
  const { changelog } = release(`${top}\n${older}`, ["Fix b (#5)"], { candidate: true });
  assert.match(changelog, /- Fix b \(#5\)/);
});

test("nothing to list changes nothing", () => {
  const { status, stderr, changelog } = release(`${top}\n${older}`, ["Release 0.1.1 (#4)"]);
  assert.equal(status, 1);
  assert.match(stderr, /Nothing to list/);
  assert.equal(changelog, `${top}\n${older}`);
});
