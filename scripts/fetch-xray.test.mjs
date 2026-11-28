import AdmZip from "adm-zip";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("node:child_process", () => {
  const spawnSync = vi.fn(() => ({ status: 0, stdout: "Xray 1.8.0-test\n" }));
  return { default: { spawnSync }, spawnSync };
});

import { extractZipEntry, stageGeoAssets, verifyBinaries } from "./fetch-xray.mjs";
import { GEO_ASSETS } from "./xray-assets.mjs";

const ANTI_ASSETS = ["winws2.exe", "cygwin1.dll", "WinDivert.dll", "WinDivert64.sys"];
const TEST_ENV = { platform: "win32", arch: "x64" };

function sha256Hex(text) {
  return createHash("sha256").update(text).digest("hex");
}

function fixture({ shaContents, geoAssets = GEO_ASSETS } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "mglaroute-xray-verify-"));
  writeFileSync(join(dir, "xray.exe"), "bundled-payload");
  writeFileSync(join(dir, "xray-x86_64-pc-windows-msvc.exe"), "bundled-payload");
  const antiDir = join(dir, "antiddpi");
  mkdirSync(antiDir);
  for (const name of ANTI_ASSETS) writeFileSync(join(antiDir, name), "stub");
  writeFileSync(join(dir, "xray.exe.sha256"), shaContents ?? `sha256:${sha256Hex("bundled-payload")}\n`);
  for (const name of geoAssets) {
    const payload = `${name}-payload`;
    writeFileSync(join(dir, name), payload);
    writeFileSync(join(dir, `${name}.sha256`), `sha256:${sha256Hex(payload)}\n`);
  }
  const override = join(dir, "dev-xray.exe");
  writeFileSync(override, "dev-override-payload");
  return { dir, override };
}

function zipFixture() {
  const zip = new AdmZip();
  zip.addFile("xray.exe", Buffer.from("bundled-payload"));
  const contents = {};
  for (const name of GEO_ASSETS) {
    const payload = `${name}-zip-payload`;
    const digest = sha256Hex(payload);
    zip.addFile(name, Buffer.from(payload));
    zip.addFile(`${name}.sha256`, Buffer.from(`sha256:${digest}\n`));
    contents[name] = { payload, digest };
  }
  return { zip, contents };
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

describe("geo assets", () => {
  it("copies geo assets and sidecars from the verified zip", () => {
    // AdmZip extraction helper reads every staged geo file and its published digest
    // from the same archive whose zip hash was already verified.
    const { zip, contents } = zipFixture();
    const ctx = fixture();
    try {
      for (const name of GEO_ASSETS) {
        const data = extractZipEntry(zip, name);
        expect(data.toString("utf8")).toBe(contents[name].payload);
        const sidecar = extractZipEntry(zip, `${name}.sha256`).toString("utf8");
        expect(sidecar).toContain(contents[name].digest);
        writeFileSync(join(ctx.dir, name), data);
        writeFileSync(join(ctx.dir, `${name}.sha256`), sidecar);
      }
      const io = captureConsole();
      try {
        expect(() => verifyBinaries({ ...TEST_ENV, binDir: ctx.dir })).not.toThrow();
      } finally {
        io.restore();
      }
    } finally {
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });

  it("verifyBinaries fails when a geo asset is missing", () => {
    const ctx = fixture({ geoAssets: ["geoip.dat"] }); // geosite.dat absent
    const io = captureConsole();
    try {
      expect(() => verifyBinaries({ ...TEST_ENV, binDir: ctx.dir })).toThrow(/geosite\.dat/i);
    } finally {
      io.restore();
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });

  it("stageGeoAssets rejects a published digest that does not match the geo bytes", () => {
    // Production must validate the zip's published sidecar against the extracted
    // .dat bytes. A self-consistent sidecar computed from the bytes is NOT enough.
    const { zip } = zipFixture();
    zip.updateFile("geoip.dat.sha256", Buffer.from(`sha256:${sha256Hex("different-payload")}\n`));
    const ctx = fixture({ geoAssets: [] });
    try {
      expect(() => stageGeoAssets(zip, ctx.dir)).toThrow(/sha256 mismatch/i);
      expect(existsSync(join(ctx.dir, "geoip.dat"))).toBe(false);
      expect(existsSync(join(ctx.dir, "geoip.dat.sha256"))).toBe(false);
    } finally {
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });

  it("stageGeoAssets rejects a geo asset with no published sidecar", () => {
    const { zip } = zipFixture();
    zip.deleteFile("geosite.dat.sha256");
    const ctx = fixture({ geoAssets: [] });
    try {
      expect(() => stageGeoAssets(zip, ctx.dir)).toThrow(/geosite\.dat\.sha256/i);
      expect(existsSync(join(ctx.dir, "geoip.dat"))).toBe(false); // validate all before writing any
      expect(existsSync(join(ctx.dir, "geosite.dat"))).toBe(false);
    } finally {
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });

  it("verifyBinaries fails on a geo asset sha mismatch", () => {
    const ctx = fixture();
    writeFileSync(join(ctx.dir, "geoip.dat"), "tampered");
    const io = captureConsole();
    try {
      expect(() => verifyBinaries({ ...TEST_ENV, binDir: ctx.dir })).toThrow(/sha256 mismatch/i);
    } finally {
      io.restore();
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });
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
