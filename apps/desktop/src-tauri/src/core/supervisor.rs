//! Xray process supervision: binary discovery, command construction, port
//! readiness and child-process lifetime. No config generation or network
//! probing here — callers pass an immutable [`ConfigSnapshot`].

use std::env;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::job::JobHandle;
use crate::core::snapshot::ConfigSnapshot;

pub const BINARY_PATH_ENV: &str = "XRAY_BINARY_PATH";

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
pub fn binary_path(override_path: Option<&str>) -> PathBuf {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let env_path = env::var_os(BINARY_PATH_ENV);
    binary_path_from(exe_dir.as_deref(), override_path, env_path.as_deref())
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

/// Build a xray invocation for one snapshot. stdout/stderr go to null so raw
/// config (which holds credentials) never reaches a log sink.
pub fn build_command(snapshot: &ConfigSnapshot) -> Result<Command> {
    let mut command = Command::new(binary_path(None));
    command.arg("run").arg("-c").arg(&snapshot.config_path);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    apply_hidden_console(&mut command);
    Ok(command)
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
        let mut child = build_command(snapshot)?.spawn()?;
        if let Some(job) = job {
            if let Err(error) = job.assign(&child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SupervisorError::Spawn(error));
            }
        }
        Ok(Self { child })
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

/// Pure resolution helper used by [`binary_path`] and tested without touching
/// the real executable directory.
pub fn binary_path_from(
    exe_dir: Option<&std::path::Path>,
    override_path: Option<&str>,
    env_path: Option<&std::ffi::OsStr>,
) -> PathBuf {
    if let Some(value) = env_path.filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    if let Some(value) = override_path.filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    if let Some(dir) = exe_dir {
        let sidecar = dir.join(if cfg!(windows) { "xray.exe" } else { "xray" });
        if sidecar.exists() {
            return sidecar;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(if cfg!(windows) { "xray.exe" } else { "xray" })
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

    use crate::core::job::{reap_orphans_with, reap_orphans_with_liveness, JobHandle};
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
}
