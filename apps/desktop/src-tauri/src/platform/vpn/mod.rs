//! Mobile VPN backend seam. The concrete Android `VpnService` implementation
//! lives in a later mobile plugin; this module only defines the contract and
//! desktop/test doubles. No OS state is touched here.

/// Parameters for one `VpnService` session, derived from the shared config
/// snapshot. Addresses and routes are CIDR strings; DNS is a plain address;
/// `disallowed_packages` are packages excluded from the VPN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnStartRequest {
    pub address: String,
    pub dns: String,
    pub routes: Vec<String>,
    pub disallowed_packages: Vec<String>,
}

/// A started session: the fd to hand to xray plus the interface name for
/// diagnostics. The fd is owned by the caller until handed off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnSession {
    pub fd: i32,
    pub interface_name: String,
}

/// Backend failures. Raw text from a concrete backend is deliberately not
/// carried: the command layer maps every variant to a generic command error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VpnError {
    #[error("vpn permission required")]
    PermissionRequired,
    #[error("vpn interface unavailable")]
    InterfaceUnavailable,
    #[error("vpn start failed")]
    StartFailed,
    #[error("vpn stop failed")]
    StopFailed,
}

/// Lifecycle of a platform VPN interface. `start`/`stop` are separate calls so
/// a plugin can own its session without exposing OS handles to the commands.
pub trait VpnBackend: Send + Sync {
    /// Ask for the platform VPN permission. `Ok(false)` means the user declined
    /// or the platform cannot grant it; no session may be started then.
    fn prepare(&self) -> Result<bool, VpnError>;
    fn start(&self, req: VpnStartRequest) -> Result<VpnSession, VpnError>;
    fn stop(&self) -> Result<(), VpnError>;
    /// Physical (non-VPN) interface name, used for route setup in a plugin.
    fn physical_interface(&self) -> Result<String, VpnError>;
    /// Block until the `VpnService` interface is up, or report failure.
    fn wait_established(&self) -> bool;
}

/// Desktop default: no VPN interface exists. Keeps the seam usable without a
/// mobile plugin and never grants permission.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopVpnBackend;

impl VpnBackend for NoopVpnBackend {
    fn prepare(&self) -> Result<bool, VpnError> {
        Ok(false)
    }

    fn start(&self, _req: VpnStartRequest) -> Result<VpnSession, VpnError> {
        Err(VpnError::InterfaceUnavailable)
    }

    fn stop(&self) -> Result<(), VpnError> {
        Err(VpnError::StopFailed)
    }

    fn physical_interface(&self) -> Result<String, VpnError> {
        Err(VpnError::InterfaceUnavailable)
    }

    fn wait_established(&self) -> bool {
        false
    }
}

/// Test double with observable call order, intentionally public at module scope
/// so command tests can inject it without a separate crate.
#[derive(Debug, Default, Clone)]
pub struct FakeVpnBackend {
    pub calls: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl FakeVpnBackend {
    /// Recorded backend calls in order (`prepare`, `start`, `stop`).
    pub fn calls(&self) -> Vec<&'static str> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn record(&self, call: &'static str) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(call);
    }
}

impl VpnBackend for FakeVpnBackend {
    fn prepare(&self) -> Result<bool, VpnError> {
        self.record("prepare");
        Ok(true)
    }

    fn start(&self, _req: VpnStartRequest) -> Result<VpnSession, VpnError> {
        self.record("start");
        Ok(VpnSession {
            fd: 42,
            interface_name: "wlan0".to_string(),
        })
    }

    fn stop(&self) -> Result<(), VpnError> {
        self.record("stop");
        Ok(())
    }

    fn physical_interface(&self) -> Result<String, VpnError> {
        Ok("wlan0".to_string())
    }

    fn wait_established(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{NoopVpnBackend, VpnBackend, VpnError, VpnStartRequest};
    use crate::platform::vpn::FakeVpnBackend;

    #[test]
    fn fake_backend_records_start_stop_order() {
        let backend = FakeVpnBackend::default();
        assert!(backend.prepare().expect("prepare"));
        let session = backend
            .start(VpnStartRequest {
                address: "10.0.0.2/30".to_string(),
                dns: "1.1.1.1".to_string(),
                routes: vec!["0.0.0.0/0".to_string()],
                disallowed_packages: vec!["com.example".to_string()],
            })
            .expect("start");
        assert_eq!(session.fd, 42);
        assert_eq!(session.interface_name, "wlan0");
        assert!(backend.wait_established());
        backend.stop().expect("stop");

        assert_eq!(backend.calls(), vec!["prepare", "start", "stop"]);
    }

    #[test]
    fn noop_backend_never_grants_permission() {
        let backend = NoopVpnBackend;
        assert!(!backend.prepare().expect("noop prepare is not an error"));
        assert!(matches!(
            backend.start(VpnStartRequest {
                address: "10.0.0.2/30".to_string(),
                dns: "1.1.1.1".to_string(),
                routes: Vec::new(),
                disallowed_packages: Vec::new(),
            }),
            Err(VpnError::InterfaceUnavailable)
        ));
        assert!(matches!(backend.stop(), Err(VpnError::StopFailed)));
        assert!(matches!(
            backend.physical_interface(),
            Err(VpnError::InterfaceUnavailable)
        ));
        assert!(!backend.wait_established());
    }
}
