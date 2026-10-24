import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const worktree = fileURLToPath(new URL("../", import.meta.url));
for (const [error, expected] of [["GraphQL: Auto-merge is not supported for stacked pull requests. (enablePullRequestAutoMerge)", 0], ["GraphQL: Resource not accessible by integration", 1]]) {
  test(`review gate handles ${expected === 0 ? "native stacks" : "ordinary API refusal"} without merging`, () => {
    const target = join(worktree, "target"); mkdirSync(target, { recursive: true });
    const root = mkdtempSync(join(target, "gate-test-"));
    try {
      const log = join(root, "calls.jsonl");
      writeFileSync(join(root, "gh"), `#!/usr/bin/env node
import fs from "node:fs";
const args = process.argv.slice(2), index = args.indexOf("--body-file");
fs.appendFileSync(process.env.TEST_GH_LOG, JSON.stringify({args, body:index < 0 ? "" : fs.readFileSync(args[index + 1], "utf8")}) + "\\n");
if (args[0] === "pr" && args[1] === "view") console.log(JSON.stringify({state:"OPEN", isDraft:false, title:"Test change", body:"A change.", headRefName:"storm/test", labels:[], autoMergeRequest:null, mergeStateStatus:"BLOCKED", comments:[]}));
else if (args[0] === "pr" && args[1] === "merge") { console.error(process.env.TEST_GH_ERROR); process.exit(1); }
else if (args[0] === "api" || (args[0] === "pr" && args[1] === "comment")) console.log("{}");
else process.exit(91);
`, { mode: 0o700 });
      const head = "a".repeat(40);
      const output = spawnSync("bash", [".github/storm/gate.sh"], { cwd: worktree, encoding: "utf8", timeout: 10000,
        env: { ...process.env, PATH: `${root}:${process.env.PATH}`, REPO:"owner/repo", PR:"1", EVENT:"comment", HEAD_SHA:head,
          HEAD_DIR:join(root,"absent"), AUTHOR:"stormlight-thunderbot[bot]", BODY:`Review finished at ${head}`, TEST_GH_LOG:log, TEST_GH_ERROR:error } });
      assert.equal(output.error, undefined); assert.equal(output.status, expected, output.stderr);
      const calls = readFileSync(log,"utf8").trim().split("\n").map(line => JSON.parse(line));
      assert(calls.some(call => call.args.includes(`conclusion=${expected === 0 ? "success" : "failure"}`)));
      assert(calls.filter(call => call.args[1] === "merge").every(call => call.args.includes("--auto")));
      if (expected === 0) {
        const preview = calls.find(call => call.body.includes("native stacks"))?.body;
        assert(preview?.includes("Co-authored-by:")); assert(!preview.includes("can merge now"));
      }
    } finally { rmSync(root, { recursive:true, force:false }); }
  });
}
