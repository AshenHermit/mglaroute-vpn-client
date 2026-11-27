# Release pipeline + in-app updater — design

Status: accepted. Date: 2026-11-26.

## Intent

Ship installable builds and let users update from inside the client. Three
connected deliverables:

1. One local command that produces a signed NSIS installer for Windows.
2. A GitHub Actions workflow that builds on `v*` tags and publishes installer,
   signatures and updater manifest to a GitHub Release.
3. Automatic update check in the client, a visible notice, and an `Install`
   button that performs the update in-app on desktop.

User explicitly required cross-platform readiness: Windows now, Linux and
Android later. The chosen shape must not paint us into a Windows-only corner.

## Goals

- `pnpm installer:build` builds an NSIS installer with bundled xray + anti-DPI
  assets, verifying their sha256 first.
- Pushing tag `v<version>` publishes a release with NSIS setup, `.sig` and
  `latest.json`; updater endpoint resolves to that release.
- Client checks for updates on startup and periodically; shows a notice when a
  newer version exists; user can install by button without leaving the app on
  Windows (and later Linux).
- Backend command surface stays the same shape across desktop/mobile; UI never
  branches on OS.
- Future Linux (AppImage) and Android can reuse the same contracts and UI.

## Non-goals

- No Linux, macOS or Android builds in CI yet (structure must allow adding
  them; no matrix implemented now).
- No self-install on Android. Mobile OS/store rules: update check may report a
  new version, but the action is "open releases page", not silent APK install.
- No third-party Android updater plugin now (keep a seam for one later).
- No changes to subscription/probe/connection logic, no TUN, no anti-DPI work.
- No forced/background silent updates; user always confirms with the button.
- No server-side update service; GitHub Releases is the only source of truth.

## Anti-goals

- Do not ship unsigned artifacts or disable signature verification.
- Do not forward native error text to the UI (it may include URLs or paths).
- Do not add durable user settings for update state unless needed — current
  store is session-only and SQLite migrations are costly.

## Constraints / findings

- Runtime: Node 20+, pnpm 9 + Nx; Rust MSRV currently 1.77.
  `tauri-plugin-updater` requires Rust >= 1.90 → bump `rust-version` to 1.90
  and use `dtolnay/rust-toolchain@stable` in CI.
- `apps/desktop/src-tauri/tauri.conf.json`: version 0.1.0, `bundle.targets:
  "all"`, `externalBin: ["binaries/xray"]`, `resources:
  ["binaries/antiddpi/**/*"]`. No updater config today.
- No `@tauri-apps/cli`, no updater/process JS plugin in dependencies.
  `apps/desktop/package.json` has `tauri:dev = cargo tauri dev`; root scripts
  own binary fetch/verify.
- `scripts/fetch-xray.mjs` + `scripts/stage-antiddpi.mjs` need network and
  produce gitignored binaries; `pnpm binaries:verify` fails without them.
  No real binary in this checkout.
- `apps/desktop/src-tauri/capabilities/default.json` grants only
  `core:event:default`. Updater permissions must be added for desktop.
- `lib.rs` `command_list!` has a test asserting the exact sorted command name
  list; new commands must be registered there and the test updated.
- `packages/contracts/src/schemas.ts` owns all Tauri payload shapes and the
  `EVENTS` map; `lib/invoke.ts` has a `SAFE_CODES` whitelist for error codes.
- `tauri-action@v1`: `projectPath` must NOT be gitignored (CI runs at repo
  root → `apps/desktop`); `uploadUpdaterJson` defaults true;
  `updaterJsonPreferNsis` must be true to make `latest.json` point at NSIS;
  signing private key comes from `TAURI_SIGNING_PRIVATE_KEY` (and optional
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
- Updater `latest.json` needs signed artifacts and inline public key in
  `plugins.updater.pubkey` (not a path). Signature check is mandatory.

## Approaches

1. **A — one Rust command layer + official Tauri updater (chosen).**
   Rust exposes `update_check` / `update_install`; contracts define one
   `UpdateInfo` shape; desktop implementation uses `tauri-plugin-updater` with
   signed artifacts. Linux later = same plugin with AppImage. Android later =
   same check, install action opens the release page. UI is identical on every
   platform. Trades off a Rust/plugin dependency and a key-management step for
   a single, secure, cross-platform contract.
2. B — JS updater plugin in the frontend. Fewer Rust changes, but pushes
   platform branching into React, needs two extra JS plugins, and duplicates
   error/payload handling. Rejected: conflicts with "UI platform-agnostic".
3. C — hand-rolled GitHub Releases polling + manual EXE download. No signing
   key or Tauri plugin, but no in-app install and weak integrity story.
   Rejected: user asked for install-by-button.

User approved A and the Rust bump.

## Parts of the how (decided)

### Rust backend

- `Cargo.toml`: `rust-version = "1.90"`; add `tauri-plugin-updater` gated to
  `cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))`.
- `lib.rs`: under `#[cfg(desktop)]`, register
  `tauri_plugin_updater::Builder::new().build()` in `setup`; add module
  `update`; register three commands in `command_list!`:
  - `update_check() -> Option<UpdateInfo>`: `None` means "no update"; network
    or config failure also resolves to `None` (startup check must be quiet).
  - `update_install() -> ()`: run the shared shutdown cleanup first (below),
    then `download_and_install` with an event callback. Windows install mode
    `passive`; the plugin quits the app before installing. On platforms that
    do not auto-restart, call `AppHandle::restart()` (or exit and ask the user
    to relaunch) only after cleanup has completed.
  - `update_open_releases(url: String) -> ()`: Rust-side `shell::open` on the
    `releaseUrl` from `UpdateInfo` for `canInstallInApp: false`
    (Android/fallback). Rust validates the GitHub releases tag prefix before
    opening. Needed because `window.open` in the WebView is not guaranteed to
    reach the system browser.
- New `src/commands/update.rs` + re-export in `commands/mod.rs`. Errors use
  `CommandError` additions: `UpdateBusy` (`update_busy`) for a double install
  click, `Update` (`update`) for install/open failure; no raw plugin error
  text. Both codes added to `lib/invoke.ts` `SAFE_CODES`.
- Cleanup before update (my call): `lifecycle::shutdown()` currently owns the
  only safe teardown (`disconnect_impl()` → proxy restore, xray stop, temp
  config removal, gated by `SHUTTING_DOWN`). Factor a shared "cleanup then
  terminal action" helper so tray `Exit` and `update_install` use the same
  path. `AppHandle::restart()` must never bypass it. Install must not fight a
  live connection/probe: wait for or cancel in-flight guards before teardown.
- Backend seam: a small platform abstraction (e.g. `UpdateBackend` trait /
  `#[cfg]` functions) so Android can later return `canInstallInApp: false`
  without changing commands or UI. Do not add Android code now.
- `capabilities/default.json`: add `updater:default` (check/download/install)
  and `shell:allow-open` (release page fallback).
- `Cargo.toml`: add `tauri-plugin-shell` for the open-releases command; no new
  JS plugin (frontend still calls Rust commands only).
- Existing `command_names_registered` test updated for the three new names.
  Unit tests: payload mapping, error-code mapping, double-install guard,
  `None` on check failure; keep the documented `TAURI_CONFIG` override for
  cargo tests without bundled xray.

### Contracts and frontend

- `packages/contracts/src/schemas.ts`: `UpdateInfoSchema` +
  `UpdateProgressEventSchema`; add `updateProgress` to `EVENTS`.
  `UpdateInfo` fields (all camelCase): `version`, `currentVersion`,
  `notes: string | null`, `pubDate: string | null`,
  `canInstallInApp: boolean`, `releaseUrl: string`. Rust computes
  `releaseUrl` from the tag (`v<version>`) and sets `canInstallInApp` from the
  platform backend.
- `UpdateProgressEvent` fields: `phase: "downloading" | "installing" | "done"`,
  `received: number`, `total: number | null`.
- `app.store.ts`: session-only `update: UpdateInfo | null`,
  `updateInstalling: boolean`, `updateProgress: UpdateProgressEvent | null`,
  setters. No SQLite change, no `user_version` migration, no persisted
  "skip this version" (my call: session-only matches existing store).
- Check cadence (my call): once on app mount and every 6 hours while running;
  no retry storm on failure.
- Notice (my call): new `UpdatePanel` in `Settings.tsx` (new "Updates" tab)
  shows current version, available version, notes, and the install action.
  A sonner toast via existing `NoticeToaster` announces a newly detected
  update; the panel remains the durable-in-session place with the button.
  Install button calls `update_install` when `canInstallInApp`, otherwise
  calls `update_open_releases` (Android/fallback). Show progress from
  `updateProgress` and disable the button while `updateInstalling`.
- No new Tauri JS plugin needed: Rust handles download/install/restart and
  opening the release page.

### Build + CI

- Add `@tauri-apps/cli` to `apps/desktop` devDependencies. Root `package.json`
  gets `installer:build`:
  `pnpm binaries:fetch && pnpm binaries:verify && pnpm --filter @mglaroute/desktop exec tauri build --bundles nsis`.
  Prerequisite: signing key env present (see below); local dev builds without
  bundling (`pnpm tauri:dev`) stay unaffected.
- `.github/workflows/release.yml`: trigger `push.tags: ["v*"]`;
  `permissions: contents: write`; `windows-latest`; steps checkout@v4,
  pnpm/action-setup@v4 (9), setup-node@v4 (20 + pnpm cache), rust stable,
  `pnpm install --frozen-lockfile`, `pnpm binaries:fetch`,
  `pnpm binaries:verify`, `tauri-apps/tauri-action@v1` with
  `projectPath: apps/desktop`, `tagName: v__VERSION__`,
  `releaseName: mglaroute v__VERSION__`, `generateReleaseNotes: true`,
  `args: --bundles nsis`, `updaterJsonPreferNsis: true`,
  `uploadUpdaterJson: true`, `uploadUpdaterSignatures: true`,
  `uploadWorkflowArtifacts: true`; env `GITHUB_TOKEN` and signing secrets.
- Version source of truth: `tauri.conf.json` version must match the `v*`
  tag; no auto-bump in this pass. CI pins `XRAY_VERSION` via repo variable if
  desired (default: script resolves latest); pinning recommended for
  reproducible releases.
- Key management (operator step): `cargo tauri signer generate -w
  ~/.tauri/mglaroute.key`; commit the public key inline in
  `plugins.updater.pubkey`; set repo secrets
  `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
  `plugins.updater.pubkey` still holds `PUBLIC_KEY_PLACEHOLDER`: operator
  replaces it with the generated public key before the first signed release.
  No private key in repo.
- `tauri.conf.json`: `bundle.createUpdaterArtifacts: true`,
  `plugins.updater.windows.installMode: "passive"`,
  `plugins.updater.endpoints` =
  `https://github.com/AshenHermit/mglaroute-vpn-client/releases/latest/download/latest.json`.
- `docs/e2e-checklist.md`: manual pass for update notice, progress, install,
  restart, and no proxy/core left behind; `AGENTS.md` status + contracts
  updated when behaviour lands.

### Tests

- TS: Zod schema round-trips, store setters, update panel renders with mocked
  `invoke` (Vitest); no real network.
- Rust: command names register, error codes, `None` on check failure,
  install guard, progress event mapping.
- CI workflow validated by dry tag push after secrets are set (operator).

## Builder-check decisions (my calls)

- Teardown before install: shared lifecycle helper, described above. No
  restart path may skip proxy/core cleanup.
- Updater network policy: do not force `.no_proxy()` for the updater client.
  Use the plugin's default OS/proxy environment so traffic follows the active
  VPN; forcing a bypass would expose the user's real IP. A failed check stays
  silent (`None`) and is retried at the next interval.
- Release page: Rust `shell::open` via `update_open_releases`, not WebView
  `window.open`. Contract and capability added now even though Android UI work
  is deferred.

## Left to the builder

- Exact `tauri-plugin-updater` API signatures and event callback names in the
  installed crate version; adjust if they differ from docs.
- Whether `AppHandle::restart()` is available in the resolved Tauri patch
  version; if not, fall back to `app.exit(0)` and document that user relaunches.
- Exact `latest.json` platform key produced on windows-x86_64 CI runner
  (expected `windows-x86_64`); verify against generated file after first
  release.
- Generated public key value replaces the documented placeholder before the
  first release; private key stays only in GitHub Secrets.
- Linux CI/AppImage matrix and Android release-page seam are intentionally
  deferred; keep code paths cfg-gated so adding them needs no UI rewrite.

## Delivered notes (2026-11-26)

- Release wiring landed: `@tauri-apps/cli` in desktop devDependencies, root
  `installer:build`, and `.github/workflows/release.yml` with the inputs above.
  `scripts/release-config.test.mjs` verifies the wiring as text, no YAML dep.
- `plugins.updater.pubkey` still holds `PUBLIC_KEY_PLACEHOLDER`; operator must
  generate the keypair, commit the public key and set the two signing secrets
  before the first signed release. No private key committed.
- Windows terminal progress (`done`) is best-effort: the updater may exit before
  the final event or `app.restart`. The UI clears its busy flag in `finally`, so
  failed installs remain retryable.
- Manual pass for update notice, install/cleanup and the release-page fallback:
  `docs/e2e-checklist.md` steps 16–17. `installer:build` itself is an operator
  smoke test (network + signing env), not a CI-unit step.
