// Xray release asset mapping. Mirrors the server's backend/scripts/xray-assets.mjs.
export const XRAY_VERSION = "v26.11.9";

// Geo assets shipped inside each Xray release zip and bundled next to the core.
export const GEO_ASSETS = ["geoip.dat", "geosite.dat"];

const CANDIDATES = {
  win32: {
    x64: ["Xray-win7-64.zip", "Xray-windows-64.zip"],
    arm64: ["Xray-win7-arm64-v8a.zip", "Xray-windows-arm64-v8a.zip"],
    ia32: ["Xray-win7-32.zip", "Xray-windows-32.zip"],
  },
  linux: {
    x64: ["Xray-linux-64.zip"],
    arm64: ["Xray-linux-arm64-v8a.zip"],
    arm: ["Xray-linux-arm32-v7a.zip"],
    ia32: ["Xray-linux-32.zip"],
  },
  darwin: {
    x64: ["Xray-macos-64.zip"],
    arm64: ["Xray-macos-arm64-v8a.zip"],
  },
};

const TARGET_TRIPLES = {
  win32: {
    x64: "x86_64-pc-windows-msvc",
    arm64: "aarch64-pc-windows-msvc",
    ia32: "i686-pc-windows-msvc",
  },
  linux: {
    x64: "x86_64-unknown-linux-gnu",
    arm64: "aarch64-unknown-linux-gnu",
    arm: "armv7-unknown-linux-gnueabihf",
    ia32: "i686-unknown-linux-gnu",
  },
  darwin: {
    x64: "x86_64-apple-darwin",
    arm64: "aarch64-apple-darwin",
  },
};

export function assetNamesFor(platform, arch) {
  const names = CANDIDATES[platform]?.[arch];
  if (!names) throw new Error(`Unsupported platform/arch: ${platform}/${arch}`);
  return [...names];
}

export function targetTriple(platform, arch) {
  const triple = TARGET_TRIPLES[platform]?.[arch];
  if (!triple) throw new Error(`Unsupported platform/arch: ${platform}/${arch}`);
  return triple;
}

export function pinnedRelease(platform, arch, tag = XRAY_VERSION) {
  const base = `https://github.com/XTLS/Xray-core/releases/download/${tag}`;
  return {
    tag_name: tag,
    assets: assetNamesFor(platform, arch).map((name) => ({
      name,
      browser_download_url: `${base}/${name}`,
    })),
  };
}

export function selectAsset(release, platform, arch) {
  const names = assetNamesFor(platform, arch);
  for (const name of names) {
    const asset = release?.assets?.find((a) => a.name === name);
    if (asset) return asset;
  }
  throw new Error(`No xray asset found for ${platform}/${arch}`);
}
