import { describe, expect, it } from "vitest";

import { ANDROID_XRAY_ASSETS, assetNamesFor, selectAsset } from "./xray-assets.mjs";

describe("selectAsset", () => {
  it("select_asset_maps_win32_x64", () => {
    const release = {
      assets: [
        { name: "Xray-windows-64.zip", browser_download_url: "https://example.test/windows" },
        { name: "Xray-win7-64.zip", browser_download_url: "https://example.test/win7" },
      ],
    };

    expect(selectAsset(release, "win32", "x64").name).toBe("Xray-win7-64.zip");
    expect(selectAsset(release, "win32", "x64").browser_download_url).toBe("https://example.test/win7");

    expect(assetNamesFor("win32", "x64")).toEqual(["Xray-win7-64.zip", "Xray-windows-64.zip"]);
    expect(selectAsset({ assets: release.assets.slice(0, 1) }, "win32", "x64").name).toBe(
      "Xray-windows-64.zip",
    );
  });
});

describe("ANDROID_XRAY_ASSETS", () => {
  it("android_assets_map_to_tauri_sidecar_triples", () => {
    // Xray v26.11.9 publishes exactly two Android assets; do not invent armv7/x86.
    expect(ANDROID_XRAY_ASSETS).toEqual([
      { asset: "Xray-android-arm64-v8a.zip", triple: "aarch64-linux-android" },
      { asset: "Xray-android-amd64.zip", triple: "x86_64-linux-android" },
    ]);
    for (const { asset, triple } of ANDROID_XRAY_ASSETS) {
      expect(asset).toMatch(/^Xray-android-/);
      expect(triple).toMatch(/^x86_64-linux-android|^aarch64-linux-android$/);
    }
  });
});
