//! Mobile VPN backend seam. The Android `VpnService` plugin adapts the Kotlin
//! side to this contract; desktop keeps the no-op/test doubles. No OS state is
//! touched here.

pub mod mobile;

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
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
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
    /// Physical-network DNS servers as IP literals. Must not be the virtual
    /// address handed to `VpnService`, or xray's dns outbound would loop.
    fn dns_upstreams(&self) -> Result<Vec<String>, VpnError>;
    /// Block until the `VpnService` interface is up, or report failure.
    fn wait_established(&self) -> bool;
}

/// Generic mapping from a VPN backend failure to the command surface. Kept
/// here so both desktop and mobile targets compile the same connection code.
pub fn map_vpn_error(error: VpnError) -> crate::commands::CommandError {
    use crate::commands::CommandError;
    match error {
        VpnError::PermissionRequired => CommandError::VpnPermissionRequired,
        VpnError::InterfaceUnavailable => CommandError::VpnInterfaceUnavailable,
        VpnError::StartFailed | VpnError::StopFailed => CommandError::Core,
    }
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

    fn dns_upstreams(&self) -> Result<Vec<String>, VpnError> {
        Ok(Vec::new())
    }

    fn wait_established(&self) -> bool {
        false
    }
}

/// Test double with observable call order, intentionally public at module scope
/// so command tests can inject it without a separate crate. Defaults describe
/// a healthy Android session; tests override individual fields to inject
/// failures.
#[derive(Clone)]
pub struct FakeVpnBackend {
    pub calls: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    pub prepare_granted: bool,
    pub interface_name: Option<String>,
    pub session_fd: i32,
    pub established: bool,
    pub established_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub dns_upstreams: Vec<String>,
    pub dns_error: Option<VpnError>,
    pub start_error: Option<VpnError>,
    /// Observes `stop()` from command tests without leaking into production.
    pub stop_hook: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

impl Default for FakeVpnBackend {
    fn default() -> Self {
        Self {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            prepare_granted: true,
            interface_name: Some("wlan0".to_string()),
            session_fd: 42,
            established: true,
            established_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            dns_upstreams: vec!["1.1.1.1".to_string()],
            dns_error: None,
            start_error: None,
            stop_hook: None,
        }
    }
}

impl FakeVpnBackend {
    /// Recorded backend calls in order (`prepare`, `dns_upstreams`,
    /// `physical_interface`, `start`, `stop`).
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
        Ok(self.prepare_granted)
    }

    fn start(&self, _req: VpnStartRequest) -> Result<VpnSession, VpnError> {
        self.record("start");
        if let Some(error) = &self.start_error {
            return Err(error.clone());
        }
        Ok(VpnSession {
            fd: self.session_fd,
            interface_name: "wlan0".to_string(),
        })
    }

    fn stop(&self) -> Result<(), VpnError> {
        self.record("stop");
        if let Some(hook) = &self.stop_hook {
            hook();
        }
        Ok(())
    }

    fn physical_interface(&self) -> Result<String, VpnError> {
        self.record("physical_interface");
        self.interface_name
            .clone()
            .ok_or(VpnError::InterfaceUnavailable)
    }

    fn dns_upstreams(&self) -> Result<Vec<String>, VpnError> {
        self.record("dns_upstreams");
        if let Some(error) = &self.dns_error {
            return Err(error.clone());
        }
        Ok(self.dns_upstreams.clone())
    }

    fn wait_established(&self) -> bool {
        self.established_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.established
    }
}

#[cfg(test)]
mod tests {
    use super::{map_vpn_error, NoopVpnBackend, VpnBackend, VpnError, VpnStartRequest};
    use crate::commands::CommandError;
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
        assert!(backend.dns_upstreams().expect("noop DNS").is_empty());
        assert!(!backend.wait_established());
    }

    #[test]
    fn fake_backend_dns_upstreams_are_configurable() {
        let mut backend = FakeVpnBackend::default();
        assert_eq!(
            backend.dns_upstreams().expect("default DNS"),
            vec!["1.1.1.1".to_string()]
        );
        backend.dns_upstreams = vec!["8.8.8.8".to_string(), "1.0.0.1".to_string()];
        assert_eq!(
            backend.dns_upstreams().expect("configured DNS"),
            vec!["8.8.8.8".to_string(), "1.0.0.1".to_string()]
        );
        backend.dns_error = Some(VpnError::InterfaceUnavailable);
        assert_eq!(
            backend.dns_upstreams().expect_err("injected DNS failure"),
            VpnError::InterfaceUnavailable
        );
    }

    #[test]
    fn vpn_errors_map_to_generic_command_errors() {
        assert_eq!(
            map_vpn_error(VpnError::PermissionRequired),
            CommandError::VpnPermissionRequired
        );
        assert_eq!(
            map_vpn_error(VpnError::InterfaceUnavailable),
            CommandError::VpnInterfaceUnavailable
        );
        assert_eq!(map_vpn_error(VpnError::StartFailed), CommandError::Core);
        assert_eq!(map_vpn_error(VpnError::StopFailed), CommandError::Core);
    }
}
