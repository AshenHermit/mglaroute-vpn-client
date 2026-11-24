//! System-proxy abstraction: portable state model, persistence and test double.
//!
//! Windows specifics live in [`super::proxy_windows`]. Nothing here touches
//! the registry, environment proxies or the network.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("proxy state I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("proxy state JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Windows registry call failed with code {0}")]
    Registry(u32),
    #[error("Windows API call failed with code {0}")]
    WindowsApi(u32),
    #[error("system proxy is only available on Windows")]
    Unsupported,
}

pub type Result<T> = std::result::Result<T, ProxyError>;

/// Proxy configuration captured before we take over the system proxy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PreviousProxy {
    Direct,
    Pac(String),
    Manual {
        server: String,
        override_list: String,
    },
}

/// Applies and restores a system proxy. Both operations are idempotent enough
/// for connect/disconnect; callers own the captured [`PreviousProxy`] value.
pub trait ProxyBackend {
    fn apply(&self, port: u16, exclusions: &[String]) -> Result<()>;
    fn restore(&self, previous: &PreviousProxy) -> Result<()>;
}

/// WinInet `ProxyServer` value for a local xray `mixed` inbound.
pub fn proxy_string(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

/// WinInet `ProxyOverride` value. `<local>` is always first (loopback bypass).
pub fn override_string(exclusions: &[String]) -> String {
    let mut value = String::from("<local>");
    for exclusion in exclusions {
        if exclusion.is_empty() {
            continue;
        }
        value.push(';');
        value.push_str(exclusion);
    }
    value
}

/// Write the captured pre-connect proxy state to `path` (JSON).
///
/// Parent directories are created. The file holds no credentials or
/// subscription data — only WinInet settings.
pub fn persist_previous(path: &Path, previous: &PreviousProxy) -> Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(previous)?)?;
    Ok(())
}

/// Read a previously persisted proxy state from `path` (JSON).
pub fn load_previous(path: &Path) -> Result<PreviousProxy> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

/// Test double with observable state; no OS interaction.
#[derive(Debug)]
pub struct InMemoryProxy {
    state: std::sync::Mutex<PreviousProxy>,
}

impl InMemoryProxy {
    pub fn new(initial: PreviousProxy) -> Self {
        Self {
            state: std::sync::Mutex::new(initial),
        }
    }

    pub fn state(&self) -> PreviousProxy {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Default for InMemoryProxy {
    fn default() -> Self {
        Self::new(PreviousProxy::Direct)
    }
}

impl ProxyBackend for InMemoryProxy {
    fn apply(&self, port: u16, exclusions: &[String]) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = PreviousProxy::Manual {
            server: proxy_string(port),
            override_list: override_string(exclusions),
        };
        Ok(())
    }

    fn restore(&self, previous: &PreviousProxy) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *state = previous.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_string_shape() {
        assert_eq!(proxy_string(10808), "127.0.0.1:10808");
    }

    #[test]
    fn override_prepends_local() {
        assert_eq!(
            override_string(&["example.com".to_string()]),
            "<local>;example.com"
        );
    }

    #[test]
    fn restore_previous_from_disk_on_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("proxy_previous.json");
        let previous = PreviousProxy::Manual {
            server: "127.0.0.1:10808".to_string(),
            override_list: "<local>;example.com".to_string(),
        };

        persist_previous(&path, &previous).expect("persist");
        assert!(path.exists(), "state file should exist after persist");

        // Simulate a process restart: in-memory state is gone, disk survives.
        let fresh = InMemoryProxy::default();
        let loaded = load_previous(&path).expect("load");
        assert_eq!(loaded, previous);
        fresh.restore(&loaded).expect("restore");
        assert_eq!(fresh.state(), previous);
    }

    #[test]
    fn in_memory_apply_then_restore() {
        let original = PreviousProxy::Manual {
            server: "proxy.example:8080".to_string(),
            override_list: "<local>;intranet".to_string(),
        };
        let backend = InMemoryProxy::new(original.clone());

        backend
            .apply(10808, &["example.com".to_string()])
            .expect("apply");
        assert_eq!(
            backend.state(),
            PreviousProxy::Manual {
                server: "127.0.0.1:10808".to_string(),
                override_list: "<local>;example.com".to_string(),
            }
        );

        backend.restore(&original).expect("restore");
        assert_eq!(backend.state(), original);
    }
}
