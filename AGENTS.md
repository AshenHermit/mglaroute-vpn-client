# mglaroute-vpn-client

Cross-platform VPN client for the mglaroute ecosystem. Companion to `../mglaroute-vpn-server`.

## Status

Windows desktop MVP implemented. Subscription import by URL, service profiles,
real delay + service probing, auto-sort, auto-reconnect with cooldown, system
proxy connect/disconnect with crash + startup restore. Linux, Android and iOS
deferred. Design: `docs/superpowers/specs/2026-11-24-mglaroute-vpn-client-design.md`.
Plan: `docs/superpowers/plans/2026-11-24-mglaroute-vpn-client-mvp.md`.
Manual pass: `docs/e2e-checklist.md`.

v2rayN-parity connect + probe pass implemented on top of the MVP: first-click
connect from a fresh import (explicit row selection, cache-free fallback),
independent connection/probe/mutation domains with distinct busy codes, parallel
mixed probe capped by `test_concurrency`, live per-server events, and no orphaned
xray cores (Windows Job Object + startup sidecar reaping). Design (status
`accepted`): `docs/superpowers/specs/2026-11-25-v2rayn-parity-connect-probe-design.md`.
Plan: `docs/superpowers/plans/2026-11-25-v2rayn-parity-connect-probe.md`.

Tray lifecycle pass implemented: closing the window hides it while the process
stays resident; left-click tray icon restores/focuses the window; right-click
menu has `Open` and `Exit`, where `Exit` disconnects first (proxy restored, xray
stopped, temp config removed) and then exits the process. Design (status
`accepted`): `docs/superpowers/specs/2026-11-26-tray-lifecycle-design.md`.
Plan: `docs/superpowers/plans/2026-11-26-tray-lifecycle.md`.

Release + in-app updater pass implemented: `pnpm installer:build` (fetch/verify
binaries, then `tauri build --bundles nsis`), tag-triggered
`.github/workflows/release.yml` publishing signed NSIS setup + `.sig` +
`latest.json`, startup/6 h update check and an `Updates` tab with Install /
Open release page actions. Design (status `accepted`):
`docs/superpowers/specs/2026-11-26-release-updater-design.md`.

## Stack (fixed by operator)

- pnpm workspace + Nx. Node 20+.
- Desktop: Tauri v2 (Rust), Vite, React 18, TypeScript strict.
- UI: shadcn-style local Radix wrappers + Tailwind 3.
- State: Zustand 5 with `persist` only for non-secret UI preferences; durable data lives in SQLite.
- Rust side: `tauri` commands, `rusqlite` (SQLite), `reqwest`, `serde`, `tokio`. No extra web framework.
- Target: desktop Windows now; Linux desktop later. Android later; iOS after Android.
  All OS-specific behaviour behind Rust traits so mobile can reuse core logic.

## Layout

```
apps/desktop/
  src/                    React app: pages, features, shadcn ui, Zustand stores
  src-tauri/
    src/commands/         thin Tauri command surface
    src/core/             config snapshot builder, xray process supervisor
    src/subscription/     fetch + decode + upsert
    src/probe/            real delay + service checks
    src/storage/          SQLite schema + repositories
    src/platform/         PlatformProxy trait, Windows impl
    binaries/             bundled xray + anti-DPI engine assets (gitignored)
packages/contracts/       shared TS types/Zod schemas for Tauri payloads
packages/ui/              shared shadcn primitives
scripts/                  build-time binary fetch + license staging
docs/superpowers/specs/   design docs
```

## MVP scope

1. Desktop app (Windows) with import of subscriptions by URL.
2. Online-service profiles: named list of test URLs, add/edit/delete/enable/disable.
   Mirrors server entity: `name` (unique), `testUrls[]`, `isEnabled`, `isDefault`.
3. Per-subscription activation/deactivation + periodic auto-refetch. Subscription
   rows expose enable switch + per-row refetch interval (minutes) via
   `subscription_update`; fresh rows default to 15. Global settings
   `refetchIntervalMinutes` remains in storage/contract but is not shown in UI.
4. Periodic auto-test: real delay per server + reachability of all URLs of each active
   service profile through the same proxy; auto-sort by delay (failed last).
   Tests are a parallel mixed probe (one temporary core per server, up to
   `test_concurrency` at once) and run independently of a live connection.
5. Auto-reconnect: after each test cycle switch to best server if current is not best,
   with cooldown. Only servers passing all active service profiles are eligible.
6. connect/disconnect using system proxy on Windows (WinInet + registry, no admin).
   Restore previous proxy state on disconnect. TUN is a later feature.

## Not in MVP (current limitations)

- No TUN mode — per-user WinInet registry proxy only, no admin or driver.
- No functional anti-DPI. Engine binaries, licence notices and disabled stubs
  ship in builds; every `anti_dpi_*` command reports `unsupported`. Windows:
  winws2.exe + WinDivert driver (admin/UAC). Linux: nfqws2 built locally +
  nftables NFQUEUE (root). zapret2/Igareck catalogue has no domain-list input;
  only strategy preset, adapter, TCP port scope. GPL-3.0 wrapper; WinDivert
  LGPL/GPL — preserve all notices.
- No Clash YAML / sing-box / v2ray JSON full-config import (v2rayN hands these
  to other cores).
- No `format=json` feed import — subscription text only.
- RAS-entry proxy iteration skipped — no `RasEnumEntriesW`, no dial-up proxy
  restore.
- Linux desktop, Android and iOS deferred — Windows desktop only.
- No `subscription-userinfo` traffic/expiry parsing (v2rayN has none).
- No credential login to the server panel; client imports public subscription URLs only.

## Verification

- `pnpm test` runs Nx projects plus `vitest run scripts` (sha256/asset suites);
  exit code is meaningful. `pnpm build`: set `NX_DAEMON='false'` (Nx daemon flake).
  `Cannot switch to pnpm@9` and zod annotation warnings are pre-existing.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`: plain command
  needs a bundled binary (tauri-build validates `externalBin`). Tests use a fake
  launcher; test-only `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'`
  skips the sidecar check without a real binary. Append `-- --test-threads=1`:
  the process-global `EVENT_SINK` in `state.rs` makes a parallel run flaky
  (`connect_port_in_use_returns_distinct_error_without_faulted_event`).
- `pnpm binaries:fetch` / `pnpm binaries:verify`: fetch/verify staged xray with
  `.dgst` sha256. `XRAY_BINARY_PATH` overrides the binary for dev. No real binary
  in this checkout — `binaries:verify` fails with `ENOENT ... xray.exe.sha256`;
  deferred to a real build machine with network access.
- Both suites pass. Tests cover guard independence (`probe_busy` never blocks
  `ConnectionGuard` and vice versa), concurrency cap, per-server event order,
  cancellation, connect selection, Job Object assignment and startup reaping.
  Manual smoke on a real binary (orphan check) is operator-only; see
  `docs/e2e-checklist.md`.
- `pnpm installer:build` needs network + signing env
  (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); operator
  smoke test only, not a CI-unit step. `pnpm version:set <version>` bumps the
  root/desktop packages, `tauri.conf.json`, `Cargo.toml` and `Cargo.lock`
  together; the release tag must equal `v<version>` from `tauri.conf.json`.
  `pnpm vitest run scripts/release-config.test.mjs scripts/set-version.test.mjs`
  checks the release wiring (scripts, CLI dep, workflow inputs, updater config,
  version sync).

## Contracts

- Desktop tray lifecycle: `CloseRequested` hides the window (process stays
  alive); tray left-click / `Open` shows and focuses `main`; tray `Exit` runs
  `disconnect_impl` then `app.exit(0)` behind an idempotent shutdown gate.
  No new Tauri commands — WebView state is not on the exit path.
- Update pipeline: `bundle.createUpdaterArtifacts`, inline
  `plugins.updater.pubkey` (generated public key committed; private key only in
  GitHub Secrets), endpoint
  `https://github.com/AshenHermit/mglaroute-vpn-client/releases/latest/download/latest.json`,
  Windows `installMode: passive`. Tag `v<version>` must match `tauri.conf.json`.
  Secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`;
  `GITHUB_TOKEN` from Actions. `tauri-action` uses `uploadUpdaterJson: true`,
  `updaterJsonPreferNsis: true` so `latest.json` points at NSIS
  (`windows-x86_64`). Private key stays in GitHub Secrets, never in the repo.
- Update commands: `update_check` (quiet `None` on failure), `update_install`
  (shared shutdown cleanup → proxy restore, xray stop, temp config removal,
  then download/install), `update_open_releases` (Rust `shell::open`, validates
  GitHub release tag prefix). Progress event `update-progress` (`phase`,
  `received`, `total`); `update_busy` blocks a double install, `update` covers
  install/open failures with no raw plugin text. Terminal `done` progress is
  best-effort on Windows (updater may exit before the final event).
- Update state (`update`, `updateInstalling`, `updateProgress`) is session-only
  in Zustand; no SQLite change, no persisted "skip this version".
- Subscription decode is cascading best-effort, like v2rayN `AddBatchServers`:
  whole-body base64 (standard + URL-safe) → plain lines. MVP URI schemes:
  `vmess://` (legacy base64 JSON), `vless://`, `ss://` (SIP002 + legacy), `trojan://`.
  Bad lines skipped and counted. Dedup by exact link. `server_id` = stable hash of link.
- Real delay matches v2rayN: temporary xray core, `mixed` inbound on free loopback port,
  2 HTTP GETs to ping URL (default `https://www.google.com/generate_204`) through explicit
  loopback proxy, min positive ms. Service URL check = 2xx on **all** URLs of a profile,
  through same proxy session; no OS-proxy fallback. Probe is a **parallel mixed** run:
  one temporary core per server via `buffer_unordered(test_concurrency)`, independent
  of the live connection (running a test never touches system proxy or the working core).
- `test_concurrency` (settings/contract) defaults to `10` (v2rayN
  `MixedConcurrencyCount`), clamped `1..=20`; stored in `settings.test_concurrency`
  behind `PRAGMA user_version = 2` (v1 → v2 `ALTER TABLE` migration).
- Per-server probe events drive live row state: `probe-server`
  (`status: testing | done | failed | cancelled`, `latencyMs`, `serviceResults`)
  fires before spawn and after each result; `probe-progress` after each server;
  `probe-cancelled` on accepted Stop; `probe-finished` (`cancelled: bool`) at the
  end. A row is highlighted only for the exact `profileId` in a `testing` event;
  its terminal event removes the highlight and immediately patches `latencyMs`
  and `reachable` in the table. `cancelled` removes the highlight but keeps the
  previous result. Payload shapes live in `packages/contracts/src/schemas.ts`.
- Manual Run test sets only optimistic `probeTesting` on click (spinner, Stop
  test) and clears it in `finally`; it starts with `probeTestingIds: []` and
  never reads `test_concurrency`. A fast `probe_busy` or cycle error emits no
  per-server event, so this keeps visible feedback without lying about which
  cores actually run. The final `profile_list` re-fetch stays authoritative for
  persisted rows; `checkedAt` is not synthesized client-side. `probe-finished`
  keeps `cancelled`/`finishedAt` authoritative.
- Three independent guards emit distinct busy codes: `connection_busy`
  (connect/disconnect/apply_reconnect), `probe_busy` (probe_run/auto-test),
  `mutation_busy` (subscriptions/settings/refetch). `probe_busy` never blocks
  Connect. Stop test keeps partial samples but skips the global re-sort.
- No orphaned xray cores: every core joins a Windows Job Object
  (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) with a PID sidecar next to its temp config;
  startup reaping kills only PIDs verified as xray with a matching config path.
  `%TEMP%\mglaroute-probe-*` config/sidecar files are removed on stop/exit.
- Server feed is a valid import source: `GET /api/feed?format=subscription` (raw text,
  4 URI schemes). Feed is public, no JWT. `format=json` (scan metadata) consumption
  deferred — MVP imports subscription text only.
- xray + anti-DPI binaries are **bundled at build time**, not downloaded on first run.
  Build script fetches from GitHub releases with sha256 (`.dgst`) verification, then
  Tauri `externalBin` bundles them. `XRAY_BINARY_PATH` override for dev.
- SQLite tables: `subscriptions`, `profiles`, `service_profiles`,
  `profile_service_results`, `settings`. Test results never stored inside profile row.
- Never put subscription URLs, credentials or raw config into logs — or into
  probe PID sidecars/events (sidecars hold only a verified PID and config path).

## Conventions

- Terse output per `C:\Users\hermit\.config\opencode\AGENTS.caveman.md`.
- Keep server and client concerns separated; shared contracts documented at workspace root.
- Update this file and the spec when behaviour changes.
- Config generation and process launch must use one immutable snapshot, like v2rayN
  `CoreConfigContext`; no second source of truth.
