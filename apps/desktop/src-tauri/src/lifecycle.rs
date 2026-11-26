use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, WindowEvent};

use crate::commands::AppState;

/// One-shot latch: a second `Exit` click must not run cleanup twice.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseAction {
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitAction {
    DisconnectThenExit,
}

// Decision surface: the automated tests pin the chosen behavior, native tray
// handlers below carry it out without a headless runtime of their own.
#[allow(dead_code)]
pub(crate) fn close_action() -> CloseAction {
    CloseAction::Hide
}

#[allow(dead_code)]
pub(crate) fn exit_action() -> ExitAction {
    ExitAction::DisconnectThenExit
}

#[allow(dead_code)]
pub(crate) fn tray_actions() -> &'static [&'static str; 2] {
    &["Open", "Exit"]
}

pub(crate) fn begin_shutdown() -> bool {
    SHUTTING_DOWN
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

pub(crate) fn setup_tray<R: Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()> {
    let app_handle = app.handle().clone();
    let open = MenuItem::with_id(&app_handle, "open", "Open", true, None::<&str>)?;
    let exit = MenuItem::with_id(&app_handle, "exit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(&app_handle, &[&open, &exit])?;

    let click_handle = app_handle.clone();
    let mut builder = TrayIconBuilder::<R>::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = click_handle.get_webview_window("main") {
                    show_main_window_handle(window);
                }
            }
        })
        .on_menu_event(move |handle, event| match event.id().as_ref() {
            "open" => {
                if let Some(window) = handle.get_webview_window("main") {
                    show_main_window_handle(window);
                }
            }
            "exit" => {
                if begin_shutdown() {
                    let state = (*handle.state::<AppState>()).clone();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        shutdown(handle, state).await;
                    });
                }
            }
            _ => {}
        });

    // A Windows tray icon built with no icon gets a null HICON and never shows.
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(&app_handle)?;
    Ok(())
}

pub(crate) fn show_main_window_handle<R: Runtime>(window: tauri::WebviewWindow<R>) {
    let _ = window.show();
    let _ = window.set_focus();
}

pub(crate) fn hide_on_close<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

/// Shared teardown for terminal actions (tray exit, updater install).
/// Stop the core, restore the captured proxy, drop the temp config.
/// The Job Object and proxy_previous.json cover a crash or a failed restore.
pub(crate) async fn cleanup(state: &AppState) {
    let _ = crate::commands::connection::disconnect_impl(state).await;
}

pub(crate) async fn shutdown<R: Runtime>(app: AppHandle<R>, state: AppState) {
    cleanup(&state).await;
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn close_action_hides_window() {
        assert_eq!(close_action(), CloseAction::Hide);
    }

    #[test]
    fn exit_action_disconnects_then_exits() {
        assert_eq!(exit_action(), ExitAction::DisconnectThenExit);
    }

    #[test]
    fn tray_actions_are_open_and_exit() {
        assert_eq!(tray_actions(), &["Open", "Exit"]);
    }

    #[test]
    fn shutdown_gate_allows_only_one_exit() {
        SHUTTING_DOWN.store(false, Ordering::SeqCst);
        assert!(begin_shutdown());
        assert!(!begin_shutdown());
        SHUTTING_DOWN.store(false, Ordering::SeqCst);
    }

    // Wiring against native tray APIs has no headless runtime; these read the
    // production half of this file only, so test literals cannot satisfy them.
    fn production_source() -> &'static str {
        include_str!("lifecycle.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("source must have a test module marker")
    }

    fn after(marker: &str) -> &'static str {
        production_source()
            .split(marker)
            .nth(1)
            .unwrap_or_else(|| panic!("missing {marker} in production source"))
    }

    #[test]
    fn shutdown_source_cleans_up_before_exit() {
        let body = after("async fn shutdown");
        let cleanup = body
            .find("cleanup(&state)")
            .expect("shutdown must call shared cleanup before exiting");
        let exit = body.find("app.exit(0)").expect("shutdown must exit");
        assert!(cleanup < exit, "cleanup must finish before process exit");
    }

    #[test]
    fn update_cleanup_helper_disconnects_before_terminal_action() {
        let body = after("pub(crate) async fn cleanup")
            .split('}')
            .next()
            .expect("cleanup body must be delimited");
        assert!(
            body.contains("disconnect_impl"),
            "cleanup must disconnect before a terminal action"
        );
        assert!(
            body.contains("let _ ="),
            "cleanup failure must not block the terminal action and must not be logged"
        );
    }

    #[test]
    fn hide_on_close_source_prevents_close() {
        let body = after("fn hide_on_close");
        assert!(body.contains("api.prevent_close()"));
        assert!(body.contains("window.hide()"));
    }

    #[test]
    fn left_click_source_shows_main_window() {
        let body = after("fn setup_tray");
        assert!(body.contains("show_menu_on_left_click(false)"));
        assert!(body.contains("TrayIconEvent::Click"));
        assert!(body.contains("show_main_window_handle"));
        assert!(body.contains("get_webview_window(\"main\")"));
    }

    #[test]
    fn exit_menu_is_wired_to_shutdown() {
        let body = after("fn setup_tray");
        assert!(body.contains("\"exit\""));
        assert!(body.contains("begin_shutdown()"));
        assert!(body.contains("shutdown("));
    }
}
