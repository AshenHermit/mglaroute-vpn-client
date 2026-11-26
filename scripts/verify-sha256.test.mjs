import { createHash } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { parseSha256, verifyFileSha256 } from "./verify-sha256.mjs";

function fixture(contents) {
  const dir = mkdtempSync(join(tmpdir(), "mglaroute-sha-"));
  const file = join(dir, "asset.zip");
  writeFileSync(file, contents);
  return { dir, file };
}

describe("verifySha256", () => {
  it("verify_sha256_rejects_mismatch", () => {
    const { dir, file } = fixture("payload");
    try {
      expect(() => verifyFileSha256(file, "00".repeat(32))).toThrow(/sha256 mismatch/i);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("verify_sha256_accepts_match", () => {
    const expected = createHash("sha256").update("payload").digest("hex");
    const { dir, file } = fixture("payload");
    try {
      expect(verifyFileSha256(file, expected)).toBe(true);
      expect(parseSha256(`sha256:${expected}`)).toBe(expected);
      expect(parseSha256(`SHA2-256 = ${expected}`)).toBe(expected);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
