import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("node:child_process", () => {
  const spawnSync = vi.fn(() => ({ status: 0, stdout: "Xray 1.8.0-test\n" }));
  return { default: { spawnSync }, spawnSync };
});

import { verifyBinaries } from "./fetch-xray.mjs";

const ANTI_ASSETS = ["winws2.exe", "cygwin1.dll", "WinDivert.dll", "WinDivert64.sys"];
const TEST_ENV = { platform: "win32", arch: "x64" };

function fixture({ shaContents } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "mglaroute-xray-verify-"));
  writeFileSync(join(dir, "xray.exe"), "bundled-payload");
  writeFileSync(join(dir, "xray-x86_64-pc-windows-msvc.exe"), "bundled-payload");
  const antiDir = join(dir, "antiddpi");
  mkdirSync(antiDir);
  for (const name of ANTI_ASSETS) writeFileSync(join(antiDir, name), "stub");
  if (shaContents !== undefined) writeFileSync(join(dir, "xray.exe.sha256"), shaContents);
  const override = join(dir, "dev-xray.exe");
  writeFileSync(override, "dev-override-payload");
  return { dir, override };
}

function captureConsole() {
  const log = vi.spyOn(console, "log").mockImplementation(() => {});
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  return {
    restore() {
      log.mockRestore();
      warn.mockRestore();
    },
    log,
    warn,
  };
}

afterEach(() => {
  vi.clearAllMocks();
});

describe("verifyBinaries", () => {
  it("xray_binary_path_override_skips_bundled_sha", () => {
    // No SHA_FILE anywhere: the dev override must not read the bundled digest,
    // must not force the override to match bundled bytes, and must run -version on the override.
    const ctx = fixture();
    const io = captureConsole();
    try {
      expect(() => verifyBinaries({ ...TEST_ENV, binDir: ctx.dir, xrayBinaryPath: ctx.override })).not.toThrow();
      expect(spawnSync).toHaveBeenCalledWith(ctx.override, ["-version"], expect.anything());
      expect(io.log.mock.calls.flat().join("\n")).toContain("Xray 1.8.0-test");
      expect(io.warn.mock.calls.flat().join("\n")).toMatch(/dev.*sha256|sha256.*dev/i);
    } finally {
      io.restore();
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });

  it("bundled_binary_sha_mismatch_still_fails", () => {
    // Production path unchanged: bundled binaries still require SHA_FILE verification.
    const bogus = createHash("sha256").update("different-payload").digest("hex");
    const ctx = fixture({ shaContents: `sha256:${bogus}\n` });
    const io = captureConsole();
    try {
      expect(() => verifyBinaries({ ...TEST_ENV, binDir: ctx.dir })).toThrow(/sha256 mismatch/i);
      expect(spawnSync).not.toHaveBeenCalled();
    } finally {
      io.restore();
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });
});
