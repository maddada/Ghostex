//! The system's memory pressure, read the way each system reports it.
//!
//! CDXC:MemoryGuard 2026-11-14 WHY:
//! macOS has no fixed "free memory" line worth acting on: it compresses and swaps long before it
//! runs out, so the guard follows the kernel's own level (`kern.memorystatus_vm_pressure_level`:
//! 1 normal, 2 warning, 4 critical, the green, yellow and red of Activity Monitor). Linux reports
//! stall time instead (PSI, `/proc/pressure/memory`): "full" means every task was stalled on
//! memory, the closest thing to the kernel saying it is critical; MemAvailable backs it up on
//! kernels without PSI. Windows is not read yet (the guard stays off there).

/// How close the system is to running out of memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PressureLevel {
    Normal,
    Warning,
    Critical,
}

impl PressureLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "normal" => Some(Self::Normal),
            "warning" | "warn" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// The current level, or `None` where the system's level cannot be read.
///
/// A debug build (the throwaway `cargo xtask test-server`) reads the level from the file named by
/// `GHOSTEX_MEMORY_GUARD_TEST_PRESSURE_FILE` when it is set, so the stages can be proven without
/// driving the test computer itself out of memory. Release builds never read it.
pub(crate) fn read_pressure_level() -> Option<PressureLevel> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("GHOSTEX_MEMORY_GUARD_TEST_PRESSURE_FILE") {
        return std::fs::read_to_string(path)
            .ok()
            .and_then(|text| PressureLevel::parse(&text));
    }
    system_level()
}

#[cfg(target_os = "macos")]
fn system_level() -> Option<PressureLevel> {
    let mut level: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    let name = c"kern.memorystatus_vm_pressure_level";
    let result = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut level as *mut libc::c_int).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    // The kernel's DISPATCH_MEMORYPRESSURE_* bits: 1 normal, 2 warn, 4 critical.
    Some(match level {
        4 => PressureLevel::Critical,
        2 => PressureLevel::Warning,
        _ => PressureLevel::Normal,
    })
}

#[cfg(target_os = "linux")]
fn system_level() -> Option<PressureLevel> {
    let psi = std::fs::read_to_string("/proc/pressure/memory").ok();
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    Some(linux_level(psi.as_deref(), &meminfo))
}

/// PSI "full" over 10% for the last 10 seconds, or MemAvailable under 5% of RAM and under 1 GiB:
/// critical. PSI "some" over 10%, or MemAvailable under 10%: warning.
#[cfg(target_os = "linux")]
fn linux_level(psi: Option<&str>, meminfo: &str) -> PressureLevel {
    let avg10 = |kind: &str| -> Option<f64> {
        psi?.lines()
            .find(|line| line.starts_with(kind))?
            .split_whitespace()
            .find_map(|field| field.strip_prefix("avg10="))?
            .parse()
            .ok()
    };
    let kib = |key: &str| -> Option<u64> {
        meminfo
            .lines()
            .find(|line| line.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let (total, available) = (kib("MemTotal:"), kib("MemAvailable:"));
    let available_share = match (total, available) {
        (Some(total), Some(available)) if total > 0 => Some(available as f64 / total as f64),
        _ => None,
    };
    let available_bytes = available.map(|kib| kib * 1024);
    if avg10("full").is_some_and(|value| value >= 10.0)
        || (available_share.is_some_and(|share| share < 0.05)
            && available_bytes.is_some_and(|bytes| bytes < 1 << 30))
    {
        return PressureLevel::Critical;
    }
    if avg10("some").is_some_and(|value| value >= 10.0)
        || available_share.is_some_and(|share| share < 0.10)
    {
        return PressureLevel::Warning;
    }
    PressureLevel::Normal
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn system_level() -> Option<PressureLevel> {
    None
}
