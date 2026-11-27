import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it } from "vitest";

// Missing files become empty strings: assertions fail with a clear message
// instead of throwing ENOENT while the release wiring is not in place yet.
// Vitest runs with the repo root as cwd.
function read(relative) {
  const path = join(process.cwd(), relative);
  return existsSync(path) ? readFileSync(path, "utf8") : "";
}

const rootPackage = read("package.json");
const desktopPackage = read("apps/desktop/package.json");
const workflow = read(".github/workflows/release.yml");
const tauriConfig = read("apps/desktop/src-tauri/tauri.conf.json");

function contains(text, needle) {
  assert.ok(text.includes(needle), `expected config to contain ${JSON.stringify(needle)}`);
}

describe("release config wiring", () => {
  it("root installer:build script fetches, verifies and builds an NSIS bundle", () => {
    assert.match(rootPackage, /"installer:build"\s*:/);
    const script = rootPackage.match(/"installer:build"\s*:\s*"([^"]+)"/)?.[1] ?? "";
    contains(script, "binaries:fetch");
    contains(script, "binaries:verify");
    contains(script, "tauri build --bundles nsis");
  });

  it("desktop devDependencies include the Tauri CLI", () => {
    contains(desktopPackage, "@tauri-apps/cli");
  });

  it("release workflow triggers on v* tags with write contents permission on Windows", () => {
    contains(workflow, "tags:");
    contains(workflow, "'v*'");
    contains(workflow, "windows-latest");
    contains(workflow, "contents: write");
  });

  it("release workflow sets up pnpm, Node and the Tauri action", () => {
    contains(workflow, "pnpm/action-setup");
    contains(workflow, "tauri-apps/tauri-action@v1");
    contains(workflow, "projectPath: apps/desktop");
    contains(workflow, "--bundles nsis");
  });

  it("release workflow uploads an NSIS updater manifest and is not a draft", () => {
    contains(workflow, "uploadUpdaterJson: true");
    contains(workflow, "updaterJsonPreferNsis: true");
    contains(workflow, "releaseDraft: false");
  });

  it("release workflow reads both signing secrets from repository secrets", () => {
    contains(workflow, "TAURI_SIGNING_PRIVATE_KEY");
    contains(workflow, "TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    contains(workflow, "secrets.TAURI_SIGNING_PRIVATE_KEY");
    contains(workflow, "secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
  });

  it("pins the updater's xray fetch via the XRAY_VERSION repo variable", () => {
    contains(workflow, "XRAY_VERSION: ${{ vars.XRAY_VERSION }}");
  });

  it("fails early when the release tag does not match the packaged version", () => {
    contains(workflow, "RELEASE_TAG: ${{ github.ref_name }}");
    contains(workflow, "ConvertFrom-Json");
    contains(workflow, "tauri.conf.json");
    contains(workflow, '$expected = "v$version"');
  });

  it("Tauri config enables updater artifacts and points at the releases latest.json", () => {
    contains(tauriConfig, "createUpdaterArtifacts");
    contains(
      tauriConfig,
      "https://github.com/AshenHermit/mglaroute-vpn-client/releases/latest/download/latest.json",
    );
    contains(tauriConfig, '"installMode": "passive"');
    contains(tauriConfig, "PUBLIC_KEY_PLACEHOLDER");
  });
});
