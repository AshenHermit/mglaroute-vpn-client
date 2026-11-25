import { describe, expect, it } from "vitest";

import { assetNamesFor, selectAsset } from "./xray-assets.mjs";

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
