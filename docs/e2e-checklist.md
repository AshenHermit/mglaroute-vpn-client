# End-to-end checklist (manual: Windows desktop + Android)

MVP behaviour is verified in code by unit tests with fakes. The checks below are
the manual, real-machine pass: real bundled xray, real subscription, real Windows
registry. Run them in this order on Windows desktop.

## Prerequisites

- Build machine with a real xray binary: `pnpm binaries:fetch` (or a dev run with
  `XRAY_BINARY_PATH` pointing at a real binary). This checkout has **no** binary —
  `pnpm binaries:verify` fails here with `ENOENT ... xray.exe.sha256`. Do not run
  these checks against this checkout. Routing checks (step 18) also need the
  bundled `geoip.dat` and `geosite.dat` assets staged next to xray under
  `binaries/`; a missing asset fails the build, it is not silently skipped.
- Working subscription URL that returns text with `vmess://`, `vless://`, `ss://`
  or `trojan://` links (`GET /api/feed?format=subscription` also works).
- Two test URLs that return 2xx through the proxy and at least one that does not.
- Record proxy state before step 1: `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`,
  values `ProxyEnable`, `ProxyServer`, `ProxyOverride`, `AutoConfigURL`.
- For steps 16–17: a published GitHub release newer than the running version with
  a signed NSIS setup, `.sig` and `latest.json`, and `plugins.updater.pubkey`
  matching the signing key. The release-page fallback needs only the releases URL.

Legend: **binary** = needs the real bundled xray binary (including geo assets);
**Windows** = needs a real Windows registry run; **network** = needs live
subscription/test URLs; **update** = needs a published signed release with a newer
version. All steps are manual.

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

10. **Fresh import → Connect works without a test.** (binary, Windows, network)
    Import a subscription (step 2), run **no** test, then on **Dashboard** click
    **Connect** — first with a server row selected, then again with no selection.
    Expected: connects on the first click with no cached delay/service results;
    the selected row wins when set, otherwise the best cached server, otherwise
    the first enabled server by `sort_index` (never one from a disabled
    subscription). Manual connect does **not** require passing all active service
    profiles — no `NotFound` / `Connection failed`. Working core runs and system
    proxy is set; Connect is enabled while a test is in progress.

11. **Run test while connected → connection and proxy unchanged, rows show `Testing...`.** (binary, Windows, network)
    Stay `Connected` (step 4) and click **Run test** on **Servers**.
    Expected: the connection card stays `Connected`, registry values and the
    working core PID are unchanged throughout, and Connect never returns
    `probe_busy`. Each row shows `Testing...` in State and `…` in Delay while it
    is probed, highlighted, with a visible `n / total` progress counter. Events
    arrive as `probe-server(testing)` → `probe-server(done|failed)` per server and
    `probe-progress` after each; a manual test does **not** trigger auto-reconnect,
    so the active server is unchanged when the cycle finishes.

12. **Second click during a test → Run test disabled.** (UI)
    While the cycle from step 11 is running, observe the **Run test** button.
    Expected: it reads `Testing...` and is `disabled`; a second click cannot start
    another cycle and no `probe_busy` / "Another operation is in progress" message
    appears. **Stop test** is available alongside it.

13. **Stop test → all temporary cores gone, no `%TEMP%` leftovers.** (binary, Windows)
    During a running test click **Stop test**.
    Expected: the cycle stops promptly; `probe-cancelled` fires on acceptance and
    `probe-finished(cancelled: true)` at the end. Every temporary probe xray core
    is gone (only the working connection core, if connected, remains) — check for
    orphaned `xray.exe` in Task Manager. No `%TEMP%\mglaroute-probe-*.json` or
    `%TEMP%\mglaroute-probe-*.pid` files remain. Partial results are kept but the
    global sort is not re-run, so untested rows keep their order. Repeat after a
    full test and after app exit: no orphaned xray or sidecar left behind (Windows
    Job Object kill-on-close + startup reaping of verified PIDs).

14. **Refresh failures show a message per error code.** (UI, network)
    Force refresh failures with distinct causes and read the subscription row
    message each time.
    Expected: `mutation_busy` → "Another operation is in progress. Try again
    shortly."; `fetch` → "Could not reach subscription URL"; `not_found` →
    "Subscription not found"; `storage` → "Local storage error"; any other code →
    "Could not refresh subscription". On connect, `connection_busy` → "Connection
    operation in progress" instead of a generic failure.

15. **Tray lifecycle → window hides, reopens, and Exit cleans up once.** (binary, Windows)
    From a `Connected` state (step 4) click the window's close button, then
    left-click the tray icon, then right-click it and choose **Exit**.
    Expected: the close button hides the window but the process stays alive —
    the tray icon is still present, the connection card state is unchanged, the
    registry proxy is still set, and the working xray core is still running.
    Left-click restores and focuses the `main` window (same connection state).
    Right-click shows a menu with `Open` and `Exit` only. Choosing `Exit`
    restores the recorded proxy values, removes `proxy_previous.json`, and
    leaves no `xray.exe` process or `%TEMP%\mglaroute-probe-*` sidecars; the
    process then ends. Repeat the connect → close → Exit sequence, and try a
    rapid double-click on `Exit`: no second cleanup runs (no error dialog, no
    duplicate proxy restore, no process left behind).

16. **Update notice → Install update updates and restarts cleanly.** (update, Windows)
    With a newer signed release published, launch the app (or wait for the 6 h
    check) and open **Settings → Updates**.
    Expected: a toast announces the new version; the panel shows the available
    version and notes. Click **Install update**: progress advances through
    `downloading` → `installing`, and the app closes before the passive NSIS
    install so the new version can run. Before install, the recorded proxy values
    are restored, `proxy_previous.json` is removed, no `xray.exe` remains and no
    `%TEMP%\mglaroute-probe-*` sidecars are left. A second click while installing
    is a no-op (`update_busy`). The final `done` progress line may never arrive
    because the Windows updater can exit first — do not read that as failure.

17. **No in-app install → release page opens.** (update, UI)
    Run a build where `canInstallInApp` is false (mobile seam; Windows currently
    always true), so **Updates** shows **Open release page**.
    Expected: clicking it opens the GitHub releases page for `v<version>` in the
    system browser via Rust `shell::open`, not the WebView. No download/install
    runs and proxy/core state is unchanged; a failed install leaves the button
    clickable again.

18. **Routing rules apply on a fresh connect, with bundled geo assets.** (binary, Windows, network)
    Open **Settings → Routing**. Add a profile, click **Activate** on it, and add
    rules: one `direct` rule with a `geosite:private` domain and `geoip:private` IP,
    one `block` rule with `reg.ru` and `10.0.0.0/8`. Toggle a rule off and back on,
    use **Move up** / **Move down**, then connect (step 4) and, while connected,
    reconnect via a fresh config.
    Expected: rules appear in profile order, remark/outbound/domain/IP preview match
    the editor, and the active profile's enabled rules reach xray on connect and
    reconnect only — a running core is never rewritten in place. A `direct` rule
    bypasses the proxy and a `block` rule drops traffic. `geosite:` / `geoip:`
    entries resolve from the bundled `geoip.dat` and `geosite.dat` (no silent
    fallback when assets are missing). Rule edits are local to SQLite and survive
    a restart; no drag-and-drop, import/export, port/protocol/network/process
    fields, or DNS rules are present.

## Android (manual, real device — operator only)

Android builds are **not** produced in CI. These steps need a real API 34+
device, the Android SDK/NDK and a signing keystore. The signed AAB is built
independently of `installer:build` (which only produces the Windows NSIS setup).
`check-android` in CI only runs `cargo check` for the four Android targets — no
binaries are downloaded and no APK/AAB is built there.

Prerequisites:

- Android SDK + NDK (r26d or compatible) installed via
  `android-actions/setup-android` / `nttld/setup-ndk` locally, or Android Studio
  SDK Manager; NDK LLVM bin on `PATH` for Android `cargo check`.
- A signing keystore and `keystore.properties` under
  `apps/desktop/src-tauri/gen/android/` (gitignored).
- A real subscription URL, and a way to check exit IP and DNS leaks from both a
  browser and another installed app.

A1. **Stage Android binaries.** (build machine, network)
    Run `pnpm binaries:fetch:android` then `pnpm binaries:verify:android`.
    Expected: official `Xray-android-arm64-v8a.zip` and
    `Xray-android-amd64.zip` assets are sha256-verified and staged as the two
    published Android sidecars (`xray-aarch64-linux-android`,
    `xray-x86_64-linux-android`); geoip.dat/geosite.dat come from the verified
    arm64 zip. The other two Rust targets (`armv7-linux-androideabi`,
    `i686-linux-android`) are compile-only in CI — no release zip exists for
    them. A missing `geoip.dat` / `geosite.dat` fails the build.

A2. **Build a signed AAB.** (build machine, signing)
    Run `pnpm --filter @mglaroute/desktop exec tauri android build --aab --target aarch64 --target x86_64`
    with signing configured. This is independent of `installer:build`: do not
    run the Windows installer script to produce the Android artifact.
    Expected: a signed AAB containing the xray Android sidecars and geo assets,
    no Windows anti-DPI binaries and no updater plugin.

A3. **Install and grant VPN consent.** (device)
    Install the signed AAB (e.g. via `adb install` of a generated APK, or sideload),
    launch the app, import a subscription and tap Connect.
    Expected: first connect shows the Android VPN consent dialog; after grant,
    the persistent VPN/notification icon appears and stays until disconnect.

A4. **Browser + another app exit IP.** (device, network)
    With the tunnel connected, check the exit IP in a browser and in a second,
    unrelated app.
    Expected: both report the selected proxy exit IP, not the device's physical
    IP. The uplink does not loop back into the tunnel.

A5. **DNS leak check.** (device, network)
    Run a DNS-leak test page while connected.
    Expected: DNS resolves through the tunnel; no query escapes via the physical
    link (Android Private DNS behaviour is documented if it interferes).

A6. **Excluded app cannot bind the TUN.** (device)
    Attempt to bind a socket to the TUN interface from an excluded app.
    Expected: blocked by the first Xray rule (`process: ["-1"] → block`); the app
    cannot learn the proxy exit IP by bypassing the tunnel.

A7. **Disconnect clears the session.** (device)
    Tap Disconnect.
    Expected: VPN icon and foreground notification disappear, no xray process
    remains and no stale TUN route/session is left behind.

A8. **Kill xray clears the session.** (device, shell)
    While connected, kill the xray process from a shell.
    Expected: UI shows disconnected and the VPN session is gone — no dead
    tunnel left in the status bar.

A9. **Probe while connected does not disturb the tunnel.** (device, network)
    While connected, re-import/probe servers (Run test).
    Expected: the live VPN session, TUN routes and working xray core are
    unchanged; probe cores stay on loopback `mixed` and never touch the TUN.

These Android steps are the operator acceptance gate for the design at
`docs/superpowers/specs/2026-11-28-android-vpnservice-tun-design.md`; its status
stays `proposed` until a real-device pass confirms all of them.

## Not covered by this checklist

TUN on Windows, functional anti-DPI, Clash YAML / sing-box / v2ray JSON
full-config import, `format=json` feed import, RAS-entry proxy parity, Linux and
iOS are out of MVP scope. See `README.md` for the full limitation list.
