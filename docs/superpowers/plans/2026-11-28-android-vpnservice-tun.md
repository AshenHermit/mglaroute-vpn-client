# Android VpnService TUN Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Android system-wide VPN capture to the existing Tauri client via Android `VpnService` and Xray's native `tun` inbound, without changing Windows behaviour.

**Architecture:** Keep one Rust crate and one shared `invoke_handler`. A Kotlin foreground `VpnService` obtains a TUN fd and hands it to Rust; Rust spawns the existing xray sidecar with `XRAY_TUN_FD` and an Android-specific config from the existing immutable `ConfigSnapshot`. Probe cores remain loopback `mixed` and never touch the VPN.

**Tech Stack:** Tauri v2, Rust 1.90, Kotlin/Android SDK 34+, Xray-core v26.11.9, pnpm workspace + Nx, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-28-android-vpnservice-tun-design.md`

## Global Constraints

- Stack fixed: Tauri v2, Rust + `rusqlite`/`reqwest`/`tokio`, pnpm workspace + Nx, React + TS strict, Zustand 5 (persist only for non-secret UI prefs).
- Existing config generation must use one immutable `ConfigSnapshot`; no second source of truth.
- Probe cores must stay `mixed` on loopback and must not disturb the live VPN.
- Asset path contract: `resource_dir()/binaries` holds `geoip.dat`, `geosite.dat`; missing geo assets are a build error, not a silent fallback.
- Never log subscription URLs, credentials or raw config.
- First release is a **signed APK/AAB for sideload/manual distribution only**; Google Play publishing is deferred.
- Android 14+ (target 34+) still requires the manifest foreground-service type and matching permission on-device; Play Console declaration is not included.
- Windows CI and the test-only `TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` override must stay green.
- Desktop config output and Windows proxy/tray/updater behaviour must not change.
- Android binaries in this release are only `Xray-android-arm64-v8a.zip` and `Xray-android-amd64.zip`; build Android APKs for `aarch64` and `x86_64` only. CI still checks the four Rust Android targets from the spec.
- Kotlin source lives in the committed `apps/desktop/src-tauri/gen/android/` project. Tauri-generated schemas stay gitignored.
- The existing `[lib] name = "mglaroute_client_lib"` with `staticlib`/`cdylib`/`rlib` already satisfies the one-crate decision; do not rename it to `app_lib`.

## Review Focus

- **Missing VPN consent**: `prepare` returning false must map to `vpn_permission_required` and never start xray or leave a "preparing" state — test in Task 5.
- **Failing fd handoff**: if the fd is missing, stale or not inherited, connect must fail before reporting `Connected` — test in Task 6.
- **Uplink loop / DNS leak**: Android config must bind Xray's own sockets to the physical interface and must send port-53 TUN traffic to `dns-out`, with no hostname DNS bootstrap — test in Task 2.
- **Unknown-owner bypass**: `process:["-1"] → block` must be the first routing rule, ahead of DNS and user rules — test in Task 2.
- **Stale VPN session**: child death, disconnect and failed connect must stop the Kotlin service, close the fd exactly once and remove the notification — test in Task 6.

---

### Task 1: Mobile Tauri shell and Android config overlay

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/tauri.android.conf.json`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Test: `apps/desktop/src-tauri/src/lib.rs` (`mod tests`)

**Interfaces:**
- Consumes: existing `run()` in `lib.rs`; `lifecycle::setup_tray`; `commands::bootstrap`.
- Produces: `run()` annotated with `#[cfg_attr(mobile, tauri::mobile_entry_point)]`; desktop-only registration block; Android config overlay consumed by all later Android builds.

- [ ] **Step 1: Write the failing config-overlay test**

In `lib.rs` `mod tests`, add `android_config_overlay_removes_desktop_only_bundles`. Read `tauri.android.conf.json`, parse it as JSON, assert: `plugins.updater` is absent; `bundle.resources` equals `["binaries/geoip.dat", "binaries/geosite.dat"]`; `bundle.createUpdaterArtifacts` is `false`; `bundle.android.minSdkVersion` is `26`; `bundle.android.versionCode` is `2003`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml android_config_overlay_removes_desktop_only_bundles -- --exact --test-threads=1`
Expected: FAIL, file not found.

- [ ] **Step 3: Create the Android overlay**

Create `tauri.android.conf.json` as an RFC 7396 merge patch over `tauri.conf.json`: remove `plugins.updater`; set `bundle.createUpdaterArtifacts` to `false`; set `bundle.resources` to the two geo dat files only; set `bundle.android.minSdkVersion` to `26` and `bundle.android.versionCode` to `2003` (Tauri's `0.2.3` derivation). Do not change `identifier` or `productName`.

- [ ] **Step 4: Annotate the mobile entry point and gate desktop-only setup**

In `lib.rs`, add `#[cfg_attr(mobile, tauri::mobile_entry_point)]` to `pub fn run()`. Move `reap_orphans`, updater/shell plugin registration and `lifecycle::setup_tray` behind `#[cfg(desktop)]`. Keep `bootstrap`, event sink, scheduler spawning, `invoke_handler` and `.run()` shared.

- [ ] **Step 5: Run the test to verify it passes**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/tauri.android.conf.json
git commit -m "feat(android): add mobile entry point and config overlay"
```

---

### Task 2: Android config generation from the shared snapshot

**Files:**
- Modify: `apps/desktop/src-tauri/src/core/config.rs`
- Test: `apps/desktop/src-tauri/src/core/config.rs` (`mod tests`)

**Interfaces:**
- Consumes: `ConfigSnapshot`, `RoutingRule`, `XRAY_BYPASS_IPS`, `build_outbound`.
- Produces: `pub enum ConnectionPlatform<'a> { Desktop, Android { interface_name: &'a str, dns_upstreams: &'a [String] } }`; existing `build_connection_config(...)` keeps its signature and delegates to `Desktop`; new `pub fn build_connection_config_for(snapshot, exclusions, rules, asset_dir, platform) -> Result<Value>` used by later tasks.

- [ ] **Step 1: Write failing Android config tests**

Add tests to `config.rs`:
- `android_config_has_exactly_one_tun_inbound`: config inbounds length 1, `protocol == "tun"`, `tag == "tun-in"`, no `listen`/`port`.
- `android_config_sets_interface_name_and_mtu`: `inbounds[0].settings.autoOutboundsInterface == "wlan0"`, `name == "mglaroute-tun"`, `mtu == 1500`, no `gateway`, no `autoSystemRoutingTable`.
- `android_unknown_owner_rule_is_first`: `routing.rules[0] == {"type":"field","process":["-1"],"outboundTag":"block"}`.
- `android_dns_rule_precedes_user_rules`: after the first rule, find `inboundTag == ["tun-in"] && port == 53 && outboundTag == "dns-out"`; its index must be lower than any rule created from a supplied user rule.
- `android_config_has_ip_literal_dns_upstreams`: `dns.tag == "dns-out"`, `dns.servers` contains the two supplied literals and no hostname; `outbounds` contains `{"tag":"dns-out","protocol":"dns","settings":{...}}`.
- `build_probe_config_is_unchanged_on_android`: existing probe config output has one `mixed` inbound and no `tun` inbound.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::config::tests::android -- --test-threads=1`
Expected: FAIL, `ConnectionPlatform` and `build_connection_config_for` not found.

- [ ] **Step 3: Implement the platform parameter**

Refactor the existing builder into a private `build_connection_config_inner(..., platform)`. Keep `build_connection_config` as the Desktop wrapper. For `Android`, replace the mixed inbound with the tun inbound, put the two mandatory rules ahead of exclusions/user rules, change the catch-all `inboundTag` to `["tun-in"]`, and always add a `dns` section plus a `dns-out` outbound. Keep `XRAY_LOCATION_ASSET` insertion unchanged.

- [ ] **Step 4: Add the desktop golden test**

Add `desktop_connection_config_matches_existing_shape`: serialize a fixed `ConfigSnapshot` with no rules and assert the exact JSON string recorded before this change. No `dns` key, one `mixed` inbound, catch-all tag `proxy-in`.

- [ ] **Step 5: Run all config tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::config::tests -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/core/config.rs
git commit -m "feat(android): generate tun config from shared snapshot"
```

---

### Task 3: xray fd injection and Android binary resolution

**Files:**
- Modify: `apps/desktop/src-tauri/src/core/supervisor.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Test: `apps/desktop/src-tauri/src/core/supervisor.rs` (`mod tests`)

**Interfaces:**
- Consumes: `ConfigSnapshot`, `JobHandle`, `ChildGuard`.
- Produces: `pub fn build_command_with(snapshot: &ConfigSnapshot, tun_fd: Option<i32>) -> Result<Command>`; `pub fn spawn_with(snapshot, job: Option<&JobHandle>, tun_fd: Option<i32>) -> Result<ChildGuard>`; `build_command` and `ChildGuard::spawn` remain unchanged for desktop callers; `pub(crate) fn close_tun_fd(fd: i32)` on Android.

- [ ] **Step 1: Write failing fd tests**

Add tests:
- `tun_fd_env_value_is_xray_tun_fd`: pure helper returns key `XRAY_TUN_FD` and decimal fd string for `17`.
- `build_command_without_fd_has_no_tun_env`: helper returning the env map from `build_command_with(..., None)` has no `XRAY_TUN_FD`.
- `build_command_with_fd_sets_tun_env`: helper map contains `XRAY_TUN_FD=17`.
- `spawn_with_failed_job_assignment_closes_fd`: use the existing fake-failure JobHandle pattern if available; otherwise assert the pure close-order helper log is `["kill", "wait", "close"]`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::supervisor::tests::tun -- --test-threads=1`
Expected: FAIL, helpers not found.

- [ ] **Step 3: Implement fd injection**

Add a small `CommandEnv` helper that collects the env overrides without requiring `Command` introspection. `build_command_with` applies it; `build_command` delegates with `None`. `spawn_with` uses `build_command_with`, kills/waits the child on job-assignment failure, then closes the fd. Add `libc = "0.2"` under `[target.'cfg(target_os = "android")'.dependencies]` for `close_tun_fd`; Windows builds see a no-op stub.

- [ ] **Step 4: Extend binary resolution for Android sidecars**

Add a pure `binary_name_for(resource_dir: Option<&Path>) -> String` helper: on Android prefer `resource_dir.join("binaries").join("xray-aarch64-linux-android")` (or the detected ABI from `std::env::consts::ARCH`), then fall back to the existing env/override/exe-dir path. Keep the Windows resolution order unchanged. Add a test asserting the Android name for `aarch64` is `xray-aarch64-linux-android`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml core::supervisor::tests -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/core/supervisor.rs apps/desktop/src-tauri/Cargo.toml
git commit -m "feat(android): pass TUN fd to xray and resolve Android sidecars"
```

---

### Task 4: VPN backend seam and generic errors

**Files:**
- Create: `apps/desktop/src-tauri/src/platform/vpn/mod.rs`
- Modify: `apps/desktop/src-tauri/src/platform/mod.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Test: `apps/desktop/src-tauri/src/platform/vpn/mod.rs` (`mod tests`)

**Interfaces:**
- Consumes: Tauri `AppHandle` / `PluginHandle` only in the concrete mobile implementation (Task 5).
- Produces: `pub trait VpnBackend: Send + Sync { fn prepare(&self) -> Result<bool, VpnError>; fn start(&self, req: VpnStartRequest) -> Result<VpnSession, VpnError>; fn stop(&self) -> Result<(), VpnError>; fn physical_interface(&self) -> Result<String, VpnError>; fn wait_established(&self) -> bool; }`; `VpnStartRequest { address: String, dns: String, routes: Vec<String>, disallowed_packages: Vec<String> }`; `VpnSession { fd: i32, interface_name: String }`; `VpnError::{PermissionRequired, InterfaceUnavailable, StartFailed, StopFailed}`; `NoopVpnBackend` for desktop/tests; fake backend for command tests.

- [ ] **Step 1: Write failing backend tests**

Add `fake_backend_records_start_stop_order` and `noop_backend_never_grants_permission`. The fake records calls into a shared `Arc<Mutex<Vec<&'static str>>>`, returns `Ok(true)` from `prepare`, a fixed `VpnSession { fd: 42, interface_name: "wlan0" }` from `start`, and `true` from `wait_established`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml platform::vpn -- --test-threads=1`
Expected: FAIL, module missing.

- [ ] **Step 3: Implement the trait and fakes**

Create `platform/vpn/mod.rs` with exact types above. Export it from `platform/mod.rs`. Add `CommandError::{VpnPermissionRequired, VpnInterfaceUnavailable}` with snake_case serialization and generic Display text (`vpn permission required`, `vpn interface unavailable`). Do not expose raw backend errors to commands.

- [ ] **Step 4: Run tests to verify they pass**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/platform apps/desktop/src-tauri/src/commands/mod.rs
git commit -m "feat(android): add VPN backend seam"
```

---

### Task 5: Kotlin VpnService plugin and manifest

**Files:**
- Create: `apps/desktop/src-tauri/gen/android/app/src/main/java/com/mglaroute/client/MglarouteVpnPlugin.kt`
- Create: `apps/desktop/src-tauri/gen/android/app/src/main/java/com/mglaroute/client/MglarouteVpnService.kt`
- Modify: `apps/desktop/src-tauri/gen/android/app/src/main/AndroidManifest.xml`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/src/platform/vpn/mobile.rs`
- Test: `apps/desktop/src-tauri/src/platform/vpn/mobile.rs` (`mod tests`)

**Interfaces:**
- Consumes: `VpnBackend`, `VpnStartRequest`, `VpnSession`, `VpnError` from Task 4.
- Produces: `MglarouteVpnBackend` implementing `VpnBackend` on mobile; Tauri plugin name `mglaroute-vpn`; Kotlin commands `prepare`, `start`, `stop`, `interface_name`, `wait_established`; event `tun-established`.

- [ ] **Step 1: Write failing static contract tests**

In `mobile.rs`, add `android_manifest_declares_vpn_service`: read `gen/android/app/src/main/AndroidManifest.xml` and assert it contains `android.permission.BIND_VPN_SERVICE`, `android.permission.FOREGROUND_SERVICE`, `android.permission.FOREGROUND_SERVICE_SPECIAL_USE`, `android.permission.POST_NOTIFICATIONS`, `android.net.VpnService`, `foregroundServiceType="specialUse"`, and `SUPPORTS_ALWAYS_ON`. Add `kotlin_service_detaches_fd_and_excludes_self`: read both Kotlin files and assert they contain `detachFd()`, `startForeground`, `establish()`, `addAddress("10.10.0.1/30")`, `addDnsServer("10.10.0.2")`, `setBlocking(true)`, `addDisallowedApplication`, and that `registerForActivityResult` appears before `VpnService.prepare` in the plugin file.

- [ ] **Step 2: Run tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml platform::vpn::mobile -- --test-threads=1`
Expected: FAIL, files missing.

- [ ] **Step 3: Generate the Android project**

Run `pnpm --filter @mglaroute/desktop exec tauri android init` in the worktree. Commit `gen/android/`. Leave `gen/schemas/` ignored. Do not run `tauri android build` yet.

- [ ] **Step 4: Implement `MglarouteVpnService`**

Extend `VpnService`. Build the interface from `VpnStartRequest`, call `startForeground` before `establish()`, detach the fd with `ParcelFileDescriptor.detachFd()`, return `{fd, interfaceName}` from `start`, and clear the session in `onDestroy`.

- [ ] **Step 5: Implement `MglarouteVpnPlugin`**

Extend Tauri's `Plugin`. Register the activity-result callback in `load()` before any `prepare()` call, request `POST_NOTIFICATIONS` on Android 13+, and implement the five commands. `interface_name` uses `ConnectivityManager.getLinkProperties(activeNetwork)` and returns an error when null.

- [ ] **Step 6: Implement Rust mobile backend and register plugin**

Create `MglarouteVpnBackend` using `PluginHandle::run_mobile_plugin`. Map `PermissionRequired` to `CommandError::VpnPermissionRequired` and `InterfaceUnavailable` to `CommandError::VpnInterfaceUnavailable`. In `run()`, register the plugin under `#[cfg(mobile)]` before `setup` uses it. Keep desktop registration unchanged.

- [ ] **Step 7: Run tests to verify they pass**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/desktop/src-tauri/gen/android apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/platform/vpn
git commit -m "feat(android): add Kotlin VpnService plugin"
```

---

### Task 6: Android connect/disconnect lifecycle

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/commands/connection.rs`
- Test: `apps/desktop/src-tauri/src/commands/connection.rs` (`mod tests`)

**Interfaces:**
- Consumes: `VpnBackend`, `VpnSession`, `ConnectionPlatform`, `build_command_with`, `close_tun_fd`.
- Produces: `AppState::new_with_vpn(...)`; existing `AppState::new` delegates with `NoopVpnBackend`; `connect_impl` uses `ConnectionPlatform::Android` on mobile; `RuntimeState` owns `tun_fd: Option<i32>` and `vpn_active: bool`.

- [ ] **Step 1: Write failing lifecycle tests**

Add tests using the fake backend and fake launcher:
- `android_connect_requires_permission_before_spawning_core`: fake `prepare` returns `false`; assert error is `VpnPermissionRequired`, no launch call, no fd.
- `android_connect_sets_tun_env_and_reaches_connected`: fake backend returns fd `42`; assert launch received `Some(42)`, `wait_established` was called, and state is `Connected`.
- `android_connect_fails_when_interface_name_unavailable`: fake returns `InterfaceUnavailable`; assert distinct error and no launch.
- `android_disconnect_stops_core_then_fd_then_vpn_service`: fake records order in `["core-stop", "fd-close", "vpn-stop"]`.
- `android_child_death_stops_vpn_session`: simulate exited child; assert `vpn-stop` recorded and state `Faulted`.
- `android_connect_failure_closes_fd_exactly_once`: force launcher failure; assert fd-close count is 1 and no VPN session remains.

- [ ] **Step 2: Run tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands::connection::tests::android -- --test-threads=1`
Expected: FAIL, constructors/fields missing.

- [ ] **Step 3: Add VPN backend to state**

Add `vpn: Box<dyn VpnBackend>` to `AppStateInner` and `tun_fd`/`vpn_active` to `RuntimeState`. Add `new_with_vpn`; keep `new` and `new_with_job` signatures unchanged by delegating to `NoopVpnBackend`. In `bootstrap`, install `MglarouteVpnBackend` under `#[cfg(mobile)]` and `NoopVpnBackend` otherwise.

- [ ] **Step 4: Branch connect_impl**

Keep the desktop path byte-for-byte unchanged. On Android: `prepare` first; then `physical_interface`; then `start` with the fixed request (`10.10.0.1/30`, DNS `10.10.0.2`, routes `0.0.0.0/0` and `::/0`, `addDisallowedApplication(self)`); build config with `ConnectionPlatform::Android`; spawn with `tun_fd`; wait for `tun-established` + child alive instead of `wait_for_port`; on any failure stop child, close fd once, stop VPN service, remove config, mark disconnected/faulted per the existing error policy.

- [ ] **Step 5: Branch disconnect and child-death handling**

On Android, skip proxy restore. Use fixed shutdown order: stop child → close fd → `vpn.stop()`. In `handle_exited_child`, call `vpn.stop()` and clear `vpn_active`. Do not let `ChildGuard::drop` close the fd a second time.

- [ ] **Step 6: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Expected: PASS, including all existing desktop tests.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/commands
git commit -m "feat(android): wire VPN lifecycle into connect and disconnect"
```

---

### Task 7: Android binary and asset pipeline

**Files:**
- Modify: `scripts/xray-assets.mjs`
- Modify: `scripts/fetch-xray.mjs`
- Modify: `scripts/xray-assets.test.mjs`
- Modify: `scripts/fetch-xray.test.mjs`
- Modify: `package.json`
- Test: `scripts/xray-assets.test.mjs`, `scripts/fetch-xray.test.mjs`

**Interfaces:**
- Consumes: `downloadPair`, `getBuffer`, `getLatestRelease`, `parseSha256`, `verifyBufferSha256`, existing host fetch/verify helpers.
- Produces: `ANDROID_XRAY_ASSETS` mapping; `fetchAndroidBinaries({ release, binDir })`; `verifyAndroidBinaries({ binDir })`; root script `binaries:fetch:android`.

- [ ] **Step 1: Write failing asset-mapping tests**

Add tests: `android_assets_map_to_tauri_sidecar_triples` asserts `Xray-android-arm64-v8a.zip → xray-aarch64-linux-android`, `Xray-android-amd64.zip → xray-x86_64-linux-android`; `android_verify_requires_both_binaries_and_geo` asserts missing file names in the error. Do not invent armv7/x86 assets; this release does not publish them.

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm vitest run scripts/xray-assets.test.mjs scripts/fetch-xray.test.mjs`
Expected: FAIL, exports missing.

- [ ] **Step 3: Implement Android staging**

Add the two asset names to `xray-assets.mjs` with a pinned tag from `XRAY_VERSION`. Extend `fetch-xray.mjs` with `fetchAndroidBinaries`: verify each zip's `.dgst`, extract `xray` by basename, stage as the Tauri sidecar name, `chmod 755`, write per-file sha256 sidecars, and stage geo assets from the arm64 zip only. Extend `verifyBinaries` with an Android mode that skips the Windows anti-DPI check.

- [ ] **Step 4: Add the root script**

Add `"binaries:fetch:android": "node scripts/fetch-xray.mjs --android"` to root `package.json`. Keep `binaries:fetch` and `binaries:verify` unchanged.

- [ ] **Step 5: Run tests to verify they pass**

Run: `pnpm vitest run scripts/xray-assets.test.mjs scripts/fetch-xray.test.mjs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts package.json
git commit -m "feat(android): stage Android xray sidecars"
```

---

### Task 8: Frontend desktop-only surfaces and capabilities

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `packages/contracts/src/schemas.ts`
- Modify: `apps/desktop/src/pages/Settings.tsx`
- Modify: `apps/desktop/src/features/updates/UpdatePanel.tsx`
- Modify: `apps/desktop/src/app-shell/ServerRail.tsx`
- Modify: `apps/desktop/src/pages/settings-page.test.tsx`
- Test: `apps/desktop/src/pages/settings-page.test.tsx`, `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: shared `invoke_handler` from `lib.rs`.
- Produces: command `platform_info` returning `{ mobile: bool }`; frontend gate used by `Settings.tsx` and `UpdatePanel`.

- [ ] **Step 1: Write failing frontend tests**

In `settings-page.test.tsx`, mock `platform_info` as `{ mobile: true }` and assert the `Updates` tab is absent and no anti-DPI action button is rendered. Keep the existing desktop mock (`{ mobile: false }`) asserting both remain visible. In `lib.rs` tests, assert `COMMAND_NAMES` contains `platform_info`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm --filter @mglaroute/desktop test -- settings-page.test.tsx` and the Rust `command_names` test from Task 6's full command.
Expected: FAIL, command unknown and UI ungated.

- [ ] **Step 3: Add platform_info command**

Add a thin command in `commands/mod.rs` returning `PlatformInfoPayload { mobile: bool }` from `cfg!(mobile)`. Add it to `command_list!` in `lib.rs` and add the Zod schema to `packages/contracts/src/schemas.ts`.

- [ ] **Step 4: Gate UI**

Call `platform_info` once at app-shell mount and pass `mobile` down. Hide the `Updates` tab and `UpdatePanel`; keep the anti-DPI status text but hide its action buttons on mobile. No new persisted state.

- [ ] **Step 5: Run tests to verify they pass**

Run: `pnpm test` and the full Rust command from Task 6 Step 6.
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src packages/contracts/src
git commit -m "feat(android): hide desktop-only UI surfaces"
```

---

### Task 9: CI and operator acceptance documentation

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `scripts/ci-config.test.mjs`
- Modify: `docs/e2e-checklist.md`
- Modify: `AGENTS.md`
- Modify: `docs/superpowers/specs/2026-11-28-android-vpnservice-tun-design.md`
- Test: `scripts/ci-config.test.mjs`

**Interfaces:**
- Consumes: existing CI jobs `test-js` and `test-rust`.
- Produces: new `check-android` job; operator manual checklist; spec status `accepted` after real-device verification.

- [ ] **Step 1: Write failing CI test**

In `ci-config.test.mjs`, assert `.github/workflows/ci.yml` contains a job named `check-android`, JDK 17 setup, the four Rust Android targets, and `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --target aarch64-linux-android`. Assert it does **not** contain `tauri android build`.

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm vitest run scripts/ci-config.test.mjs`
Expected: FAIL, job missing.

- [ ] **Step 3: Add the CI job**

Add `check-android` on `ubuntu-latest` with JDK 17, Android SDK/NDK setup, Rust targets `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`, `x86_64-linux-android`, pnpm install and four `cargo check` commands using the test-only `TAURI_CONFIG` override. No binary downloads and no APK build in CI.

- [ ] **Step 4: Document operator steps**

In `docs/e2e-checklist.md`, add manual Android steps: `pnpm binaries:fetch:android`, `pnpm installer:build`-independent signed `pnpm --filter @mglaroute/desktop exec tauri android build --aab --target aarch64 --target x86_64`, install, consent, VPN icon, browser + another app exit IP, DNS leak, excluded app cannot bind TUN, disconnect clears session, kill xray clears session, probe while connected does not disturb tunnel. In `AGENTS.md`, record Android status and commands.

- [ ] **Step 5: Run test to verify it passes**

Run: `pnpm vitest run scripts/ci-config.test.mjs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml scripts/ci-config.test.mjs docs/e2e-checklist.md AGENTS.md docs/superpowers/specs/2026-11-28-android-vpnservice-tun-design.md
git commit -m "feat(android): add CI checks and operator checklist"
```

---

## Operator acceptance gate

Tasks 1–9 produce a buildable, testable tree. Do **not** change the spec status to `accepted` until an operator has run the manual device checklist and confirmed the five Review Focus behaviours on a real API 34+ device. If Xray's TUN inbound proves unstable there, record the failure in the spec and switch to design approach B (go-tun2socks AAR) as a new brainstorming/design cycle, not as an in-place patch to this plan.
