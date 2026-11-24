# mglaroute-vpn-client

Cross-platform VPN client for the mglaroute ecosystem. Companion to `mglaroute-vpn-server`.

## Status

MVP in progress. Desktop (Windows) first; Linux, Android and iOS later.
Design: `docs/superpowers/specs/2026-11-24-mglaroute-vpn-client-design.md`.

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

Desktop binaries (xray, anti-DPI) are bundled at build time, never downloaded on first run.
