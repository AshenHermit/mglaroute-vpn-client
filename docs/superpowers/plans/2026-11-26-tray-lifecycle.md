# Tray Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the desktop client resident in the system tray on window close, reopen from a left-click, and exit safely from the tray menu after disconnecting the system proxy and stopping the xray core.

**Architecture:** A new Rust `lifecycle` module owns tray setup, hide-on-close, and a one-shot shutdown gate. Exit calls the existing `disconnect_impl` (stop child → restore proxy → remove config) and then `app.exit(0)`; no new Tauri commands and no frontend changes.

**Tech Stack:** Tauri v2.12.7 (Rust, edition 2021), Windows desktop, pnpm/Nx for the frontend test suite.

**Spec:** `docs/superpowers/specs/2026-11-26-tray-lifecycle-design.md`

## Global Constraints

- Work only in `D:\Users\hermit\vpn\mglaroute-vpn-client\.worktrees\tray-lifecycle`, branch `feature/tray-lifecycle`. Do not touch `.worktrees/mvp` or the C: worktree.
- Tauri `2.12.7`, Rust edition **2021**, rust-version `1.77`: no let-chains from the 2024-edition reference code — use nested `if`.
- `tauri` features must include `tray-icon` (the crate already has `tray-icon 0.25.1` in `Cargo.lock`; no new network dependency expected).
- `bundle.icon` must contain `icons/icon.ico`; window label must be explicitly `"main"`.
- Exactly 20 Tauri commands: `COMMAND_NAMES` / `invoke_handler` unchanged. No frontend contracts change.
- Reuse `crate::commands::connection::disconnect_impl(&AppState)`; do not reimplement proxy restore or child kill. It takes no connection permit and returns `Result<ConnectionStatePayload, CommandError>`.
- Never log subscription URLs, credentials or raw/generated config. Ignore cleanup errors without printing their contents.
- Verification: cargo tests require `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'` because the bundled xray binary is absent. Frontend suite: `pnpm test`.
- Terse output per project convention. Update `AGENTS.md` and the spec when behaviour changes.

## Review Focus

- Closing the window while connected must hide, not exit; the live core and proxy stay untouched — pinned by `close_action_hides_window` and `hide_on_close_source_prevents_close` (Task 2/3) plus manual e2e (Task 4).
- Exit while connected must restore the proxy and kill xray even if `disconnect_impl` returns `Err` — pinned by `shutdown_source_cleans_up_before_exit` (Task 3) plus manual e2e (Task 4).
- Double-clicking Exit must not run cleanup twice — pinned by `shutdown_gate_allows_only_one_exit` (Task 2).
- Left-click on a hidden window must restore and focus it, not show the menu — pinned by `left_click_source_shows_main_window` (Task 3) plus manual e2e (Task 4).
- Tray icon must exist in a real build — pinned by the `tests/lifecycle_config.rs` config/feature assertions (Task 1) plus manual e2e.

---

### Task 1: Tray icon configuration and feature

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml:16`
- Modify: `apps/desktop/src-tauri/tauri.conf.json:13-19,24-29`
- Create: `apps/desktop/src-tauri/tests/lifecycle_config.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `tauri` built with `tray-icon`; window label `"main"`; bundle icon `icons/icon.ico`.

- [ ] **Step 1: Write failing config tests** in `tests/lifecycle_config.rs` using `include_str!("../tauri.conf.json")` and `include_str!("../Cargo.toml")`; assert: `/app/windows/0/label == "main"`, `/bundle/icon` array contains `"icons/icon.ico"`, and the `tauri` dependency line contains `tray-icon`.

- [ ] **Step 2: Run tests to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test lifecycle_config`
Expected: FAIL on the three assertions.

- [ ] **Step 3: Change config and feature**

In `Cargo.toml` set `tauri = { version = "2", features = ["tray-icon"] }`. In `tauri.conf.json` add `"label": "main"` to the single window object and set `"icon": ["icons/icon.ico"]` under `bundle`. Leave `externalBin` and `resources` untouched.

- [ ] **Step 4: Run tests to verify pass**

Run: same command as Step 2.
Expected: 3 tests PASS, 0 failures.

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/tests/lifecycle_config.rs` then `git commit -m "feat(desktop): enable tray icon and pin main window config"`

### Task 2: Lifecycle decisions and one-shot shutdown gate

**Files:**
- Create: `apps/desktop/src-tauri/src/lifecycle.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs:1-10` (module declaration only)

**Interfaces:**
- Consumes: nothing.
- Produces: `CloseAction::Hide`, `ExitAction::DisconnectThenExit`, `close_action() -> CloseAction`, `exit_action() -> ExitAction`, `tray_actions() -> &'static [&'static str; 2]`, `begin_shutdown() -> bool`, static `SHUTTING_DOWN: AtomicBool`.

- [ ] **Step 1: Write failing unit tests** in `src/lifecycle.rs` `#[cfg(test)] mod tests`:
  - `close_action_hides_window`: `assert_eq!(close_action(), CloseAction::Hide)`
  - `exit_action_disconnects_then_exits`: `assert_eq!(exit_action(), ExitAction::DisconnectThenExit)`
  - `tray_actions_are_open_and_exit`: `assert_eq!(tray_actions(), &["Open", "Exit"])`
  - `shutdown_gate_allows_only_one_exit`: `SHUTTING_DOWN.store(false, Ordering::SeqCst)`, then `assert!(begin_shutdown())`, `assert!(!begin_shutdown())`, reset to `false` at the end.

- [ ] **Step 2: Run tests to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib lifecycle`
Expected: FAIL, `lifecycle` module / symbols not found.

- [ ] **Step 3: Implement the decision surface**

In `src/lifecycle.rs`: `static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false)`; `begin_shutdown()` uses `compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()`; derive `Debug, Clone, Copy, PartialEq, Eq` on both enums. In `src/lib.rs` add `pub mod lifecycle;` in alphabetical position. No tray API yet.

- [ ] **Step 4: Run tests to verify pass**

Run: same as Step 2.
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri/src/lifecycle.rs apps/desktop/src-tauri/src/lib.rs` then `git commit -m "feat(desktop): add tray lifecycle decisions and shutdown gate"`

### Task 3: Tray wiring, hide-on-close, safe shutdown

**Files:**
- Modify: `apps/desktop/src-tauri/src/lifecycle.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs:60-78`

**Interfaces:**
- Consumes: `begin_shutdown`, `tray_actions`, `CloseAction`, `ExitAction` from Task 2; `crate::commands::AppState`; `crate::commands::connection::disconnect_impl`.
- Produces: `setup_tray<R: Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()>`, `show_main_window_handle<R: Runtime>(window: tauri::WebviewWindow<R>)`, `hide_on_close<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent)`, `shutdown(app: AppHandle, state: AppState) -> ()` (async).

- [ ] **Step 1: Write failing source-level tests** in `src/lifecycle.rs` tests, reading `include_str!("lifecycle.rs")`:
  - `shutdown_source_cleans_up_before_exit`: inside the text after `pub async fn shutdown`, assert the index of `disconnect_impl` is less than the index of `app.exit(0)`, and that the result is bound with `let _ =` (errors ignored, never logged).
  - `hide_on_close_source_prevents_close`: inside the `hide_on_close` body assert `api.prevent_close()` and `window.hide()` both appear.
  - `left_click_source_shows_main_window`: assert the `TrayIconEvent::Click` handler calls `show_main_window_handle` and `get_webview_window("main")`; assert `show_menu_on_left_click(false)` is present.
  - `exit_menu_is_wired_to_shutdown`: assert the `"exit"` menu branch calls `begin_shutdown()` and spawns `shutdown(`.

- [ ] **Step 2: Run tests to verify failure**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib lifecycle`
Expected: FAIL on the new assertions.

- [ ] **Step 3: Implement tray and shutdown**

In `src/lifecycle.rs` import `std::sync::atomic::Ordering`, `tauri::menu::{Menu, MenuItem}`, `tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent}`, `tauri::{AppHandle, Manager, Runtime, WindowEvent}`, and `crate::commands::AppState`.

Implement:
- `show_main_window_handle`: `let _ = window.show(); let _ = window.set_focus();`
- `hide_on_close`: on `WindowEvent::CloseRequested { api, .. }` call `api.prevent_close()` then `let _ = window.hide()`. No bypass flag — `app.exit` does not depend on the close handler here.
- `setup_tray`: build `MenuItem::with_id(&handle, "open", "Open", true, None::<&str>)` and the analogous `"exit"` / `"Exit"`; `Menu::with_items(&handle, &[&open, &exit])`; `TrayIconBuilder::new().menu(&menu).show_menu_on_left_click(false)`; `.on_tray_icon_event` matches `TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }` (nested `if let` — edition 2021), then `if let Some(window) = click_handle.get_webview_window("main") { show_main_window_handle(window); }`. `.on_menu_event(move |handle, event| ...)` — Tauri 2.12 passes `&AppHandle<R>` as first argument: `"open"` shows the window; `"exit"` calls `begin_shutdown()` and, only if true, clones the handle and `AppState` and `tauri::async_runtime::spawn(async move { shutdown(handle, state).await; })`. `.build(&handle)?`.
- `pub async fn shutdown(app: AppHandle, state: AppState)`: `let _ = crate::commands::connection::disconnect_impl(&state).await; app.exit(0);` — the exit is unconditional.

In `src/lib.rs`: add `.on_window_event(lifecycle::hide_on_close)` before `.setup(...)`; inside `setup`, after all `handle.spawn(...)` calls and before `Ok(())`, call `lifecycle::setup_tray(app)?;`.

- [ ] **Step 4: Run tests to verify pass**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Expected: all Rust tests PASS, including the untouched `command_names_registered` (20 names).

- [ ] **Step 5: Commit**

`git add apps/desktop/src-tauri/src/lifecycle.rs apps/desktop/src-tauri/src/lib.rs` then `git commit -m "feat(desktop): tray reopen and safe exit from Rust"`

### Task 4: Documentation, manual checklist, full verification

**Files:**
- Modify: `docs/superpowers/specs/2026-11-26-tray-lifecycle-design.md` (status → `accepted`)
- Modify: `AGENTS.md`
- Modify: `docs/e2e-checklist.md`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces: documented tray behaviour and a repeatable manual pass.

- [ ] **Step 1: Update docs**

Spec: set `Status: accepted`. `AGENTS.md`: add tray + safe-exit line to Status and one contract bullet (`CloseRequested hides; tray Exit runs disconnect_impl then app.exit(0); no new commands`). `docs/e2e-checklist.md`: add steps — connect, close window (process alive, proxy still on), left-click tray icon (window restored/focused), right-click → Exit, verify proxy setting restored and no `xray` process remains, repeat Exit to check no second cleanup.

- [ ] **Step 2: Run full verification**

Run: `$env:TAURI_CONFIG='{"bundle":{"externalBin":[],"resources":[]}}'; cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
Expected: all Rust tests PASS.

Run: `$env:NX_DAEMON='false'; pnpm test`
Expected: exit code 0; pre-existing `Cannot switch to pnpm@9` and zod annotation warnings acceptable.

- [ ] **Step 3: Commit**

`git add AGENTS.md docs/e2e-checklist.md docs/superpowers/specs/2026-11-26-tray-lifecycle-design.md` then `git commit -m "docs(desktop): document tray lifecycle and safe exit checks"`
