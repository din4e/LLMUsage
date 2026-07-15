use std::{
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS},
    System::Threading::CreateMutexW,
    UI::WindowsAndMessaging::FindWindowW,
};

const APP_IDENTIFIER: &str = "cn.ttpublic.llmusage";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(10);

/// Keeps the startup mutex open for the entire lifetime of the primary process.
#[cfg(target_os = "windows")]
pub struct StartupGuard(isize);

#[cfg(target_os = "windows")]
impl Drop for StartupGuard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0 as _) };
    }
}

/// Serializes process startup until the single-instance plugin has created its
/// event target window. This closes the plugin's small startup race on Windows.
#[cfg(target_os = "windows")]
pub fn acquire_startup_guard() -> Result<Option<StartupGuard>, String> {
    let mutex_name = wide(&format!("{APP_IDENTIFIER}-startup-guard"));
    let handle = unsafe { CreateMutexW(std::ptr::null(), false.into(), mutex_name.as_ptr()) };
    if handle == 0 {
        return Err("无法创建单实例启动锁".into());
    }

    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return Ok(Some(StartupGuard(handle as _)));
    }

    unsafe { CloseHandle(handle) };
    if wait_for_target(
        primary_event_target_exists,
        STARTUP_TIMEOUT,
        RETRY_INTERVAL,
        thread::sleep,
    ) {
        // The plugin will forward this launch to the now-ready primary instance.
        Ok(None)
    } else {
        Err("已有实例仍在启动，未创建重复实例".into())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn acquire_startup_guard() -> Result<Option<()>, String> {
    Ok(None)
}

fn wait_for_target(
    mut target_exists: impl FnMut() -> bool,
    timeout: Duration,
    retry_interval: Duration,
    mut sleep: impl FnMut(Duration),
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if target_exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(retry_interval);
    }
}

#[cfg(target_os = "windows")]
fn primary_event_target_exists() -> bool {
    let class_name = wide(&format!("{APP_IDENTIFIER}-sic"));
    let window_name = wide(&format!("{APP_IDENTIFIER}-siw"));
    unsafe { FindWindowW(class_name.as_ptr(), window_name.as_ptr()) != 0 }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn waits_until_the_primary_event_target_is_ready() {
        let mut attempts = 0;
        let mut sleeps = Vec::new();

        let ready = wait_for_target(
            || {
                attempts += 1;
                attempts == 3
            },
            Duration::from_secs(1),
            Duration::from_millis(10),
            |duration| sleeps.push(duration),
        );

        assert!(ready);
        assert_eq!(attempts, 3);
        assert_eq!(sleeps, vec![Duration::from_millis(10); 2]);
    }
}
