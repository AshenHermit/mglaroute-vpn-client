import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

export function parseSha256(text) {
  const lines = String(text).split(/\r?\n/);
  const line = lines.find((l) => /^\s*(sha2-256|sha256)\s*[=:]/i.test(l));
  if (line) {
    return line
      .replace(/^\s*(sha2-256|sha256)\s*[=:]\s*/i, "")
      .trim()
      .split(/\s+/)[0]
      .toLowerCase();
  }
  return String(text).replace(/^sha256:/i, "").trim().split(/\s+/)[0].toLowerCase();
}

export function sha256Hex(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

export function verifyBufferSha256(buffer, expectedHex) {
  return sha256Hex(buffer) === parseSha256(expectedHex);
}

export function verifyFileSha256(file, expectedHex) {
  const expected = parseSha256(expectedHex);
  const actual = sha256Hex(readFileSync(file));
  if (actual !== expected) {
    throw new Error(`sha256 mismatch: expected ${expected}, got ${actual}`);
  }
  return true;
}
