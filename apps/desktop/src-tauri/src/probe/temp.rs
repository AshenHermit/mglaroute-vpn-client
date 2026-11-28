//! Temporary files for one probe core. The `.pid` sidecar holds only a decimal
//! PID and shares its stem with the generated `.json` config, so startup
//! reaping can match both without reading config contents into a log.

use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Config and PID sidecar paths for one temporary probe core.
pub struct ProbeTempFiles {
    pub config_path: PathBuf,
    pub pid_path: PathBuf,
}

impl ProbeTempFiles {
    /// Allocate a unique pair of paths in the system temp directory. Windows
    /// behaviour is unchanged; Android callers use [`Self::new_in`] with the
    /// app cache dir injected at bootstrap.
    pub fn new() -> io::Result<Self> {
        Self::new_in(&std::env::temp_dir())
    }

    /// Allocate a unique pair of paths under `base`. The base is created if
    /// missing so a fresh app cache dir works without extra bootstrap work.
    pub fn new_in(base: &std::path::Path) -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let stem = format!("mglaroute-probe-{}-{nanos}", std::process::id());
        std::fs::create_dir_all(base)?;
        Ok(Self {
            config_path: base.join(format!("{stem}.json")),
            pid_path: base.join(format!("{stem}.pid")),
        })
    }

    /// Write the core's PID as decimal digits only.
    pub fn write_pid(&self, pid: u32) -> io::Result<()> {
        std::fs::write(&self.pid_path, pid.to_string())
    }

    /// Best-effort removal of both files; called on every exit path.
    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.config_path);
        let _ = std::fs::remove_file(&self.pid_path);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Final-review Critical 2: probes on Android must write into the app cache
    /// dir injected at bootstrap, never into the process-global system temp dir.
    #[test]
    fn probe_temp_files_use_injected_base_not_system_temp() {
        let system_temp = std::env::temp_dir();
        // Deliberately choose a base outside the system temp dir so the
        // negative assertion is meaningful (tempfile::tempdir() would itself
        // live under the system temp dir on Windows).
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("probe-temp-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("create injected base");

        let files = ProbeTempFiles::new_in(&base).expect("allocate in injected base");
        assert!(
            files.config_path.starts_with(&base),
            "config must live under injected app cache dir: {}",
            files.config_path.display()
        );
        assert!(
            files.pid_path.starts_with(&base),
            "pid sidecar must live under injected app cache dir: {}",
            files.pid_path.display()
        );
        assert!(
            !files.config_path.starts_with(&system_temp),
            "config must not use system temp dir: {}",
            files.config_path.display()
        );
        assert!(
            !files.pid_path.starts_with(&system_temp),
            "pid sidecar must not use system temp dir: {}",
            files.pid_path.display()
        );

        std::fs::write(&files.config_path, "{}").expect("write config in injected base");
        files.write_pid(4242).expect("write pid in injected base");
        assert_eq!(
            std::fs::read_to_string(&files.pid_path).expect("read pid"),
            "4242"
        );
        files.cleanup();
        assert!(!files.config_path.exists());
        assert!(!files.pid_path.exists());
        let _ = std::fs::remove_dir(&base);
    }
}
