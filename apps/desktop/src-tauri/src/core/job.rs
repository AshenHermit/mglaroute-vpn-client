//! Process-tree containment and startup recovery for xray cores.
//!
//! A Windows Job Object with `KILL_ON_JOB_CLOSE` owns every spawned core so a
//! crashed or abandoned client cannot leave children behind. Stale probe cores
//! from a prior process are reaped at startup only after image and command-line
//! verification: a recycled PID must never be killed. Non-Windows builds get
//! no-op handles and a verifier that trusts nothing.

use std::io;
use std::marker::PhantomData;
use std::path::Path;
use std::process::Child;

/// Owns a Windows Job Object. Dropping the last handle terminates every
/// process assigned to it, including grandchildren spawned by the core.
pub struct JobHandle {
    handle: isize,
    // Raw handles are not thread-safe; access is serialized by explicit impls.
    _raw_handle: PhantomData<*const ()>,
}

// The handle is owned by this struct and only ever passed to Win32 APIs.
unsafe impl Send for JobHandle {}
unsafe impl Sync for JobHandle {}

impl JobHandle {
    /// Create a kill-on-close job. Non-Windows builds return a no-op handle.
    pub fn create() -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::ffi::c_void;
            use std::mem::{size_of, zeroed};

            use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
            use windows_sys::Win32::System::JobObjects::{
                CreateJobObjectW, SetInformationJobObject, JobObjectExtendedLimitInformation,
                JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };

            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
            info.BasicLimitInformation = JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..unsafe { zeroed() }
            };
            let ok = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == FALSE {
                let error = io::Error::last_os_error();
                unsafe { CloseHandle(handle) };
                return Err(error);
            }

            Ok(Self {
                handle: handle as isize,
                _raw_handle: PhantomData,
            })
        }
        #[cfg(not(windows))]
        {
            Ok(Self::noop())
        }
    }

    /// Handle that owns nothing. Tests and non-Windows builds use this.
    pub fn noop() -> Self {
        Self {
            handle: 0,
            _raw_handle: PhantomData,
        }
    }

    /// Move an already spawned child into this job. Called immediately after
    /// spawn so no child or grandchild can escape containment.
    #[cfg(windows)]
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        use std::ffi::c_void;
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Foundation::FALSE;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let ok = unsafe {
            AssignProcessToJobObject(self.handle as *mut c_void, child.as_raw_handle())
        };
        if ok == FALSE {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[cfg(not(windows))]
    pub fn assign(&self, _child: &Child) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            if self.handle != 0 {
                use windows_sys::Win32::Foundation::CloseHandle;
                unsafe { CloseHandle(self.handle as *mut std::ffi::c_void) };
            }
        }
    }
}

/// Remove stale probe sidecars, killing only cores that still match the
/// recorded PID *and* the config filename in their command line. `verify` and
/// `kill` are injected so the recycled-PID case is testable without spawning
/// real processes. Returns the number of reaped cores.
pub fn reap_orphans_with(
    temp_dir: &Path,
    mut verify: impl FnMut(u32, &Path) -> bool,
    mut kill: impl FnMut(u32),
) -> io::Result<usize> {
    let entries = match std::fs::read_dir(temp_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };

    let mut reaped = 0;
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("mglaroute-probe-") || !name.ends_with(".pid") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(pid) = raw.trim().parse::<u32>() else {
            continue;
        };
        let config_path = path.with_extension("json");
        // A failed verification leaves the sidecar in place for inspection.
        if !verify(pid, &config_path) {
            continue;
        }
        kill(pid);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&config_path);
        reaped += 1;
    }
    Ok(reaped)
}

/// Production reaper: image name AND command line must match before the tree
/// is forcibly killed. Neither the command line nor any config content is
/// logged.
pub fn reap_orphans(temp_dir: &Path) -> io::Result<usize> {
    reap_orphans_with(temp_dir, verify_probe_process, |pid| {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    })
}

#[cfg(windows)]
fn verify_probe_process(pid: u32, config_path: &Path) -> bool {
    use std::process::Command;

    let listing = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output();
    let Ok(listing) = listing else {
        return false;
    };
    if !listing.status.success() {
        return false;
    }
    if !String::from_utf8_lossy(&listing.stdout)
        .to_ascii_lowercase()
        .contains("xray.exe")
    {
        return false;
    }

    let Some(config_name) = config_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    // PID is numeric; the config name is compared, never interpolated.
    let script = format!("(Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\").CommandLine");
    let Ok(cmdline) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    else {
        return false;
    };
    if !cmdline.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&cmdline.stdout);
    text.contains(config_name)
}

#[cfg(not(windows))]
fn verify_probe_process(_pid: u32, _config_path: &Path) -> bool {
    false
}
