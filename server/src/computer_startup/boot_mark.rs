//! Which boot (and on Windows, which sign-in) this gxserver runs in, kept across gxserver starts
//! so the first start after the computer went down can tell.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

const FILE_NAME: &str = "computer-startup.json";

/// Boot times read from a clock can move by a few seconds when the clock is corrected.
const BOOT_TIME_TOLERANCE_SECONDS: i64 = 120;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BootMark {
    /// Linux's `/proc/sys/kernel/random/boot_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
    /// When the computer booted, in Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub booted_at: Option<i64>,
    /// Windows: the sign-in (logon session) gxserver runs in, see `current`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<String>,
}

impl BootMark {
    /// `None` when the two marks cannot be compared, which never counts as a restart.
    fn restarted_since(&self, earlier: &BootMark) -> Option<bool> {
        if let (Some(now), Some(before)) = (&self.sign_in, &earlier.sign_in) {
            if now != before {
                return Some(true);
            }
        }
        if let (Some(now), Some(before)) = (&self.boot_id, &earlier.boot_id) {
            return Some(now != before);
        }
        if let (Some(now), Some(before)) = (self.booted_at, earlier.booted_at) {
            return Some((now - before).abs() > BOOT_TIME_TOLERANCE_SECONDS);
        }
        None
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredState {
    #[serde(default)]
    boot: Option<BootMark>,
}

/// Saves this boot's mark and says whether the computer restarted since the previous gxserver
/// saved one. The first start ever (nothing saved) is not a restart.
pub(crate) fn note_this_boot(paths: &GxserverPaths) -> bool {
    let Some(current) = current() else {
        return false;
    };
    let file = paths.app_state_dir.join(FILE_NAME);
    let previous = fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<StoredState>(&text).ok())
        .and_then(|state| state.boot);
    if previous.as_ref() != Some(&current) {
        if let Ok(text) = serde_json::to_string_pretty(&StoredState {
            boot: Some(current.clone()),
        }) {
            let _ = fs::create_dir_all(&paths.app_state_dir);
            let temporary = file.with_extension("json.tmp");
            if fs::write(&temporary, text).is_ok() {
                let _ = fs::rename(&temporary, &file);
            }
        }
    }
    previous
        .and_then(|previous| current.restarted_since(&previous))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn current() -> Option<BootMark> {
    let mut boot = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut size = std::mem::size_of::<libc::timeval>();
    let name = c"kern.boottime";
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut boot as *mut libc::timeval).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && boot.tv_sec > 0).then(|| BootMark {
        booted_at: Some(boot.tv_sec),
        ..BootMark::default()
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn current() -> Option<BootMark> {
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    let booted_at = fs::read_to_string("/proc/stat").ok().and_then(|stat| {
        stat.lines()
            .find_map(|line| line.strip_prefix("btime "))
            .and_then(|value| value.trim().parse::<i64>().ok())
    });
    (boot_id.is_some() || booted_at.is_some()).then_some(BootMark {
        boot_id,
        booted_at,
        sign_in: None,
    })
}

/// CDXC:ComputerStartup 2026-11-06 WHY: Windows' default Fast Startup turns Shut down into signing out and hibernating the kernel, so the boot time stays the same while every session daemon ended with the sign-out. The logon session of this process (its token's AuthenticationId, new at every sign-in) catches that case; the boot time catches a power loss.
#[cfg(windows)]
fn current() -> Option<BootMark> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TokenStatistics, TOKEN_QUERY, TOKEN_STATISTICS},
        System::{
            Threading::{GetCurrentProcess, OpenProcessToken},
            WindowsProgramming::QueryInterruptTime,
        },
    };
    let mut interrupt_time = 0u64;
    unsafe { QueryInterruptTime(&mut interrupt_time) };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let booted_at = now - (interrupt_time / 10_000_000) as i64;
    let sign_in = unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            None
        } else {
            let mut statistics: TOKEN_STATISTICS = std::mem::zeroed();
            let mut size = 0u32;
            let read = GetTokenInformation(
                token,
                TokenStatistics,
                (&mut statistics as *mut TOKEN_STATISTICS).cast(),
                std::mem::size_of::<TOKEN_STATISTICS>() as u32,
                &mut size,
            );
            CloseHandle(token);
            (read != 0).then(|| {
                format!(
                    "{:x}-{:x}",
                    statistics.AuthenticationId.HighPart, statistics.AuthenticationId.LowPart
                )
            })
        }
    };
    Some(BootMark {
        boot_id: None,
        booted_at: Some(booted_at),
        sign_in,
    })
}
