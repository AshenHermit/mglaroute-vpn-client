//! Shared application state: single-occupancy mutation token plus Tauri event
//! dispatch. Neither secrets nor raw configs enter logs or event payloads.

use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MutationError {
    #[error("another mutation is already in progress")]
    Busy,
}

pub type Result<T> = std::result::Result<T, MutationError>;

/// Managed guard instance holding one permit. `try_acquire` never waits: a
/// second concurrent mutation fails immediately with [`MutationError::Busy`].
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

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Called once from app setup. Later calls are ignored.
pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

/// Emit a Tauri event when the app handle is available; a no-op in tests and
/// before setup. Payloads never include links, credentials or raw configs.
pub fn emit<S: Serialize + Clone>(event: &str, payload: S) {
    if let Some(handle) = APP_HANDLE.get() {
        use tauri::Emitter;
        let _ = handle.emit(event, payload);
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
