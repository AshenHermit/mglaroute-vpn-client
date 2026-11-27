# Launch optimization — design (short)

Status: proposed. Date: 2026-11-27.

## Intent

Release build: the window appears immediately but stays unresponsive for
~5 s; the working UI appears only after that. Optimize time from executable
start to usable interface. `pnpm dev` startup is out of scope.

Tauri creates the window before `.setup()` runs, but the WebView cannot
respond until setup returns. Today `commands::bootstrap` runs synchronously
inside setup, and its orphan reaper spawns one blocking `tasklist` subprocess
per stale `mglaroute-probe-*.pid` sidecar (101 sidecars / 11 unique PIDs on the
dev machine, ~60–135 ms each, plus a ~320 ms PowerShell CIM call for matching
PIDs). That is the dominant startup cost. The remaining setup work (SQLite
migration, proxy restore, updater/shell plugins, tray) is small but must stay
correct.

## Goals

- Move orphan reaping off the `.setup()` critical path so the window becomes
  responsive as soon as setup returns.
- Make reaping itself cheap and bounded: no subprocess per sidecar, no
  subprocess at all in the common case (no live probe cores).
- Preserve the recycled-PID safety rule: kill only after image name **and**
  command-line/config-file verification. Never log config contents, URLs or
  credentials.
- Add opt-in startup phase timing to measure a real release build.
- Remove duplicated startup IPC (`profile_list` fetched by both `AppShell` and
  `Servers`) and defer the mount-time update check so it does not compete with
  initial data hydration.

## Non-goals

- No change to connect/disconnect, proxy capture/restore semantics, auto-connect
  policy, scheduler intervals, release pipeline, signing, or anti-DPI stubs.
- No TUN, no new commands, no persisted settings change.
- No attempt to reproduce a full cold-start benchmark in this checkout: no
  bundled xray binary or built installer is available here.

## Anti-goals (must not happen)

- Killing a live xray core whose PID was recycled or whose config does not
  match the sidecar.
- Reaping a probe core started **after** this process launched (the reaper now
  runs in the background, so it must only touch sidecars that predate launch).
- Leaving the UI blocked on network or subprocess work.

## Constraints

- Windows is the shipping target; non-Windows builds keep no-op behavior.
- SQLite bootstrap, Job Object creation, proxy restore, plugins and tray stay on
  the synchronous path: commands and connection state must be valid before the
  WebView can invoke them. Probe cores bind their own loopback proxy and never
  touch the system proxy, so moving reap after proxy restore is safe.
- Existing tests pin reaping behavior and update-check timing; they must be
  extended, not weakened.
- `cargo test` needs `TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'`
  and `-- --test-threads=1`.

## Approaches considered

1. **Fast reaper in setup.** Replace per-PID `tasklist` with Win32 process
   liveness (`OpenProcess` + `GetExitCodeProcess`/`WaitForSingleObject`) and
   Win32 image-name check; dedup PIDs; delete sidecars whose PID is dead without
   spawning anything. Small change, removes most of the 5 s, but setup still
   waits for the remaining work.
2. **Deferred reaper.** Keep bootstrap minimal and spawn reaping in a tokio
   blocking task after `app.manage`. Window becomes responsive immediately;
   cleanup runs in parallel. Requires a launch-time cutoff so a freshly
   started probe is never reaped.
3. **Frontend only.** Dedupe IPC, lazy-load tabs, defer update check. Helps the
   table populate but cannot fix "window not responding".

Chosen: **1 + 2** (operator decision). The reaper becomes fast *and* moves to
the background; cheap frontend fixes come along.

## How it will work

### Rust

- `core/job.rs`
  - Keep `reap_orphans_with(temp_dir, verify, kill)` as a thin wrapper so
    existing tests stay valid; add
    `reap_orphans_with_liveness(temp_dir, alive, verify, kill, cutoff)`.
  - Collect `mglaroute-probe-*.pid` files into a `HashMap<u32, Vec<(pid_path,
    config_path)>>`; one liveness check and one verification per unique PID.
  - Ignore sidecars modified at/after process launch (`cutoff`), so a probe
    started after launch is never touched.
  - Dead PID: remove its sidecar files, no kill. Live PID: verify image name
    via `QueryFullProcessImageNameW`, then command line via the existing
    PowerShell CIM script only for those rare live xray candidates; kill via
    `taskkill /PID /T /F` only on full match, then remove sidecars. Failed
    verification leaves sidecars in place.
  - `windows-sys` already a dependency; no new crates.
- `commands/mod.rs`: remove `reap_orphans` from synchronous `bootstrap`; update
  the comment to explain why deferred cleanup is safe.
- `lib.rs`: capture process start time, after `app.manage` spawn
  `handle.spawn_blocking(...)` running the reaper with that cutoff. Errors from
  reaping stay non-fatal (log-free, best-effort) as today.
- Opt-in timing: when `MGLAROUTE_STARTUP_TRACE=1`, print `Instant` deltas for
  runtime ready, bootstrap done, plugins done, tray done, setup done. No
  request data in output. Document the flag in `AGENTS.md`.

### Frontend

- `AppShell.tsx` becomes the single owner of startup `profile_list`; on failure
  it sets the existing safe notice ("Could not load servers"). `Servers.tsx`
  drops its duplicate mount fetch and keeps its post-probe re-fetch.
- `use-update-check.ts` moves the first check off mount onto a short timer,
  keeping the 6 h interval and the one-announcement-per-version rule; cleanup
  clears both timers.

## Tests

- Rust: extend reaping tests for dead-PID cleanup, duplicate-PID dedup (verify
  called once), cutoff ignoring fresh sidecars, and "failed verification keeps
  sidecars". Keep the existing verified-kill test semantics.
- Frontend: assert one `profile_list` mount call across shell + page; update
  update-check tests for deferred first call with fake timers.
- Full suites (`pnpm test`, cargo test as documented) must stay green.

## What is left to the builder

- Exact Win32 API wiring and the timeout/cutoff constant choice.
- Whether the deferred check delay is 1 s or 2 s (noise-free vs freshness).
- Operator verification on a real release build with the trace flag enabled.
