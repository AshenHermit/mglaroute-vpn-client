import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it } from "vitest";

// Vitest runs with the repo root as cwd. Missing file becomes an empty string
// so assertions fail with a clear message instead of ENOENT.
const workflowPath = join(process.cwd(), ".github/workflows/ci.yml");
const workflow = existsSync(workflowPath) ? readFileSync(workflowPath, "utf8") : "";

describe("CI workflow hardening", () => {
  it("grants only read access to repository contents", () => {
    assert.match(workflow, /^permissions:\s*$/m);
    assert.match(workflow, /^\s{2}contents:\s*read\s*$/m);
    assert.doesNotMatch(workflow, /contents:\s*write/);
  });

  it("bounds every job with an explicit timeout", () => {
    assert.match(workflow, /^\s{4}timeout-minutes:\s*15\s*$/m);
    assert.match(workflow, /^\s{4}timeout-minutes:\s*25\s*$/m);
    assert.equal(workflow.match(/timeout-minutes:/g)?.length ?? 0, 3);
  });
});

describe("Android CI compile gate", () => {
  it("defines a check-android job on ubuntu with JDK 17", () => {
    assert.match(workflow, /^\s{2}check-android:\s*$/m);
    assert.match(workflow, /runs-on:\s*ubuntu-latest/);
    assert.match(workflow, /uses:\s*actions\/setup-java@v4/);
    assert.match(workflow, /java-version:\s*["']?17["']?/);
  });

  it("sets up the Android SDK and NDK with the LLVM bin on PATH", () => {
    assert.match(workflow, /uses:\s*android-actions\/setup-android@v3/);
    assert.match(workflow, /uses:\s*nttld\/setup-ndk@v1/);
    assert.match(workflow, /ndk-version:/);
    assert.match(workflow, /toolchains\/llvm\/prebuilt\/linux-x86_64\/bin/);
    assert.match(workflow, /GITHUB_PATH/);
  });

  it("adds all four Android Rust targets", () => {
    for (const target of [
      "aarch64-linux-android",
      "armv7-linux-androideabi",
      "i686-linux-android",
      "x86_64-linux-android",
    ]) {
      assert.match(workflow, new RegExp(target));
    }
  });

  it("runs cargo check for every Android target with the test-only config override", () => {
    assert.match(workflow, /TAURI_CONFIG:\s*["']\{"bundle":\{"externalBin":\[\],"resources":\[\]\}\}["']/);
    for (const target of [
      "aarch64-linux-android",
      "armv7-linux-androideabi",
      "i686-linux-android",
      "x86_64-linux-android",
    ]) {
      assert.match(
        workflow,
        new RegExp(
          `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --target ${target}`,
        ),
      );
    }
  });

  it("installs dependencies with pnpm and does not build APKs or fetch binaries in CI", () => {
    assert.match(workflow, /pnpm install --frozen-lockfile/);
    assert.doesNotMatch(workflow, /tauri android build/);
    assert.doesNotMatch(workflow, /binaries:fetch/);
  });
});
