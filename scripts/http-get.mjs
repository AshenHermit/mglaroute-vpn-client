const RELEASES_API = "https://api.github.com/repos/XTLS/Xray-core/releases";
const LATEST_REDIRECT = "https://github.com/XTLS/Xray-core/releases/latest";

export function isRateLimited(error) {
  return /HTTP (403|429)/.test(error?.message ?? "");
}

async function request(url, { timeoutMs = 120000, ...options } = {}) {
  return fetch(url, {
    ...options,
    signal: AbortSignal.timeout(timeoutMs),
    headers: {
      Accept: "application/octet-stream",
      "User-Agent": "mglaroute-client-build",
      ...(options.headers ?? {}),
    },
  });
}

export async function getJson(url, options) {
  const response = await request(url, {
    ...options,
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) throw new Error(`HTTP ${response.status} fetching release metadata`);
  return response.json();
}

export async function getBuffer(url, options) {
  const response = await request(url, options);
  if (!response.ok) throw new Error(`HTTP ${response.status} downloading asset`);
  return Buffer.from(await response.arrayBuffer());
}

export async function latestTagViaRedirect() {
  const response = await fetch(LATEST_REDIRECT, { redirect: "manual" });
  const location = response.headers.get("location");
  if (!location) throw new Error(`Latest release redirect missing (HTTP ${response.status})`);
  const tag = location.split("/").filter(Boolean).pop();
  if (!tag) throw new Error("Latest release redirect has no tag");
  return tag;
}

export async function getLatestRelease() {
  try {
    return await getJson(`${RELEASES_API}/latest`);
  } catch (error) {
    if (!isRateLimited(error)) throw error;
    const tag = await latestTagViaRedirect();
    return getJson(`${RELEASES_API}/tags/${tag}`);
  }
}

export async function downloadPair(downloadBuffer, zipUrl, dgstUrl) {
  const zip = await downloadBuffer(zipUrl);
  const dgst = await downloadBuffer(dgstUrl);
  return [zip, dgst];
}
