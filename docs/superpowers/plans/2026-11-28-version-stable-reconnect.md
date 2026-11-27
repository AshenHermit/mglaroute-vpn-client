# Version display + stable connection on server-list change Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the current client version in the rail and Updates tab, and keep the VPN connected when its server disappears from the list, switching immediately to the best available server when Auto reconnect is on.

**Architecture:** A thin Tauri command exposes `package_info().version`; one session-only store field feeds both render sites. Connection stability is backend-only: `decide_reconnect` never disconnects, and a `ProbeProgress` wrapper spawns a small task on each `probe-server`/`done` event that switches an absent connected server via `connect_impl` under the connection permit.

**Tech Stack:** Tauri 2 / Rust, tokio, rusqlite, React 19 + Zustand 5, Zod 4, vitest.

**Spec:** `docs/superpowers/specs/2026-11-28-version-stable-reconnect-design.md`

## Global Constraints

- Version `0.2.2` is the current source value in root `package.json`, `apps/desktop/package.json`, `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`; `pnpm version:set <version>` is the only bump path.
- Never put subscription URLs, credentials, link data, raw config or native error text into logs, events or UI notices.
- Connection mutations go through `state.try_connection_permit()`; probe mutations through `state.probe_permit()`; read commands never mutate.
- No SQLite migration, no new settings, no new Tauri event names, no changes to existing event payload shapes.
- `command_list!` name test (`commands/mod.rs:947-983`) must match the registered command list exactly.
- Rust tests: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` with test-only `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` and `-- --test-threads=1`.
- JS tests: `pnpm test`; `NX_DAEMON='false' pnpm build`.

## Review Focus

- Absent connected server + no passing candidates → connection must stay up, never `Disconnect`. Pin: Task 3 `no_passing_candidate_keeps_current_connection`.
- `probe-server/done` with autoReconnect off, current server present, failing service results or a busy connection permit → no switch. Pin: Task 4 tests `present_current_no_emergency_switch`, `auto_reconnect_off_no_emergency_switch`, `failing_sample_no_emergency_switch`, `busy_connection_no_emergency_switch`.
- Emergency switch must not restore the previous proxy or emit a disconnect state; proxy ownership and `last_switch` stay untouched. Pin: Task 4 `absent_current_switches_without_disconnecting`.
- `app_version` failure must not surface native text or break the shell. Pin: Task 2 `version_fetch_failure_leaves_ui_without_notice`.
- Tray Exit and updater-install teardown still disconnect deliberately. Pin: existing `tests/lifecycle_config.rs` and `tests/update_command_surface.rs` remain unchanged and green.

---

### Task 1: `app_version` Tauri command

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/version.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:30-47` (module + re-export)
- Modify: `apps/desktop/src-tauri/src/lib.rs:35-59` (`command_list!`)
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:947-983` (`command_names_registered`)
- Test: `apps/desktop/src-tauri/src/commands/mod.rs:947-983`

**Interfaces:**
- Consumes: `tauri::AppHandle` (`package_info().version`), pattern from `commands/update.rs:35-38`.
- Produces: `pub fn app_version(app: tauri::AppHandle) -> String`; registered command name `app_version`.

- [ ] **Step 1: Extend the failing name-list test**

In `command_names_registered`, add `"app_version"` to the expected sorted vector (first entry, before `"anti_dpi_start"`).

- [ ] **Step 2: Run the test to verify it fails**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml command_names_registered -- --test-threads=1`
Expected: FAIL — actual list lacks `"app_version"`.

- [ ] **Step 3: Implement and register the command**

`commands/version.rs`:
```rust
use tauri::AppHandle;

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
```
In `commands/mod.rs` add `pub mod version;` and `pub use version::app_version;`. In `lib.rs` add `app_version,` as the first item of `command_list!`.

- [ ] **Step 4: Run the test to verify it passes**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/commands/version.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/lib.rs
git commit -m "feat: expose client version via tauri command"
```

### Task 2: Render the current version in rail and Updates

**Files:**
- Modify: `apps/desktop/src/stores/app.store.ts:28-61,63-82`
- Modify: `apps/desktop/src/app-shell/AppShell.tsx:25-45`
- Modify: `apps/desktop/src/app-shell/ServerRail.tsx:70-88`
- Modify: `apps/desktop/src/features/updates/UpdatePanel.tsx:19-30,64-71`
- Modify: `apps/desktop/src/app-shell/app-shell.test.tsx`
- Modify: `apps/desktop/src/features/updates/UpdatePanel.test.tsx`

**Interfaces:**
- Consumes: `app_version` from Task 1, validated with `z.string()`.
- Produces: `clientVersion: string | null` and `setClientVersion(version: string | null): void` on `useAppStore`.

- [ ] **Step 1: Write failing frontend tests**

In `app-shell.test.tsx`: mock `app_version` to resolve `"0.2.2"` and assert `screen.getByText("v0.2.2")` plus `expect(mockedInvoke).toHaveBeenCalledWith("app_version", expect.anything())`. Add a second case where `app_version` rejects and assert no notice and no version text.

In `UpdatePanel.test.tsx`: set `clientVersion: "0.2.2"` in the store and assert `screen.getByText("Current version 0.2.2")` in both the no-update card and the update-found card; keep the existing `Version 0.2.0` (available version) assertion.

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm --filter @mglaroute/desktop test -- --run src/app-shell/app-shell.test.tsx src/features/updates/UpdatePanel.test.tsx`
Expected: FAIL — `v0.2.2` / `Current version` not rendered.

- [ ] **Step 3: Add store field and fetch**

In `app.store.ts` add `clientVersion: string | null` (default `null`) and `setClientVersion`, following the adjacent `update` field/setter pattern. In `AppShell.tsx` add to the existing hydration effect:
```ts
void invoke("app_version", z.string())
  .then((version) => { if (active) setClientVersion(version); })
  .catch(() => {/* no notice: native text must not reach UI */});
```
Import `z` from `zod` and select `setClientVersion` from the store; add it to the effect deps.

- [ ] **Step 4: Render in both places**

In `ServerRail.tsx` read `clientVersion` from the store and render `<span data-testid="client-version">v{clientVersion}</span>` inside the title row's `<div>` (`:72-88`), outside the `h1` so it shows at narrow widths. In `UpdatePanel.tsx` read `clientVersion` and render `<p className="text-sm text-muted-foreground">Current version {clientVersion ?? "—"}</p>` in both card bodies, before the existing available-version line.

- [ ] **Step 5: Run tests and build**

Run: `pnpm --filter @mglaroute/desktop test -- --run src/app-shell/app-shell.test.tsx src/features/updates/UpdatePanel.test.tsx`
Expected: PASS.
Run: `$env:NX_DAEMON='false'; pnpm build`
Expected: PASS (pre-existing zod annotation warnings tolerated).

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/stores/app.store.ts apps/desktop/src/app-shell/AppShell.tsx apps/desktop/src/app-shell/ServerRail.tsx apps/desktop/src/features/updates/UpdatePanel.tsx apps/desktop/src/app-shell/app-shell.test.tsx apps/desktop/src/features/updates/UpdatePanel.test.tsx
git commit -m "feat: show current client version in rail and updates"
```

### Task 3: Auto-reconnect never disconnects

**Files:**
- Modify: `apps/desktop/src-tauri/src/reconnect.rs:7-74,76-199`
- Modify: `apps/desktop/src-tauri/src/commands/probe.rs:151-169`
- Test: `apps/desktop/src-tauri/src/reconnect.rs:76-199`

**Interfaces:**
- Consumes: unchanged `decide_reconnect(current_id, candidates, settings, now, last_switch)`.
- Produces: `ReconnectDecision` with only `None` and `Switch { profile_id }`.

- [ ] **Step 1: Change the failing unit test**

In `reconnect.rs`, rename `no_candidate_passing_disconnects` to `no_passing_candidate_keeps_current_connection` and assert `ReconnectDecision::None` for `candidates(&[("current", 10, false)])` with auto-reconnect enabled.

- [ ] **Step 2: Run the test to verify it fails**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reconnect::tests::no_passing_candidate_keeps_current_connection -- --test-threads=1`
Expected: FAIL — actual `Disconnect`.

- [ ] **Step 3: Implement**

Remove the `Disconnect` variant from `ReconnectDecision` and return `ReconnectDecision::None` at `reconnect.rs:52`. Update the enum doc comment: "When no profile passes, the live connection is kept." Remove the `ReconnectDecision::Disconnect => disconnect_impl(state).await?` arm in `commands/probe.rs:163-165`; remove the now-unused `disconnect_impl` import from that file only if no other call remains there.

- [ ] **Step 4: Run reconnect and command tests**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reconnect -- --test-threads=1`
Expected: PASS.
Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml commands::tests -- --test-threads=1`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/reconnect.rs apps/desktop/src-tauri/src/commands/probe.rs
git commit -m "fix: keep VPN connected when no server passes reconnect filters"
```

### Task 4: Immediate switch when the connected server is absent

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/probe.rs:1-20,122-256`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:249-256,516-544,1093-1160`
- Test: `apps/desktop/src-tauri/src/commands/mod.rs:1093-1160` (reuse `conn_with_profiles`, `prepare_connected`)

**Interfaces:**
- Consumes: `AppState`, `Settings`, `ProbeServerEvent` (`probe/cycle.rs:71-78`), `candidates()` (`probe.rs:69-97`), `connect_impl` (`connection.rs:43-215`), `ReconnectProgress` from this task.
- Produces:
  - `pub(crate) fn missing_server_target(conn: &Connection, current_id: Option<&str>, event: &ProbeServerEvent) -> rusqlite::Result<Option<String>>`
  - `pub(crate) async fn switch_if_current_missing(state: &AppState, settings: &Settings, event: &ProbeServerEvent) -> Result<(), CommandError>`
  - `pub(crate) struct ReconnectProgress { state: AppState, settings: Settings }` with `ReconnectProgress::new(state: AppState, settings: Settings) -> Self`, implementing `ProbeProgress`.

- [ ] **Step 1: Write failing tests for the target selector**

Add to `commands/mod.rs` tests module, using `conn_with_profiles()` and `ProbeServerEvent`:
```rust
fn done_event(id: &str, latency: Option<u64>, services: &[(&str, bool)]) -> ProbeServerEvent
```
Assertions:
- `absent_current_picks_best_done_server`: current `"p-gone"` (not in profiles), event for `"p-best"` with `latency_ms: Some(10)`, `status: Done`, no enabled service profiles ⇒ `Some("p-best")`.
- `present_current_no_emergency_switch`: current `"p-1"` present ⇒ `None`.
- `failing_sample_no_emergency_switch`: `status: Failed` or `latency_ms: None` or a reachable service result missing ⇒ `None`.
- `missing_target_uses_persisted_delay`: seed `p-1` latency 5 via `profile_repo::set_result` and `p-best` event latency 10 with no services ⇒ `Some("p-1")` (best known at that moment).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml missing_server_target -- --test-threads=1`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement `missing_server_target`**

In `commands/probe.rs`: import `ProbeServerEvent`, `ProbeServerStatus`, `ProbeServiceResult` from `crate::probe::cycle`. Logic in order: `current_id?`; `profile_repo::list_present_enabled(conn)?` must not contain current; `event.status == Done`; `event.latency_ms.filter(|v| *v > 0)`; every enabled service id from `service_profile_repo::list_enabled(conn)?` appears in `event.service_results` with `reachable == true`. Then take `candidates(conn)?`, replace the tuple whose id equals `event.profile_id` with `(event.profile_id.clone(), latency, true)`, and return the id with the minimum `(latency, id)`.

- [ ] **Step 4: Run the selector tests**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 5: Write failing async switch tests**

Add to `commands/mod.rs` tests module:
- `absent_current_switches_without_disconnecting`: build `AppState::new` with `InMemoryProxy` + `FakeLauncher` as in `reconnect_gate_records_last_switch` (`:1283-1307`), but `prepare_connected(&state, "p-gone", ...)` even though `"p-gone"` is absent from `conn_with_profiles`; set `settings.auto_reconnect_enabled = true`; call `switch_if_current_missing(&state, &settings, &done_event("p-best", Some(10), &[]))`; assert `runtime.connection.status == Connected`, `profile_id == Some("p-best")`, `last_switch == None`, and `InMemoryProxy::state()` is still `PreviousProxy::Manual` for the new port (never restored to the original `PreviousProxy`).
- `auto_reconnect_off_no_emergency_switch`: same setup with `auto_reconnect_enabled = false` ⇒ profile id stays `"p-gone"`.
- `busy_connection_no_emergency_switch`: hold `state.connection_permit()` before the call ⇒ no switch, `Ok(())`.

- [ ] **Step 6: Run the tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml switch_if_current_missing -- --test-threads=1`
Expected: FAIL — function not found.

- [ ] **Step 7: Implement `switch_if_current_missing` and `ReconnectProgress`**

`switch_if_current_missing`: `let Some(_permit) = state.try_connection_permit() else { return Ok(()) };`; gate `settings.auto_reconnect_enabled`; read `(status, current_id)` under `runtime.lock().await`; require `Connected`; read `missing_server_target` under a short `conn.lock().await`; call `connect_impl(state, Some(target)).await?`. Do not touch `last_switch`.

`ReconnectProgress`: fields `state: AppState`, `settings: Settings`. `on_server` calls `TauriProgress.on_server(event)`, then for `status == Done` clones `state`, `settings`, `event.clone()` into `tokio::spawn(async move { let _ = switch_if_current_missing(&state, &settings, &event).await; })`. `on_progress`, `on_cancelled`, `on_finished` delegate to `TauriProgress`. Use trait-qualified calls (`ProbeProgress::on_server(&TauriProgress, event)` etc.).

- [ ] **Step 8: Wire the wrapper into both probe paths**

In `commands/probe.rs::probe_run` (`:249-256`): read `settings_repo::get` from the connection lock before `run_impl`, then pass `&ReconnectProgress::new(state.clone(), settings)` instead of `&TauriProgress`. In `commands/mod.rs::auto_test_loop` (`:524-540`): keep the full `Settings` value from the existing read instead of only the boolean, gate on `settings.auto_test_enabled`, and pass `&probe::ReconnectProgress::new(state.clone(), settings)`. Keep `TauriProgress` for unit tests that call `run_impl` directly.

- [ ] **Step 9: Run full Rust suite**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add apps/desktop/src-tauri/src/commands/probe.rs apps/desktop/src-tauri/src/commands/mod.rs
git commit -m "feat: switch absent server immediately when a probe finds a better one"
```

### Task 5: Keep the absent connected server visible in UI

**Files:**
- Modify: `apps/desktop/src/features/connection-panel/ConnectionPanel.tsx:139-188`
- Modify: `apps/desktop/src/features/connection-panel/connection-panel.test.tsx`

**Interfaces:**
- Consumes: `useConnectionStore().state` (already subscribed in this component) and `useAppStore().profiles`.

- [ ] **Step 1: Write failing test**

In `connection-panel.test.tsx`, emit a `connection-state` event with `{ status: "connected", profileId: "p-gone", proxyPort: 10808 }` while `profiles` is empty (no rows). Assert the active-server block contains text `Connected server (not in list)` and `p-gone`, and does **not** contain `No active server`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm --filter @mglaroute/desktop test -- --run src/features/connection-panel/connection-panel.test.tsx`
Expected: FAIL — `No active server` still rendered.

- [ ] **Step 3: Implement the label**

In `ConnectionPanel.tsx`, when `connected && activeProfile === null && activeId !== null`, render the fixed label `Connected server (not in list)` in place of `activeProfile?.remarks ?? ... ?? "No active server"`, and render `activeId` in the secondary line. Keep `activeState === "connected"`. No new data source, no new command.

- [ ] **Step 4: Run the test**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/features/connection-panel/ConnectionPanel.tsx apps/desktop/src/features/connection-panel/connection-panel.test.tsx
git commit -m "fix: show connected server that is missing from the list"
```

### Task 6: Docs and final verification

**Files:**
- Modify: `AGENTS.md` (contracts/conventions: version command, no-disconnect rule, immediate-switch hook)
- Modify: `docs/e2e-checklist.md` (manual case: remove connected server, verify no drop, then Auto reconnect + probe switch)

**Interfaces:** None.

- [ ] **Step 1: Document the behaviour**

Add short bullets to `AGENTS.md` Contracts: `app_version` command and both render sites; auto-reconnect never disconnects; `switch_if_current_missing` on `probe-server/done`; absent-server UI label. Add one manual scenario to `docs/e2e-checklist.md` matching Review Focus.

- [ ] **Step 2: Run both suites and build**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Run: `pnpm test`
Run: `$env:NX_DAEMON='false'; pnpm build`
Expected: all PASS (known pre-existing zod annotation warnings and pnpm@9 notice tolerated).

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md docs/e2e-checklist.md
git commit -m "docs: record version display and stable reconnect behaviour"
```
