//! Anti-DPI service surface. Deferred in MVP: the engine binaries still ship
//! in the build, but no engine is started and every call reports `Unsupported`.

use serde::Serialize;

/// Status value matching `AntiDpiStatusSchema` in `packages/contracts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AntiDpiMode {
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntiDpiStatus {
    pub status: AntiDpiMode,
}

pub fn status() -> AntiDpiStatus {
    AntiDpiStatus {
        status: AntiDpiMode::Unsupported,
    }
}

pub fn start() -> AntiDpiStatus {
    status()
}

pub fn stop() -> AntiDpiStatus {
    status()
}
