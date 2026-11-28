import AdmZip from "adm-zip";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import fs, { existsSync, mkdtempSync, rmSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { getBuffer, getLatestRelease } from "./http-get.mjs";

vi.mock("node:child_process", () => {
  const spawnSync = vi.fn(() => ({ status: 0, stdout: "Xray 1.8.0-test\n" }));
  return { default: { spawnSync }, spawnSync };
});

// No network in tests: feed known zip/dgst bytes keyed by URL.
vi.mock("./http-get.mjs", () => ({
  getBuffer: vi.fn(),
  downloadPair: vi.fn(async (download, zipUrl, dgstUrl) => [await download(zipUrl), await download(dgstUrl)]),
  getLatestRelease: vi.fn(),
}));

import { extractZipEntry, fetchAndroidBinaries, stageGeoAssets, verifyAndroidBinaries, verifyBinaries } from "./fetch-xray.mjs";
import { ANDROID_XRAY_ASSETS, GEO_ASSETS, pinnedRelease } from "./xray-assets.mjs";

const ANDROID_RELEASE = {
  tag_name: "v26.11.9",
  assets: ANDROID_XRAY_ASSETS.flatMap(({ asset }) => [
    { name: asset, browser_download_url: `https://example.test/${asset}` },
    { name: `${asset}.dgst`, browser_download_url: `https://example.test/${asset}.dgst` },
  ]),
};

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

  it("stageGeoAssets stages official-shaped zip without published sidecars", () => {
    // Official Xray release zips ship geoip.dat/geosite.dat but no per-file .sha256.
    // The zip-level .dgst is verified before extraction; sidecars are written locally
    // so verifyBinaries can detect later tampering of the staged files.
    const zip = new AdmZip();
    for (const name of GEO_ASSETS) zip.addFile(name, Buffer.from(`${name}-payload`));
    const ctx = fixture({ geoAssets: [] });
    try {
      expect(() => stageGeoAssets(zip, ctx.dir)).not.toThrow();
      for (const name of GEO_ASSETS) {
        expect(existsSync(join(ctx.dir, name))).toBe(true);
        expect(existsSync(join(ctx.dir, `${name}.sha256`))).toBe(true);
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

describe("Android binaries", () => {
  function androidZip(payload, { geo = false } = {}) {
    const zip = new AdmZip();
    zip.addFile("xray", payload);
    if (geo) {
      for (const name of GEO_ASSETS) zip.addFile(`assets/${name}`, `${name}-android-payload`);
    }
    return zip.toBuffer();
  }

  function androidDownloads({ corrupt = false } = {}) {
    const byUrl = new Map();
    for (const { asset, triple } of ANDROID_XRAY_ASSETS) {
      const zip = androidZip(`${triple}-xray-payload`, { geo: triple === "aarch64-linux-android" });
      const digest = corrupt && triple === "aarch64-linux-android"
        ? sha256Hex("wrong")
        : sha256Hex(zip);
      for (const base of [
        "https://example.test",
        "https://github.com/XTLS/Xray-core/releases/download/v26.11.9",
      ]) {
        byUrl.set(`${base}/${asset}`, zip);
        byUrl.set(`${base}/${asset}.dgst`, Buffer.from(`sha256:${digest}\n`));
      }
    }
    getBuffer.mockImplementation(async (url) => {
      const body = byUrl.get(url);
      if (!body) throw new Error(`unexpected URL: ${url}`);
      return body;
    });
  }

  function androidDir() {
    return mkdtempSync(join(tmpdir(), "mglaroute-android-xray-"));
  }

  it("android_fetch_stages_both_sidecars_and_geo_from_arm64_only", async () => {
    androidDownloads();
    const dir = androidDir();
    const chmod = vi.spyOn(fs, "chmodSync");
    const io = captureConsole();
    try {
      await fetchAndroidBinaries({ release: ANDROID_RELEASE, binDir: dir });
      for (const { triple } of ANDROID_XRAY_ASSETS) {
        const file = join(dir, `xray-${triple}`);
        expect(existsSync(file)).toBe(true);
        expect(existsSync(`${file}.sha256`)).toBe(true);
        expect(existsSync(join(dir, `xray-${triple}.exe`))).toBe(false);
      }
      expect(existsSync(join(dir, "geoip.dat"))).toBe(true);
      expect(existsSync(join(dir, "geosite.dat"))).toBe(true);
      expect(existsSync(join(dir, "geoip.dat.sha256"))).toBe(true);
      expect(existsSync(join(dir, "geosite.dat.sha256"))).toBe(true);
      // The x86_64 fixture has no geo entries: staging would fail if geo were
      // read from that zip instead of the arm64 one.
      expect(chmod).toHaveBeenCalledWith(expect.stringContaining("xray-aarch64-linux-android"), 0o755);
      if (process.platform !== "win32") {
        for (const { triple } of ANDROID_XRAY_ASSETS) {
          expect(statSync(join(dir, `xray-${triple}`)).mode & 0o777).toBe(0o755);
        }
      }
      expect(() => verifyAndroidBinaries({ binDir: dir })).not.toThrow();
      expect(spawnSync).not.toHaveBeenCalled(); // cross-compiled Android binary is not runnable on host
    } finally {
      chmod.mockRestore();
      io.restore();
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("android_fetch_rejects_a_zip_whose_dgst_does_not_match", async () => {
    androidDownloads({ corrupt: true });
    const dir = androidDir();
    const io = captureConsole();
    try {
      await expect(fetchAndroidBinaries({ release: ANDROID_RELEASE, binDir: dir })).rejects.toThrow(/sha256 mismatch/i);
      expect(existsSync(join(dir, "xray-aarch64-linux-android"))).toBe(false);
      expect(existsSync(join(dir, "xray-x86_64-linux-android"))).toBe(false);
    } finally {
      io.restore();
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("android_verify_requires_both_binaries_and_geo", () => {
    const ctx = fixture({ geoAssets: [] }); // no Windows xray here, no geo, no antiddpi
    const io = captureConsole();
    try {
      expect(() => verifyAndroidBinaries({ binDir: ctx.dir })).toThrow(/xray-aarch64-linux-android/);
      expect(() => verifyAndroidBinaries({ binDir: ctx.dir })).toThrow(/xray-x86_64-linux-android/);
      expect(() => verifyAndroidBinaries({ binDir: ctx.dir })).toThrow(/geoip\.dat/);
      expect(() => verifyAndroidBinaries({ binDir: ctx.dir })).toThrow(/geosite\.dat/);
    } finally {
      io.restore();
      rmSync(ctx.dir, { recursive: true, force: true });
    }
  });

  it("android_verify_skips_windows_anti_dpi_check", () => {
    const dir = androidDir();
    for (const { triple } of ANDROID_XRAY_ASSETS) {
      const payload = `${triple}-payload`;
      writeFileSync(join(dir, `xray-${triple}`), payload);
      writeFileSync(join(dir, `xray-${triple}.sha256`), `sha256:${sha256Hex(payload)}\n`);
    }
    for (const name of GEO_ASSETS) {
      const payload = `${name}-payload`;
      writeFileSync(join(dir, name), payload);
      writeFileSync(join(dir, `${name}.sha256`), `sha256:${sha256Hex(payload)}\n`);
    }
    // No antiddpi/ directory: Android must not require it.
    const io = captureConsole();
    try {
      expect(() => verifyAndroidBinaries({ binDir: dir })).not.toThrow();
      expect(existsSync(join(dir, "antiddpi"))).toBe(false);
    } finally {
      io.restore();
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("android_fetch_falls_back_to_pinned_release_without_network", async () => {
    androidDownloads();
    getLatestRelease.mockRejectedValueOnce(new Error("offline"));
    const dir = androidDir();
    const io = captureConsole();
    try {
      await fetchAndroidBinaries({ binDir: dir });
      expect(existsSync(join(dir, "xray-aarch64-linux-android"))).toBe(true);
      expect(existsSync(join(dir, "xray-x86_64-linux-android"))).toBe(true);
    } finally {
      io.restore();
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("pinned_android_release_uses_xray_version_and_exact_assets", () => {
    const release = pinnedRelease("android", "arm64");
    expect(release.tag_name).toBe("v26.11.9");
    expect(release.assets.map((a) => a.name)).toEqual(["Xray-android-arm64-v8a.zip"]);
  });
});
