# System tray + safe exit — design

Status: proposed (small change). Date: 2026-11-26.

## Intent

Desktop client must stay resident when the user closes the window, expose a
system-tray icon, and offer a safe exit that leaves no system proxy override and
no xray processes behind. Reference: `SewerNet/apps/desktop/src-tauri/src/lifecycle.rs`.

## Goals

- Tray icon present while the app runs.
- Window close button (`CloseRequested`) hides the window, does not end the process.
- Left-click tray icon: show + focus the main window.
- Right-click tray icon: context menu with `Open` and `Exit`.
- `Exit`: stop the live core, restore the user's captured proxy, remove the temp
  config, then terminate the process. No orphaned cores.

## Non-goals

- No `Check updates` item (no updater in this client).
- No TUN, no anti-DPI change, no frontend UI exit button.
- No Linux/Android/iOS tray work now (Windows desktop only, behind Tauri APIs).

## Constraints / findings

- Tauri v2, `tauri` currently has no `tray-icon` feature — required.
- `icons/icon.ico` exists but `bundle.icon` is empty — must be set so a tray icon
  exists. Explicit window `label: "main"` avoids relying on the default.
- All cleanup already exists: `commands::connection::disconnect_impl(&AppState)`
  stops the child, restores `PreviousProxy`, removes the config path. It is
  async and takes no connection permit. `AppState` is managed in `setup`.
- Second safety net: every core joins a Windows Job Object with
  `KILL_ON_JOB_CLOSE` (dies with the process); `proxy_previous.json` is written
  on first `apply` and restored at next startup. Neither replaces explicit
  cleanup — they cover crashes and cleanup failure.
- No new Tauri commands: frontend contracts and the command-surface test stay
  unchanged.

## Approaches

1. **Rust-side exit (chosen).** Tray `Exit` handler spawns an async task:
   `disconnect_impl` → `app.exit(0)`. Independent of WebView state; works while
   the window is hidden. Add an `AtomicBool` shut-down gate so a double click
   cannot run cleanup twice.
2. Frontend round-trip (SewerNet style): emit `app-exit-requested`, frontend
   calls a command that cleans up and exits. Rejected: extra hop and a live
   WebView dependency for a safety-critical path.

## Parts of the how (decided)

- New `src/lifecycle.rs` in `src-tauri`:
  - `setup_tray(&mut tauri::App<R>)`: menu items `open`, `exit`;
    `.show_menu_on_left_click(false)`; left-click → show/focus; menu `open` →
    show/focus; menu `exit` → `begin_shutdown()` + spawn cleanup.
  - `hide_on_close(&Window, &WindowEvent)`: `CloseRequested` → `api.prevent_close()` + `window.hide()`.
  - `show_main_window_handle` helper.
  - `shutdown(AppHandle, AppState)` async: call `disconnect_impl`, ignore its
    error (log generic only), then `app.exit(0)` unconditionally.
  - `AtomicBool` gate, tested for idempotence.
- `lib.rs`: register module, `.on_window_event(lifecycle::hide_on_close)`,
  call `lifecycle::setup_tray(app)?` in `setup`.
- `Cargo.toml`: add `tray-icon` to `tauri` features.
- `tauri.conf.json`: explicit `label: "main"`, `bundle.icon: ["icons/icon.ico"]`.
- Tests (Rust, no GUI): tray action list, close→hide mapping, shutdown gate,
  source assertion that cleanup precedes `app.exit`. Run with documented
  `TAURI_CONFIG` override to skip the missing bundled xray.
- Manual pass (`docs/e2e-checklist.md`): connect, close window, left-click
  restores, right-click Exit, confirm proxy cleared and no xray process remains.
- Update `AGENTS.md` status and add a contract bullet.

## Left to the builder

- Exact Tauri v2 API names for menu/tray builders (verify against the vendored
  crate already in the lockfile; adjust if signatures differ).
- Whether `app.exit(0)` itself triggers `CloseRequested` (hide guard is
  harmless either way; no updater-style bypass flag needed).
