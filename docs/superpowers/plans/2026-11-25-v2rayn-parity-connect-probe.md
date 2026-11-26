# v2rayN Parity: Connect/Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Connect work on first click, run server tests in parallel without touching the live connection, show per-server testing state, and never leave orphaned xray processes.

**Architecture:** Replace the single mutation semaphore with three domain guards; rewrite the probe as one temporary xray per server behind `futures::stream::buffer_unordered`, with `watch`-based cancellation and Windows Job Object supervision; let the UI select a server and drive connect explicitly. All cross-cutting state stays behind Rust commands and Zod contracts.

**Tech Stack:** Tauri v2, Rust 1.77, tokio, rusqlite, reqwest, `futures = "0.3"`, `windows-sys 0.59`, React 18, Zustand 5, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-25-v2rayn-parity-connect-probe-design.md`

## Global Constraints

- `test_concurrency` default `10`, clamp `1..=20` (v2rayN `MixedConcurrencyCount`).
- Probe startup timeout `2000 ms` (`XRAY_STARTUP_TIMEOUT_MS`); delay = 2 HTTP GETs, min positive ms; service check = 2xx on all URLs.
- Crash watcher tick `500 ms`; `stop_child` retries once.
- No subscription URL, credential, raw config or generated config in logs/events/sidecars. Sidecars contain only a decimal PID.
- Windows is MVP; all OS-specific code behind `cfg(windows)` with a no-op non-Windows fallback.
- Contracts in `packages/contracts/src/schemas.ts` are the only source of TS payload shapes.
- `PRAGMA user_version` moves from `1` to `2`; v1 databases migrate non-destructively.
- Test commands: `pnpm test`; cargo tests require `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'`.

## Review Focus

These cases are implied by the spec but no single happy-path test covers them. Each is pinned in the owning task.

- **Cancel while a core is starting**: in-flight core must be stopped, `config` + `.pid` deleted, partial samples persisted without re-sort, `probe-finished(cancelled: true)` emitted — Task 4.
- **Recycled PID in a stale sidecar**: `reap_orphans` must verify image name **and** command line before `taskkill`; a non-matching PID is never killed — Task 3.
- **Job assignment failure after spawn**: spawned child must be killed immediately; no config or pid sidecar left behind — Task 3 and Task 4.
- **Manual test while connected**: system proxy, runtime child and connection state stay untouched; Connect must not return `probe_busy` — Task 2 and Task 4.
- **Fallback select from a disabled subscription**: `connect(None)` must never choose it — Task 5.

---

### Task 1: Settings — `testConcurrency` end to end

**Files:**
- Modify: `apps/desktop/src-tauri/src/storage/schema.sql:1,64-79`
- Modify: `apps/desktop/src-tauri/src/storage/mod.rs:6-10`
- Modify: `apps/desktop/src-tauri/src/storage/models.rs:125-155`
- Modify: `apps/desktop/src-tauri/src/storage/repos.rs:91-105,413-469`
- Modify: `apps/desktop/src-tauri/src/commands/settings.rs:8-40,48-117`
- Modify: `packages/contracts/src/schemas.ts:58-74`
- Modify: `apps/desktop/src/features/settings-form/SettingsForm.tsx:18-24`
- Test: `apps/desktop/src-tauri/src/storage/mod.rs` (new `#[cfg(test)]` module), `apps/desktop/src/features/settings-form/settings-form.test.tsx`

**Interfaces:**
- Consumes: existing `Settings`, `settings_repo::get/update`, `SettingsSchema`, `SettingsForm` field unions.
- Produces: `Settings.test_concurrency: i64` (default `10`); `SettingsPayload.test_concurrency`; `settings_update` argument `test_concurrency: Option<i64>` placed after `test_timeout_ms`; `SettingsSchema.testConcurrency: z.number().int().min(1).max(20)`; `SettingsForm` `NumberField` includes `"testConcurrency"`.

- [ ] **Step 1: Write failing Rust migration test**

```rust
#[test]
fn migrates_v1_settings_to_test_concurrency() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("v1.db")).unwrap();
    conn.execute_batch(
        "PRAGMA user_version = 1;
         CREATE TABLE settings (id INTEGER PRIMARY KEY CHECK (id = 1),
           auto_refetch_enabled INTEGER NOT NULL DEFAULT 0,
           refetch_interval_minutes INTEGER NOT NULL DEFAULT 15,
           auto_test_enabled INTEGER NOT NULL DEFAULT 0,
           test_interval_minutes INTEGER NOT NULL DEFAULT 15,
           test_timeout_ms INTEGER NOT NULL DEFAULT 5000,
           auto_reconnect_enabled INTEGER NOT NULL DEFAULT 0,
           reconnect_cooldown_seconds INTEGER NOT NULL DEFAULT 60,
           ping_url TEXT NOT NULL DEFAULT 'x',
           local_proxy_port INTEGER NOT NULL DEFAULT 10808,
           proxy_exclusions TEXT NOT NULL DEFAULT '[]',
           system_proxy_enabled INTEGER NOT NULL DEFAULT 1);
         INSERT INTO settings (id) VALUES (1);",
    ).unwrap();
    migrate(&conn).unwrap();
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
    assert_eq!(version, 2);
    let value: i64 = conn.query_row("SELECT test_concurrency FROM settings WHERE id = 1", [], |r| r.get(0)).unwrap();
    assert_eq!(value, 10);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml migrates_v1_settings`
Expected: FAIL — no `test_concurrency` column.

- [ ] **Step 3: Implement the migration and field plumbing**

In `schema.sql` add `test_concurrency INTEGER NOT NULL DEFAULT 10` to the `settings` table and change the pragma to `2`. In `storage/mod.rs`, before executing the schema batch, read `user_version`; if `1`, execute `ALTER TABLE settings ADD COLUMN test_concurrency INTEGER NOT NULL DEFAULT 10;` and `PRAGMA user_version = 2;` in the same transaction. Keep `include_str!("schema.sql")` as the second step so a fresh v2 DB also gets the column. Add the field to `Settings` (`Default` = `10`), `settings_from_row`, both INSERT/UPDATE SQL statements, `SettingsPayload` + `From<Settings>`, and the `settings_update` argument/validation (`1..=20`, else `CommandError::InvalidInput`). Add `testConcurrency` to `SettingsSchema` and to `SettingsForm`'s `NumberField` union with the other numeric inputs.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml storage::`
Run: `pnpm test`
Expected: PASS, including existing settings-form test.

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri/src/storage apps/desktop/src-tauri/src/commands/settings.rs packages/contracts/src/schemas.ts apps/desktop/src/features/settings-form && git commit -m "feat: add test concurrency setting with v2 migration"`

---

### Task 2: Three domain guards and addressable busy codes

**Files:**
- Modify: `apps/desktop/src-tauri/src/state.rs:10-46`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:47-74,161-206,246-337,380-426`
- Modify: `apps/desktop/src-tauri/src/commands/connection.rs:264-279`
- Modify: `apps/desktop/src-tauri/src/commands/probe.rs:205-209`
- Modify: `apps/desktop/src-tauri/src/commands/settings.rs:48-64`
- Modify: `apps/desktop/src-tauri/src/commands/subscription.rs:156-162,194-203,232-237,246-253`
- Modify: `packages/contracts/src/schemas.ts` (no structural change; codes live in `invoke.ts`)
- Modify: `apps/desktop/src/lib/invoke.ts:5-15`
- Test: `apps/desktop/src-tauri/src/commands/mod.rs` tests, `apps/desktop/src/lib/invoke.test.ts`

**Interfaces:**
- Consumes: `MutationGuard`, `MutationPermit`, `MutationError::Busy`.
- Produces: `AppStateInner` fields `connection_guard: MutationGuard`, `probe_guard: MutationGuard`, `mutation_guard: MutationGuard`; methods `connection_permit() -> Result<MutationPermit, CommandError>`, `try_connection_permit() -> Option<MutationPermit>`, `probe_permit() -> Result<MutationPermit, CommandError>`, `permit() -> Result<MutationPermit, CommandError>` (mutation). `CommandError` replaces `Busy` with `ConnectionBusy`, `ProbeBusy`, `MutationBusy` (serde snake_case → `connection_busy`, `probe_busy`, `mutation_busy`). `invoke.ts` `SAFE_CODES` contains all three plus legacy `busy`.

- [ ] **Step 1: Write failing guard independence tests**

```rust
#[tokio::test]
async fn probe_guard_does_not_block_connection_guard() {
    // Add `fn state_with_fake_launcher() -> AppState` next to the existing
    // helpers: in-memory Connection + migrate + InMemoryProxy + FakeLauncher.
    let state = state_with_fake_launcher();
    let probe = state.probe_permit().expect("probe permit");
    assert!(state.connection_permit().is_ok());
    assert_eq!(state.probe_permit().err(), Some(CommandError::ProbeBusy));
    drop(probe);
    assert!(state.probe_permit().is_ok());
}
```

Add a second test asserting a held `connection_permit` yields `ConnectionBusy` from `connection_permit()` but does not block `permit()` (mutation).

- [ ] **Step 2: Run them to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml guard_does_not_block`
Expected: FAIL — methods/variants do not exist.

- [ ] **Step 3: Split guards at every call site**

Keep `MutationGuard` as the shared semaphore type; give each `AppStateInner` field its own instance. Map each guard's `MutationError::Busy` to its domain code in the permit method. `connection_connect`, `connection_disconnect`, `apply_reconnect`, and `child_exit_watch` take `connection_permit()` (watcher uses `try_connection_permit()` and skips the tick on `None`). `probe_run` and `auto_test_loop` take `probe_permit()`. Subscription commands, settings update and `refetch_loop` keep `permit()`. Remove `CommandError::Busy` and `From<MutationError>`; update every reference and the command-surface list test if needed. Extend `SAFE_CODES` and add a Vitest assertion that `commandErrorCode({ probe_busy: null }) === "probe_busy"`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Run: `pnpm test`
Expected: PASS with no `busy` string left in Rust `CommandError` matches.

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri/src apps/desktop/src/lib packages/contracts/src && git commit -m "refactor: split mutation guard into connection/probe/mutation domains"`

---

### Task 3: Job Object supervision, pid sidecars, startup reaping

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml:28-35`
- Modify: `apps/desktop/src-tauri/src/core/supervisor.rs:1-136`
- Create: `apps/desktop/src-tauri/src/core/job.rs`
- Create: `apps/desktop/src-tauri/src/probe/temp.rs`
- Modify: `apps/desktop/src-tauri/src/probe/mod.rs:1-9`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:118-150,161-230`
- Test: `apps/desktop/src-tauri/src/core/supervisor.rs` tests (cfg(windows))

**Interfaces:**
- Consumes: `ConfigSnapshot`, `ChildGuard`, `XrayLauncher`, `bootstrap`.
- Produces: `JobHandle::create() -> std::io::Result<Self>`, `JobHandle::noop() -> Self`, `JobHandle::assign(&self, child: &std::process::Child) -> std::io::Result<()>`; `ChildGuard::spawn(snapshot: &ConfigSnapshot, job: Option<&JobHandle>) -> Result<Self>`; `XrayLauncher { job: Arc<JobHandle> }`; `AppState::new_with_job(..., job: Arc<JobHandle>)` (keep `AppState::new` as no-op job for tests); `ProbeTempFiles { config_path: PathBuf, pid_path: PathBuf }` with `ProbeTempFiles::new() -> io::Result<Self>`, `write_pid(pid: u32)`, `cleanup()`; `reap_orphans_with(temp_dir: &Path, verify: impl FnMut(u32, &Path) -> bool, kill: impl FnMut(u32)) -> io::Result<usize>` plus production `reap_orphans(temp_dir: &Path)`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn reaping_only_kills_verified_xray() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
    std::fs::write(&pid_path, "4242").unwrap();
    let killed = std::cell::Cell::new(0);
    let count = reap_orphans_with(
        dir.path(),
        |_pid, _config| false,
        |_pid| killed.set(killed.get() + 1),
    ).unwrap();
    assert_eq!(count, 0);
    assert_eq!(killed.get(), 0);
    assert!(pid_path.exists(), "suspect sidecar stays for inspection");
}
```

Second test: same fixture but `verify` returns `true`; assert `killed.get() == 1`, the pid was `4242`, and both `.pid` and its sibling `.json` are removed. Third test (`#[cfg(windows)]`): create a `JobHandle`, spawn `cmd /C timeout /T 30`, assign it, drop the handle, assert child exits within 2 s.

- [ ] **Step 2: Run them to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reaping`
Expected: FAIL — functions do not exist.

- [ ] **Step 3: Implement Windows job + reaper**

Add windows-sys features `Win32_System_JobObjects`, `Win32_System_Threading`. In `core/job.rs`, `JobHandle` wraps the job HANDLE as `isize` with `unsafe impl Send + Sync` and `Drop` calling `CloseHandle`. `create()` calls `CreateJobObjectW`, `SetInformationJobObject` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`0x2000`), no name; non-Windows returns a no-op handle. `assign()` casts `child.raw_handle()` (cfg(windows)) and calls `AssignProcessToJobObject`. Change `ChildGuard::spawn` to accept an optional job, assign immediately, and on assign failure kill+wait the child before returning the error. `XrayLauncher` stores `Arc<JobHandle>` and passes it for connection cores. `bootstrap` calls `reap_orphans(temp_dir)` before proxy restore, creates the job, and uses `new_with_job`. Production `verify` checks `tasklist /FI "PID eq <pid>" /FO CSV /NH` for `xray.exe`, then a PowerShell `Get-CimInstance Win32_Process -Filter "ProcessId=<pid>"` command line containing the sidecar's config filename; production `kill` runs `taskkill /PID <pid> /T /F`. Never log the command line. `ProbeTempFiles::new()` uses `mglaroute-probe-{pid}-{nanos}` for both `.json` and `.pid`; `write_pid` writes only decimal digits.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Expected: PASS; cfg(windows) job test may be skipped on non-Windows runner.

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri && git commit -m "feat: supervise xray with Windows job objects and reap stale cores"`

---

### Task 4: Parallel probe with per-server events and cancellation

**Files:**
- Modify: `apps/desktop/src-tauri/src/core/config.rs:329-368`
- Modify: `apps/desktop/src-tauri/src/probe/cycle.rs:1-317`
- Modify: `apps/desktop/src-tauri/src/probe/mod.rs:1-9`
- Modify: `apps/desktop/src-tauri/src/commands/probe.rs:164-218`
- Modify: `apps/desktop/src-tauri/src/lib.rs:30-50`
- Modify: `packages/contracts/src/schemas.ts:91-108`
- Test: `apps/desktop/src-tauri/src/probe/cycle.rs` tests, `apps/desktop/src-tauri/src/commands/mod.rs` command-name test

**Interfaces:**
- Consumes: `ProbeTempFiles`, `JobHandle`, `test_concurrency`, three guards, `measure_delay`, `check_service_profile`.
- Produces:
  - `pub fn build_probe_config(profile: &Profile, port: u16) -> Result<Value>` — one `mixed` inbound, one outbound via existing `build_outbound`, one routing rule; no `direct`, no exclusions.
  - `pub struct ProbeContext<'a> { pub cancel: &'a watch::Receiver<bool>, pub job: &'a JobHandle }` (`Clone, Copy`).
  - `type ProbeFutureOne<'a> = Pin<Box<dyn Future<Output = Result<Option<ProbeSample>>> + Send + 'a>>`.
  - `pub trait ProbeTransport { fn probe_profile<'a>(&'a self, profile: &'a Profile, services: &'a [ServiceProfile], settings: &'a Settings, ctx: ProbeContext<'a>) -> ProbeFutureOne<'a>; }`
  - `pub trait ProbeProgress { fn on_server(&self, event: &ProbeServerEvent); fn on_progress(&self, tested: usize, total: usize); fn on_cancelled(&self); fn on_finished(&self, outcome: &ProbeOutcome); }`
  - `pub struct ProbeServerEvent { profile_id, status: ProbeServerStatus, latency_ms: Option<u64>, service_results: Vec<ProbeServiceResult> }`; `ProbeServerStatus::{Testing, Done, Failed}` serialized lowercase. `ProbeServiceResult` derives `Serialize` with camelCase.
  - `pub struct PersistOptions { pub sort: bool, pub cancelled: bool }`.
  - `ProbeOutcome { tested, sorted, finished_at, cancelled: bool }`.
  - `pub async fn probe_cycle_detached(profiles, services, settings, transport, progress, cancel: watch::Receiver<bool>) -> Result<Vec<ProbeSample>>`.
  - `pub fn persist_probe_samples(conn, samples, progress, options: PersistOptions) -> Result<ProbeOutcome>`.
  - `AppState::start_probe() -> watch::Receiver<bool>` (reset cancel flag, set running true) and `AppState::finish_probe()`.
  - `pub async fn probe_cancel(state: State<'_, AppState>) -> bool` (`true` = cancellation accepted), registered in `command_list!`.
  - `run_impl(state: &AppState, apply_reconnect: bool, cancel: watch::Receiver<bool>)`.
  - Contracts: `EVENTS.probeServer = "probe-server"`, `EVENTS.probeCancelled = "probe-cancelled"`, `ProbeServerEventSchema`, `ProbeResultSchema.cancelled` default `false`.

- [ ] **Step 1: Write failing cycle tests**

```rust
#[tokio::test]
async fn parallel_cycle_caps_concurrency_and_isolates_failures() {
    let mut conn = seeded_conn(); // 25 profiles, as existing helper pattern
    let settings = Settings { test_concurrency: 3, ..Settings::default() };
    let transport = CountingTransport::new(); // tracks max in-flight, fails p-1
    let (tx, rx) = watch::channel(false);
    let samples = probe_cycle_detached(&profiles, &services, &settings, &transport, &SilentProgress, rx).await.unwrap();
    assert_eq!(transport.max_in_flight(), 3);
    assert_eq!(samples.len(), 25);
    assert_eq!(samples.iter().find(|s| s.profile_id == "p-1").unwrap().latency_ms, None);
    assert!(samples.iter().filter(|s| s.profile_id != "p-1").all(|s| s.latency_ms.is_some()));
    let _ = tx;
}
```

Second test: cancellation after first completed profile; assert only completed samples persisted, `sort` was skipped (`sort_index` unchanged for untested profiles), `outcome.cancelled == true`, and every started core's stop counter is 1 with both `.json`/`.pid` removed. Third test: event order per profile is `testing` then `done|failed`, and `probe-progress` count reaches `total`.

- [ ] **Step 2: Run them to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml probe_cycle_detached`
Expected: FAIL — new signatures do not exist.

- [ ] **Step 3: Rewrite the probe cycle**

Replace `PROBE_BATCH_SIZE` batching and `build_speedtest_config` from the runtime path with `stream::iter(...).buffer_unordered(settings.test_concurrency.clamp(1,20))`. Each future: check `ctx.cancel.borrow()` before spawn and after port wait; emit `testing`; create `ProbeTempFiles`, write config + pid, spawn via `ChildGuard::spawn(snapshot, Some(ctx.job))`; `wait_for_port` with `XRAY_STARTUP_TIMEOUT_MS = 2000`; `measure_delay` + `check_service_profile`; stop core and delete both files in a cleanup scope equivalent to `finally` (RAII guard or explicit closure called on every exit path); return `Some(sample)` or `None` on cancel. Local failures convert to a failed sample instead of `Err`. `probe_cycle_detached` emits `on_server` and `on_progress` per completed profile. `persist_probe_samples` takes `PersistOptions`: when `sort == false` it still writes latency/service results but skips `profile_repo::sort_all`, sets `sorted: 0`; emits `on_finished` with `cancelled`. Update `TauriProgress` to the four new methods: `on_cancelled` emits `probe-cancelled` with payload `{}`. `probe_cycle_detached` calls `on_cancelled()` once, the first time it observes the flag true. `probe_cancel` checks `AppState`'s running flag first; returns `false` when idle and emits nothing. `probe_run` takes `probe_permit`, calls `start_probe`, `run_impl(&state, false, rx)`, then `finish_probe`; `auto_test_loop` calls `run_impl(&state, true, rx)`. `apply_reconnect` stays in `run_impl` only when the flag is true.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Run: `pnpm test`
Expected: PASS; command-name test updated with `"probe_cancel"`.

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri packages/contracts/src && git commit -m "feat: parallel per-server probe with cancellation and job-bound cores"`

---

### Task 5: Server selection and first-click Connect

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/connection.rs:17-32,264-279`
- Modify: `apps/desktop/src-tauri/src/commands/probe.rs:97-117,164-203`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:986-1066` (tests)
- Modify: `apps/desktop/src/stores/app.store.ts:19-53`
- Modify: `apps/desktop/src/features/server-table/ServerTable.tsx:11-70`
- Modify: `apps/desktop/src/features/connection-panel/ConnectionPanel.tsx:16-57`
- Modify: `apps/desktop/src/pages/Servers.tsx:62`
- Test: `apps/desktop/src-tauri/src/commands/mod.rs` tests, `apps/desktop/src/features/server-table/server-table.test.tsx`, `apps/desktop/src/features/connection-panel/connection-panel.test.tsx`

**Interfaces:**
- Consumes: `AppState`, `connect_impl`, `best_eligible_profile`, `profile_repo::list_present_enabled`.
- Produces: `connect_impl(state, Some(id))` unchanged lookup but no service gating; `connect_impl(state, None)` = `best_eligible_profile` → first row from `list_present_enabled()` ordered by `sort_index` → `NotFound`. `AppStore.selectedProfileId: string | null` + `setSelectedProfileId(id: string | null)`. `ServerTable` props `{ profiles, testingIds?, selectedId?, onSelect? }`; rows are clickable with `data-selected` and `role="button"`. `ConnectionPanel` passes `{ profileId: selectedProfileId }`.

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn connect_none_falls_back_only_to_enabled_subscriptions() {
    let conn = seeded_conn(); // no passing cached results
    // insert disabled subscription with profile p-disabled, sort_index = -1
    let chosen = connect_impl_without_launch(&conn, None).await.expect("fallback");
    assert_ne!(chosen.id, "p-disabled");
    assert_eq!(chosen.id, "p-fast"); // smallest sort_index among enabled present
}
```

If a launch-free helper is awkward, assert on a new pure function `pub fn select_connect_profile(conn: &Connection, profile_id: Option<String>) -> rusqlite::Result<Option<Profile>>` and call that from `connect_impl`. TS: render `ServerTable`, click the row with `data-profile-id="p-2"`, assert `onSelect` called with `"p-2"` and `data-selected="true"`; render `ConnectionPanel` with a selected id and assert `invoke` received `profileId: "p-2"`.

- [ ] **Step 2: Run them to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml connect_none`
Run: `pnpm test`
Expected: FAIL.

- [ ] **Step 3: Implement selection**

Add `select_connect_profile` in `connection.rs` and use it at the top of `connect_impl` (replace the inline `match profile_id`). For `Some(id)` keep `list_present_enabled` lookup. For `None` use `best_eligible_profile()` then `list_present_enabled()` sorted by `(sort_index, id)`. Update the existing `apply_reconnect` tests: they no longer take an outer permit from the caller because `apply_reconnect` acquires `ConnectionGuard` internally and returns `Ok(())` when busy; assert a held connection permit causes a skipped tick rather than a busy error. In UI: add selection state to `app.store`, wire `onSelect` through `Servers` → `ServerTable`, and pass `selectedProfileId` from `ConnectionPanel`. Replace both `busy` checks in `ConnectionPanel` with `connection_busy` → `"Connection operation in progress"` and keep other errors as `Connection failed` / `Disconnect failed`. Do not gate Connect on `probeTesting`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Run: `pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

`git add apps/desktop && git commit -m "feat: selectable servers and first-click connect with safe fallback"`

---

### Task 6: Testing indicators, Stop test, and subscription error codes

**Files:**
- Modify: `apps/desktop/src/stores/app.store.ts:19-128`
- Modify: `apps/desktop/src/pages/Servers.tsx:30-65`
- Modify: `apps/desktop/src/features/server-table/ServerTable.tsx:46-64`
- Modify: `apps/desktop/src/pages/Dashboard.tsx:85-99`
- Modify: `apps/desktop/src/features/subscription-list/SubscriptionList.tsx:69-77`
- Test: `apps/desktop/src/stores/app.store.test.tsx`, `apps/desktop/src/pages/Dashboard.test.tsx`, `apps/desktop/src/features/subscription-list/subscription-list.test.tsx`

**Interfaces:**
- Consumes: `EVENTS.probeServer/probeCancelled/probeFinished`, `ProbeResultSchema`, `commandErrorCode`.
- Produces: `AppStore.probeTesting: boolean`, `probeTestingIds: string[]`, `probeCancelled: boolean`, `setProbeServer(event)`, `setProbeCancelled()`, `setProbeFinished(result)`; `Servers` `Stop test` invokes `probe_cancel` with `z.boolean()`; `ServerTable` accepts `testingIds` and shows `Testing...` in State / `…` in Delay.

- [ ] **Step 1: Write failing TS tests**

```tsx
it("shows per-server testing and disables Run test", () => {
  useAppStore.getState().setProbeServer({
    profileId: "p-1", status: "testing", latencyMs: null, serviceResults: [],
  });
  render(<Servers />);
  expect(screen.getByTestId("server-row").textContent).toContain("Testing...");
  expect(screen.getByRole("button", { name: "Testing..." })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Stop test" })).toBeEnabled();
});
```

Second test: emit `probe-cancelled`, then `probe-finished({ tested: 3, sorted: 0, finishedAt, cancelled: true })`; assert `probeTesting === false`, `probeTestingIds` empty, `probeCancelled === true`, and notice contains `Test cancelled`. Third test: `refresh` failure with `{ mutation_busy: null }` shows `"Another operation is in progress. Try again shortly."`, `{ fetch: null }` shows `"Could not reach subscription URL"`, `{ not_found: null }` shows `"Subscription not found"`, `{ storage: null }` shows `"Local storage error"`.

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm test`
Expected: FAIL.

- [ ] **Step 3: Wire UI state and messages**

In `app.store.ts`, validate `probe-server` payload with `ProbeServerEventSchema`; `status: "testing"` adds the id and sets `probeTesting = true`, `done|failed` removes it; `probe-progress` sets counts; `probe-cancelled` sets `probeCancelled = true`; `probe-finished` clears `probeTesting`/ids and stores `cancelled`. In `Servers.tsx`, disable `Run test` while `probeTesting`, label it `Testing...`, render `Stop test` while testing, show `tested / total` whenever `probeTesting || total > 0`, and report cancellation from the returned `ProbeResult.cancelled`. In `ServerTable.tsx`, render `Testing...`/`…` for ids in `testingIds`. In `Dashboard.tsx`, show `Testing...` when `probeTesting`. In `SubscriptionList.tsx`, map error codes exactly as in the test above with a generic fallback.

- [ ] **Step 4: Run tests to verify they pass**

Run: `pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

`git add apps/desktop/src && git commit -m "feat: show live probe progress, cancellation and precise subscription errors"`

---

### Task 7: Docs, end-to-end checklist, and full verification

**Files:**
- Modify: `apps/desktop/src-tauri/AGENTS.md` (worktree `AGENTS.md` at repo root)
- Modify: `docs/e2e-checklist.md`
- Modify: `docs/superpowers/specs/2026-11-25-v2rayn-parity-connect-probe-design.md` (status → `accepted`)

**Interfaces:**
- Consumes: completed Tasks 1–6.
- Produces: updated operator docs and a passing full test run.

- [ ] **Step 1: Update docs**

In worktree `AGENTS.md`, update Status/Contracts/Verification: parallel mixed probe, `test_concurrency`, per-server events, Job Object reaping, distinct busy codes. In `docs/e2e-checklist.md`, add scenarios: fresh import → Connect works without test; Run test while connected → connection/proxy unchanged and rows show `Testing...`; second click → disabled; Stop test → cores gone, no `%TEMP%\mglaroute-probe-*` leftovers; refresh failure messages by code. Change SPEC status from `proposed` to `accepted`.

- [ ] **Step 2: Run full verification**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Run: `pnpm test`
Expected: PASS with zero failures. Capture both outputs.

- [ ] **Step 3: Manual smoke on real binary**

Run the desktop app with a real `xray.exe`. Confirm no orphaned `xray.exe` remains after a full test, after Stop test, and after app exit. This is the only step requiring the operator's machine; report results verbatim.

- [ ] **Step 4: Commit**

`git add AGENTS.md docs && git commit -m "docs: record v2rayN-parity connect and probe behaviour"`
