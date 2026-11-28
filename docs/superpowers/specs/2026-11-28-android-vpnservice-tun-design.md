# Android VpnService TUN (Xray-native) — design

Status: proposed  
Date: 2026-11-28

> Acceptance gate: status becomes `accepted` only after an operator has run the
> manual Android device checklist in `docs/e2e-checklist.md` and confirmed the
> five Review Focus behaviours on a real API 34+ device. Passing CI and unit
> tests alone do not change this status. If Xray's TUN inbound proves unstable
> on device, record the failure here and switch to approach B (go-tun2socks AAR)
> as a new brainstorming/design cycle, not as an in-place patch to this plan.

## Intent

The client is a Windows-desktop MVP today. All portable logic (subscription
import/decode, SQLite repositories, probes, busy guards, config snapshot,
reconnect scheduler) already lives in platform-neutral Rust. The only Windows
boundary is `platform::proxy_windows` plus the Job Object/startup reaper, tray
and updater.

Android needs a real system VPN, not a localhost proxy: traffic from arbitrary
apps must go through the selected profile, with a visible VPN session and a
foreground service. The operator has chosen **VpnService TUN with Xray's native
TUN inbound** — no Go tun2socks layer, no second core.

Xray documents this path directly: Android's `VpnService` returns a TUN file
descriptor; setting `XRAY_TUN_FD` (or `xray.tun.fd`) before starting Xray makes
its `tun` inbound read from that fd. Official Xray releases ship
`Xray-android-*.zip` assets. So the same xray core can serve both desktop and
Android.

## Goals

- Build an Android APK/AAB from this repository without breaking Windows.
- Real system-wide capture via Android `VpnService` + Xray `tun` inbound.
- Reuse the existing config snapshot, outbounds, routing rules, probe cores,
  subscription storage, auto-test and auto-reconnect loops unchanged.
- One visible VPN session with a foreground notification, user consent flow,
  and clean stop/crash teardown.
- All Android-specific behaviour behind Rust traits/plugins so Windows code
  does not grow `#[cfg(target_os = "android")]` branches outside the seams.
- Anti-DPI remains explicitly `unsupported` on Android; desktop stays unchanged.

## Non-goals

- No tun2socks/Go AAR, no sing-box core, no root, no iptables/nftables.
- No per-app split-tunnel UI in v1 (the plugin may accept package lists, but
  settings/UI for them are deferred).
- No Android in-app updater. `update_check` / `update_install` keep their
  existing mobile no-op/error behaviour.
- No Play Store upload automation in v1; produce signed AAB/APK artifacts only.
- No iOS work in this pass.
- No change to Windows proxy, tray, Job Object or updater behaviour.

## Anti-goals (failure even if it compiles)

- A "connected" state where Android routes exist but xray never receives TUN
  packets (fd handoff silently failed).
- An uplink loop: Xray's own connection to the remote node goes back into the
  same TUN and stalls. Must be prevented by `autoOutboundsInterface`.
- DNS leaks: system DNS queries escaping via the physical link instead of the
  tunnel.
- Excluded apps binding directly to the TUN (`SO_BINDTODEVICE`) and learning the
  proxy exit IP.
- Orphaned xray children or a stale VPN session after disconnect/crash.
- Android resources bundled with Windows anti-DPI binaries or Windows-only
  sidecar names.

## Constraints

- Stack fixed: Tauri v2, Rust + `rusqlite`/`reqwest`/`tokio`, pnpm workspace +
  Nx, React 18 + TS strict, Zustand 5 (persist only for non-secret UI prefs).
- Existing config generation follows one immutable `ConfigSnapshot`; no second
  source of truth.
- Probe cores must stay `mixed` on loopback and must not disturb the live VPN.
- Asset path contract: `resource_dir()/binaries` holds `geoip.dat`,
  `geosite.dat`; missing geo assets are a build error, not a silent fallback.
- Never log subscription URLs, credentials or raw config.
- First release is a **signed APK/AAB for sideload/manual distribution only**;
  Google Play publishing is deferred. Play Console requirements (organization
  account, VpnService declaration, privacy policy, prominent in-app disclosure)
  stay documented but are not part of this pass.
- Android 14+ (target 34+) still requires the manifest foreground-service type
  and matching permission on-device; Play Console declaration is not included.

## Approaches

### A. Xray-native TUN inbound (chosen)

Kotlin foreground service calls `VpnService.prepare()` and `establish()`, gets
the TUN fd, hands it to Rust, which spawns the existing xray sidecar with
`XRAY_TUN_FD` and a generated `tun` inbound config.

- Pros: one core; reuses subscription decode, outbounds, routing, probes,
  reconnect; officially documented by Xray; no Go toolchain/AAR.
- Cons: TUN inbound is newer than tun2socks; fd inheritance across the
  Kotlin→Rust→xray boundary must be proven on a real device; DNS and
  unknown-owner rules need careful ordering.

### B. go-tun2socks AAR → xray SOCKS (rejected)

Kotlin TUN fd → Go tun2socks AAR → local `mixed` inbound of xray. Battle-tested
in v2rayNG-style clients, but adds a second network stack, a Go/AAR build,
extra licenses and DNS-leak surface. Keep as documented fallback if A proves
unstable.

### C. sing-box as Android core (rejected)

Mature Android TUN inbound, but a second core means config conversion, duplicated
tests and drift from Windows behaviour. Rejected.

## Design

### 1. Tauri mobile shell

- Convert the crate to a mobile-capable library:
  - `[lib] name = "app_lib", crate-type = ["staticlib", "cdylib", "rlib"]`.
  - Move `run()` into `lib.rs` and annotate
    `#[cfg_attr(mobile, tauri::mobile_entry_point)]`.
  - Keep `main.rs` as a thin desktop wrapper calling `app_lib::run()`.
- Run `pnpm tauri android init` once to generate `gen/android/` (gitignored or
  committed per existing convention — decision in implementation plan).
- Add `tauri.android.conf.json` overlay (JSON Merge Patch) that:
  - removes `plugins.updater`,
  - overrides `bundle.resources` to `binaries/geoip.dat`,
    `binaries/geosite.dat` only (no `binaries/antiddpi/**/*`),
  - sets `bundle.android.minSdkVersion` (24 minimum; choose 26+ if needed by
    foreground-service notification channel behaviour),
  - sets an explicit `bundle.android.versionCode` scheme if the derived
    `major*1000000 + minor*1000 + patch` is unsuitable.
- `lib.rs::setup` keeps updater/shell registration under `#[cfg(desktop)]` and
  registers the new VPN plugin only under `#[cfg(mobile)]`. Tray setup likewise
  becomes desktop-only.

### 2. Kotlin/Rust VPN plugin

New local Tauri plugin, e.g. `plugins/mglaroute-vpn` (Rust crate + Kotlin
Android module), with a shared Rust facade used by commands:

```rust
pub trait VpnBackend {
    fn prepare(&self) -> Result<bool>;       // false => consent needed
    fn start(&self, req: VpnStartRequest) -> Result<VpnSession>;
    fn stop(&self, session: &VpnSession) -> Result<()>;
    fn physical_interface(&self) -> Result<String>;
}
```

Mobile side (`run_mobile_plugin`) implements commands:

- `prepare` — calls `VpnService.prepare(context)`; if it returns an intent, the
  plugin launches it and resolves only after the activity result.
- `start` — starts the foreground `VpnService`; Kotlin builds the interface:
  - `addAddress("10.10.0.1/30")` (and an IPv6 address if enabled),
  - `addRoute("0.0.0.0/0")`, `addRoute("::/0")`,
  - `addDnsServer("10.10.0.2")` (virtual DNS endpoint inside the tunnel),
  - `setSession("mglaroute")`, `setBlocking(true)` (Android otherwise makes the
    fd non-blocking; Xray's TUN stack wants a blocking fd),
  - optional `addAllowedApplication` / `addDisallowedApplication` from request
    (no UI in v1),
  - `startForeground(...)` with a persistent notification before `establish()`,
  - returns `{ fd, interfaceName }` to Rust.
- `stop` — closes the TUN fd, calls `stopSelf()`, removes the notification.
- `interface_name` — resolves the active network's interface name through
  `ConnectivityManager`; Xray requires the explicit name on Android for
  `autoOutboundsInterface`. Failure is a distinct connect error, not a silent
  `null`.

Manifest additions in `gen/android/app/src/main/AndroidManifest.xml`:

- `<service android:name=".MglarouteVpnService"
  android:permission="android.permission.BIND_VPN_SERVICE"
  android:exported="false"
  android:foregroundServiceType="specialUse">` with
  `<intent-filter><action android:name="android.net.VpnService"/></intent-filter>`
  and `<meta-data android:name="android.net.VpnService.SUPPORTS_ALWAYS_ON"
  android:value="true"/>`.
- Permissions: `android.permission.BIND_VPN_SERVICE`,
  `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`, `POST_NOTIFICATIONS`
  (runtime request on Android 13+).
- Manifest foreground-service type/permissions are required for on-device
  execution; Play Console declaration is deferred with Play publishing.

### 3. TUN fd handoff to Xray

- Kotlin calls `ParcelFileDescriptor.detachFd()` so ownership transfers out of
  the JVM; it returns the integer fd to Rust through the plugin response.
- Rust wraps it in `OwnedFd`, clears `FD_CLOEXEC`, and extends
  `core::supervisor::build_command` with an optional env map / fd injection:
  - `XRAY_TUN_FD=<fd>` for the child,
  - existing `XRAY_LOCATION_ASSET` from `ConfigSnapshot::asset_dir`,
  - stdout/stderr still `null` (raw config must never reach logs).
- Xray duplicates the fd internally and closes only its copy; Rust closes its
  `OwnedFd` after spawn. This duplication behaviour is a **spike item** to
  confirm on a device before implementation is trusted (the Xray README states
  it explicitly for iOS; Android shares the same fd code path).

### 4. Config generation on Android

`core::config::build_connection_config` gets a platform argument (or a `tun`
option) and produces a platform-specific inbound set from the same snapshot:

- Desktop: unchanged `[mixed_inbound("proxy-in", port)]`.
- Android: `[{"protocol":"tun","tag":"tun-in","settings":{
  "name":"mglaroute-tun","mtu":1500,
  "autoOutboundsInterface":"<physical iface from plugin>"}}]`.
  No `gateway`, no `autoSystemRoutingTable` — routes/addresses/DNS are owned by
  `VpnService`.
- Outbounds and user routing rules are identical. The catch-all rule's
  `inboundTag` becomes `["tun-in"]`.
- Mandatory Android-only rules, ahead of user rules:
  1. `process: ["-1"] → block` — stops excluded apps that bind to the TUN and
     would otherwise learn the proxy exit IP. If system DNS breaks, refine with
     an explicit allow-list of system UIDs discovered on-device; never move this
     rule after generic routing rules.
  2. `inboundTag: ["tun-in"], port: 53 → dns-out` — DNS arriving through the
     tunnel.
- Add a `dns` section with `tag: "dns-out"` and a `dns-out` outbound. Upstreams
  come from the active network's `LinkProperties.getDnsServers()` as IP literals
  (no hostname-based DoH bootstrap that could loop through the tunnel). Virtual
  DNS address `10.10.0.2` is configured on the VpnService side; the
  `inboundTag: ["tun-in"], port: 53 → dns-out` rule makes Xray answer queries
  sent there instead of leaking them to the physical link.
- `build_probe_config` stays untouched: probe cores remain `mixed` on loopback
  and never touch the TUN, VPN permission or live session.

### 5. Connect / disconnect lifecycle

- `connection_connect` on Android:
  1. `prepare`; if consent needed, surface distinct `vpn_permission_required`
     busy/error and let the UI retry after the system dialog.
  2. `interface_name` (fail with `vpn_interface_unavailable`).
  3. `start` → obtain fd.
  4. Build config, spawn xray with `XRAY_TUN_FD`.
  5. Readiness = plugin `tunEstablished` event + child alive. **No**
     `wait_for_port` on Android.
- `connection_disconnect`: kill xray child, close fd, `stop`, clear notification;
  no proxy restore path (VpnService owns routes). Shutdown order is fixed:
  Rust kills the child, drops its `OwnedFd`, then calls `plugin.stop()`; Kotlin
  never closes an fd already detached to Rust. This avoids a stale VPN session
  with a dead core and a double-close race during `ChildGuard::drop`.
- `child_exit_watch` already detects core death; on Android it must also stop
  the VPN session so the status bar does not show a dead tunnel.
- Process-startup orphan reaping stays Windows-only. Android crash recovery is
  via `VpnService.onDestroy`/service restart plus Rust child watch; if Android
  kills the process the system tears down the TUN automatically.

### 6. Binary and asset pipeline

- Extend `scripts/fetch-xray.mjs` with an Android matrix using the two official
  published assets: `Xray-android-arm64-v8a.zip` and `Xray-android-amd64.zip`
  (plus `.dgst` verification), while keeping the Windows host asset. No
  armv7/x86 Android release zips exist — do not invent them.
- Stage exactly two Android sidecars, named by Tauri triple:
  `xray-aarch64-linux-android` and `xray-x86_64-linux-android`; Windows keeps
  `xray-x86_64-pc-windows-msvc.exe`. `armv7-linux-androideabi` and
  `i686-linux-android` are Rust compile-only targets in CI (`check-android`),
  not shipped Android binaries.
- On mobile Tauri `resources` are APK assets (`asset://localhost/`), not real
  filesystem paths, and `externalBin` sidecars are not established as mobile
  packaging. The Android overlay therefore bundles both xray binaries as
  `bundle.resources`; the Kotlin plugin copies them (and the geo dat files)
  from APK assets into the app files dir (also idempotently at plugin load, and
  explicitly via a `stage_assets` plugin command before every Android connect)
  and marks xray executable. `core::supervisor::binary_path` resolution must use that injected
  app binaries dir, not only `current_exe().parent()` / `CARGO_MANIFEST_DIR`.
  Desktop keeps the existing env → override → exe-dir → bundled order.
- `geoip.dat` / `geosite.dat` come from the verified arm64 Android zip and are
  staged alongside the sidecars; they continue to be read from
  `resource_dir()/binaries`, and the build fails if missing. Anti-DPI assets are
  excluded from Android by the config overlay.

### 7. UI and CI

- Frontend: hide/disable desktop-only surfaces (`Updates`, anti-DPI actions,
  tray-specific copy) using Tauri's platform API; no new persisted state.
- CI: the `check-android` job in `.github/workflows/ci.yml` uses JDK 17,
  Android SDK/NDK (LLVM bin on PATH for `cc-rs`), Rust targets
  `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`,
  `x86_64-linux-android`, and runs four `cargo check` commands with the
  test-only `TAURI_CONFIG` override. No binary downloads and no
  `tauri android build` in CI — APK/AAB builds and signing stay operator-only,
  so missing release assets cannot silently pass CI.
- Documentation: root `AGENTS.md` and `docs/e2e-checklist.md` carry the Android
  status, build commands and manual device checklist. This spec's status moves
  from `proposed` to `accepted` only after the operator runs that checklist and
  confirms the behaviours below on a real API 34+ device.

## Verification (acceptance for implementation)

- Rust unit tests: Android config has exactly one `tun` inbound; no `mixed`
  inbound; `autoOutboundsInterface` present; `process:-1 → block` is first;
  DNS rule present; probe config unchanged; desktop config byte-for-byte
  unchanged.
- Plugin tests with a fake `VpnBackend`: consent required path, start/stop
  ordering, fd closed exactly once, interface-name failure surfaces distinct
  error, child death stops session.
- `cargo test` (desktop) stays green with the test-only `TAURI_CONFIG` override.
- On device/emulator (manual, operator):
  1. First connect prompts VPN consent; after grant, VPN icon appears.
  2. Browser and another app reach the internet through the proxy exit IP.
  3. DNS query does not leak (check via a DNS-leak test page).
  4. Excluded app cannot reach the tunnel by binding to it.
  5. Disconnect removes notification and VPN session; no xray process remains.
  6. Kill xray from a shell → UI shows disconnected and VPN session is gone.
  7. Re-import/probe while connected does not disturb the live tunnel.

## Risks / spikes before coding

- **fd inheritance**: prove `detachFd → OwnedFd → XRAY_TUN_FD` works on a real
  API 34+ device; confirm Xray duplicates rather than stealing the fd.
- **Xray TUN stability**: if TCP/UDP or ICMP behaves badly on device, fall back
  to approach B (go-tun2socks AAR) and record why in this doc.
- **DNS**: Android Private DNS by hostname may break IP-only upstream rules;
  test and document.
- **FGS types**: confirm `specialUse` manifest declaration is accepted on a
  real target 34+ device. Play Console declaration is deferred with publishing.
- **Unknown-owner rule**: verify `process:-1 → block` ordering with real UIDs so
  system DNS/captive portal are not broken.

## Builder calls (settled during review)

These were open in review but are sensible defaults, not operator decisions:

- **One crate, not two.** The existing `apps/desktop/src-tauri` crate becomes
  the shared library (`[lib] app_lib`); `main.rs` stays a thin desktop wrapper.
  No separate `apps/android` Cargo crate. Windows CI and the test-only
  `TAURI_CONFIG` override must stay green.
- **Uplink loop protection.** `autoOutboundsInterface` is primary. Additionally
  call `addDisallowedApplication(context.packageName)` so the app's own sockets
  never re-enter its TUN. If a Wi-Fi↔mobile handoff changes the physical
  interface, the next connect/reconnect rewrites the config from a fresh
  snapshot; a running core is never mutated.
- **Unknown-owner rule stays first.** `process: ["-1"] → block` precedes DNS
  and user rules. Only an on-device failure with real system UIDs justifies an
  explicit system-UID allow-list; never reorder it around generic rules.
- **Mobile command surface.** Keep the shared `invoke_handler` list. Desktop-only
  plugins are gated at registration; mobile already returns no-op/error from
  `update`/`anti_dpi_*` commands. `POST_NOTIFICATIONS` runtime request lives in
  the Kotlin plugin, not in React UI.
- **Consent result.** The plugin registers an activity-result callback before
  calling `VpnService.prepare()`. If the process dies across the consent dialog,
  connect ends as `vpn_permission_required` and the user retries; no stale
  "preparing" state is persisted.
- **CI boundary.** PR CI runs `cargo check` for the four Android targets with
  the test-only config override and no downloaded binaries. `tauri android
  build --apk` and binary staging are operator/manual steps, so missing release
  assets cannot silently pass CI. A real build still fails on missing geo
  assets.
- **Probe temp files.** Android probes write temporary configs/sidecars into
  the app cache dir; xray still runs from `resource_dir()/binaries`. The probe
  config itself stays `mixed`/loopback and unchanged.

## Left to the builder

- Exact Kotlin package/class names and plugin directory layout.
- Whether `gen/android/` is committed or generated by CI.
- Exact virtual address/DNS values (`10.10.0.1/30` / `.2` are placeholders).
- Min SDK level and `versionCode` scheme.
- Whether to bundle only `aarch64` first to cut build time, then add other ABIs.
