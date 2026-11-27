# Version display + stable connection on server-list change — design

Date: 2026-11-28. Status: proposed.

## Intent

Two operator requests:

1. Show the current client version next to the app title and in Settings →
   Updates.
2. A subscription refresh that removes the connected server from the list must
   never drop the VPN. The server stays connected even when absent. When Auto
   reconnect is on, the first available server found by a scan immediately
   replaces the absent connected server with the best known eligible server —
   without tearing down the running proxy.

## Goals

- A Tauri command exposes the running app version; one fetch hydrates the UI.
- Current version rendered beside `mglaroute` in the rail and in `UpdatePanel`.
- Auto-reconnect never disconnects on its own: no passing candidate ⇒ keep the
  live connection.
- On `probe-server` with `status: done`, if the connected profile is absent
  from present enabled profiles, switch at once to the best eligible server.
- Switch uses `connect_impl` (same snapshot/guard/proxy-ownership path as a
  manual Connect), never `disconnect_impl` first.
- Connection UI keeps showing the absent connected server instead of
  "No active server".

## Non-goals

- No new settings, SQLite migration, or change to subscription decode/import.
- No change to manual Connect/Disconnect, tray Exit, or updater-install teardown.
- No persisted version string; session-only state.
- No TUN, anti-DPI, or server-side changes.
- No change to probe concurrency, event names, or event payload shapes.

## Constraints / findings

- Version `0.2.2` is duplicated in root `package.json`, `apps/desktop/package.json`,
  `apps/desktop/src-tauri/tauri.conf.json`, `Cargo.toml`, `Cargo.lock`;
  `scripts/set-version.mjs` owns the bump. Running app version is available as
  `app.package_info().version` (`update/desktop.rs:16`).
- No version command exists. `lib.rs:35-59` lists commands; inline test
  `command_names_registered` (`commands/mod.rs:947-982`) asserts the exact sorted
  list and must be updated.
- `UpdateInfo.currentVersion` (`schemas.ts:127`) is populated only when an
  update is found, so it cannot drive a always-visible version label.
  `UpdatePanel.tsx:70` shows only the available version.
- `ServerRail.tsx:73-75` holds the `h1`; rail footer is `:138-151`.
- `subscription::import` → `profile_repo::mark_absent` (`repos.rs:251-275`)
  only sets `present = 0`; it does not touch xray or the system proxy. The
  disconnect actually comes from `decide_reconnect` (`reconnect.rs:45-54`):
  current not in passing candidates and none passing ⇒ `Disconnect`, applied
  by `commands/probe.rs:163-165` via `disconnect_impl`.
- `apply_reconnect` (`commands/probe.rs:126-169`) owns the connection permit,
  the `auto_reconnect_enabled` gate, the `Connected` gate and `last_switch`.
- `candidates` (`commands/probe.rs:69-97`) uses persisted probe results;
  `best_eligible_profile` (`:100-120`) picks the lowest latency passing profile.
- `connect_impl` (`commands/connection.rs:43-215`) switches servers while
  preserving proxy ownership; no prior disconnect is needed.
- `ProbeProgress::on_server` (`probe/cycle.rs:110-115`) fires on each terminal
  event; `tracked_probe` (`:212-220`) emits `done` with latency and
  `service_results` before results are persisted. `TauriProgress::on_server`
  (`:345-347`) only emits the event.
- `AppState` is `Clone` (`commands/mod.rs:188-191`); `try_connection_permit()`
  serialises connection mutations.
- `ConnectionPanel.tsx:139-141` resolves the active profile via
  `profiles.find(...)`; an absent id currently renders "No active server".

## Approaches (operator picked 1 / command)

Version:

1. **Chosen — Tauri `app_version` command.** One runtime source of truth,
   contract-free string, no build-time drift.
2. `update.currentVersion` — rejected: absent until an update is found.
3. Vite `define` from package.json — rejected: can drift from `tauri.conf.json`.

Reconnect:

1. **Chosen — Rust progress wrapper + spawned task.** On each `done` event,
   clone `AppState`/`Settings`/event into a task; the task re-checks state under
   the connection permit and switches if needed. Rust stays authoritative;
   existing `connect_impl` is reused; races are suppressed by the permit.
2. Async hook inside `ProbeProgress`/`probe_cycle_detached` — rejected as
   invasive: changes the trait and every test double for the same behaviour.
3. Frontend `probe-server` listener calling `connection_connect` — rejected:
   duplicates "best server" logic in TS and can race backend auto-reconnect.

## Decisions

### Version

- Add `#[tauri::command] app_version(state: State<'_, AppState>) -> String` in
  a new `commands/version.rs` (or the existing thin command module), returning
  `state.app_handle().package_info().version.to_string()`. No new error code.
- Register in `lib.rs` command list; update the sorted-name test.
- `AppShell` fetches once (`invoke("app_version", z.string())`) and writes
  `clientVersion: string | null` in `app.store` via `setClientVersion`.
  Failure leaves it `null`; no notice (avoid native error text).
- `ServerRail` renders `v{version}` next to the `h1` (visible at all widths,
  unlike the `lg`-only title). `UpdatePanel` renders `Current version {version}`
  in both the update-found and no-update cards; keep `Version {update.version}`
  for the available release.

### Connection stability

- Remove `ReconnectDecision::Disconnect`. `decide_reconnect` returns `None`
  when no passing candidate exists, so auto-reconnect keeps the live
  connection. Update inline unit tests (`no_candidate_passing_disconnects`
  becomes a "keeps current" assertion).
- New `switch_if_current_missing(state, settings, event)` in `commands/probe.rs`:
  1. `try_connection_permit()`, else skip.
  2. gate `auto_reconnect_enabled`, then `Connected` status.
  3. read current `profile_id`; if the id is present in
     `profile_repo::list_present_enabled`, return (normal reconnect logic owns
     that case).
  4. require event `status == done`, positive `latency_ms`, and every enabled
     service profile reachable in `service_results`.
  5. rebuild the candidate list from `candidates(&conn)`, overriding the
     freshly probed id with the new `(latency, passes)` pair; pick best by
     `(latency, id)`.
  6. `connect_impl(state, Some(best_id))`; do **not** record `last_switch`, so
     the post-cycle `apply_reconnect` can still refine to the best of the full
     result set.
- A `ProbeProgress` wrapper in `commands/probe.rs` emits the event through
  `TauriProgress` and, for `done` events, spawns the task above. Use it from
  both `probe_run` and the auto-test loop. Tests keep `TauriProgress` so they
  do not spawn tasks.
- `ConnectionPanel`: when `connected` and `activeProfile` is `null`, show a
  fixed label "Connected server (not in list)" plus the backend `profileId`;
  do not invent details that the store no longer has.

## Verification

- Rust: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` with
  `TAURI_CONFIG` test override and `-- --test-threads=1`; new tests for
  "no passing candidate keeps connection", "absent current + passing event
  switches", "present current -> no emergency switch", "autoReconnect off ->
  no switch", command name list.
- JS: `pnpm test`, `NX_DAEMON=false pnpm build`; new tests for version hydration
  and both render locations, `ConnectionPanel` absent-server label.
- Manual: `docs/e2e-checklist.md` — remove the connected server from a
  subscription, confirm proxy stays up, enable Auto reconnect, run test,
  confirm immediate switch with no Disconnect state.
