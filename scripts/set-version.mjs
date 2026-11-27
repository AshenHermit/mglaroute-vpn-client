import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

export const VERSION_TARGETS = [
  { path: "package.json", kind: "json" },
  { path: "apps/desktop/package.json", kind: "json" },
  { path: "apps/desktop/src-tauri/tauri.conf.json", kind: "json" },
  { path: "apps/desktop/src-tauri/Cargo.toml", kind: "cargoToml" },
  { path: "apps/desktop/src-tauri/Cargo.lock", kind: "cargoLock" },
];

export function parseVersion(value) {
  if (!SEMVER.test(value ?? "")) {
    throw new Error(`Invalid version ${JSON.stringify(value)}; expected semver like 0.2.0`);
  }
  return value;
}

function splitLines(text) {
  return {
    eol: text.includes("\r\n") ? "\r\n" : "\n",
    lines: text.split(/\r?\n/),
  };
}

function replaceJson(lines, eol, version) {
  const index = lines.findIndex((line) => /^\s*"version"\s*:/.test(line));
  if (index < 0) throw new Error("version field not found");
  lines[index] = lines[index].replace(
    /("version"\s*:\s*)"[^"]*"/,
    `$1"${version}"`,
  );
  return lines.join(eol);
}

function replaceCargoToml(lines, eol, version) {
  const start = lines.findIndex((line) => line.trim() === "[package]");
  if (start < 0) throw new Error("version field not found");
  for (let i = start + 1; i < lines.length; i += 1) {
    const line = lines[i].trim();
    if (line.startsWith("[")) break;
    if (/^version\s*=/.test(line)) {
      lines[i] = lines[i].replace(
        /(version\s*=\s*)"[^"]*"/,
        `$1"${version}"`,
      );
      return lines.join(eol);
    }
  }
  throw new Error("version field not found");
}

function replaceCargoLock(lines, eol, version) {
  for (let i = 0; i < lines.length; i += 1) {
    if (lines[i].trim() !== 'name = "mglaroute-client"') continue;
    for (let j = i + 1; j < lines.length; j += 1) {
      const line = lines[j].trim();
      if (line === "[[package]]") break;
      if (/^version\s*=/.test(line)) {
        lines[j] = lines[j].replace(
          /(version\s*=\s*)"[^"]*"/,
          `$1"${version}"`,
        );
        return lines.join(eol);
      }
    }
  }
  throw new Error("version field not found");
}

export function replaceVersion(text, kind, version) {
  const parsed = parseVersion(version);
  const { eol, lines } = splitLines(text);
  if (kind === "json") return replaceJson(lines, eol, parsed);
  if (kind === "cargoToml") return replaceCargoToml(lines, eol, parsed);
  if (kind === "cargoLock") return replaceCargoLock(lines, eol, parsed);
  throw new Error(`Unknown version target kind: ${kind}`);
}

export function setVersion(root, version) {
  const parsed = parseVersion(version);
  return VERSION_TARGETS.map((target) => {
    const path = join(root, target.path);
    const before = readFileSync(path, "utf8");
    const after = replaceVersion(before, target.kind, parsed);
    if (after !== before) writeFileSync(path, after, "utf8");
    return { path: target.path, changed: after !== before };
  });
}

const invokedDirectly =
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href;

if (invokedDirectly) {
  const version = process.argv[2];
  if (!version || process.argv.length > 3) {
    console.error("Usage: pnpm version:set <x.y.z>");
    process.exit(1);
  }
  try {
    for (const result of setVersion(process.cwd(), version)) {
      console.log(`${result.changed ? "updated" : "unchanged"} ${result.path} -> ${version}`);
    }
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
