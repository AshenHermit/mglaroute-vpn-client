use std::time::Duration;

use rusqlite::Connection;
use tauri::State;

use super::probe::best_eligible_profile;
use super::{
    emit_connection, exclusions, mark_faulted, remove_config, stop_child, AppState, CommandError,
    ConnectionStatePayload, ConnectionStatus,
};
use crate::core::config::{build_connection_config, build_connection_config_for, ConnectionPlatform};
use crate::core::snapshot::snapshot_for;
use crate::core::supervisor::wait_for_port;
use crate::platform::vpn::map_vpn_error;
use crate::platform::vpn::mobile::VPN_PLUGIN_PACKAGE;
use crate::storage::models::Profile;
use crate::storage::repos::{profile_repo, settings_repo};
use crate::storage::routing_repo;

/// Resolve which server a Connect should launch, without touching the network
/// or the process. An explicit id keeps the enabled-present lookup and is not
/// gated on cached service results. With no id, a passing cached result wins;
/// otherwise fall back to the first enabled-present server by `(sort_index,
/// id)` so the very first click works before any test cycle has run. Profiles
/// from disabled subscriptions are never eligible: `list_present_enabled`
/// joins `subscriptions` and filters on `enabled`.
pub fn select_connect_profile(
    conn: &Connection,
    profile_id: Option<String>,
) -> rusqlite::Result<Option<Profile>> {
    match profile_id {
        Some(id) => Ok(profile_repo::list_present_enabled(conn)?
            .into_iter()
            .find(|profile| profile.id == id)),
        None => {
            if let Some(best) = best_eligible_profile(conn)? {
                return Ok(Some(best));
            }
            Ok(profile_repo::list_present_enabled(conn)?.into_iter().next())
        }
    }
}

/// Android live session request. Values are fixed by the VPN service design:
/// virtual address/DNS owned by `VpnService`, both IP families routed, and the
/// app itself excluded so traffic cannot loop back into the core.
fn android_start_request() -> crate::platform::vpn::VpnStartRequest {
    crate::platform::vpn::VpnStartRequest {
        address: "10.10.0.1/30".to_string(),
        dns: "10.10.0.2".to_string(),
        routes: vec!["0.0.0.0/0".to_string(), "::/0".to_string()],
        disallowed_packages: vec![VPN_PLUGIN_PACKAGE.to_string()],
    }
}

/// Android-only teardown shared by connect failure, disconnect and child
/// death. Order is fixed: child, unconsumed fd, VPN service, temp config.
pub(crate) async fn android_cleanup(
    state: &AppState,
    child: &mut Option<Box<dyn super::CoreHandle>>,
    config_path: &Option<std::path::PathBuf>,
) {
    stop_child(child);
    let fd = state.inner.runtime.lock().await.tun_fd.take();
    if let Some(fd) = fd {
        state.inner.fd_closer.close(fd);
    }
    if state.inner.runtime.lock().await.vpn_active {
        let _ = state.inner.vpn.stop();
        state.inner.runtime.lock().await.vpn_active = false;
    }
    remove_config(config_path);
}

/// Bounded wait for the `tun-established` signal through the plugin. The
/// Kotlin side emits the event when `VpnService.establish()` succeeds and
/// `wait_established` reads the same session flag.
async fn wait_tun_established(state: &AppState) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if state.inner.vpn.wait_established() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Android connect. No system proxy is touched. A failure after `start`
/// always releases the fd (the launcher owns a supplied fd on every path),
/// stops the core and the VPN session, and removes the temp config.
async fn connect_impl_android(
    state: &AppState,
    profile: Profile,
    rules: &[crate::storage::models::RoutingRule],
    port: u16,
    exclusions: &[String],
) -> Result<ConnectionStatePayload, CommandError> {
    // 0. APK resources are assets; stage them into the real app binaries dir
    //    that both the launcher pre-check and XRAY_LOCATION_ASSET use. Doing
    //    this here (not at Rust startup) guarantees the Kotlin plugin is
    //    loaded, and it is idempotent.
    let asset_dir = state
        .inner
        .asset_dir
        .clone()
        .unwrap_or_else(|| state.inner.config_dir.join("binaries"));
    state
        .inner
        .vpn
        .stage_assets(&asset_dir)
        .map_err(map_vpn_error)?;

    // 1. Permission first: the system dialog may still be pending.
    let granted = state.inner.vpn.prepare().map_err(map_vpn_error)?;
    if !granted {
        return Err(CommandError::VpnPermissionRequired);
    }

    // 2. DNS upstreams are the physical link's literals. Feeding the virtual
    //    10.10.0.2 back would loop or leak name resolution.
    let dns_upstreams = state.inner.vpn.dns_upstreams().map_err(map_vpn_error)?;
    if dns_upstreams.is_empty() {
        return Err(CommandError::VpnInterfaceUnavailable);
    }

    // 3. Physical interface name for `autoOutboundsInterface`.
    let interface_name = state
        .inner
        .vpn
        .physical_interface()
        .map_err(map_vpn_error)?;

    // 4. Take over the slot before any OS state is created.
    let stopped = {
        let mut runtime = state.inner.runtime.lock().await;
        stop_child(&mut runtime.child)
    };
    if !stopped {
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }
    let old_config = {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.config_path.take()
    };
    remove_config(&old_config);

    let session = match state.inner.vpn.start(android_start_request()) {
        Ok(session) => session,
        Err(error) => {
            // Kotlin starts the foreground service before establish(); a
            // rejected start may leave that session live even though Rust
            // never set `vpn_active`. Stop defensively; ignore teardown errors
            // so the original start error stays authoritative.
            let _ = state.inner.vpn.stop();
            return Err(map_vpn_error(error));
        }
    };
    {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.tun_fd = Some(session.fd);
        runtime.vpn_active = true;
        runtime.connection = ConnectionStatePayload {
            status: ConnectionStatus::Connecting,
            profile_id: Some(profile.id.clone()),
            proxy_port: Some(port),
        };
        emit_connection(&runtime.connection);
    }

    let snapshot = snapshot_for(&profile, port, &state.inner.config_dir);
    let config = build_connection_config_for(
        &snapshot,
        exclusions,
        rules,
        state.inner.asset_dir.as_deref(),
        ConnectionPlatform::Android {
            interface_name: &interface_name,
            dns_upstreams: &dns_upstreams,
        },
    );
    let config = match config {
        Ok(config) => config,
        Err(_) => {
            let mut none = None;
            android_cleanup(state, &mut none, &None).await;
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };
    if let Some(parent) = snapshot.config_path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            let mut none = None;
            android_cleanup(state, &mut none, &None).await;
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    }
    let serialized = match serde_json::to_string(&config) {
        Ok(serialized) => serialized,
        Err(_) => {
            let mut none = None;
            android_cleanup(state, &mut none, &None).await;
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };
    if std::fs::write(&snapshot.config_path, serialized).is_err() {
        let mut none = None;
        android_cleanup(state, &mut none, &Some(snapshot.config_path.clone())).await;
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }

    // Handoff: `launch_with_tun` consumes and closes the fd on every path.
    let fd = match state.inner.runtime.lock().await.tun_fd.take() {
        Some(fd) => fd,
        None => {
            let mut none = None;
            android_cleanup(state, &mut none, &Some(snapshot.config_path.clone())).await;
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };
    let mut child = match state.inner.launcher.launch_with_tun(&snapshot, fd) {
        Ok(child) => Some(child),
        Err(_) => {
            let mut none = None;
            android_cleanup(state, &mut none, &Some(snapshot.config_path.clone())).await;
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };

    let established = wait_tun_established(state).await;
    let alive = match child
        .as_mut()
        .map(|handle| handle.try_wait().map(|exited| !exited))
        .transpose()
    {
        Ok(alive) => alive.unwrap_or(false),
        // A failed liveness check cannot leave the VPN session active, the
        // temp config on disk or an unowned fd: same cleanup as a dead core.
        Err(_) => {
            android_cleanup(state, &mut child, &Some(snapshot.config_path.clone())).await;
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };
    if !established || !alive {
        android_cleanup(state, &mut child, &Some(snapshot.config_path.clone())).await;
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }

    let mut runtime = state.inner.runtime.lock().await;
    runtime.child = child;
    runtime.config_path = Some(snapshot.config_path);
    runtime.connection = ConnectionStatePayload {
        status: ConnectionStatus::Connected,
        profile_id: Some(profile.id),
        proxy_port: Some(port),
    };
    emit_connection(&runtime.connection);
    Ok(runtime.connection.clone())
}

/// Caller holds the connection permit. Config generation and process launch
/// both consume the same immutable `ConfigSnapshot`.
pub(crate) async fn connect_impl(
    state: &AppState,
    profile_id: Option<String>,
) -> Result<ConnectionStatePayload, CommandError> {
    let (settings, profile, rules) = {
        let conn = state.inner.conn.lock().await;
        let settings = settings_repo::get(&conn)?;
        let profile = select_connect_profile(&conn, profile_id)?
            .ok_or(CommandError::NotFound)?;
        let rules = routing_repo::list_active_rules(&conn)?;
        (settings, profile, rules)
    };

    let port = u16::try_from(settings.local_proxy_port).map_err(|_| CommandError::InvalidInput)?;
    if port == 0 {
        return Err(CommandError::InvalidInput);
    }
    let exclusions = exclusions(&settings);

    if state.inner.android {
        return connect_impl_android(state, profile, &rules, port, &exclusions).await;
    }

    // A switch keeps the ownership taken at connect: the applied local proxy
    // and the user's original state stay in place. Restoring here would send
    // browser traffic direct (or to the old user proxy) while the replacement
    // core starts; `wait_for_port` allows a two-second leak window on every
    // probe-triggered switch. Give up ownership only when the system proxy was
    // turned off since connect. On failure the previous state stays owned and
    // recoverable, and a child whose stop failed stays owned (never orphaned).
    let stopped = {
        let mut runtime = state.inner.runtime.lock().await;
        stop_child(&mut runtime.child)
    };
    if !stopped {
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }

    let owned_previous = state.inner.runtime.lock().await.previous.clone();
    let previous = match owned_previous {
        Some(previous) if !settings.system_proxy_enabled => {
            let mut restored = false;
            for _ in 0..2 {
                let runtime = state.inner.runtime.lock().await;
                if runtime.proxy.restore(&previous).is_ok() {
                    restored = true;
                    break;
                }
            }
            if !restored {
                mark_faulted(state).await;
                return Err(CommandError::Proxy);
            }
            None
        }
        // `Some(previous)` is carried forward untouched; `None` is captured
        // from the user's state after the new core is ready.
        other => other,
    };

    let old_config = {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.previous = None;
        runtime.config_path.take()
    };
    remove_config(&old_config);

    {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.connection = ConnectionStatePayload {
            status: ConnectionStatus::Connecting,
            profile_id: Some(profile.id.clone()),
            proxy_port: Some(port),
        };
        emit_connection(&runtime.connection);
    }

    let snapshot = snapshot_for(&profile, port, &state.inner.config_dir);
    let config = build_connection_config(
        &snapshot,
        &exclusions,
        &rules,
        state.inner.asset_dir.as_deref(),
    )
    .map_err(|_| CommandError::Core)?;
    if let Some(parent) = snapshot.config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| CommandError::Core)?;
    }
    let serialized =
        serde_json::to_string(&config).map_err(|_| CommandError::Core)?;
    std::fs::write(&snapshot.config_path, serialized).map_err(|_| CommandError::Core)?;

    let mut child = match state.inner.launcher.launch(&snapshot) {
        Ok(child) => Some(child),
        Err(_) => {
            remove_config(&Some(snapshot.config_path.clone()));
            mark_faulted(state).await;
            return Err(CommandError::Core);
        }
    };

    if wait_for_port(port, Duration::from_secs(2)).is_err() {
        stop_child(&mut child);
        remove_config(&Some(snapshot.config_path.clone()));
        mark_faulted(state).await;
        return Err(CommandError::Core);
    }
    // The old core may still be closing its port; readiness alone cannot prove
    // the replacement bound successfully. A dead new core is a hard failure.
    let new_core_alive = child
        .as_mut()
        .map(|handle| handle.try_wait().map(|exited| !exited))
        .transpose()
        .map_err(|_| CommandError::Core)?
        .unwrap_or(false);
    if !new_core_alive {
        stop_child(&mut child);
        remove_config(&Some(snapshot.config_path.clone()));
        // `wait_for_port` succeeded while our core is already gone, so the
        // listener belongs to another program. This is a port conflict, not a
        // crash of a core we owned: keep the connection slot disconnected and
        // hand the user an actionable error instead of a faulted state.
        let mut runtime = state.inner.runtime.lock().await;
        runtime.previous = previous;
        runtime.connection = ConnectionStatePayload {
            status: ConnectionStatus::Disconnected,
            profile_id: None,
            proxy_port: None,
        };
        emit_connection(&runtime.connection);
        return Err(CommandError::PortInUse);
    }

    let previous = if settings.system_proxy_enabled {
        // Reuse the capture from the first connect. Calling `current()` on a
        // switch would read our own local proxy and leak it on disconnect.
        let captured = if previous.is_some() {
            None
        } else {
            let current = {
                let runtime = state.inner.runtime.lock().await;
                runtime.proxy.current()
            };
            match current {
                Ok(current) => Some(current),
                Err(_) => {
                    stop_child(&mut child);
                    remove_config(&Some(snapshot.config_path.clone()));
                    mark_faulted(state).await;
                    return Err(CommandError::Proxy);
                }
            }
        };
        // Re-apply only after the replacement core is ready. The applied state
        // is unchanged across a switch with the same port/exclusions.
        let applied = {
            let runtime = state.inner.runtime.lock().await;
            runtime.proxy.apply(port, &exclusions)
        };
        if applied.is_err() {
            stop_child(&mut child);
            remove_config(&Some(snapshot.config_path.clone()));
            mark_faulted(state).await;
            return Err(CommandError::Proxy);
        }
        previous.or(captured)
    } else {
        previous
    };

    let mut runtime = state.inner.runtime.lock().await;
    runtime.child = child;
    runtime.previous = previous;
    runtime.config_path = Some(snapshot.config_path);
    runtime.connection = ConnectionStatePayload {
        status: ConnectionStatus::Connected,
        profile_id: Some(profile.id),
        proxy_port: Some(port),
    };
    emit_connection(&runtime.connection);
    Ok(runtime.connection.clone())
}

/// Android disconnect. No proxy restore: `VpnService` owns the routes. The
/// shutdown order is fixed by the VPN service contract.
async fn disconnect_impl_android(
    state: &AppState,
) -> Result<ConnectionStatePayload, CommandError> {
    let (mut child, config_path) = {
        let mut runtime = state.inner.runtime.lock().await;
        (runtime.child.take(), runtime.config_path.take())
    };
    let stopped = stop_child(&mut child);
    let fd = state.inner.runtime.lock().await.tun_fd.take();
    if let Some(fd) = fd {
        state.inner.fd_closer.close(fd);
    }
    if !stopped && child.is_some() {
        let mut runtime = state.inner.runtime.lock().await;
        if runtime.child.is_none() {
            runtime.child = child.take();
        }
    }
    let mut vpn_ok = true;
    if state.inner.runtime.lock().await.vpn_active {
        vpn_ok = state.inner.vpn.stop().is_ok();
        state.inner.runtime.lock().await.vpn_active = false;
    }
    remove_config(&config_path);

    let status = if stopped && vpn_ok {
        ConnectionStatus::Disconnected
    } else {
        ConnectionStatus::Faulted
    };
    {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.connection = ConnectionStatePayload {
            status,
            profile_id: None,
            proxy_port: None,
        };
        emit_connection(&runtime.connection);
    }
    if !stopped {
        Err(CommandError::Core)
    } else if !vpn_ok {
        Err(CommandError::Core)
    } else {
        Ok(ConnectionStatePayload {
            status,
            profile_id: None,
            proxy_port: None,
        })
    }
}

/// Caller holds the connection permit. Restores the captured proxy state even
/// if stopping the core fails, so the machine is never left behind our proxy.
pub(crate) async fn disconnect_impl(
    state: &AppState,
) -> Result<ConnectionStatePayload, CommandError> {
    if state.inner.android {
        return disconnect_impl_android(state).await;
    }
    let (mut child, previous, config_path) = {
        let mut runtime = state.inner.runtime.lock().await;
        (
            runtime.child.take(),
            runtime.previous.take(),
            runtime.config_path.take(),
        )
    };

    let stopped = stop_child(&mut child);
    if !stopped && child.is_some() {
        // `stop_child` kept the handle: leave it owned by the runtime so a
        // later disconnect can retry instead of orphaning the process.
        let mut runtime = state.inner.runtime.lock().await;
        if runtime.child.is_none() {
            runtime.child = child.take();
        }
    }
    let mut restored = true;
    if let Some(previous) = previous {
        let mut ok = false;
        for _ in 0..2 {
            let runtime = state.inner.runtime.lock().await;
            if runtime.proxy.restore(&previous).is_ok() {
                ok = true;
                break;
            }
        }
        if !ok {
            let mut runtime = state.inner.runtime.lock().await;
            if runtime.previous.is_none() {
                runtime.previous = Some(previous);
            }
            restored = false;
        }
    }
    remove_config(&config_path);

    let status = if stopped && restored {
        ConnectionStatus::Disconnected
    } else {
        ConnectionStatus::Faulted
    };
    {
        let mut runtime = state.inner.runtime.lock().await;
        runtime.connection = ConnectionStatePayload {
            status,
            profile_id: None,
            proxy_port: None,
        };
        emit_connection(&runtime.connection);
    }

    if !stopped {
        Err(CommandError::Core)
    } else if !restored {
        Err(CommandError::Proxy)
    } else {
        Ok(ConnectionStatePayload {
            status,
            profile_id: None,
            proxy_port: None,
        })
    }
}

#[tauri::command]
pub async fn connection_status(
    state: State<'_, AppState>,
) -> Result<ConnectionStatePayload, CommandError> {
    let runtime = state.inner.runtime.lock().await;
    Ok(runtime.connection.clone())
}

#[tauri::command]
pub async fn connection_connect(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> Result<ConnectionStatePayload, CommandError> {
    let _permit = state.connection_permit()?;
    connect_impl(&state, profile_id).await
}

#[tauri::command]
pub async fn connection_disconnect(
    state: State<'_, AppState>,
) -> Result<ConnectionStatePayload, CommandError> {
    let _permit = state.connection_permit()?;
    disconnect_impl(&state).await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;

    use super::*;
    use crate::commands::{
        handle_exited_child, AppState, CommandError, ConnectionStatePayload, ConnectionStatus,
        CoreHandle, CoreLauncher, FdCloser,
    };
    use crate::core::snapshot::ConfigSnapshot;
    use crate::platform::proxy::{InMemoryProxy, PreviousProxy};
    use crate::platform::vpn::FakeVpnBackend;
    use crate::state::{set_event_sink, RecordingEventSink};
    use crate::storage::migrate;
    use crate::storage::models::{Profile, Subscription};
    use crate::storage::repos::{profile_repo, subscription_repo};

    #[derive(Clone, Default)]
    struct Log(Arc<Mutex<Vec<&'static str>>>);

    impl Log {
        fn push(&self, entry: &'static str) {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(entry);
        }

        fn all(&self) -> Vec<&'static str> {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }

        fn count(&self, entry: &'static str) -> usize {
            self.all().iter().filter(|value| **value == entry).count()
        }
    }

    struct RecordingFdCloser(Log);

    impl FdCloser for RecordingFdCloser {
        fn close(&self, _fd: i32) {
            self.0.push("fd-close");
        }
    }

    struct TestHandle {
        exited: Arc<AtomicBool>,
        log: Log,
        try_wait_error: bool,
    }

    impl CoreHandle for TestHandle {
        fn stop(&mut self) -> Result<(), String> {
            self.log.push("core-stop");
            self.exited.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn try_wait(&mut self) -> Result<bool, String> {
            if self.try_wait_error {
                return Err("poll failed".to_string());
            }
            Ok(self.exited.load(Ordering::SeqCst))
        }
    }

    struct TestLauncher {
        fd_seen: Arc<Mutex<Vec<i32>>>,
        log: Log,
        exited: Arc<AtomicBool>,
        fail: bool,
        try_wait_error: bool,
    }

    impl CoreLauncher for TestLauncher {
        fn launch(
            &self,
            snapshot: &ConfigSnapshot,
        ) -> Result<Box<dyn CoreHandle>, String> {
            self.launch_with_tun(snapshot, -1)
        }

        fn launch_with_tun(
            &self,
            _snapshot: &ConfigSnapshot,
            tun_fd: i32,
        ) -> Result<Box<dyn CoreHandle>, String> {
            self.fd_seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(tun_fd);
            // `spawn_with` consumes and closes the parent fd on every path.
            self.log.push("fd-close");
            if self.fail {
                return Err("launch failed".to_string());
            }
            Ok(Box::new(TestHandle {
                exited: self.exited.clone(),
                log: self.log.clone(),
                try_wait_error: self.try_wait_error,
            }))
        }
    }

    struct Harness {
        state: AppState,
        log: Log,
        fd_seen: Arc<Mutex<Vec<i32>>>,
        vpn: FakeVpnBackend,
        exited: Arc<AtomicBool>,
        dir: tempfile::TempDir,
    }

    fn android_state(vpn: FakeVpnBackend, fail_launch: bool) -> Harness {
        android_state_inner(vpn, fail_launch, false)
    }

    fn android_state_with_try_wait_error(vpn: FakeVpnBackend) -> Harness {
        android_state_inner(vpn, false, true)
    }

    fn android_state_inner(
        mut vpn: FakeVpnBackend,
        fail_launch: bool,
        try_wait_error: bool,
    ) -> Harness {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = Connection::open_in_memory().expect("open in-memory db");
        migrate(&conn).expect("migrate");
        subscription_repo::insert(
            &conn,
            &Subscription {
                id: "sub-1".to_string(),
                name: "test".to_string(),
                url: "https://example.test/sub".to_string(),
                enabled: true,
                ..Default::default()
            },
        )
        .expect("insert subscription");
        profile_repo::upsert_many(
            &conn,
            &[Profile {
                id: "p-1".to_string(),
                subscription_id: "sub-1".to_string(),
                server_id: "p-1".to_string(),
                link: "vless://uuid@node.example.com:443#p-1".to_string(),
                protocol: Some("vless".to_string()),
                address: Some("node.example.com".to_string()),
                port: Some(443),
                raw_config: serde_json::json!({"security": "none", "type": "raw"}).to_string(),
                ..Default::default()
            }],
        )
        .expect("insert profile");

        let log = Log::default();
        let stop_log = log.clone();
        vpn.stop_hook = Some(Arc::new(move || stop_log.push("vpn-stop")));
        let fd_seen = Arc::new(Mutex::new(Vec::new()));
        let exited = Arc::new(AtomicBool::new(false));
        let launcher = TestLauncher {
            fd_seen: fd_seen.clone(),
            log: log.clone(),
            exited: exited.clone(),
            fail: fail_launch,
            try_wait_error,
        };
        let state = AppState::new_with_vpn(
            conn,
            dir.path().to_path_buf(),
            Box::new(InMemoryProxy::new(PreviousProxy::Direct)),
            Box::new(launcher),
            Arc::new(crate::core::job::JobHandle::noop()),
            Box::new(vpn.clone()),
            true,
            Box::new(RecordingFdCloser(log.clone())),
        );
        Harness {
            state,
            log,
            fd_seen,
            vpn,
            exited,
            dir,
        }
    }

    async fn connect(harness: &Harness) -> Result<ConnectionStatePayload, CommandError> {
        let _permit = harness.state.connection_permit().expect("connection permit");
        connect_impl(&harness.state, Some("p-1".to_string())).await
    }

    #[tokio::test]
    async fn android_connect_requires_permission_before_spawning_core() {
        let mut vpn = FakeVpnBackend::default();
        vpn.prepare_granted = false;
        let harness = android_state(vpn, false);

        let error = connect(&harness).await.expect_err("permission must be required");
        assert_eq!(error, CommandError::VpnPermissionRequired);
        assert!(
            harness.fd_seen.lock().unwrap().is_empty(),
            "no core may launch before permission"
        );
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none());
        assert!(!runtime.vpn_active);
        assert_eq!(runtime.connection.status, ConnectionStatus::Disconnected);
        assert_eq!(harness.vpn.calls(), vec!["stage_assets", "prepare"]);
    }

    #[tokio::test]
    async fn android_connect_sets_tun_env_and_reaches_connected() {
        let harness = android_state(FakeVpnBackend::default(), false);

        let result = connect(&harness).await.expect("connect");
        assert_eq!(result.status, ConnectionStatus::Connected);
        assert_eq!(
            *harness.fd_seen.lock().unwrap(),
            vec![42],
            "launcher must receive the TUN fd"
        );
        assert!(harness.vpn.established_calls.load(Ordering::SeqCst) > 0);
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none(), "fd ownership handed to launcher");
        assert!(runtime.vpn_active);
        assert!(runtime.child.is_some());
        assert!(runtime.previous.is_none(), "Android must not capture system proxy");
    }

    #[tokio::test]
    async fn android_connect_fails_when_interface_name_unavailable() {
        let mut vpn = FakeVpnBackend::default();
        vpn.interface_name = None;
        let harness = android_state(vpn, false);

        let error = connect(&harness)
            .await
            .expect_err("interface name is required");
        assert_eq!(error, CommandError::VpnInterfaceUnavailable);
        assert!(
            harness.fd_seen.lock().unwrap().is_empty(),
            "no core launch without an interface name"
        );
        assert!(!harness.vpn.calls().contains(&"start"));
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none());
        assert!(!runtime.vpn_active);
    }

    #[tokio::test]
    async fn android_disconnect_stops_core_then_fd_then_vpn_service() {
        let harness = android_state(FakeVpnBackend::default(), false);
        let config_path = harness.dir.path().join("xray-p-1-10808.json");
        std::fs::write(&config_path, "{}").expect("write config");
        {
            let mut runtime = harness.state.inner.runtime.lock().await;
            runtime.child = Some(Box::new(TestHandle {
                exited: harness.exited.clone(),
                log: harness.log.clone(),
                try_wait_error: false,
            }));
            runtime.tun_fd = Some(7);
            runtime.vpn_active = true;
            runtime.config_path = Some(config_path.clone());
            runtime.connection = ConnectionStatePayload {
                status: ConnectionStatus::Connected,
                profile_id: Some("p-1".to_string()),
                proxy_port: Some(10808),
            };
        }

        let result = {
            let _permit = harness.state.connection_permit().expect("connection permit");
            disconnect_impl(&harness.state).await.expect("disconnect")
        };
        assert_eq!(result.status, ConnectionStatus::Disconnected);
        assert_eq!(harness.log.all(), vec!["core-stop", "fd-close", "vpn-stop"]);
        assert!(!config_path.exists());
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none());
        assert!(!runtime.vpn_active);
        assert!(runtime.child.is_none());
    }

    #[tokio::test]
    async fn android_child_death_stops_vpn_session() {
        set_event_sink(Arc::new(RecordingEventSink::new()));
        let harness = android_state(FakeVpnBackend::default(), false);
        let config_path = harness.dir.path().join("xray-p-1-10808.json");
        std::fs::write(&config_path, "{}").expect("write config");
        harness.exited.store(true, Ordering::SeqCst);
        {
            let mut runtime = harness.state.inner.runtime.lock().await;
            runtime.child = Some(Box::new(TestHandle {
                exited: harness.exited.clone(),
                log: harness.log.clone(),
                try_wait_error: false,
            }));
            runtime.vpn_active = true;
            runtime.config_path = Some(config_path.clone());
            runtime.connection = ConnectionStatePayload {
                status: ConnectionStatus::Connected,
                profile_id: Some("p-1".to_string()),
                proxy_port: Some(10808),
            };
        }

        let changed = {
            let _permit = harness.state.connection_permit().expect("connection permit");
            handle_exited_child(&harness.state).await.expect("child exit")
        };
        assert!(changed);
        assert_eq!(harness.log.all(), vec!["vpn-stop"]);
        assert!(!config_path.exists());
        let runtime = harness.state.inner.runtime.lock().await;
        assert_eq!(runtime.connection.status, ConnectionStatus::Faulted);
        assert!(!runtime.vpn_active);
        assert!(runtime.tun_fd.is_none());
    }

    #[tokio::test]
    async fn android_connect_failure_closes_fd_exactly_once() {
        let harness = android_state(FakeVpnBackend::default(), true);

        let error = connect(&harness).await.expect_err("launch failure");
        assert_eq!(error, CommandError::Core);
        assert_eq!(harness.log.count("fd-close"), 1);
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none(), "fd must not remain owned");
        assert!(!runtime.vpn_active, "VPN session must be stopped");
        assert!(runtime.child.is_none());
        assert_eq!(runtime.connection.status, ConnectionStatus::Faulted);
        assert!(harness.vpn.calls().contains(&"stop"));
    }

    /// Final-review Important 1: Rust must not rely solely on Kotlin to clean
    /// up a session that may already exist when `start` rejects.
    #[tokio::test]
    async fn android_connect_stages_assets_into_app_binaries_dir_first() {
        let harness = android_state(FakeVpnBackend::default(), false);
        let expected = harness.dir.path().join("binaries");

        connect(&harness).await.expect("connect");
        assert_eq!(
            *harness.vpn.staged_dir.lock().expect("lock staged dir"),
            Some(expected),
            "APK assets must be staged into the same dir the launcher resolves"
        );
        let calls = harness.vpn.calls();
        assert_eq!(
            calls.first(),
            Some(&"stage_assets"),
            "staging must precede permission, DNS and interface lookups: {calls:?}"
        );
    }

    #[tokio::test]
    async fn android_start_error_defensively_stops_vpn_session() {
        use crate::platform::vpn::VpnError;

        let mut vpn = FakeVpnBackend::default();
        vpn.start_error = Some(VpnError::StartFailed);
        let harness = android_state(vpn, false);

        let error = connect(&harness).await.expect_err("start failure");
        assert_eq!(error, CommandError::Core);
        assert!(
            harness.vpn.calls().contains(&"stop"),
            "start failure must defensively stop a possibly-live VPN session"
        );
        assert!(
            harness.fd_seen.lock().unwrap().is_empty(),
            "no fd handoff after start failure"
        );
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(!runtime.vpn_active);
        assert!(runtime.tun_fd.is_none());
    }

    #[tokio::test]
    async fn android_try_wait_failure_cleans_up_vpn_and_fd() {
        let harness = android_state_with_try_wait_error(FakeVpnBackend::default());
        let config_path = harness.dir.path().join("xray-p-1-10808.json");

        let error = connect(&harness).await.expect_err("liveness poll failure");
        assert_eq!(error, CommandError::Core);
        assert_eq!(harness.log.count("fd-close"), 1);
        assert_eq!(
            harness.log.all(),
            vec!["fd-close", "core-stop", "vpn-stop"],
            "launch consumes the fd once; cleanup stops core then VPN without a second fd close"
        );
        assert!(harness.vpn.calls().contains(&"stop"));
        assert!(!config_path.exists(), "temp config must be removed");
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none());
        assert!(!runtime.vpn_active, "VPN session must not stay active");
        assert!(runtime.child.is_none());
        assert!(runtime.config_path.is_none());
        assert_eq!(runtime.connection.status, ConnectionStatus::Faulted);
        // Ruling 10 order: DNS is fetched before the physical interface name.
        assert_eq!(
            harness.vpn.calls(),
            vec![
                "stage_assets",
                "prepare",
                "dns_upstreams",
                "physical_interface",
                "start",
                "stop"
            ]
        );
    }

    #[tokio::test]
    async fn android_connect_uses_fetched_dns_upstreams() {
        let mut vpn = FakeVpnBackend::default();
        vpn.dns_upstreams = vec!["8.8.8.8".to_string()];
        let harness = android_state(vpn, false);

        connect(&harness).await.expect("connect");
        let config_path = harness
            .dir
            .path()
            .join(format!("xray-p-1-{}.json", 10808));
        let text = std::fs::read_to_string(&config_path).expect("read generated config");
        assert!(text.contains("8.8.8.8"), "physical DNS literal must be the upstream");
        assert!(!text.contains("\"10.10.0.2\""), "virtual DNS must not be an upstream");
    }

    #[tokio::test]
    async fn android_connect_rejects_empty_dns_upstreams_without_launch() {
        let mut vpn = FakeVpnBackend::default();
        vpn.dns_upstreams.clear();
        let harness = android_state(vpn, false);

        let error = connect(&harness)
            .await
            .expect_err("empty DNS upstreams must be rejected");
        assert_eq!(error, CommandError::VpnInterfaceUnavailable);
        assert!(
            harness.fd_seen.lock().unwrap().is_empty(),
            "no core launch without DNS upstreams"
        );
        let runtime = harness.state.inner.runtime.lock().await;
        assert!(runtime.tun_fd.is_none());
        assert!(!runtime.vpn_active);
    }
}
