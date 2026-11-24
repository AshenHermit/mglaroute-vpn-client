# mglaroute-vpn-client

Cross-platform VPN client for the mglaroute ecosystem. Companion to `../mglaroute-vpn-server`.

## Status

Planned. No source code yet. Design: `docs/superpowers/specs/2026-11-24-mglaroute-vpn-client-design.md`.
Task-by-task plan to be written after spec approval.

## Stack (fixed by operator)

- pnpm workspace + Nx. Node 20+.
- Desktop: Tauri v2 (Rust), Vite, React 18, TypeScript strict.
- UI: shadcn-style local Radix wrappers + Tailwind 3.
- State: Zustand 5 with `persist` only for non-secret UI preferences; durable data lives in SQLite.
- Rust side: `tauri` commands, `rusqlite` (SQLite), `reqwest`, `serde`, `tokio`. No extra web framework.
- Target: desktop Windows now; Linux desktop later. Android later; iOS after Android.
  All OS-specific behaviour behind Rust traits so mobile can reuse core logic.

## Layout (planned)

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
3. Per-subscription activation/deactivation + periodic auto-refetch.
4. Periodic auto-test: real delay per server + reachability of all URLs of each active
   service profile through the same proxy; auto-sort by delay (failed last).
5. Auto-reconnect: after each test cycle switch to best server if current is not best,
   with cooldown. Only servers passing all active service profiles are eligible.
6. connect/disconnect using system proxy on Windows (WinInet + registry, no admin).
   Restore previous proxy state on disconnect. TUN is a later feature.

## Not in MVP

- Anti-DPI service. Deferred. Engine binaries and licence notices still ship in builds;
  reserve `AntiDpiService` trait + disabled command stubs. Windows: winws2.exe +
  WinDivert driver (admin/UAC). Linux: nfqws2 built locally + nftables NFQUEUE (root).
  zapret2/Igareck catalogue has no domain-list input; only strategy preset, adapter,
  TCP port scope. GPL-3.0 wrapper; WinDivert LGPL/GPL — preserve all notices.
- TUN mode, iOS, Android UI.
- Clash YAML / sing-box / v2ray JSON full-config import (v2rayN hands these to other cores).
- `subscription-userinfo` traffic/expiry parsing (v2rayN has none).
- Credential login to the server panel; client imports public subscription URLs only.

## Contracts

- Subscription decode is cascading best-effort, like v2rayN `AddBatchServers`:
  whole-body base64 (standard + URL-safe) → plain lines. MVP URI schemes:
  `vmess://` (legacy base64 JSON), `vless://`, `ss://` (SIP002 + legacy), `trojan://`.
  Bad lines skipped and counted. Dedup by exact link. `server_id` = stable hash of link.
- Real delay matches v2rayN: temporary xray core, `mixed` inbound on free loopback port,
  2 HTTP GETs to ping URL (default `https://www.google.com/generate_204`) through explicit
  loopback proxy, min positive ms. Service URL check = 2xx on **all** URLs of a profile,
  through same proxy session; no OS-proxy fallback.
- Server feed is a valid import source: `GET /api/feed?format=subscription` (raw text,
  4 URI schemes). Feed is public, no JWT. `format=json` (scan metadata) consumption
  deferred — MVP imports subscription text only.
- xray + anti-DPI binaries are **bundled at build time**, not downloaded on first run.
  Build script fetches from GitHub releases with sha256 (`.dgst`) verification, then
  Tauri `externalBin` bundles them. `XRAY_BINARY_PATH` override for dev.
- SQLite tables: `subscriptions`, `profiles`, `service_profiles`,
  `profile_service_results`, `settings`. Test results never stored inside profile row.
- Never put subscription URLs, credentials or raw config into logs.

## Conventions

- Terse output per `C:\Users\hermit\.config\opencode\AGENTS.caveman.md`.
- Keep server and client concerns separated; shared contracts documented at workspace root.
- Update this file and the spec when behaviour changes.
- Config generation and process launch must use one immutable snapshot, like v2rayN
  `CoreConfigContext`; no second source of truth.
