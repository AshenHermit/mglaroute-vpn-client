//! Xray process supervision: binary discovery, command construction, port
//! readiness and child-process lifetime. No config generation or network
//! probing here — callers pass an immutable [`ConfigSnapshot`].

use std::env;
use std::ffi::{OsStr, OsString};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::job::JobHandle;
use crate::core::snapshot::ConfigSnapshot;

pub const BINARY_PATH_ENV: &str = "XRAY_BINARY_PATH";
/// Handoff key for the Android `VpnService` TUN fd. Value is a decimal fd.
pub const XRAY_TUN_FD_ENV: &str = "XRAY_TUN_FD";

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("failed to spawn xray: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("timed out after {timeout:?} waiting for proxy port {port}")]
    PortTimeout { port: u16, timeout: Duration },
}

pub type Result<T> = std::result::Result<T, SupervisorError>;

/// Resolve the xray binary: `XRAY_BINARY_PATH`, then explicit override, then a
/// sidecar next to the running executable (dev and installed builds), then the
/// build-time bundled `binaries/xray.exe` (Windows x64) / `binaries/xray`.
/// Android additionally prefers the ABI-named sidecar staged under
/// `resource_dir/binaries`.
pub fn binary_path(override_path: Option<&str>) -> PathBuf {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let env_path = env::var_os(BINARY_PATH_ENV);
    binary_path_with_resource_dir(
        exe_dir.as_deref(),
        override_path,
        env_path.as_deref(),
        None,
    )
}

/// Sidecar file name for the current target. Android uses the exact names
/// staged by the build (`xray-aarch64-linux-android` /
/// `xray-x86_64-linux-android`); other targets keep the existing names.
pub fn binary_name_for(resource_dir: Option<&Path>) -> String {
    #[cfg(target_os = "android")]
    {
        let _ = resource_dir;
        android_binary_name(env::consts::ARCH)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = resource_dir;
        if cfg!(windows) {
            "xray.exe".to_string()
        } else {
            "xray".to_string()
        }
    }
}

/// Pure ABI → sidecar name mapping, tested independently of the host target.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn android_binary_name(arch: &str) -> String {
    match arch {
        "aarch64" => "xray-aarch64-linux-android".to_string(),
        "x86_64" => "xray-x86_64-linux-android".to_string(),
        // Unknown ABIs keep the generic staged name rather than guessing.
        _ => "xray".to_string(),
    }
}

/// Candidate path for an Android sidecar under a Tauri resource directory.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn android_binary_candidate(resource_dir: &Path, arch: &str) -> Option<PathBuf> {
    Some(
        resource_dir
            .join("binaries")
            .join(android_binary_name(arch)),
    )
}

/// Resolution used by [`binary_path`] and the Android launch path. Windows
/// order stays: env → override → exe-dir sidecar → bundled path. On Android a
/// staged resource-dir sidecar is preferred before those fallbacks.
pub fn binary_path_with_resource_dir(
    exe_dir: Option<&Path>,
    override_path: Option<&str>,
    env_path: Option<&OsStr>,
    resource_dir: Option<&Path>,
) -> PathBuf {
    #[cfg(target_os = "android")]
    if let Some(dir) = resource_dir {
        if let Some(candidate) = android_binary_candidate(dir, env::consts::ARCH) {
            if candidate.exists() {
                return candidate;
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = resource_dir;

    let name = binary_name_for(resource_dir);
    if let Some(value) = env_path.filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    if let Some(value) = override_path.filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    if let Some(dir) = exe_dir {
        let sidecar = dir.join(&name);
        if sidecar.exists() {
            return sidecar;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(name)
}

/// Windows process-creation flags that keep a console child's window hidden.
pub(crate) fn hidden_console_creation_flags() -> u32 {
    #[cfg(windows)]
    {
        0x0800_0000 // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Suppress the console window Windows allocates for a console child of a
/// GUI process. No-op off Windows.
pub(crate) fn apply_hidden_console(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(hidden_console_creation_flags());
    }
    command
}

/// TUN fd handoff as `(key, decimal value)`. The fd number itself is not a
/// secret; no config or subscription data is ever put in the environment.
pub(crate) fn tun_fd_env_value(fd: i32) -> (&'static str, String) {
    (XRAY_TUN_FD_ENV, fd.to_string())
}

/// Environment overrides for a launched core. Kept separate from `Command`
/// so the fd handoff can be tested without spawning or introspecting a child.
#[derive(Debug, Default)]
pub(crate) struct CommandEnv {
    overrides: Vec<(OsString, Option<OsString>)>,
}

impl CommandEnv {
    pub(crate) fn with_tun_fd(fd: i32) -> Self {
        let (key, value) = tun_fd_env_value(fd);
        Self {
            overrides: vec![(OsString::from(key), Some(OsString::from(value)))],
        }
    }

    pub(crate) fn apply(&self, command: &mut Command) {
        for (key, value) in &self.overrides {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
    }
}

/// Build a xray invocation for one snapshot with optional TUN fd handoff.
/// stdout/stderr go to null so raw config (which holds credentials) never
/// reaches a log sink. Only Android callers pass `tun_fd`, so desktop commands
/// never carry `XRAY_TUN_FD`.
pub fn build_command_with(snapshot: &ConfigSnapshot, tun_fd: Option<i32>) -> Result<Command> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let env_path = env::var_os(BINARY_PATH_ENV);
    let program = binary_path_with_resource_dir(
        exe_dir.as_deref(),
        None,
        env_path.as_deref(),
        exe_dir.as_deref(),
    );
    let mut command = Command::new(program);
    command.arg("run").arg("-c").arg(&snapshot.config_path);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(fd) = tun_fd {
        CommandEnv::with_tun_fd(fd).apply(&mut command);
    }
    apply_hidden_console(&mut command);
    Ok(command)
}

/// Desktop entry point. Signature and behaviour are unchanged.
pub fn build_command(snapshot: &ConfigSnapshot) -> Result<Command> {
    build_command_with(snapshot, None)
}

/// Close the parent's copy of the TUN fd. The child receives its own copy via
/// `XRAY_TUN_FD` at exec; this must be called exactly once per supplied fd.
#[cfg(target_os = "android")]
pub(crate) fn close_tun_fd(fd: i32) {
    unsafe {
        libc::close(fd);
    }
}

/// Windows/desktop stub. Desktop callers never supply a TUN fd.
#[cfg(not(target_os = "android"))]
pub(crate) fn close_tun_fd(_fd: i32) {}

/// Ordered cleanup for a failed job assignment: reap the child (kill + wait),
/// then release the fd. Injected as closures so the order is testable without
/// a real spawn.
pub(crate) fn cleanup_failed_job_assignment(stop_child: impl FnOnce(), close_fd: impl FnOnce()) {
    stop_child();
    close_fd();
}

/// Poll a loopback TCP listener until it accepts or the deadline passes.
pub fn wait_for_port(port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let poll = Duration::from_millis(50);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let attempt = poll.min(remaining);
        if TcpStream::connect_timeout(&address, attempt).is_ok() {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(SupervisorError::PortTimeout { port, timeout });
        }
        thread::sleep(poll.min(remaining));
    }
}

/// Owns a spawned xray child and reaps it on `stop`. `Drop` is a backstop for
/// panics/early returns that skip `stop`: no child is left behind on shutdown.
pub struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    /// Spawn xray for `snapshot` and immediately assign it to `job` when one
    /// is supplied. A failed assignment kills and waits for the child before
    /// surfacing the error — an unmanaged core is never left running.
    pub fn spawn(snapshot: &ConfigSnapshot, job: Option<&JobHandle>) -> Result<Self> {
        spawn_with(snapshot, job, None)
    }

    /// Adopt an already spawned child (e.g. a temporary core for probing).
    pub fn new(child: Child) -> Self {
        Self { child }
    }

    /// PID of the owned child, for the probe PID sidecar.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Non-blocking exit check for the watcher task. `Ok(None)` means the
    /// child is still running; `Ok(Some(_))` means it exited.
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    /// Kill then wait. Both results are collected so a child that already
    /// exited is still reaped instead of being left as a zombie.
    pub fn stop(&mut self) -> Result<()> {
        let killed = self.child.kill();
        let waited = self.child.wait();
        waited?;
        killed?;
        Ok(())
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        // Never propagate from a destructor; `stop()` remains the explicit,
        // error-reporting path. This covers panics and forgotten `stop()`.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn a core for `snapshot`, optionally handing a TUN fd to the child.
/// `tun_fd` is consumed exactly once: on a spawn failure, on job-assignment
/// failure (after kill + wait), and on success once the child owns its copy.
/// Job assignment failure never leaves an unmanaged child running.
pub fn spawn_with(
    snapshot: &ConfigSnapshot,
    job: Option<&JobHandle>,
    tun_fd: Option<i32>,
) -> Result<ChildGuard> {
    let mut command = match build_command_with(snapshot, tun_fd) {
        Ok(command) => command,
        Err(error) => {
            if let Some(fd) = tun_fd {
                close_tun_fd(fd);
            }
            return Err(error);
        }
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(fd) = tun_fd {
                close_tun_fd(fd);
            }
            return Err(SupervisorError::Spawn(error));
        }
    };
    if let Some(job) = job {
        if let Err(error) = job.assign(&child) {
            cleanup_failed_job_assignment(
                || {
                    let _ = child.kill();
                    let _ = child.wait();
                },
                || {
                    if let Some(fd) = tun_fd {
                        close_tun_fd(fd);
                    }
                },
            );
            return Err(SupervisorError::Spawn(error));
        }
    }
    if let Some(fd) = tun_fd {
        close_tun_fd(fd);
    }
    Ok(ChildGuard { child })
}

/// Pure resolution helper used by [`binary_path`] and tested without touching
/// the real executable directory.
pub fn binary_path_from(
    exe_dir: Option<&std::path::Path>,
    override_path: Option<&str>,
    env_path: Option<&std::ffi::OsStr>,
) -> PathBuf {
    binary_path_with_resource_dir(exe_dir, override_path, env_path, None)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ffi::{OsStr, OsString};
    use std::net::{SocketAddr, TcpListener};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, Instant, SystemTime};
    use std::{env, process::Command};

    use crate::core::job::{
        open_process_failure_means_dead, reap_orphans_with, reap_orphans_with_liveness, JobHandle,
    };
    use crate::core::snapshot::snapshot_for;
    use crate::storage::models::Profile;

    use super::*;

    // Env vars are process-global: serialize the tests that mutate them.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        key: &'static str,
        previous: Option<OsString>,
        _lock: MutexGuard<'static, ()>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn lock_env(key: &'static str) -> EnvRestore {
        let lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        EnvRestore {
            key,
            previous: env::var_os(key),
            _lock: lock,
        }
    }

    fn exe_name() -> &'static str {
        if cfg!(windows) {
            "xray.exe"
        } else {
            "xray"
        }
    }

    fn test_snapshot() -> ConfigSnapshot {
        snapshot_for(
            &Profile {
                server_id: "srv".to_string(),
                ..Default::default()
            },
            10808,
            Path::new("C:/tmp/xray-supervisor-test"),
        )
    }

    #[test]
    fn command_matches_snapshot() {
        let _env = lock_env(BINARY_PATH_ENV);
        env::set_var(
            BINARY_PATH_ENV,
            if cfg!(windows) {
                "C:/tools/xray.exe"
            } else {
                "/tmp/xray"
            },
        );

        let snapshot = test_snapshot();
        let command: Command = build_command(&snapshot).expect("command");
        let program = PathBuf::from(command.get_program());
        assert!(
            program.to_string_lossy().ends_with(exe_name()),
            "unexpected program: {}",
            program.display()
        );

        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(args.len(), 3, "unexpected arg count: {}", args.len());
        assert_eq!(args[0], OsStr::new("run"));
        assert_eq!(args[1], OsStr::new("-c"));
        assert_eq!(Path::new(args[2]), snapshot.config_path.as_path());
    }

    #[test]
    fn child_process_creation_flags_hide_console_on_windows() {
        #[cfg(windows)]
        assert_eq!(
            hidden_console_creation_flags(),
            0x0800_0000,
            "CREATE_NO_WINDOW must be set"
        );
        #[cfg(not(windows))]
        assert_eq!(hidden_console_creation_flags(), 0, "no-op off Windows");
    }

    #[test]
    fn wait_for_port_success() {
        let listener =
            TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).expect("ephemeral listener");
        let port = listener.local_addr().expect("local addr").port();

        wait_for_port(port, Duration::from_secs(1)).expect("listener should be ready");
    }

    #[test]
    fn wait_for_port_timeout() {
        let probe =
            TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).expect("ephemeral listener");
        let port = probe.local_addr().expect("local addr").port();
        drop(probe);

        let started = Instant::now();
        assert!(wait_for_port(port, Duration::from_secs(1)).is_err());
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(1500),
            "timeout took {elapsed:?}"
        );
    }

    #[test]
    fn binary_path_prefers_sidecar_next_to_executable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sidecar = dir.path().join(exe_name());
        std::fs::write(&sidecar, b"xray").expect("write sidecar");
        let _env = lock_env(BINARY_PATH_ENV);
        env::remove_var(BINARY_PATH_ENV);

        assert_eq!(binary_path_from(Some(dir.path()), None, None), sidecar);
    }

    #[test]
    fn reaping_only_kills_verified_xray() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
        std::fs::write(&pid_path, "4242").unwrap();
        let killed = std::cell::Cell::new(0);
        let count = reap_orphans_with(
            dir.path(),
            |_pid, _config| false,
            |_pid| killed.set(killed.get() + 1),
        )
        .unwrap();
        assert_eq!(count, 0);
        assert_eq!(killed.get(), 0);
        assert!(pid_path.exists(), "suspect sidecar stays for inspection");
    }

    #[test]
    fn reaping_kills_and_cleans_verified_xray() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
        let config_path = dir.path().join("mglaroute-probe-42-1.json");
        std::fs::write(&pid_path, "4242").unwrap();
        std::fs::write(&config_path, "{}").unwrap();
        let seen = std::cell::Cell::new(0);
        let killed = std::cell::Cell::new(0);
        let count = reap_orphans_with(
            dir.path(),
            |pid, config| {
                seen.set(pid);
                config == config_path.as_path()
            },
            |pid| {
                assert_eq!(pid, 4242);
                killed.set(killed.get() + 1);
            },
        )
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(seen.get(), 4242);
        assert_eq!(killed.get(), 1);
        assert!(!pid_path.exists(), "verified pid sidecar must be removed");
        assert!(!config_path.exists(), "verified config must be removed");
    }

    #[test]
    fn reaping_dead_pid_cleans_sidecars_without_kill() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
        let config_path = dir.path().join("mglaroute-probe-42-1.json");
        std::fs::write(&pid_path, "4242").unwrap();
        std::fs::write(&config_path, "{}").unwrap();
        let killed = Cell::new(0);
        let count = reap_orphans_with_liveness(
            dir.path(),
            |_pid| false,
            |_pid, _config| panic!("dead PID must not be verified"),
            |_pid| killed.set(killed.get() + 1),
            None,
        )
        .unwrap();
        assert_eq!(count, 0);
        assert_eq!(killed.get(), 0);
        assert!(!pid_path.exists());
        assert!(!config_path.exists());
    }

    #[test]
    fn open_process_failure_classifies_dead_pid_without_loosening_access_denied() {
        // ERROR_INVALID_PARAMETER means the PID no longer exists. Other
        // failures (e.g. ERROR_ACCESS_DENIED) must stay conservative.
        assert!(open_process_failure_means_dead(87));
        assert!(!open_process_failure_means_dead(5));
    }

    #[test]
    fn reaping_deduplicates_pid_and_cleans_all_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("mglaroute-probe-42-1.pid");
        let second = dir.path().join("mglaroute-probe-42-2.pid");
        std::fs::write(&first, "4242").unwrap();
        std::fs::write(&second, "4242").unwrap();
        let verified = Cell::new(0);
        let killed = Cell::new(0);
        let count = reap_orphans_with_liveness(
            dir.path(),
            |_pid| true,
            |_pid, _config| {
                verified.set(verified.get() + 1);
                true
            },
            |_pid| killed.set(killed.get() + 1),
            None,
        )
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(verified.get(), 1);
        assert_eq!(killed.get(), 1);
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn reaping_ignores_sidecars_newer_than_cutoff() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("mglaroute-probe-42-1.pid");
        std::fs::write(&pid_path, "4242").unwrap();
        let cutoff = SystemTime::now() - Duration::from_secs(3600);
        let verified = Cell::new(0);
        let count = reap_orphans_with_liveness(
            dir.path(),
            |_pid| true,
            |_pid, _config| {
                verified.set(verified.get() + 1);
                true
            },
            |_pid| panic!("fresh sidecar must not be killed"),
            Some(cutoff),
        )
        .unwrap();
        assert_eq!(count, 0);
        assert_eq!(verified.get(), 0);
        assert!(pid_path.exists());
    }

    #[test]
    fn reaping_empty_dir_calls_no_verifier() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Cell::new(0);
        let count = reap_orphans_with_liveness(
            dir.path(),
            |_pid| {
                calls.set(calls.get() + 1);
                true
            },
            |_pid, _config| {
                calls.set(calls.get() + 1);
                true
            },
            |_pid| panic!("no processes to kill"),
            None,
        )
        .unwrap();
        assert_eq!(count, 0);
        assert_eq!(calls.get(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn job_object_kills_child_tree_on_handle_close() {
        let job = JobHandle::create().expect("create job");
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "timeout /T 30"])
            .spawn()
            .expect("spawn cmd");
        job.assign(&child).expect("assign child to job");
        assert!(
            child.try_wait().expect("poll").is_none(),
            "child must still be alive before job close"
        );
        drop(job);

        let deadline = Instant::now() + Duration::from_secs(2);
        while child.try_wait().expect("poll").is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(
            child.try_wait().expect("poll").is_some(),
            "closing the job handle must kill the child tree"
        );
    }

    #[test]
    fn binary_path_override() {
        let _env = lock_env(BINARY_PATH_ENV);

        env::set_var(BINARY_PATH_ENV, "/tmp/custom");
        assert_eq!(binary_path(Some("ignored")), PathBuf::from("/tmp/custom"));

        env::remove_var(BINARY_PATH_ENV);
        assert_eq!(
            binary_path(Some("override.exe")),
            PathBuf::from("override.exe")
        );

        let bundled = binary_path(None);
        assert!(
            bundled.ends_with(exe_name()),
            "unexpected bundled path: {}",
            bundled.display()
        );
        assert!(
            bundled
                .parent()
                .is_some_and(|parent| parent.ends_with("binaries")),
            "bundled binary should live under binaries/: {}",
            bundled.display()
        );
    }

    fn command_env_map(snapshot: &ConfigSnapshot, fd: Option<i32>) -> Vec<(String, Option<String>)> {
        let command = build_command_with(snapshot, fd).expect("command with fd");
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn tun_fd_env_value_is_xray_tun_fd() {
        let (key, value) = tun_fd_env_value(17);
        assert_eq!(key, "XRAY_TUN_FD");
        assert_eq!(value, "17");
    }

    #[test]
    fn build_command_without_fd_has_no_tun_env() {
        let snapshot = test_snapshot();
        let map = command_env_map(&snapshot, None);
        assert!(
            !map.iter().any(|(key, _)| key == "XRAY_TUN_FD"),
            "unexpected TUN fd env: {map:?}"
        );
    }

    #[test]
    fn build_command_with_fd_sets_tun_env() {
        let snapshot = test_snapshot();
        let map = command_env_map(&snapshot, Some(17));
        assert_eq!(
            map.iter()
                .find(|(key, _)| key == "XRAY_TUN_FD")
                .map(|(_, value)| value.as_deref()),
            Some(Some("17")),
            "unexpected TUN fd env: {map:?}"
        );
    }

    #[test]
    fn spawn_with_failed_job_assignment_closes_fd() {
        let log = std::cell::RefCell::new(Vec::new());
        cleanup_failed_job_assignment(
            || {
                log.borrow_mut().push("kill");
                log.borrow_mut().push("wait");
            },
            || log.borrow_mut().push("close"),
        );
        assert_eq!(log.into_inner(), ["kill", "wait", "close"]);
    }

    #[test]
    fn android_binary_name_for_aarch64() {
        assert_eq!(android_binary_name("aarch64"), "xray-aarch64-linux-android");
        assert_eq!(android_binary_name("x86_64"), "xray-x86_64-linux-android");
    }

    #[test]
    fn android_binary_candidate_under_resource_dir() {
        let resource = Path::new("/data/app/mglaroute");
        assert_eq!(
            android_binary_candidate(resource, "aarch64"),
            Some(resource.join("binaries").join("xray-aarch64-linux-android"))
        );
    }
}
