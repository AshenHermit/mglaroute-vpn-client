import AdmZip from "adm-zip";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { downloadPair, getBuffer, getLatestRelease } from "./http-get.mjs";
import {
  ANDROID_XRAY_ASSETS,
  GEO_ASSETS,
  pinnedRelease,
  selectAsset,
  targetTriple,
  XRAY_VERSION,
} from "./xray-assets.mjs";
import { parseSha256, sha256Hex, verifyBufferSha256, verifyFileSha256 } from "./verify-sha256.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..");
const BIN_DIR = path.join(REPO_ROOT, "apps", "desktop", "src-tauri", "binaries");
const PLATFORM = process.platform;
const ARCH = process.arch;
const EXT = PLATFORM === "win32" ? ".exe" : "";
const TRIPLE = targetTriple(PLATFORM, ARCH);
const SIMPLE = path.join(BIN_DIR, `xray${EXT}`);
const SUITED = path.join(BIN_DIR, `xray-${TRIPLE}${EXT}`);
const SHA_FILE = path.join(BIN_DIR, `xray${EXT}.sha256`);

function resolveRelease() {
  if (process.env.XRAY_VERSION) return pinnedRelease(PLATFORM, ARCH, process.env.XRAY_VERSION);
  return getLatestRelease().catch(() => pinnedRelease(PLATFORM, ARCH));
}

function findZipEntry(zip, name) {
  const wanted = name.toLowerCase();
  return zip.getEntries().find((e) => {
    const lower = e.entryName.toLowerCase();
    return lower === wanted || lower.endsWith(`/${wanted}`);
  });
}

// Read a named entry from a verified release zip by basename. Callers verify
// the zip digest before constructing the archive; this helper only selects the
// entry to stage.
export function extractZipEntry(zip, name) {
  const entry = findZipEntry(zip, name);
  if (!entry) throw new Error(`${name} not found in downloaded archive`);
  return entry.getData();
}

// Official Xray zips ship geoip.dat/geosite.dat without per-file .sha256 files.
// The zip-level .dgst is verified before extraction, so the extracted bytes are
// already upstream-authenticated. If a published per-file sidecar is present,
// validate it too; otherwise derive the local sidecar from the extracted bytes
// so verifyBinaries can detect later tampering of the staged files.
export function stageGeoAssets(zip, binDir) {
  const staged = [];
  for (const name of GEO_ASSETS) {
    const data = extractZipEntry(zip, name);
    const sidecarEntry = findZipEntry(zip, `${name}.sha256`);
    if (sidecarEntry && !verifyBufferSha256(data, sidecarEntry.getData().toString("utf8"))) {
      throw new Error(`sha256 mismatch for ${name} in downloaded archive`);
    }
    staged.push({ name, data, digest: sha256Hex(data) });
  }
  for (const { name, data, digest } of staged) {
    fs.writeFileSync(path.join(binDir, name), data);
    fs.writeFileSync(path.join(binDir, `${name}.sha256`), `sha256:${digest}\n`);
  }
}

function extractBinary(zip) {
  const data = extractZipEntry(zip, PLATFORM === "win32" ? "xray.exe" : "xray");
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "mglaroute-xray-"));
  const extracted = path.join(tempDir, PLATFORM === "win32" ? "xray.exe" : "xray");
  fs.writeFileSync(extracted, data);
  return extracted;
}

function checkVersion(binary) {
  const result = spawnSync(binary, ["-version"], { encoding: "utf8" });
  if (result.error || result.status !== 0) {
    throw new Error(`xray -version failed: ${result.stderr || result.stdout || result.error?.message || "unknown error"}`);
  }
  return result.stdout || "";
}

async function fetchBinary() {
  const release = await resolveRelease();
  const asset = selectAsset(release, PLATFORM, ARCH);
  console.log(`Using xray ${release.tag_name}`);

  const zipUrl = process.env.XRAY_DOWNLOAD_URL || asset.browser_download_url;
  let dgstUrl = `${zipUrl}.dgst`;
  if (!process.env.XRAY_DOWNLOAD_URL) {
    const dgstAsset = release.assets?.find((a) => a.name === `${asset.name}.dgst`);
    if (dgstAsset) dgstUrl = dgstAsset.browser_download_url;
  }

  const [zipBuffer, dgstBuffer] = await downloadPair(getBuffer, zipUrl, dgstUrl);
  const expected = parseSha256(dgstBuffer.toString("utf8"));
  if (!verifyBufferSha256(zipBuffer, expected)) {
    throw new Error(`sha256 mismatch for ${asset.name}`);
  }

  const zip = new AdmZip(zipBuffer);
  const extracted = extractBinary(zip);
  fs.mkdirSync(BIN_DIR, { recursive: true });
  fs.copyFileSync(extracted, SIMPLE);
  fs.copyFileSync(extracted, SUITED);
  if (PLATFORM !== "win32") {
    fs.chmodSync(SIMPLE, 0o755);
    fs.chmodSync(SUITED, 0o755);
  }
  fs.writeFileSync(SHA_FILE, `sha256:${sha256Hex(fs.readFileSync(SUITED))}\n`);

  stageGeoAssets(zip, BIN_DIR);

  console.log(checkVersion(SUITED).trim());
  console.log(`Bundled xray${EXT} and xray-${TRIPLE}${EXT}`);
  console.log(`Bundled geo assets: ${GEO_ASSETS.join(", ")}`);
}

function pinnedAndroidRelease() {
  const tag = process.env.XRAY_VERSION || XRAY_VERSION;
  const assets = ["arm64", "x64"].flatMap((arch) => {
    const release = pinnedRelease("android", arch, tag);
    return release.assets.flatMap((asset) => [
      asset,
      { name: `${asset.name}.dgst`, browser_download_url: `${asset.browser_download_url}.dgst` },
    ]);
  });
  return { tag_name: tag, assets };
}

function resolveAndroidRelease() {
  if (process.env.XRAY_VERSION) return pinnedAndroidRelease();
  return getLatestRelease().catch(() => pinnedAndroidRelease());
}

// Stage one Tauri sidecar per published Android asset. Each zip is verified
// against its upstream .dgst before extraction; cross-compiled binaries are
// never executed on the build host. Geo assets are staged from the arm64 zip
// only so the bundle carries a single authenticated copy.
export async function fetchAndroidBinaries({
  release,
  binDir = BIN_DIR,
  download = getBuffer,
  openZip = (buffer) => new AdmZip(buffer),
} = {}) {
  const target = release ?? (await resolveAndroidRelease());
  fs.mkdirSync(binDir, { recursive: true });

  for (const { asset, triple } of ANDROID_XRAY_ASSETS) {
    const selected = target.assets?.find((a) => a.name === asset);
    if (!selected) throw new Error(`No xray asset found: ${asset}`);
    const zipUrl = selected.browser_download_url;
    const dgstAsset = target.assets?.find((a) => a.name === `${asset}.dgst`);
    const dgstUrl = dgstAsset?.browser_download_url ?? `${zipUrl}.dgst`;

    const [zipBuffer, dgstBuffer] = await downloadPair(download, zipUrl, dgstUrl);
    const expected = parseSha256(dgstBuffer.toString("utf8"));
    if (!verifyBufferSha256(zipBuffer, expected)) throw new Error(`sha256 mismatch for ${asset}`);

    const zip = openZip(zipBuffer);
    const data = extractZipEntry(zip, "xray");
    const staged = path.join(binDir, `xray-${triple}`);
    fs.writeFileSync(staged, data);
    fs.chmodSync(staged, 0o755);
    fs.writeFileSync(`${staged}.sha256`, `sha256:${sha256Hex(data)}\n`);
    console.log(`Bundled xray-${triple}`);
    // Only the arm64 zip carries geo assets; stage them from this verified copy.
    if (triple === ANDROID_XRAY_ASSETS[0].triple) stageGeoAssets(zip, binDir);
  }

  console.log(`Bundled geo assets: ${GEO_ASSETS.join(", ")}`);
}

export function verifyAndroidBinaries(options = {}) {
  return verifyBinaries({ ...options, android: true });
}

export function verifyBinaries(options = {}) {
  const {
    binDir = BIN_DIR,
    platform = PLATFORM,
    arch = ARCH,
    xrayBinaryPath = process.env.XRAY_BINARY_PATH,
    android = false,
  } = options;

  if (android) {
    const missing = [];
    for (const { triple } of ANDROID_XRAY_ASSETS) {
      const file = path.join(binDir, `xray-${triple}`);
      const sidecar = path.join(binDir, `xray-${triple}.sha256`);
      if (!fs.existsSync(file)) missing.push(`xray-${triple}`);
      if (!fs.existsSync(sidecar)) missing.push(`xray-${triple}.sha256`);
    }
    for (const name of GEO_ASSETS) {
      if (!fs.existsSync(path.join(binDir, name))) missing.push(name);
      if (!fs.existsSync(path.join(binDir, `${name}.sha256`))) missing.push(`${name}.sha256`);
    }
    if (missing.length) throw new Error(`missing Android bundle: ${missing.join(", ")}`);
    for (const { triple } of ANDROID_XRAY_ASSETS) {
      const file = path.join(binDir, `xray-${triple}`);
      verifyFileSha256(file, fs.readFileSync(`${file}.sha256`, "utf8"));
    }
    for (const name of GEO_ASSETS) {
      verifyFileSha256(path.join(binDir, name), fs.readFileSync(path.join(binDir, `${name}.sha256`), "utf8"));
    }
    // Android has no Windows anti-DPI assets and cannot execute host -version.
    console.log("Android binary bundle verified");
    return;
  }

  const ext = platform === "win32" ? ".exe" : "";
  const triple = targetTriple(platform, arch);
  const simple = path.join(binDir, `xray${ext}`);
  const suited = path.join(binDir, `xray-${triple}${ext}`);
  const shaFile = path.join(binDir, `xray${ext}.sha256`);

  if (xrayBinaryPath) {
    // Dev override is a genuinely different binary: never read the bundled SHA_FILE
    // and never force its hash to match the bundled digest. No separate digest is
    // shipped for dev binaries, so state the skipped verification explicitly.
    if (!fs.existsSync(xrayBinaryPath)) throw new Error(`missing dev xray: ${path.basename(xrayBinaryPath)}`);
    console.warn(
      `[dev] XRAY_BINARY_PATH=${path.basename(xrayBinaryPath)}: bundled sha256 verification skipped; binary not verified against bundle`,
    );
    console.log(checkVersion(xrayBinaryPath).trim());
  } else {
    const shaText = fs.readFileSync(shaFile, "utf8");
    for (const candidate of [simple, suited]) {
      if (!fs.existsSync(candidate)) throw new Error(`missing bundled xray: ${path.basename(candidate)}`);
      verifyFileSha256(candidate, shaText);
    }
    for (const name of GEO_ASSETS) {
      const file = path.join(binDir, name);
      const sidecar = path.join(binDir, `${name}.sha256`);
      if (!fs.existsSync(file)) throw new Error(`missing bundled geo asset: ${name}`);
      if (!fs.existsSync(sidecar)) throw new Error(`missing bundled geo asset digest: ${name}.sha256`);
      verifyFileSha256(file, fs.readFileSync(sidecar, "utf8"));
    }
    console.log(checkVersion(suited).trim());
  }

  const antiDir = path.join(binDir, "antiddpi");
  for (const name of ["winws2.exe", "cygwin1.dll", "WinDivert.dll", "WinDivert64.sys"]) {
    if (!fs.existsSync(path.join(antiDir, name))) throw new Error(`missing anti-DPI asset: ${name}`);
  }
  console.log(xrayBinaryPath ? "Dev xray checked (bundled sha256 skipped)" : "Binary bundle verified");
}

const invokedAsCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedAsCli) {
  if (process.argv.includes("--verify")) {
    try {
      verifyBinaries();
    } catch (error) {
      console.error(`binary verification failed: ${error.message}`);
      process.exit(1);
    }
  } else if (process.argv.includes("--verify-android")) {
    try {
      verifyAndroidBinaries();
    } catch (error) {
      console.error(`Android binary verification failed: ${error.message}`);
      process.exit(1);
    }
  } else if (process.argv.includes("--android")) {
    fetchAndroidBinaries().catch((error) => {
      console.error(`Android xray fetch failed: ${error.message}`);
      process.exit(1);
    });
  } else {
    fetchBinary().catch((error) => {
      console.error(`xray fetch failed: ${error.message}`);
      process.exit(1);
    });
  }
}
