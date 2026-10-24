import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("../.github/release/changelog.sh", import.meta.url));
const worktree = fileURLToPath(new URL("../", import.meta.url));

// Runs changelog.sh in a scratch repository: an initial commit tagged v0.1.0
// with `changelog` as CHANGELOG.md, then one empty commit per subject.
// Returns the exit status, stderr and the CHANGELOG.md that results.
function release(changelog, subjects) {
  const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, "changelog-test-"));
  try {
    const git = (...args) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", ...args], { cwd: root });
    git("init", "-q");
    writeFileSync(join(root, "CHANGELOG.md"), changelog);
    git("add", "CHANGELOG.md");
    git("commit", "-qm", "Start");
    git("tag", "v0.1.0");
    for (const subject of subjects) git("commit", "-q", "--allow-empty", "-m", subject);
    const output = spawnSync("bash", [script, "0.2.0", "2026-10-25"], { cwd: root, encoding: "utf8" });
    return { status: output.status, stderr: output.stderr, changelog: readFileSync(join(root, "CHANGELOG.md"), "utf8") };
  } finally { rmSync(root, { recursive: true, force: false }); }
}

const top = "# Changelog\n\n## Unreleased\n";
const older = "## 0.1.0 - 2026-10-01\n\n- Old.\n";

test("each commit since the last tag becomes a line, newest first", () => {
  const { status, changelog } = release(`${top}\n${older}`, ["Add a (#3)", "Release 0.1.1 (#4)", "Fix b (#5)"]);
  assert.equal(status, 0);
  assert.equal(changelog, `${top}\n## 0.2.0 - 2026-10-25\n\n- Fix b (#5)\n\n- Add a (#3)\n\n${older}`);
});

test("lines under Unreleased stay first and replace their PR's line", () => {
  const { changelog } = release(`${top}\n- Add a, in my words (#3).\n\n${older}`, ["Add a (#3)", "Fix b (#5)", "Fix c (#35)"]);
  assert.equal(changelog,
    `${top}\n## 0.2.0 - 2026-10-25\n\n- Add a, in my words (#3).\n\n- Fix c (#35)\n\n- Fix b (#5)\n\n${older}`);
});

test("nothing to list changes nothing", () => {
  const { status, stderr, changelog } = release(`${top}\n${older}`, ["Release 0.1.1 (#4)"]);
  assert.equal(status, 1);
  assert.match(stderr, /Nothing to list/);
  assert.equal(changelog, `${top}\n${older}`);
});
