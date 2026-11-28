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
use super::{map_vpn_error as map_to_command_error, VpnBackend, VpnError, VpnSession, VpnStartRequest};

/// Tauri plugin identifier registered in `run()`. Kotlin class
/// `MglarouteVpnPlugin` is loaded under this name; its commands are
/// `prepare`, `start`, `stop`, `interface_name`, `dns_servers` and `wait_established`.
#[cfg(mobile)]
pub const VPN_PLUGIN_NAME: &str = "mglaroute-vpn";
/// Kotlin package of the generated Android project.
#[cfg(mobile)]
pub const VPN_PLUGIN_CLASS: &str = "MglarouteVpnPlugin";
/// Available on every target so the Android request builder compiles in tests.
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

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
struct DnsResponse {
    #[serde(rename = "dnsServers")]
    dns_servers: Vec<String>,
}

/// Adapter from a Tauri mobile plugin handle to the platform VPN contract.
/// The handle is the only state: sessions live in the Kotlin `VpnService`.
#[cfg(mobile)]
#[derive(Clone)]
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
            Some("interface_unavailable") | Some("dns_unavailable") => {
                return VpnError::InterfaceUnavailable
            }
            _ => {}
        }
    }
    let _ = error;
    fallback
}

/// Translate a backend failure for the command layer without exposing raw
/// plugin text. Kept as a thin alias; the shared mapping lives in the parent
/// module so desktop and mobile compile the same connection code.
#[cfg(mobile)]
pub fn map_vpn_error(error: VpnError) -> crate::commands::CommandError {
    map_to_command_error(error)
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

    fn dns_upstreams(&self) -> Result<Vec<String>, VpnError> {
        let response: DnsResponse = self
            .0
            .run_mobile_plugin("dns_servers", ())
            .map_err(|error| map_plugin_error(error, VpnError::InterfaceUnavailable))?;
        Ok(response.dns_servers)
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
    fn kotlin_plugin_reads_physical_dns_servers() {
        let plugin = kotlin("MglarouteVpnPlugin.kt");
        for required in [
            "fun dns_servers",
            "dnsServers",
            "getLinkProperties",
            ".dnsServers",
            ".hostAddress",
            "dns_unavailable",
        ] {
            assert!(
                plugin.contains(required),
                "Kotlin plugin must supply physical DNS literals: missing {required}"
            );
        }
        assert!(
            plugin.contains("private fun interfaceName()"),
            "plugin must keep the physical interface helper"
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
            "map_to_command_error",
            "tun-established",
            "dns_servers",
            "dns_upstreams",
            "dns_unavailable",
        ] {
            assert!(
                implementation.contains(required),
                "mobile backend missing {required}"
            );
        }
    }

    #[test]
    fn android_build_task_resolves_node_portably() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/gen/android/buildSrc/src/main/java/com/mglaroute/client/kotlin/BuildTask.kt"
        );
        let text = std::fs::read_to_string(path).expect("BuildTask.kt must exist");
        assert!(
            !text.contains(r"C:\Program Files\nodejs"),
            "generated build task must not pin a developer machine Node path"
        );
        for required in [
            "System.getenv(\"NODE\")",
            "node.exe",
            "\"node\"",
            "Os.isFamily(Os.FAMILY_WINDOWS)",
        ] {
            assert!(
                text.contains(required),
                "BuildTask.kt must resolve Node portably: missing {required}"
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
