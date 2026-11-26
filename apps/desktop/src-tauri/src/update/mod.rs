//! Update payloads and platform backends. Payloads mirror the shared
//! `UpdateInfoSchema` / `UpdateProgressEventSchema` contracts exactly; raw
//! updater errors are mapped to `None` / `Err(())` and never logged, because
//! their text can embed URLs. Updater traffic keeps the active system route:
//! no proxy is forced or bypassed here.

#[cfg(desktop)]
mod desktop;
#[cfg(not(desktop))]
mod mobile;

use serde::Serialize;

/// Event carrying per-chunk download/install progress to the frontend.
pub const EVENT_UPDATE_PROGRESS: &str = "update-progress";

const RELEASE_TAG_PREFIX: &str =
    "https://github.com/AshenHermit/mglaroute-vpn-client/releases/tag/v";

/// Frontend-facing update metadata. camelCase to match `packages/contracts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub can_install_in_app: bool,
    pub release_url: String,
}

/// Terminal phases of an in-app install; serialized lowercase for the contract.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePhase {
    Downloading,
    Installing,
    Done,
}

/// Progress payload emitted through `state::emit` while installing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressEvent {
    pub phase: UpdatePhase,
    pub received: u64,
    pub total: Option<u64>,
}

/// Human-facing release page for a version announced by the updater.
pub fn release_url(version: &str) -> String {
    format!("{RELEASE_TAG_PREFIX}{version}")
}

/// Only our own version-tag URLs may be opened by the frontend. Returns false
/// for arbitrary hosts, non-HTTPS schemes and an empty version suffix.
pub fn validate_release_url(url: &str) -> bool {
    url.strip_prefix(RELEASE_TAG_PREFIX)
        .is_some_and(|version| !version.is_empty())
}

#[cfg(desktop)]
pub(crate) use desktop::{check, install};
#[cfg(not(desktop))]
pub(crate) use mobile::{check, install};
