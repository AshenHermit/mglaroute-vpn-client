import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it } from "vitest";

// Vitest runs with the repo root as cwd. Missing file becomes an empty string
// so assertions fail with a clear message instead of ENOENT.
const workflowPath = join(process.cwd(), ".github/workflows/ci.yml");
const workflow = existsSync(workflowPath) ? readFileSync(workflowPath, "utf8") : "";

describe("CI workflow hardening", () => {
  it("grants only read access to repository contents", () => {
    assert.match(workflow, /^permissions:\s*$/m);
    assert.match(workflow, /^\s{2}contents:\s*read\s*$/m);
    assert.doesNotMatch(workflow, /contents:\s*write/);
  });

  it("bounds both jobs with an explicit timeout", () => {
    assert.match(workflow, /^\s{4}timeout-minutes:\s*15\s*$/m);
    assert.match(workflow, /^\s{4}timeout-minutes:\s*25\s*$/m);
    assert.equal(workflow.match(/timeout-minutes:/g)?.length ?? 0, 2);
  });
});
