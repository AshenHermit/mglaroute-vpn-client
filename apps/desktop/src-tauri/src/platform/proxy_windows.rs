//! Windows system proxy: per-user WinInet registry settings plus a refresh
//! broadcast. No admin, no service, no driver, no environment-proxy fallback.
//!
//! The optional state file (app-data `proxy_previous.json`) captures the state
//! seen before the first `apply`. It is never rewritten while connected and is
//! deleted only after a successful `restore`. Startup restore is wired by the
//! caller (Task 13), which loads the file and calls [`ProxyBackend::restore`].
//!
//! Follow-up (not MVP): RAS-entry proxy parity with v2rayN. The registry path
//! covers the per-user WinInet settings used by the desktop app; per-dialup
//! entries would require `RasEnumEntriesW` and a separate restore model.

use std::path::PathBuf;
use std::sync::Mutex;

use super::proxy::{
    override_string, persist_previous, proxy_string, CaptureGate, PreviousProxy, ProxyBackend,
    ProxyError, Result,
};

const INTERNET_SETTINGS: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";
const VALUE_ENABLE: &str = "ProxyEnable";
const VALUE_SERVER: &str = "ProxyServer";
const VALUE_OVERRIDE: &str = "ProxyOverride";
const VALUE_AUTO_CONFIG: &str = "AutoConfigURL";

/// Windows system-proxy backend.
///
/// `state_file: None` leaves persistence to the caller (the captured value is
/// passed to `restore`). `Some(path)` makes the backend disk-backed: first
/// `apply` writes the current settings there, `restore` removes it on success.
pub struct WindowsProxy {
    state_file: Option<PathBuf>,
    /// Suppresses re-capture even when the first `apply` fails midway.
    capture: CaptureGate,
    connected: Mutex<bool>,
}

impl WindowsProxy {
    /// Registry-only backend; the caller owns the captured previous state.
    pub fn new() -> Self {
        Self {
            state_file: None,
            capture: CaptureGate::default(),
            connected: Mutex::new(false),
        }
    }

    /// Backend that also persists and clears the captured state on disk.
    pub fn with_state_file(path: impl Into<PathBuf>) -> Self {
        Self {
            state_file: Some(path.into()),
            capture: CaptureGate::default(),
            connected: Mutex::new(false),
        }
    }

    /// Read the current per-user WinInet proxy configuration.
    pub fn current(&self) -> Result<PreviousProxy> {
        #[cfg(windows)]
        {
            read_current()
        }
        #[cfg(not(windows))]
        {
            Err(ProxyError::Unsupported)
        }
    }
}

impl Default for WindowsProxy {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyBackend for WindowsProxy {
    fn current(&self) -> Result<PreviousProxy> {
        WindowsProxy::current(self)
    }

    fn apply(&self, port: u16, exclusions: &[String]) -> Result<()> {
        #[cfg(windows)]
        {
            let mut connected = self
                .connected
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !*connected {
                if let Some(path) = &self.state_file {
                    // Capture exactly once. `capture_once` flips its flag as
                    // soon as persistence succeeds and before any registry
                    // write, so a failed `set_proxy`/`refresh` cannot make a
                    // retry capture our own partially-applied settings.
                    self.capture.capture_once(|| {
                        let previous = read_current()?;
                        persist_previous(path, &previous)?;
                        Ok(previous)
                    })?;
                }
            }

            set_proxy(&PreviousProxy::Manual {
                server: proxy_string(port),
                override_list: override_string(exclusions),
            })?;
            refresh()?;
            *connected = true;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (port, exclusions);
            Err(ProxyError::Unsupported)
        }
    }

    fn restore(&self, previous: &PreviousProxy) -> Result<()> {
        #[cfg(windows)]
        {
            set_proxy(previous)?;
            refresh()?;

            if let Some(path) = &self.state_file {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            // Re-arm only after the previous state is fully written and the
            // disk snapshot is gone; a failed restore keeps the capture.
            let mut connected = self
                .connected
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *connected = false;
            self.capture.reset();
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = previous;
            Err(ProxyError::Unsupported)
        }
    }
}

#[cfg(windows)]
fn read_current() -> Result<PreviousProxy> {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, HKEY, KEY_READ};

    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let status = RegOpenKeyExW(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            wide(INTERNET_SETTINGS).as_ptr(),
            0,
            KEY_READ,
            &mut key,
        );
        if status != ERROR_SUCCESS {
            return Err(ProxyError::Registry(status));
        }
        let result = current_from_key(key);
        RegCloseKey(key);
        result
    }
}

#[cfg(windows)]
unsafe fn current_from_key(key: windows_sys::Win32::System::Registry::HKEY) -> Result<PreviousProxy> {
    let enabled = query_dword(key, VALUE_ENABLE).unwrap_or(0) != 0;
    let pac = query_string(key, VALUE_AUTO_CONFIG).filter(|url| !url.is_empty());
    if !enabled {
        return Ok(match pac {
            Some(url) => PreviousProxy::Pac(url),
            None => PreviousProxy::Direct,
        });
    }
    Ok(PreviousProxy::Manual {
        server: query_string(key, VALUE_SERVER).unwrap_or_default(),
        override_list: query_string(key, VALUE_OVERRIDE).unwrap_or_default(),
    })
}

#[cfg(windows)]
fn set_proxy(previous: &PreviousProxy) -> Result<()> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, KEY_WRITE,
    };

    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let status = RegOpenKeyExW(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            wide(INTERNET_SETTINGS).as_ptr(),
            0,
            KEY_WRITE,
            &mut key,
        );
        if status != ERROR_SUCCESS {
            return Err(ProxyError::Registry(status));
        }
        let result = write_key(key, previous);
        RegCloseKey(key);
        result
    }
}

#[cfg(windows)]
unsafe fn write_key(
    key: windows_sys::Win32::System::Registry::HKEY,
    previous: &PreviousProxy,
) -> Result<()> {
    match previous {
        PreviousProxy::Direct => {
            set_dword(key, VALUE_ENABLE, 0)?;
            delete_value(key, VALUE_AUTO_CONFIG);
        }
        PreviousProxy::Pac(url) => {
            set_dword(key, VALUE_ENABLE, 0)?;
            set_string(key, VALUE_AUTO_CONFIG, url)?;
        }
        PreviousProxy::Manual {
            server,
            override_list,
        } => {
            set_dword(key, VALUE_ENABLE, 1)?;
            set_string(key, VALUE_SERVER, server)?;
            set_string(key, VALUE_OVERRIDE, override_list)?;
            delete_value(key, VALUE_AUTO_CONFIG);
        }
    }
    Ok(())
}

/// Broadcast the WinInet settings change so open applications pick it up.
#[cfg(windows)]
fn refresh() -> Result<()> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };

    unsafe {
        let changed = InternetSetOptionW(
            std::ptr::null(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null(),
            0,
        );
        if changed == 0 {
            return Err(ProxyError::WindowsApi(GetLastError()));
        }
        let refreshed = InternetSetOptionW(
            std::ptr::null(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null(),
            0,
        );
        if refreshed == 0 {
            return Err(ProxyError::WindowsApi(GetLastError()));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
const ERROR_SUCCESS: u32 = windows_sys::Win32::Foundation::ERROR_SUCCESS;

#[cfg(windows)]
unsafe fn query_dword(
    key: windows_sys::Win32::System::Registry::HKEY,
    name: &str,
) -> Option<u32> {
    use windows_sys::Win32::System::Registry::RegQueryValueExW;

    let name = wide(name);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = RegQueryValueExW(
        key,
        name.as_ptr(),
        std::ptr::null(),
        std::ptr::null_mut(),
        &mut value as *mut u32 as *mut u8,
        &mut size,
    );
    if status == ERROR_SUCCESS && size as usize == std::mem::size_of::<u32>() {
        Some(value)
    } else {
        None
    }
}

#[cfg(windows)]
unsafe fn query_string(
    key: windows_sys::Win32::System::Registry::HKEY,
    name: &str,
) -> Option<String> {
    use windows_sys::Win32::System::Registry::RegQueryValueExW;

    let name = wide(name);
    let mut size = 0u32;
    let status = RegQueryValueExW(
        key,
        name.as_ptr(),
        std::ptr::null(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &mut size,
    );
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }

    let mut buffer = vec![0u16; size as usize / 2 + 1];
    let status = RegQueryValueExW(
        key,
        name.as_ptr(),
        std::ptr::null(),
        std::ptr::null_mut(),
        buffer.as_mut_ptr() as *mut u8,
        &mut size,
    );
    if status != ERROR_SUCCESS {
        return None;
    }
    let units = &buffer[..(size as usize / 2).min(buffer.len())];
    let units = units
        .iter()
        .position(|&unit| unit == 0)
        .map_or(units, |index| &units[..index]);
    Some(String::from_utf16_lossy(units))
}

#[cfg(windows)]
unsafe fn set_dword(
    key: windows_sys::Win32::System::Registry::HKEY,
    name: &str,
    value: u32,
) -> Result<()> {
    use windows_sys::Win32::System::Registry::{RegSetValueExW, REG_DWORD};

    let name = wide(name);
    let status = RegSetValueExW(
        key,
        name.as_ptr(),
        0,
        REG_DWORD,
        &value as *const u32 as *const u8,
        std::mem::size_of::<u32>() as u32,
    );
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(ProxyError::Registry(status))
    }
}

#[cfg(windows)]
unsafe fn set_string(
    key: windows_sys::Win32::System::Registry::HKEY,
    name: &str,
    value: &str,
) -> Result<()> {
    use windows_sys::Win32::System::Registry::{RegSetValueExW, REG_SZ};

    let name = wide(name);
    let data = wide(value);
    let status = RegSetValueExW(
        key,
        name.as_ptr(),
        0,
        REG_SZ,
        data.as_ptr() as *const u8,
        (data.len() * std::mem::size_of::<u16>()) as u32,
    );
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(ProxyError::Registry(status))
    }
}

/// Missing value is expected in Direct mode; all other failures are ignored
/// too because a leftover `AutoConfigURL` does not affect `ProxyEnable=1`.
#[cfg(windows)]
unsafe fn delete_value(key: windows_sys::Win32::System::Registry::HKEY, name: &str) {
    use windows_sys::Win32::System::Registry::RegDeleteValueW;

    let name = wide(name);
    RegDeleteValueW(key, name.as_ptr());
}
