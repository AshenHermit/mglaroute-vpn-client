import AdmZip from "adm-zip";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { getBuffer } from "./http-get.mjs";
import { verifyBufferSha256 } from "./verify-sha256.mjs";

const COMMIT = "6eb463a6758fb48cd101bc55dfd057e6e9d98af1";
const SHA256 = "5f828239310fed0da587bc09eaa22742615b2a85cf41f55ff28128e22231c6de";
const ARCHIVE_URL = `https://github.com/bol-van/zapret-win-bundle/archive/${COMMIT}.zip`;

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..");
const DEST = path.join(REPO_ROOT, "apps", "desktop", "src-tauri", "binaries", "antiddpi");
const REQUIRED = ["winws2.exe", "cygwin1.dll", "WinDivert.dll", "WinDivert64.sys"];

function writeEntry(entry, destination) {
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.writeFileSync(destination, entry.getData());
}

function safeName(value) {
  return String(value).replace(/[^A-Za-z0-9._-]/g, "_");
}

async function main() {
  console.log(`Staging anti-DPI bundle ${COMMIT.slice(0, 12)}`);
  const archive = await getBuffer(ARCHIVE_URL);
  if (!verifyBufferSha256(archive, SHA256)) {
    throw new Error("sha256 mismatch for zapret-win-bundle");
  }

  const entries = new AdmZip(archive).getEntries();
  const byBasename = (name) =>
    entries.find((e) => {
      const lower = e.entryName.toLowerCase();
      return lower === name.toLowerCase() || lower.endsWith(`/${name.toLowerCase()}`);
    });

  fs.mkdirSync(DEST, { recursive: true });
  for (const name of REQUIRED) {
    const entry = byBasename(name);
    if (!entry) throw new Error(`missing anti-DPI asset: ${name}`);
    writeEntry(entry, path.join(DEST, name));
  }

  const luaEntries = entries.filter((e) => /(^|\/)lua\/[^/]+\.lua$/i.test(e.entryName));
  if (luaEntries.length === 0) throw new Error("missing anti-DPI lua scripts");
  for (const entry of luaEntries) {
    writeEntry(entry, path.join(DEST, "lua", path.basename(entry.entryName)));
  }

  const noticesSource = path.join(REPO_ROOT, "third_party");
  const noticesDest = path.join(DEST, "third_party");
  fs.mkdirSync(path.join(noticesDest, "licenses"), { recursive: true });
  for (const name of fs.readdirSync(noticesSource)) {
    const source = path.join(noticesSource, name);
    if (!fs.statSync(source).isFile()) continue;
    fs.copyFileSync(source, path.join(noticesDest, name));
  }
  for (const name of fs.readdirSync(path.join(noticesSource, "licenses"))) {
    const source = path.join(noticesSource, "licenses", name);
    if (!fs.statSync(source).isFile()) continue;
    fs.copyFileSync(source, path.join(noticesDest, "licenses", name));
  }

  const upstreamLicenses = entries.filter((e) => /(^|\/)(LICENSE|LICENCE|COPYING)(\.[^/]+)?$/i.test(e.entryName));
  for (const entry of upstreamLicenses) {
    const parts = entry.entryName.split("/").filter(Boolean);
    const root = parts.length > 1 ? parts[0] : "upstream";
    const file = parts[parts.length - 1];
    writeEntry(entry, path.join(noticesDest, "licenses", `${safeName(root)}-${safeName(file)}`));
  }

  console.log(`Staged anti-DPI assets (${luaEntries.length} lua scripts)`);
}

main().catch((error) => {
  console.error(`anti-DPI staging failed: ${error.message}`);
  process.exit(1);
});
