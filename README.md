# mglaroute-vpn-client

Cross-platform VPN client for the mglaroute ecosystem. Companion to `mglaroute-vpn-server`.

## Status

Windows desktop MVP implemented: subscription import by URL, service profiles,
delay/service probing, auto-sort, auto-reconnect with cooldown, system-proxy
connect/disconnect with restore-on-crash and startup restore. Linux, Android and
iOS are deferred. Design: `docs/superpowers/specs/2026-11-24-mglaroute-vpn-client-design.md`.
Manual end-to-end pass: `docs/e2e-checklist.md`.

Release + in-app updater pass implemented: signed NSIS installer via
`pnpm installer:build`, tag-triggered GitHub Actions release publishing setup,
`.sig` and `latest.json`, update check on startup and every 6 h, and an
`Updates` settings tab with Install / Open release page actions. Design (status
`accepted`): `docs/superpowers/specs/2026-11-26-release-updater-design.md`.

## Stack

- pnpm workspace + Nx, Node 20+
- Desktop: Tauri v2 (Rust), Vite, React 18, TypeScript strict
- UI: shadcn-style local Radix wrappers + Tailwind 3
- State: Zustand 5 (preferences only); durable data in SQLite
- Rust: tauri commands, rusqlite, reqwest, serde, tokio

## Layout

- `apps/desktop` — React app and Tauri shell
- `packages/contracts` — shared TS types/Zod schemas for Tauri payloads
- `packages/ui` — shared shadcn primitives
- `scripts` — build-time binary fetch and licence staging

## Development

```sh
pnpm install
pnpm dev
pnpm test
pnpm build
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Nx daemon can disconnect in this environment: use
`$env:NX_DAEMON='false'; pnpm test` and `$env:NX_DAEMON='false'; pnpm build`
(PowerShell; on POSIX use `NX_DAEMON=false`). The `Cannot switch to pnpm@9`
warning and zod `@__PURE__` annotation warnings are pre-existing.

`cargo test` runs `tauri_build`, which validates `bundle.externalBin` paths
before any test runs. With no binary in `apps/desktop/src-tauri/binaries/`, the
plain command fails with `resource path 'binaries/xray-x86_64-pc-windows-msvc.exe'
doesn't exist`. Unit tests use a fake launcher, so a test-only bundler override
works without a real binary:

```sh
$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## Binaries

Desktop binaries (xray, anti-DPI) are bundled at build time, never downloaded on
first run. `binaries:fetch` downloads pinned/released xray assets with `.dgst`
sha256 verification and stages anti-DPI assets and licence notices.
`binaries:verify` checks the staged binary checksum without fetching.

```sh
pnpm binaries:fetch
pnpm binaries:verify
```

`XRAY_BINARY_PATH` overrides the xray binary location for development
(`core::supervisor::binary_path`: env → override → bundled). No real xray binary
is present in this checkout: `pnpm binaries:verify` fails with
`ENOENT ... apps\desktop\src-tauri\binaries\xray.exe.sha256`. Fetching is deferred
to a real build machine with network access; `docs/e2e-checklist.md` requires that
real binary.

## Releases and updates

`pnpm installer:build` produces a signed NSIS installer locally. Prerequisites:
network access for `binaries:fetch` (or an already staged checkout), bundled
binaries that pass `binaries:verify`, and the Tauri signing env
`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The command
runs `binaries:fetch`, `binaries:verify`, then
`pnpm --filter @mglaroute/desktop exec tauri build --bundles nsis`. This is an
operator smoke test, not a CI-unit step.

To publish: generate the keypair with
`cargo tauri signer generate -w ~/.tauri/mglaroute.key`, commit the public key to
`plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json` (currently
`PUBLIC_KEY_PLACEHOLDER` until supplied), and set repo secrets
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Then push a
`v<version>` tag matching the `tauri.conf.json` version.
`.github/workflows/release.yml` builds on `windows-latest`, signs the NSIS
setup, and publishes the setup, `.sig` and `latest.json`;
`updaterJsonPreferNsis: true` makes `latest.json` point at the NSIS asset with
platform key `windows-x86_64`. `XRAY_VERSION` can be pinned as a repo variable
for reproducible builds.

The client checks `latest.json` on startup and every 6 h. The `Updates` tab
shows the available version and notes, installs in-app on Windows, and offers
**Open release page** where in-app install is unavailable. Terminal `done`
progress is best-effort: the Windows updater may exit before the final event is
delivered.

## Known limitations

- No TUN mode — system proxy only (per-user WinInet registry; no admin).
- No functional anti-DPI — bundled engine assets, licence notices and disabled
  command stubs only; `anti_dpi_status` always reports `unsupported`.
- No Clash YAML / sing-box / v2ray JSON full-config import.
- No `format=json` feed import — subscription text only.
- RAS-entry proxy iteration skipped — per-user WinInet registry only, no
  `RasEnumEntriesW` or dial-up proxy restore.
- Linux desktop, Android and iOS deferred — Windows desktop only in this build.
- No credential login to the server panel; public subscription URLs only.
- No `subscription-userinfo` traffic/expiry parsing.
