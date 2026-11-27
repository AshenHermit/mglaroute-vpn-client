# Launch Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut release-build time from executable start to usable UI by moving orphan reaping off Tauri's `.setup()` critical path and making the reaper cheap.

**Architecture:** Reaping becomes a deduplicated, Win32-based scan with a launch-time cutoff, spawned as a tokio blocking task after `app.manage`. SQLite/JobObject/proxy/plugins/tray stay synchronous. Frontend startup IPC is deduplicated and the update check is deferred by a short timer.

**Tech Stack:** Rust (Tauri 2, tokio, windows-sys 0.59, rusqlite), React 18 + TypeScript strict, Vitest 3, Nx/pnpm workspace.

**Spec:** `docs/superpowers/specs/2026-11-27-launch-optimization-design.md`

## Global Constraints

- Node 20+, pnpm workspace, Tauri v2, Windows desktop target. No new crates: `windows-sys` 0.59 already has `Win32_Foundation`, `Win32_System_Threading`, `Win32_Security`, `Win32_System_JobObjects`.
- `pnpm test` is the JS gate; `pnpm build` needs `NX_DAEMON='false'`.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1` with `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` is the Rust gate.
- Never log subscription URLs, credentials, raw config, or config paths. Update-check failures stay silent.
- Reaper must never kill a process without image-name and command-line/config verification; recycled PIDs and fresh post-launch probes are protected.
- Non-Windows builds keep no-op behavior. No persisted settings or API changes.

## Review Focus

- **Recycled/stale PID pointing at a live non-xray process:** must not kill and must leave sidecars for inspection → pinned in Task 1 by `reaping_only_kills_verified_xray` (existing) and the dead-PID/failed-verification tests.
- **Sidecar written after this process launched (user starts a probe immediately):** background reaper must ignore it → Task 1 `reaping_ignores_sidecars_newer_than_cutoff`.
- **Several sidecars sharing one PID:** verify and kill once, remove all → Task 1 `reaping_deduplicates_pid_and_cleans_all_sidecars`.
- **No sidecars at all (common case):** zero subprocesses, zero verifier calls → Task 1 `reaping_empty_dir_calls_no_verifier`.
- **Update check deferred but not lost:** exactly one first call after the delay, one announcement per version, clean timer teardown → Task 3 updated `use-update-check.test.tsx` cases.

---

### Task 1: Fast, bounded orphan reaper

**Files:**
- Modify: `apps/desktop/src-tauri/src/core/job.rs:124-218`
- Test: `apps/desktop/src-tauri/src/core/supervisor.rs:312-355`

**Interfaces:**
- Consumes: `crate::core::supervisor::apply_hidden_console` (existing), `windows-sys` APIs listed in Step 3.
- Produces:
  - `reap_orphans_with_liveness(temp_dir: &Path, alive: impl FnMut(u32) -> bool, verify: impl FnMut(u32, &Path) -> bool, kill: impl FnMut(u32), cutoff: Option<SystemTime>) -> io::Result<usize>` — returns verified kills only.
  - `reap_orphans(temp_dir: &Path, cutoff: Option<SystemTime>) -> io::Result<usize>` — production wrapper using Win32 helpers; same return semantics.
  - Existing `reap_orphans_with(temp_dir, verify, kill)` keeps its signature and old semantics (`alive` always true, no cutoff) for the two existing tests.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `apps/desktop/src-tauri/src/core/supervisor.rs`, next to the two existing reaping tests. Use `tempfile::tempdir()`, `std::cell::Cell`, `std::time::{Duration, SystemTime}`:

```rust
#[test]
fn reaping_dead_pid_cleans_sidecars_without_kill() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
    let config_path = dir.path().join("mglaroute-probe-42-1.json");
    std::fs::write(&pid_path, "4242").unwrap();
    std::fs::write(&config_path, "{}").unwrap();
    let killed = Cell::new(0);
    let count = reap_orphans_with_liveness(
        dir.path(),
        |_pid| false,
        |_pid, _config| panic!("dead PID must not be verified"),
        |_pid| killed.set(killed.get() + 1),
        None,
    ).unwrap();
    assert_eq!(count, 0);
    assert_eq!(killed.get(), 0);
    assert!(!pid_path.exists());
    assert!(!config_path.exists());
}

#[test]
fn reaping_deduplicates_pid_and_cleans_all_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("mglaroute-probe-42-1.pid");
    let second = dir.path().join("mglaroute-probe-42-2.pid");
    std::fs::write(&first, "4242").unwrap();
    std::fs::write(&second, "4242").unwrap();
    let verified = Cell::new(0);
    let killed = Cell::new(0);
    let count = reap_orphans_with_liveness(
        dir.path(),
        |_pid| true,
        |_pid, _config| { verified.set(verified.get() + 1); true },
        |_pid| killed.set(killed.get() + 1),
        None,
    ).unwrap();
    assert_eq!(count, 1);
    assert_eq!(verified.get(), 1);
    assert_eq!(killed.get(), 1);
    assert!(!first.exists());
    assert!(!second.exists());
}

#[test]
fn reaping_ignores_sidecars_newer_than_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
    std::fs::write(&pid_path, "4242").unwrap();
    let cutoff = SystemTime::now() - Duration::from_secs(3600);
    let verified = Cell::new(0);
    let count = reap_orphans_with_liveness(
        dir.path(),
        |_pid| true,
        |_pid, _config| { verified.set(verified.get() + 1); true },
        |_pid| panic!("fresh sidecar must not be killed"),
        Some(cutoff),
    ).unwrap();
    assert_eq!(count, 0);
    assert_eq!(verified.get(), 0);
    assert!(pid_path.exists());
}

#[test]
fn reaping_empty_dir_calls_no_verifier() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Cell::new(0);
    let count = reap_orphans_with_liveness(
        dir.path(),
        |_pid| { calls.set(calls.get() + 1); true },
        |_pid, _config| { calls.set(calls.get() + 1); true },
        |_pid| panic!("no processes to kill"),
        None,
    ).unwrap();
    assert_eq!(count, 0);
    assert_eq!(calls.get(), 0);
}
```

Keep the two existing tests (`reaping_only_kills_verified_xray`, `reaping_kills_and_cleans_verified_xray`) unchanged. The former already pins "failed verification leaves sidecars for inspection" through `reap_orphans_with`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reaping -- --test-threads=1`
Expected: compile failure — `reap_orphans_with_liveness` unresolved.

- [ ] **Step 3: Implement the reaper in `apps/desktop/src-tauri/src/core/job.rs`**

Add `use std::collections::HashMap`, `use std::path::PathBuf`, `use std::time::SystemTime`. Implement `reap_orphans_with_liveness`:
1. `read_dir(temp_dir)`; `NotFound` → `Ok(0)`.
2. Keep entries named `mglaroute-probe-*.pid`; skip when `Some(cutoff)` and `entry.metadata()?.modified()? >= cutoff`; skip unreadable/unparseable PID files.
3. Group by PID into `HashMap<u32, Vec<(PathBuf, PathBuf)>>` where the second path is `with_extension("json")`.
4. Per PID: call `alive(pid)` once. If false, remove every sidecar pair, no kill. If true, call `verify(pid, &first_config)` once; on false leave all sidecars; on true call `kill(pid)` once, remove all sidecar pairs, increment count.
5. Keep `reap_orphans_with` as a wrapper that calls the new function with `|_| true` for `alive`, the supplied `verify`/`kill`, and `None` for `cutoff`.

Implement Windows helpers:
- `fn is_probe_process_alive(pid: u32) -> bool`: return `true` when `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)` returns 0 (conservative), otherwise return false only when `WaitForSingleObject(handle, 0) == WAIT_OBJECT_0` or `GetExitCodeProcess(...) != STILL_ACTIVE`; always `CloseHandle`.
- `fn image_is_xray(pid: u32) -> bool`: open the process as above, call `QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size)` into a `vec![0u16; 32768]`, close the handle, compare the file name case-insensitively to `xray.exe`. Any failure → false.
- Rewrite `verify_probe_process(pid, config_path)`: `image_is_xray(pid)` first, then keep the existing PowerShell CIM script/closure (`Get-CimInstance Win32_Process -Filter "ProcessId={pid}"` → `.CommandLine`, `config_name` compared with `contains`, no interpolation of config text). Remove the old `tasklist` call entirely.
- `reap_orphans(temp_dir, cutoff)` calls the new function with `is_probe_process_alive`, `verify_probe_process`, the existing `taskkill /PID /T /F` closure, and the supplied `cutoff`.
- Keep `#[cfg(not(windows))]` variants returning `false`; no-op image/liveness behavior.

Exact imports: `windows_sys::Win32::Foundation::{CloseHandle, FALSE, WAIT_OBJECT_0}` and `windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, STILL_ACTIVE}`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml reaping -- --test-threads=1`
Expected: all six reaping tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/src/core/job.rs apps/desktop/src-tauri/src/core/supervisor.rs
git commit -m "perf: make startup orphan reaping fast and bounded"
```

### Task 2: Defer reaping and add opt-in startup timing

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs:18,310-339`
- Modify: `apps/desktop/src-tauri/src/lib.rs:58-92`
- Modify: `AGENTS.md` (Verification section)

**Interfaces:**
- Consumes: `reap_orphans(&Path, Option<SystemTime>) -> io::Result<usize>` from Task 1.
- Produces: no new public symbols; `MGLAROUTE_STARTUP_TRACE=1` env flag prints fixed phase labels plus elapsed time only.

- [ ] **Step 1: Remove reaping from synchronous bootstrap**

In `apps/desktop/src-tauri/src/commands/mod.rs`, drop `reap_orphans` from the `use crate::core::job::{...}` import and delete the `reap_orphans(...)` call at line 321. Replace the doc comment above `bootstrap` with: reaping is deferred to a background task because probe cores use their own loopback proxy and never touch the system proxy, so cleanup ordering relative to proxy restore is irrelevant. Keep `JobHandle::create()` and proxy restore in place.

- [ ] **Step 2: Spawn the reaper after state is managed**

In `apps/desktop/src-tauri/src/lib.rs`, add `use std::time::{Instant, SystemTime}`. At the top of `run()`, capture `let started = Instant::now();` and `let cutoff = SystemTime::now();`. Inside `.setup(...)` immediately after `app.manage(state.clone())`, spawn on the already-cloned `handle`:

```rust
let temp_dir = std::env::temp_dir();
handle.spawn_blocking(move || {
    let _ = crate::core::job::reap_orphans(&temp_dir, Some(cutoff));
});
```

Errors stay ignored: reaping is best-effort and error text may contain paths.

- [ ] **Step 3: Add opt-in phase timing**

Add a private helper in `lib.rs`:

```rust
fn trace_startup_phase(started: Instant, phase: &'static str) {
    if std::env::var_os("MGLAROUTE_STARTUP_TRACE").is_some() {
        eprintln!("startup {phase} {}ms", started.elapsed().as_millis());
    }
}
```

Call it at: runtime built (`"runtime-ready"`), after `commands::bootstrap` (`"bootstrap"`), after both plugins (`"plugins"`), after `lifecycle::setup_tray` (`"tray"`), and just before `Ok(())` (`"setup-done"`). No request data in output.

- [ ] **Step 4: Document the flag**

Add one line to `AGENTS.md` under Verification: with `MGLAROUTE_STARTUP_TRACE=1` the release exe prints fixed startup phase timings to stderr; no subscription data is printed.

- [ ] **Step 5: Verify**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1`
Expected: all tests PASS (100 unit + integration suites). Then run `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml` → PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/lib.rs AGENTS.md
git commit -m "perf: move startup reaping off the Tauri setup path"
```

### Task 3: Dedupe startup IPC and defer the update check

**Files:**
- Modify: `apps/desktop/src/app-shell/AppShell.tsx:19-43`
- Modify: `apps/desktop/src/pages/Servers.tsx:60-72`
- Modify: `apps/desktop/src/features/updates/use-update-check.ts:6-53`
- Test: `apps/desktop/src/app-shell/app-shell.test.tsx`, `apps/desktop/src/pages/Servers.test.tsx`, `apps/desktop/src/features/updates/use-update-check.test.tsx`

**Interfaces:**
- Consumes: `invoke`, `useAppStore` (existing).
- Produces: `UPDATE_CHECK_START_DELAY_MS` exported from `use-update-check.ts` (value `1000`); `useUpdateCheck()` first check fires after that delay, then every `UPDATE_CHECK_INTERVAL_MS`.

- [ ] **Step 1: Write failing frontend tests**

In `app-shell.test.tsx`, add a case with `mockedInvoke` rejecting `profile_list` with `new Error("native failure echoing https://secret.example")`: render `<AppShell />`, `await waitFor(() => expect(useAppStore.getState().notice).toBe("Could not load servers"))`, and assert the rendered text does not contain `secret.example`.

In the existing `app-shell.test.tsx` case `hydrates counts from subscription_list and profile_list without rendering secrets`, add `expect(mockedInvoke.mock.calls.filter(([command]) => command === "profile_list")).toHaveLength(1)` — `AppShell` mounts `Servers` by default, so this pins one startup call across shell + page.

In `Servers.test.tsx`, add: after `render(<Servers />)`, `expect(mockedInvoke.mock.calls.filter(([command]) => command === "profile_list")).toHaveLength(0)`, with the store already seeded in `beforeEach`.

In `use-update-check.test.tsx`, import `UPDATE_CHECK_START_DELAY_MS`, add a helper `const startChecks = async () => { await act(async () => { vi.advanceTimersByTime(UPDATE_CHECK_START_DELAY_MS); await Promise.resolve(); }); };`, call it after every `renderHook(() => useUpdateCheck())`, and add a new first test asserting `mockedInvoke` has 0 calls immediately after render and 1 call with command `"update_check"` after `startChecks()`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --filter @mglaroute/desktop exec vitest run src/app-shell/app-shell.test.tsx src/pages/Servers.test.tsx src/features/updates/use-update-check.test.tsx`
Expected: FAIL — missing notice, duplicate `profile_list` call, immediate update check.

- [ ] **Step 3: Make AppShell the single startup loader**

In `AppShell.tsx`, keep the `subscription_list` catch silent; change the `profile_list` catch to `if (active) setNotice("Could not load servers")`. No other change to hydration.

- [ ] **Step 4: Remove the duplicate mount fetch from Servers**

In `Servers.tsx`, delete the `useEffect` at lines 60-72 and keep the post-probe `profile_list` re-fetch in `runProbe`. Keep `ProfileSchema` imported (still used by `runProbe`) and keep the other `setNotice` uses.

- [ ] **Step 5: Defer the first update check**

In `use-update-check.ts`, export `export const UPDATE_CHECK_START_DELAY_MS = 1000;`. Replace the immediate `check()` with `const initialTimer = setTimeout(check, UPDATE_CHECK_START_DELAY_MS);` and keep `const timer = setInterval(check, UPDATE_CHECK_INTERVAL_MS);`. Clear both timers in cleanup. Preserve `displayed`/`announced` refs unchanged.

- [ ] **Step 6: Run tests to verify they pass**

Run: `pnpm --filter @mglaroute/desktop exec vitest run src/app-shell/app-shell.test.tsx src/pages/Servers.test.tsx src/features/updates/use-update-check.test.tsx`
Expected: all cases PASS. Then run `pnpm test` → PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/app-shell/AppShell.tsx apps/desktop/src/app-shell/app-shell.test.tsx apps/desktop/src/pages/Servers.tsx apps/desktop/src/pages/Servers.test.tsx apps/desktop/src/features/updates/use-update-check.ts apps/desktop/src/features/updates/use-update-check.test.tsx
git commit -m "perf: dedupe startup IPC and defer update check"
```

## Final verification (after all tasks)

- `$env:NX_DAEMON='false'; pnpm test` → green.
- `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --test-threads=1` → green.
- Operator-only on a real release build: run with `MGLAROUTE_STARTUP_TRACE=1`, compare `startup setup-done` against the previous ~5 s hang. This checkout has no bundled xray binary, so the end-to-end cold start cannot be reproduced here.
