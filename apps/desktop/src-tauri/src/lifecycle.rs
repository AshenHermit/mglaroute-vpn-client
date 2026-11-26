use std::sync::atomic::{AtomicBool, Ordering};

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

pub(crate) fn close_action() -> CloseAction {
    CloseAction::Hide
}

pub(crate) fn exit_action() -> ExitAction {
    ExitAction::DisconnectThenExit
}

pub(crate) fn tray_actions() -> &'static [&'static str; 2] {
    &["Open", "Exit"]
}

pub(crate) fn begin_shutdown() -> bool {
    SHUTTING_DOWN
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
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
}
