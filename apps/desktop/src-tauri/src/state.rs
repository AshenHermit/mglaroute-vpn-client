//! Shared application state: single-occupancy mutation token plus an injectable
//! event sink. Neither secrets nor raw configs enter logs or event payloads.

use std::sync::{Arc, RwLock};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MutationError {
    #[error("another mutation is already in progress")]
    Busy,
}

pub type Result<T> = std::result::Result<T, MutationError>;

/// Managed guard instance holding one permit. Each command domain owns its own
/// instance; `try_acquire` never waits: a second concurrent mutation in the
/// same domain fails immediately with [`MutationError::Busy`]. The owner maps
/// that error to its domain-specific command code.
pub struct MutationGuard {
    semaphore: Arc<Semaphore>,
}

/// Owned permit; released when dropped, including after an `await`.
#[allow(dead_code)]
pub struct MutationPermit(OwnedSemaphorePermit);

impl Default for MutationGuard {
    fn default() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }
}

impl MutationGuard {
    /// Never blocks: a caller that cannot get the permit immediately gets
    /// [`MutationError::Busy`] instead of queueing behind the current mutation.
    pub fn try_acquire(&self) -> Result<MutationPermit> {
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => Ok(MutationPermit(permit)),
            Err(TryAcquireError::NoPermits) => Err(MutationError::Busy),
            Err(TryAcquireError::Closed) => unreachable!("mutation semaphore is never closed"),
        }
    }
}

/// Event dispatch seam. Production installs a Tauri-backed sink in `setup`;
/// tests install a recorder. Keeping Tauri behind this trait means `emit` never
/// pulls the WebView2 GUI import table into unit-test binaries.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

static EVENT_SINK: RwLock<Option<Arc<dyn EventSink>>> = RwLock::new(None);

/// Install or replace the sink. Used once in app setup and by event tests.
pub fn set_event_sink(sink: Arc<dyn EventSink>) {
    *EVENT_SINK
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink);
}

/// Tauri-backed sink. Referenced only from `run()`, so unit tests never link
/// the GUI runtime.
pub fn install_tauri_sink(handle: tauri::AppHandle) {
    struct TauriSink(tauri::AppHandle);

    impl EventSink for TauriSink {
        fn emit(&self, event: &str, payload: Value) {
            use tauri::Emitter;
            let _ = self.0.emit(event, payload);
        }
    }

    set_event_sink(Arc::new(TauriSink(handle)));
}

/// Emit an event when a sink is installed; a no-op before setup. Payloads never
/// include links, credentials or raw configs.
pub fn emit<S: Serialize>(event: &str, payload: S) {
    let value = serde_json::to_value(payload).unwrap_or(Value::Null);
    let sink = EVENT_SINK
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(sink) = sink {
        sink.emit(event, value);
    }
}

#[cfg(test)]
pub struct RecordingEventSink {
    events: std::sync::Mutex<Vec<(String, Value)>>,
}

#[cfg(test)]
impl RecordingEventSink {
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<(String, Value)> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[cfg(test)]
impl EventSink for RecordingEventSink {
    fn emit(&self, event: &str, payload: Value) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((event.to_string(), payload));
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[tokio::test]
    async fn concurrent_mutation_returns_busy() {
        let guard = MutationGuard::default();
        let first = guard.try_acquire().expect("first acquire must succeed");

        let started = Instant::now();
        let second = guard.try_acquire();
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "conflicting acquire must not block"
        );
        assert_eq!(second.err(), Some(MutationError::Busy));

        drop(first);
        assert!(guard.try_acquire().is_ok(), "permit must be reusable");
    }
}
