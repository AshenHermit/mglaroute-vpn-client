import assert from "node:assert/strict";
import { describe, it } from "vitest";
import { replaceVersion, parseVersion } from "./set-version.mjs";

describe("set-version", () => {
  it("accepts plain semver and rejects malformed versions", () => {
    assert.equal(parseVersion("1.2.3"), "1.2.3");
    assert.equal(parseVersion("0.2.0-beta.1"), "0.2.0-beta.1");
    assert.throws(() => parseVersion("v1.2.3"), /Invalid version/);
    assert.throws(() => parseVersion("1.2"), /Invalid version/);
  });

  it("replaces only the top-level JSON version and preserves formatting", () => {
    const before = [
      "{",
      '  "name": "x",',
      '  "version": "0.1.0",',
      '  "scripts": { "version": "echo 0.1.0" }',
      "}",
      "",
    ].join("\n");
    const after = [
      "{",
      '  "name": "x",',
      '  "version": "0.2.0",',
      '  "scripts": { "version": "echo 0.1.0" }',
      "}",
      "",
    ].join("\n");
    assert.equal(replaceVersion(before, "json", "0.2.0"), after);
  });

  it("replaces the Cargo.toml package version only", () => {
    const before = [
      "[package]",
      'name = "mglaroute-client"',
      'version = "0.1.0"',
      "",
      "[dependencies]",
      'rust-version = "1.90"',
      "",
    ].join("\n");
    const after = [
      "[package]",
      'name = "mglaroute-client"',
      'version = "0.2.0"',
      "",
      "[dependencies]",
      'rust-version = "1.90"',
      "",
    ].join("\n");
    assert.equal(replaceVersion(before, "cargoToml", "0.2.0"), after);
  });

  it("replaces only the mglaroute-client entry in Cargo.lock", () => {
    const before = [
      "[[package]]",
      'name = "ahash"',
      'version = "0.8.12"',
      "",
      "[[package]]",
      'name = "mglaroute-client"',
      'version = "0.1.0"',
      "",
    ].join("\n");
    const after = [
      "[[package]]",
      'name = "ahash"',
      'version = "0.8.12"',
      "",
      "[[package]]",
      'name = "mglaroute-client"',
      'version = "0.2.0"',
      "",
    ].join("\n");
    assert.equal(replaceVersion(before, "cargoLock", "0.2.0"), after);
  });

  it("preserves CRLF line endings", () => {
    const before = '{\r\n  "version": "0.1.0"\r\n}\r\n';
    const after = '{\r\n  "version": "0.2.0"\r\n}\r\n';
    assert.equal(replaceVersion(before, "json", "0.2.0"), after);
  });

  it("throws when the expected version field is missing", () => {
    assert.throws(
      () => replaceVersion('{\n  "name": "x"\n}\n', "json", "0.2.0"),
      /version field not found/,
    );
  });
});
