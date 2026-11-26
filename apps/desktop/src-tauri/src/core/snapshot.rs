use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::models::Profile;

/// Immutable input for one xray process: everything needed to generate and run
/// a connection config. Cloning or rebuilding from the same inputs yields the
/// same serialized value; nothing is mutated after construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub subscription_id: String,
    pub profile: Profile,
    pub proxy_port: u16,
    pub config_path: PathBuf,
    /// Reserved for the deferred anti-DPI service. Always `None` in MVP.
    pub anti_dpi: Option<()>,
}

/// Build a snapshot from a profile, listen port and config directory.
///
/// The path is derived only from the profile identity, port and directory so
/// repeated calls are value-equal (no timestamps, random ids or side effects).
pub fn snapshot_for(profile: &Profile, port: u16, dir: &Path) -> ConfigSnapshot {
    let stem = if profile.id.is_empty() {
        &profile.server_id
    } else {
        &profile.id
    };
    ConfigSnapshot {
        subscription_id: profile.subscription_id.clone(),
        profile: profile.clone(),
        proxy_port: port,
        config_path: dir.join(format!("xray-{stem}-{port}.json")),
        anti_dpi: None,
    }
}
