//! Xray process supervision: binary discovery, command construction, port
//! readiness and child-process lifetime. No config generation or network
//! probing here — callers pass an immutable [`ConfigSnapshot`].

use std::env;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

/// Resolve the xray binary: `XRAY_BINARY_PATH`, then explicit override, then
/// the build-time bundled `binaries/xray.exe` (Windows x64) / `binaries/xray`.
pub fn binary_path(override_path: Option<&str>) -> PathBuf {
    if let Some(value) = env::var_os(BINARY_PATH_ENV).filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    if let Some(value) = override_path.filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(if cfg!(windows) { "xray.exe" } else { "xray" })
}

/// Build a xray invocation for one snapshot. stdout/stderr go to null so raw
/// config (which holds credentials) never reaches a log sink.
pub fn build_command(snapshot: &ConfigSnapshot) -> Result<Command> {
    let mut command = Command::new(binary_path(None));
    command.arg("run").arg("-c").arg(&snapshot.config_path);
    command.stdout(Stdio::null()).stderr(Stdio::null());
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

/// Owns a spawned xray child and reaps it on `stop`.
///
/// Follow-up: wrap the child in a Windows Job Object so grandchild processes
/// are killed too; not required for MVP.
pub struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    /// Spawn xray for `snapshot` using the null-redirected command builder.
    pub fn spawn(snapshot: &ConfigSnapshot) -> Result<Self> {
        Ok(Self {
            child: build_command(snapshot)?.spawn()?,
        })
    }

    /// Adopt an already spawned child (e.g. a temporary core for probing).
    pub fn new(child: Child) -> Self {
        Self { child }
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

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::net::{SocketAddr, TcpListener};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, Instant};
    use std::{env, process::Command};

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
