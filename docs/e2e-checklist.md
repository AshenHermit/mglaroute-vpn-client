# End-to-end checklist (manual, Windows desktop)

MVP behaviour is verified in code by unit tests with fakes. The checks below are
the manual, real-machine pass: real bundled xray, real subscription, real Windows
registry. Run them in this order on Windows desktop.

## Prerequisites

- Build machine with a real xray binary: `pnpm binaries:fetch` (or a dev run with
  `XRAY_BINARY_PATH` pointing at a real binary). This checkout has **no** binary —
  `pnpm binaries:verify` fails here with `ENOENT ... xray.exe.sha256`. Do not run
  these checks against this checkout.
- Working subscription URL that returns text with `vmess://`, `vless://`, `ss://`
  or `trojan://` links (`GET /api/feed?format=subscription` also works).
- Two test URLs that return 2xx through the proxy and at least one that does not.
- Record proxy state before step 1: `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`,
  values `ProxyEnable`, `ProxyServer`, `ProxyOverride`, `AutoConfigURL`.

Legend: **binary** = needs the real bundled xray binary; **Windows** = needs a real
Windows registry run; **network** = needs live subscription/test URLs. All steps are manual.

## Steps

1. **Launch with no proxy change → no auto-connect.** (Windows)
   Start the app with the recorded proxy state and no active connection.
   Expected: connection card reads `Disconnected`; no xray process is spawned;
   registry values are byte-for-byte unchanged. Startup restores a previously
   captured proxy but never reconnects automatically.

2. **Import a real subscription URL → profiles appear.** (binary, Windows, network)
   Open **Subscriptions**, paste the URL, click **Add subscription**, then **Refresh**.
   Expected: row status becomes `active`, `last_fetched_at` is set; **Servers**
   lists one row per decoded link. Bad lines are skipped/counted, duplicate links
   appear once. Import accepts the four MVP schemes only.

3. **Add active service profile with 2 URLs → run probe → only passing servers sorted first.** (binary, Windows, network)
   Open **Service profiles**, enter a name, put two reachable 2xx URLs on separate
   lines, click **Add profile**, leave its switch enabled. Open **Servers** and
   click **Run test**.
   Expected: every row gets a delay and a per-profile service result; rows that
   pass **both** URLs appear at the top ordered by ascending delay; rows that
   fail either URL show `—` delay, state `failed`, and sort last. Checks use one
   explicit loopback proxy session — no OS-proxy fallback.

4. **Connect → registry proxy set and traffic flows.** (binary, Windows, network)
   On **Dashboard** click **Connect**.
   Expected: card reads `Connected` with the configured proxy port (default
   `10808`); xray process runs with a temporary config; registry
   `ProxyEnable=1`, `ProxyServer=127.0.0.1:<port>`, `ProxyOverride` contains the
   configured exclusions with `<local>` prepended; browser traffic reaches the
   internet through the proxy. Previous proxy state is written to
   `proxy_previous.json` in app data and is not rewritten while connected.

5. **Kill xray process → state `Faulted`.** (binary, Windows)
   Terminate the spawned xray process externally.
   Expected: within ~500 ms the card reads `Faulted` and shows "Connection
   faulted. Reconnect to continue."; the crash watcher restores the captured
   proxy, removes the temporary config and emits a `connection-state` event.
   No automatic restart is attempted.

6. **Disconnect → previous proxy restored.** (Windows)
   From a `Connected` state (before step 5, or after an explicit reconnect) click
   **Disconnect**.
   Expected: card reads `Disconnected`; registry `ProxyEnable`, `ProxyServer`,
   `ProxyOverride`, `AutoConfigURL` match the values recorded before step 1;
   `proxy_previous.json` is removed after a successful restore. Note: after step 5
   the app is already `Faulted` and the UI hides **Disconnect** — the proxy has
   already been restored by the crash watcher, so verify the registry instead.

7. **Restart app → proxy restored from disk, still disconnected.** (Windows)
   Close and relaunch the app while `proxy_previous.json` is present (e.g. kill
   the app itself while connected, without clicking Disconnect).
   Expected: startup loads `proxy_previous.json`, restores that proxy, removes
   the file after success, and leaves the connection `Disconnected` — no
   auto-connect, no xray process. If the file is absent, registry state is left
   as-is.

8. **Auto-refetch tick updates `last_fetched_at`.** (binary, Windows, network)
   On **Settings** enable **Auto refetch enabled** and save. On **Subscriptions**
   keep the feed **Enabled** and set its **Refetch interval for <name> (minutes)**
   to `1` (blur to save). Wait for two 60 s scheduler ticks.
   Expected: on the first eligible tick the subscription is fetched, its
   `last_fetched_at` advances (visible in `subscriptions` in `client.db` and in
   the `subscription-refreshed` event), profiles are upserted, and a refresh
   before the interval elapses is skipped. New subscriptions default to 15
   minutes; interval `0` disables refetch for that row. The old global
   "Refetch interval (minutes)" field was removed: cadence is per subscription.

9. **Cooldown blocks rapid reconnect.** (binary, Windows, network)
   On **Settings** enable **Auto test enabled** and **Auto reconnect enabled**,
   set **Reconnect cooldown (seconds)** to `60`, and save. Ensure two passing
   servers with different delays; let a test cycle switch to the faster one.
   Trigger another test immediately.
   Expected: active profile stays on the faster server while the cooldown has
   not elapsed; after the cooldown the best eligible server is selected. A
   current server that stops passing all active service profiles is replaced
   regardless of cooldown; with no passing server the connection is dropped.
   The auto-test tick runs every 60 s.

## Not covered by this checklist

TUN, functional anti-DPI, Clash YAML / sing-box / v2ray JSON full-config import,
`format=json` feed import, RAS-entry proxy parity, and Linux/Android/iOS are out
of MVP scope. See `README.md` for the full limitation list.
