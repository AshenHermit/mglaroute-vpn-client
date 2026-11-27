# Release Pipeline + In-App Updater Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship signed NSIS releases from GitHub Actions and let users check, see and install updates from inside the client, with a contract that stays valid for future Linux/Android.

**Architecture:** Rust owns update logic behind one command surface (`update_check`, `update_install`, `update_open_releases`); desktop uses the official Tauri updater plugin with signed artifacts, while mobile later reuses the same payload and opens the release page instead of self-installing. Frontend is platform-blind: session-only Zustand state, one Zod contract, a settings tab and a toast.

**Tech Stack:** pnpm 9 + Nx, Node 20+, React 19, Zustand 5, Zod 4, Tauri v2 Rust (MSRV 1.90), `tauri-plugin-updater`, `tauri-plugin-shell`, `tauri-apps/tauri-action@v1`, GitHub Actions, NSIS.

**Spec:** `docs/superpowers/specs/2026-11-26-release-updater-design.md`

## Global Constraints

- Node `>=20`, `pnpm@9`, pnpm workspace + Nx.
- Rust `rust-version = "1.90"` (current `1.77` must be bumped).
- Tauri v2; `productName: "mglaroute"`, `version: "0.1.0"`, identifier `com.mglaroute.client`.
- NSIS installer only in this pass; release tags `v*`; `bundle.createUpdaterArtifacts: true`; `updaterJsonPreferNsis: true`.
- Updater endpoint: `https://github.com/AshenHermit/mglaroute-vpn-client/releases/latest/download/latest.json`.
- Signing secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; public key inline in config, placeholder until operator generates it; never commit a private key.
- Windows install mode `passive`; installed update never leaves system proxy or xray behind.
- All Tauri payloads camelCase; timestamps ISO strings; no native error text forwarded to UI; new error codes must enter `SAFE_CODES`.
- Update UI state is session-only; no SQLite change, no `user_version` migration.
- Check cadence: on mount + every `6 * 60 * 60 * 1000` ms; check failures are silent and retried.
- Verification commands (PowerShell): `$env:NX_DAEMON='false'; pnpm test`, `$env:NX_DAEMON='false'; pnpm build`, and `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`.
- Continue caveman-style terse docs; update `AGENTS.md` and spec when behaviour lands.

## Review Focus

- Install while connected/probing → must disconnect, restore proxy, stop core before download; pinned by Task 4 `install_source_cleans_up_before_download_and_install` and Task 3 `shutdown_source_cleans_up_before_exit`.
- Update check fails while VPN proxy is active → must yield no update, no toast, retry later; pinned by Task 5 `check_failure_keeps_ui_quiet_and_retries`.
- Double-click Install → second click must return `update_busy`, not start a second install; pinned by Task 4 `install_source_uses_shutdown_gate` and Task 6 `install_button_disabled_while_installing`.
- `canInstallInApp: false` (Android/fallback) → must open the GitHub release page, never try self-install; invalid URLs rejected; pinned by Task 4 `open_source_validates_release_url_prefix` and Task 6 `fallback_button_calls_open_command`.
- Missing/invalid signing key → release job must fail loudly rather than publish unsigned artifacts; pinned by Task 7 `workflow_requires_signing_secret_and_updater_json`.

---

### Task 1: Shared update contracts + workspace baseline

**Files:**
- Modify: `packages/contracts/src/schemas.ts`
- Modify: `packages/contracts/src/schemas.test.ts`

**Interfaces:**
- Consumes: existing `EVENTS` map and schema conventions in `schemas.ts`.
- Produces: `UpdateInfoSchema`, `UpdateInfo`, `UpdateProgressEventSchema`, `UpdateProgressEvent`, `EVENTS.updateProgress === "update-progress"`. `UpdateInfo` = `{ version, currentVersion, notes: string|null, pubDate: string|null, canInstallInApp: boolean, releaseUrl }`; `UpdateProgressEvent` = `{ phase: "downloading"|"installing"|"done", received: number, total: number|null }`.

- [ ] **Step 1: Install workspace deps and confirm clean baseline**

Run: `pnpm install`, then `$env:NX_DAEMON='false'; pnpm test`.
Expected: both exit 0. (`Cannot switch to pnpm@9` and zod annotation warnings are pre-existing.)

- [ ] **Step 2: Write failing schema tests**

Add to `packages/contracts/src/schemas.test.ts`:

```ts
it("parses update info with camelCase fields", () => {
  expect(UpdateInfoSchema.parse({
    version: "0.2.0",
    currentVersion: "0.1.0",
    notes: "fixes",
    pubDate: "2026-11-26T00:00:00Z",
    canInstallInApp: true,
    releaseUrl: "https://github.com/AshenHermit/mglaroute-vpn-client/releases/tag/v0.2.0",
  })).toMatchObject({ version: "0.2.0", canInstallInApp: true });
});

it("rejects snake_case update payloads and unknown progress phases", () => {
  expect(() => UpdateInfoSchema.parse({ version: "0.2.0", current_version: "0.1.0" })).toThrow();
  expect(() => UpdateProgressEventSchema.parse({ phase: "unknown", received: 1, total: null })).toThrow();
});

it("maps the update progress event name", () => {
  expect(EVENTS.updateProgress).toBe("update-progress");
});
```

- [ ] **Step 3: Run focused tests to verify they fail**

Run: `pnpm --filter @mglaroute/contracts exec vitest run src/schemas.test.ts`
Expected: FAIL — `UpdateInfoSchema` is not exported.

- [ ] **Step 4: Implement the schemas**

In `schemas.ts`, define `UpdateInfoSchema` and `UpdateProgressEventSchema` next to `AntiDpiStatusSchema`, export their inferred types, and add `updateProgress: "update-progress"` to `EVENTS`. Use `z.string()` for timestamps, `z.number().int().nonnegative()` for counts, `z.enum(["downloading","installing","done"])` for phase.

- [ ] **Step 5: Run focused tests, then full suite**

Run: `pnpm --filter @mglaroute/contracts exec vitest run src/schemas.test.ts`, then `$env:NX_DAEMON='false'; pnpm test`.
Expected: both PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/contracts/src/schemas.ts packages/contracts/src/schemas.test.ts docs/superpowers/specs/2026-11-26-release-updater-design.md docs/superpowers/plans/2026-11-26-release-updater.md
git commit -m "feat: add updater payload contracts"
```

---

### Task 2: Rust updater foundation — deps, config, capabilities

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Create: `apps/desktop/src-tauri/tests/update_config.rs`

**Interfaces:**
- Consumes: nothing from Task 1 (Rust payloads are separate structs; names must match the JSON keys from Task 1).
- Produces: MSRV 1.90, registered updater/shell plugins, updater permissions, signed-artifact config.

- [ ] **Step 1: Write failing config tests**

Create `apps/desktop/src-tauri/tests/update_config.rs` with tests that read `tauri.conf.json` and `capabilities/default.json` via `CARGO_MANIFEST_DIR` and assert:
- `bundle.createUpdaterArtifacts == true`;
- `plugins.updater.endpoints` contains `.../releases/latest/download/latest.json`;
- `plugins.updater.pubkey` is a non-empty string;
- `plugins.updater.windows.installMode == "passive"`;
- capability `permissions` contains `"updater:default"` and `"shell:allow-open"`.

- [ ] **Step 2: Run them to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test update_config`
Expected: FAIL on missing config keys.

- [ ] **Step 3: Implement foundation**

Set `rust-version = "1.90"`. Add target-gated deps:
`tauri-plugin-updater = "2"` and `tauri-plugin-shell = "2"` under `[target.'cfg(any(target_os = "macos", windows, target_os = "linux"))'.dependencies]`. In `lib.rs` `setup`, under `#[cfg(desktop)]`, call `app.handle().plugin(tauri_plugin_updater::Builder::new().build())?` and `app.handle().plugin(tauri_plugin_shell::init())?` before `setup_tray`. In `tauri.conf.json` add `bundle.createUpdaterArtifacts`, `plugins.updater.pubkey` (value `"PUBLIC_KEY_PLACEHOLDER"`), the endpoint array and `windows.installMode`. In capabilities add `"updater:default"` and `"shell:allow-open"`; keep `"core:event:default"`.

- [ ] **Step 4: Run config + full Rust suite**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test update_config`, then the same command without `--test update_config` but with `-- --test-threads=1`.
Expected: both PASS; existing `capabilities_allow_frontend_event_listeners` still passes.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/capabilities/default.json apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/tests/update_config.rs
git commit -m "feat: configure signed Tauri updater"
```

---

### Task 3: Shared shutdown cleanup for terminal actions

**Files:**
- Modify: `apps/desktop/src-tauri/src/lifecycle.rs`

**Interfaces:**
- Consumes: `crate::commands::connection::disconnect_impl(&AppState)`.
- Produces: `pub(crate) async fn cleanup(state: &AppState)` used by both tray exit and updater install; `begin_shutdown()` unchanged.

- [ ] **Step 1: Write failing tests**

In `lifecycle.rs` tests:
- change `shutdown_source_cleans_up_before_exit` to assert `shutdown` body contains `cleanup(&state)` before `app.exit(0)` (drop the old `disconnect_impl` literal assertion);
- add `update_cleanup_helper_disconnects_before_terminal_action` asserting the `cleanup` function body contains `disconnect_impl` and `let _ =`.

- [ ] **Step 2: Run to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib lifecycle -- --test-threads=1`
Expected: FAIL — helper not present.

- [ ] **Step 3: Implement helper**

Add `pub(crate) async fn cleanup(state: &AppState) { let _ = crate::commands::connection::disconnect_impl(state).await; }`. Rewrite `shutdown` as `cleanup(&state).await; app.exit(0);`. Keep `begin_shutdown()` and tray wiring unchanged.

- [ ] **Step 4: Run tests**

Run the Step 2 command.
Expected: PASS, including `shutdown_gate_allows_only_one_exit`.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/lifecycle.rs
git commit -m "refactor: share shutdown cleanup with updater"
```

---

### Task 4: Rust update module and command surface

**Files:**
- Create: `apps/desktop/src-tauri/src/update/mod.rs`
- Create: `apps/desktop/src-tauri/src/update/desktop.rs`
- Create: `apps/desktop/src-tauri/src/update/mobile.rs`
- Create: `apps/desktop/src-tauri/src/commands/update.rs`
- Create: `apps/desktop/src-tauri/tests/update_payloads.rs`
- Create: `apps/desktop/src-tauri/tests/update_command_surface.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `lifecycle::cleanup`, `lifecycle::begin_shutdown`, `state::emit`, `CommandError`, `AppState`, `UpdateInfoSchema` JSON shape from Task 1.
- Produces: `update::UpdateInfo`, `update::UpdateProgressEvent`, `update::UpdatePhase`, `update::release_url(&str)`, `update::validate_release_url(&str) -> bool`, `update::EVENT_UPDATE_PROGRESS = "update-progress"`, commands `update_check`, `update_install`, `update_open_releases(url: String)`, error codes `update_busy` / `update`.

- [ ] **Step 1: Write failing payload tests**

Create `tests/update_payloads.rs` with tests:
- `release_url_uses_v_tag`: `release_url("0.2.0")` equals `https://github.com/AshenHermit/mglaroute-vpn-client/releases/tag/v0.2.0`;
- `update_info_serializes_camel_case`: serialize an `UpdateInfo` with all six fields and assert JSON keys `version`, `currentVersion`, `notes`, `pubDate`, `canInstallInApp`, `releaseUrl`;
- `update_progress_serializes_phase_and_counts`: `UpdatePhase::Downloading` serializes to `"downloading"`;
- `validate_release_url_accepts_only_release_tags`: accepts the exact prefix above, rejects `https://evil.example/...` and a raw `file://` URL.

- [ ] **Step 2: Run to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test update_payloads`
Expected: FAIL — unresolved import `mglaroute_client_lib::update`.

- [ ] **Step 3: Implement `src/update/mod.rs` and cfg-gated backends**

Define the three types with `#[serde(rename_all = "camelCase")]` (phase uses `rename_all = "lowercase"`), `release_url`, `validate_release_url` (prefix `https://github.com/AshenHermit/mglaroute-vpn-client/releases/tag/v`), and `EVENT_UPDATE_PROGRESS`. Re-export `check`/`install` from `desktop.rs` under `#[cfg(desktop)]` and from `mobile.rs` otherwise. In `desktop.rs`: `check` uses `AppHandle::updater()?.check().await?` and maps `Some(update)` to `UpdateInfo` using `app.package_info().version.to_string()` and `can_install_in_app: true`; all errors map to `None`, no error text logged. `install` calls `download_and_install` with a callback that emits `UpdateProgressEvent` through `state::emit`; do not force any proxy bypass. In `mobile.rs`: `check` returns `None`, `install` returns `Err(())` (Android UI work is deferred).

- [ ] **Step 4: Run payload tests**

Run the Step 2 command.
Expected: PASS.

- [ ] **Step 5: Write failing command-surface tests**

Create `tests/update_command_surface.rs` reading production source files as text (missing file = empty string so assertions fail cleanly):
- `install_source_cleans_up_before_download_and_install`: in `src/commands/update.rs`, `cleanup` appears before `update::install`;
- `install_source_uses_shutdown_gate`: `begin_shutdown()` appears and maps to `UpdateBusy`;
- `open_source_validates_release_url_prefix`: `validate_release_url` appears before `shell::open`.

Also add the three new names (`update_check`, `update_install`, `update_open_releases`) to the expected vector in `command_names_registered` in `commands/mod.rs` tests.

- [ ] **Step 6: Run to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Expected: FAIL on new command names / source assertions.

- [ ] **Step 7: Implement commands and registration**

In `commands/mod.rs`: add `UpdateBusy` (`update_busy`) and `Update` (`update`) variants to `CommandError`, add `pub mod update;` and `pub use update::{update_check, update_install, update_open_releases};`. In `commands/update.rs`:
- `pub(crate) async fn update_check(app: AppHandle) -> Option<UpdateInfo>` delegates to `update::check`;
- `pub(crate) async fn update_install(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), CommandError>` returns `UpdateBusy` unless `lifecycle::begin_shutdown()`, then `lifecycle::cleanup(&state).await`, then `update::install(&app, |progress| emit(EVENT_UPDATE_PROGRESS, progress)).await.map_err(|_| CommandError::Update)`, then restarts via `app.restart()` or exits via `app.exit(0)` if restart is unavailable in the resolved crate version;
- `pub(crate) fn update_open_releases(url: String) -> Result<(), CommandError>` returns `InvalidInput` unless `update::validate_release_url(&url)`, else `tauri_plugin_shell::open(&url, None)` (or the resolved crate's equivalent) mapped to `CommandError::Update`.
In `lib.rs`, add the three commands to `command_list!` and `pub mod update;`.

- [ ] **Step 8: Run full Rust suite**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Expected: PASS, including `command_names_registered` with 23 names.

- [ ] **Step 9: Commit**

```bash
git add apps/desktop/src-tauri/src apps/desktop/src-tauri/tests
git commit -m "feat: add update commands and payloads"
```

---

### Task 5: Frontend update state, event hook and quiet periodic check

**Files:**
- Modify: `apps/desktop/src/lib/invoke.ts`
- Modify: `apps/desktop/src/lib/invoke.test.ts`
- Modify: `apps/desktop/src/stores/app.store.ts`
- Modify: `apps/desktop/src/stores/app.store.test.tsx`
- Create: `apps/desktop/src/features/updates/use-update-check.ts`
- Create: `apps/desktop/src/features/updates/use-update-check.test.tsx`
- Modify: `apps/desktop/src/App.tsx`

**Interfaces:**
- Consumes: `UpdateInfoSchema`, `UpdateProgressEventSchema`, `EVENTS.updateProgress` from Task 1.
- Produces: store fields `update`, `updateInstalling`, `updateProgress` and setters `setUpdate`, `setUpdateInstalling`, `setUpdateProgress`; `useUpdateCheck(): void`; `UPDATE_CHECK_INTERVAL_MS = 21600000`; error codes `update_busy`, `update`.

- [ ] **Step 1: Write failing tests**

- `invoke.test.ts`: `commandErrorCode("update_busy")` → `"update_busy"`, same for `"update"`.
- `app.store.test.tsx`: `setUpdate` / `setUpdateInstalling` / `setUpdateProgress` update state; the `EVENTS.updateProgress` listener parses a payload and stores it.
- `use-update-check.test.tsx`: mock `invoke` and `useAppStore`; assert one `update_check` call on mount, `update` set from returned info, `setNotice` called once for a newly seen version, no notice on failure, and a second call after `vi.advanceTimersByTime(UPDATE_CHECK_INTERVAL_MS)` (use `vi.useFakeTimers()`).

- [ ] **Step 2: Run to verify failure**

Run: `pnpm --filter @mglaroute/desktop exec vitest run src/lib/invoke.test.ts src/stores/app.store.test.tsx src/features/updates/use-update-check.test.tsx`
Expected: FAIL — missing hook, store fields, error codes.

- [ ] **Step 3: Implement state and helper**

Add `update_busy` and `update` to `SAFE_CODES` in `invoke.ts`. Add the three fields/setters to `AppStore` and its initial state; in `useBackendEvents`, subscribe to `EVENTS.updateProgress`, parse with `UpdateProgressEventSchema`, call `setUpdateProgress`. Implement `use-update-check.ts`: export `UPDATE_CHECK_INTERVAL_MS`, call `invoke("update_check", UpdateInfoSchema.nullable())` on mount and on interval, store result via `setUpdate`, call `setNotice("Update available: <version>")` only when a non-null version was not previously announced in this session, swallow failures. Add `useUpdateCheck()` to `App.tsx` next to `useBackendEvents()`.

- [ ] **Step 4: Run focused tests, then full suite**

Run: the Step 2 command, then `$env:NX_DAEMON='false'; pnpm test`.
Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/invoke.ts apps/desktop/src/lib/invoke.test.ts apps/desktop/src/stores/app.store.ts apps/desktop/src/stores/app.store.test.tsx apps/desktop/src/features/updates apps/desktop/src/App.tsx
git commit -m "feat: check for updates on a session timer"
```

---

### Task 6: Update panel in Settings

**Files:**
- Create: `apps/desktop/src/features/updates/UpdatePanel.tsx`
- Create: `apps/desktop/src/features/updates/UpdatePanel.test.tsx`
- Modify: `apps/desktop/src/pages/Settings.tsx`
- Modify: `apps/desktop/src/pages/settings-page.test.tsx`

**Interfaces:**
- Consumes: store fields/setters from Task 5; `update_check` result from Task 4.
- Produces: `UpdatePanel` React component; Settings tab value `"updates"`, label `"Updates"`.

- [ ] **Step 1: Write failing UI tests**

In `UpdatePanel.test.tsx`, mock `invoke` and render with `useAppStore.setState`:
- no update → text `"No update available"`;
- update with `canInstallInApp: true` → shows version and notes, button named `"Install update"`; clicking invokes `update_install`, sets `updateInstalling` true, then false after resolution;
- update with `canInstallInApp: false` → button named `"Open release page"`, clicking invokes `update_open_releases` with `{ url: releaseUrl }`;
- `install_button_disabled_while_installing`: button has `disabled` when `updateInstalling === true`;
- progress event in store renders `"downloading"`.
In `settings-page.test.tsx`, extend `settingsTabsVisible` to include `"Updates"`.

- [ ] **Step 2: Run to verify failure**

Run: `pnpm --filter @mglaroute/desktop exec vitest run src/features/updates/UpdatePanel.test.tsx src/pages/settings-page.test.tsx`
Expected: FAIL — component/tab missing.

- [ ] **Step 3: Implement the panel and tab**

`UpdatePanel` uses shadcn-style local Card/Button primitives already in `components/ui`, reads store via `useAppStore`, and uses the existing `invoke` wrapper. On click it sets `updateInstalling` true, invokes the chosen command, catches `CommandInvocationError` and calls `setNotice("Update failed", "error")`, then resets in `finally`. In `Settings.tsx`, add `"updates"` to `SettingsTab`, add `{ value: "updates", label: "Updates" }` to `tabs`, change `grid-cols-3` → `grid-cols-4`, and render `<UpdatePanel />` in a new `TabsContent`.

- [ ] **Step 4: Run tests and build**

Run: focused command from Step 2, then `$env:NX_DAEMON='false'; pnpm test` and `$env:NX_DAEMON='false'; pnpm build`.
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/features/updates apps/desktop/src/pages/Settings.tsx apps/desktop/src/pages/settings-page.test.tsx
git commit -m "feat: add in-app update panel"
```

---

### Task 7: Local NSIS command, release workflow and docs

**Files:**
- Modify: `apps/desktop/package.json`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Create: `.github/workflows/release.yml`
- Create: `scripts/release-config.test.mjs`
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/e2e-checklist.md`
- Modify: `docs/superpowers/specs/2026-11-26-release-updater-design.md`

**Interfaces:**
- Consumes: nothing new; verifies the wiring produced by Tasks 1–6.
- Produces: `pnpm installer:build`; tag-triggered release workflow; operator docs.

- [ ] **Step 1: Write failing config tests**

Create `scripts/release-config.test.mjs` (plain Node assertions, no YAML dep) reading root `package.json`, `apps/desktop/package.json`, `tauri.conf.json` and `.github/workflows/release.yml` as text. Treat missing files as empty strings so assertions fail instead of throwing ENOENT. Assert:
- root script `installer:build` contains `binaries:fetch`, `binaries:verify`, `tauri build --bundles nsis`;
- desktop devDependencies include `@tauri-apps/cli`;
- workflow contains `tags:`, `'v*'`, `windows-latest`, `contents: write`, `pnpm/action-setup`, `tauri-apps/tauri-action@v1`, `projectPath: apps/desktop`, `uploadUpdaterJson: true`, `updaterJsonPreferNsis: true`, `--bundles nsis`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `releaseDraft: false`;
- config contains `createUpdaterArtifacts` and the updater endpoint.

- [ ] **Step 2: Run to verify failure**

Run: `pnpm vitest run scripts/release-config.test.mjs`
Expected: FAIL — script/workflow missing.

- [ ] **Step 3: Implement build command**

Add `@tauri-apps/cli@^2` to `apps/desktop` devDependencies (via `pnpm --filter @mglaroute/desktop add -D @tauri-apps/cli@^2`, updating `pnpm-lock.yaml`). Add root script:
`"installer:build": "pnpm binaries:fetch && pnpm binaries:verify && pnpm --filter @mglaroute/desktop exec tauri build --bundles nsis"`.

- [ ] **Step 4: Implement release workflow**

Create `.github/workflows/release.yml`: `on.push.tags: ['v*']`; `permissions: contents: write`; one `windows-latest` job with steps: `actions/checkout@v4`, `pnpm/action-setup@v4` (version 9), `actions/setup-node@v4` (node 20, cache pnpm), `dtolnay/rust-toolchain@stable`, `pnpm install --frozen-lockfile`, `pnpm binaries:fetch`, `pnpm binaries:verify`, `tauri-apps/tauri-action@v1`. Action inputs: `projectPath: apps/desktop`, `tagName: v__VERSION__`, `releaseName: mglaroute v__VERSION__`, `generateReleaseNotes: true`, `releaseDraft: false`, `prerelease: false`, `args: --bundles nsis`, `updaterJsonPreferNsis: true`, `uploadUpdaterJson: true`, `uploadUpdaterSignatures: true`, `uploadWorkflowArtifacts: true`. Action env: `GITHUB_TOKEN` and both signing secrets from `secrets`.

- [ ] **Step 5: Run all verification**

Run: `pnpm vitest run scripts/release-config.test.mjs`, then `$env:NX_DAEMON='false'; pnpm test`, `$env:NX_DAEMON='false'; pnpm build`, and the full cargo command from Global Constraints.
Expected: all PASS. `installer:build` itself needs network + signing env, so it is an operator smoke test, not a CI-unit step.

- [ ] **Step 6: Update docs and mark spec accepted**

Add to `README.md`: `pnpm installer:build` prerequisites (fetch/verify binaries, signing key env) and tag-release flow. Add to `AGENTS.md` status + contracts: signed NSIS/updater, three new commands, session-only update state, secrets names. Add to `docs/e2e-checklist.md`: manual pass — notice appears, Install updates and restarts, proxy cleared and no xray left, fallback opens release page. Set spec status to `accepted` and replace `PUBLIC_KEY_PLACEHOLDER` note with the generated public key once the operator supplies it (keep placeholder until then).

- [ ] **Step 7: Commit**

```bash
git add .github/workflows/release.yml scripts/release-config.test.mjs package.json pnpm-lock.yaml apps/desktop/package.json README.md AGENTS.md docs
git commit -m "feat: publish signed NSIS releases from CI"
```

## Operator steps after merge

- Run `cargo tauri signer generate -w ~/.tauri/mglaroute.key`; commit the public key into `plugins.updater.pubkey`; set repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Optionally pin `XRAY_VERSION` as a repo variable for reproducible releases.
- Push tag `v0.2.0` (matching `tauri.conf.json` version) to exercise the workflow; verify release contains NSIS setup, `.sig`, `latest.json`, and that `latest.json` points at the NSIS asset with platform key `windows-x86_64`.
