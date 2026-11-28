//! Android `VpnService` backend. The concrete plugin lives in Kotlin; this
//! module adapts Tauri's mobile plugin bridge to the [`VpnBackend`] seam.
//! Nothing here touches OS state on desktop.

#[cfg(mobile)]
use serde::{Deserialize, Serialize};
#[cfg(mobile)]
use tauri::plugin::mobile::PluginInvokeError;
#[cfg(mobile)]
use tauri::plugin::PluginHandle;
#[cfg(mobile)]
use tauri::Wry;

#[cfg(mobile)]
use super::{VpnBackend, VpnError, VpnSession, VpnStartRequest};
#[cfg(mobile)]
use crate::commands::CommandError;

/// Tauri plugin identifier registered in `run()`. Kotlin class
/// `MglarouteVpnPlugin` is loaded under this name; its commands are
/// `prepare`, `start`, `stop`, `interface_name` and `wait_established`.
#[cfg(mobile)]
pub const VPN_PLUGIN_NAME: &str = "mglaroute-vpn";
/// Kotlin package of the generated Android project.
#[cfg(mobile)]
pub const VPN_PLUGIN_CLASS: &str = "MglarouteVpnPlugin";
#[cfg(mobile)]
pub const VPN_PLUGIN_PACKAGE: &str = "com.mglaroute.client";
/// Event emitted by the Kotlin plugin once the TUN interface is established.
#[cfg(mobile)]
pub const VPN_ESTABLISHED_EVENT: &str = "tun-established";

#[cfg(mobile)]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartPayload<'a> {
    address: &'a str,
    dns: &'a str,
    routes: &'a [String],
    disallowed_packages: &'a [String],
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
struct PrepareResponse {
    granted: bool,
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
struct StartResponse {
    fd: i32,
    #[serde(rename = "interfaceName")]
    interface_name: String,
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
struct InterfaceResponse {
    #[serde(rename = "interfaceName")]
    interface_name: String,
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
struct EstablishedResponse {
    established: bool,
}

/// Adapter from a Tauri mobile plugin handle to the platform VPN contract.
/// The handle is the only state: sessions live in the Kotlin `VpnService`.
#[cfg(mobile)]
pub struct MglarouteVpnBackend(PluginHandle<Wry>);

#[cfg(mobile)]
impl MglarouteVpnBackend {
    pub fn new(handle: PluginHandle<Wry>) -> Self {
        Self(handle)
    }
}

#[cfg(mobile)]
fn map_plugin_error(error: PluginInvokeError, fallback: VpnError) -> VpnError {
    if let PluginInvokeError::InvokeRejected(response) = &error {
        match response.code.as_deref() {
            Some("permission_required") => return VpnError::PermissionRequired,
            Some("interface_unavailable") => return VpnError::InterfaceUnavailable,
            _ => {}
        }
    }
    let _ = error;
    fallback
}

/// Translate a backend failure for the command layer without exposing raw
/// plugin text. Used by connection commands on mobile; desktop paths keep
/// their existing error mapping.
#[cfg(mobile)]
pub fn map_vpn_error(error: VpnError) -> CommandError {
    match error {
        VpnError::PermissionRequired => CommandError::VpnPermissionRequired,
        VpnError::InterfaceUnavailable => CommandError::VpnInterfaceUnavailable,
        VpnError::StartFailed | VpnError::StopFailed => CommandError::Core,
    }
}

#[cfg(mobile)]
impl VpnBackend for MglarouteVpnBackend {
    fn prepare(&self) -> Result<bool, VpnError> {
        let response: PrepareResponse = self
            .0
            .run_mobile_plugin("prepare", ())
            .map_err(|error| map_plugin_error(error, VpnError::PermissionRequired))?;
        Ok(response.granted)
    }

    fn start(&self, req: VpnStartRequest) -> Result<VpnSession, VpnError> {
        let payload = StartPayload {
            address: &req.address,
            dns: &req.dns,
            routes: &req.routes,
            disallowed_packages: &req.disallowed_packages,
        };
        let response: StartResponse = self
            .0
            .run_mobile_plugin("start", payload)
            .map_err(|error| map_plugin_error(error, VpnError::StartFailed))?;
        Ok(VpnSession {
            fd: response.fd,
            interface_name: response.interface_name,
        })
    }

    fn stop(&self) -> Result<(), VpnError> {
        self.0
            .run_mobile_plugin::<()>("stop", ())
            .map_err(|error| map_plugin_error(error, VpnError::StopFailed))
    }

    fn physical_interface(&self) -> Result<String, VpnError> {
        let response: InterfaceResponse = self
            .0
            .run_mobile_plugin("interface_name", ())
            .map_err(|error| map_plugin_error(error, VpnError::InterfaceUnavailable))?;
        Ok(response.interface_name)
    }

    fn wait_established(&self) -> bool {
        self.0
            .run_mobile_plugin::<EstablishedResponse>("wait_established", ())
            .map(|response| response.established)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    fn manifest() -> String {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/gen/android/app/src/main/AndroidManifest.xml"
        );
        std::fs::read_to_string(path).expect("gen/android AndroidManifest.xml must exist")
    }

    fn kotlin(relative: &str) -> String {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/gen/android/app/src/main/java/com/mglaroute/client/");
        std::fs::read_to_string(format!("{root}{relative}"))
            .unwrap_or_else(|error| panic!("{relative} must exist: {error}"))
    }

    #[test]
    fn android_manifest_declares_vpn_service() {
        let text = manifest();
        for required in [
            "android.permission.BIND_VPN_SERVICE",
            "android.permission.FOREGROUND_SERVICE",
            "android.permission.FOREGROUND_SERVICE_SPECIAL_USE",
            "android.permission.POST_NOTIFICATIONS",
            "android.net.VpnService",
            "foregroundServiceType=\"specialUse\"",
            "SUPPORTS_ALWAYS_ON",
        ] {
            assert!(text.contains(required), "manifest missing {required}");
        }
    }

    #[test]
    fn kotlin_service_detaches_fd_and_excludes_self() {
        let service = kotlin("MglarouteVpnService.kt");
        let plugin = kotlin("MglarouteVpnPlugin.kt");
        let both = format!("{service}\n{plugin}");
        for required in [
            "detachFd()",
            "startForeground",
            "establish()",
            "addAddress(\"10.10.0.1/30\")",
            "addDnsServer(\"10.10.0.2\")",
            "setBlocking(true)",
            "addDisallowedApplication",
        ] {
            assert!(both.contains(required), "Kotlin sources missing {required}");
        }
        let registered = plugin
            .find("registerForActivityResult")
            .expect("plugin must register activity result callback");
        let prepared = plugin
            .find("VpnService.prepare")
            .expect("plugin must call VpnService.prepare");
        assert!(
            registered < prepared,
            "activity-result callback must be registered in load() before prepare()"
        );
    }

    #[test]
    fn mobile_backend_uses_tauri_plugin_bridge() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/platform/vpn/mobile.rs");
        let text = std::fs::read_to_string(path).expect("mobile.rs must exist");
        // Only check the implementation section; the assertions below would
        // otherwise satisfy their own string checks.
        let implementation = text.split("#[cfg(test)]").next().expect("test module");
        for required in [
            "struct MglarouteVpnBackend",
            "impl VpnBackend for MglarouteVpnBackend",
            "run_mobile_plugin",
            "mglaroute-vpn",
            "CommandError::VpnPermissionRequired",
            "CommandError::VpnInterfaceUnavailable",
            "tun-established",
        ] {
            assert!(
                implementation.contains(required),
                "mobile backend missing {required}"
            );
        }
    }

    #[test]
    fn android_fd_handoff_clears_cloexec_before_exec() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/core/supervisor.rs");
        let text = std::fs::read_to_string(path).expect("core/supervisor.rs must exist");
        for required in ["pre_exec", "F_SETFD", "XRAY_TUN_FD"] {
            assert!(
                text.contains(required),
                "supervisor.rs must keep TUN fd across exec: missing {required}"
            );
        }
    }
}
