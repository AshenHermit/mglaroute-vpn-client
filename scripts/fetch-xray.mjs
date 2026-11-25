import AdmZip from "adm-zip";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { downloadPair, getBuffer, getLatestRelease } from "./http-get.mjs";
import { pinnedRelease, selectAsset, targetTriple } from "./xray-assets.mjs";
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

function extractBinary(zipBuffer) {
  const zip = new AdmZip(zipBuffer);
  const entry = zip.getEntries().find((e) => /(^|\/)xray(\.exe)?$/i.test(e.entryName));
  if (!entry) throw new Error("xray binary not found in downloaded archive");
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "mglaroute-xray-"));
  zip.extractEntryTo(entry, tempDir, false, true);
  return path.join(tempDir, path.basename(entry.entryName));
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

  const extracted = extractBinary(zipBuffer);
  fs.mkdirSync(BIN_DIR, { recursive: true });
  fs.copyFileSync(extracted, SIMPLE);
  fs.copyFileSync(extracted, SUITED);
  if (PLATFORM !== "win32") {
    fs.chmodSync(SIMPLE, 0o755);
    fs.chmodSync(SUITED, 0o755);
  }
  fs.writeFileSync(SHA_FILE, `sha256:${sha256Hex(fs.readFileSync(SUITED))}\n`);

  console.log(checkVersion(SUITED).trim());
  console.log(`Bundled xray${EXT} and xray-${TRIPLE}${EXT}`);
}

export function verifyBinaries(options = {}) {
  const {
    binDir = BIN_DIR,
    platform = PLATFORM,
    arch = ARCH,
    xrayBinaryPath = process.env.XRAY_BINARY_PATH,
  } = options;
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
  } else {
    fetchBinary().catch((error) => {
      console.error(`xray fetch failed: ${error.message}`);
      process.exit(1);
    });
  }
}
