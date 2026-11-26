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
    /// Allocate a unique pair of paths in the system temp directory.
    pub fn new() -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let stem = format!("mglaroute-probe-{}-{nanos}", std::process::id());
        Ok(Self {
            config_path: std::env::temp_dir().join(format!("{stem}.json")),
            pid_path: std::env::temp_dir().join(format!("{stem}.pid")),
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
