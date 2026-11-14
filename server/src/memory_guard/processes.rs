//! This user's processes as the guard sees them, and the signals that stop them.
//!
//! CDXC:MemoryGuard 2026-11-14 WHY:
//! A session is more than the process tree under its zmx daemon: a dev server started with `&`,
//! `nohup` or `setsid` leaves the tree when its shell exits and is adopted by launchd or init.
//! zmx puts `ZMX_SESSION=<name>` in the environment of the session's shell (daemonize.zig), every
//! child inherits it, so such a process is found by that tag and counted with its session. Every
//! signal re-checks the process's start time first, so a pid the system gave to another program
//! after the snapshot is never touched.

/// One process of this user, from one scan.
#[derive(Clone, Debug)]
pub(crate) struct ProcessRow {
    pub(crate) pid: u32,
    pub(crate) ppid: u32,
    /// When it started, in the system's own units: what makes `pid` this process and no other.
    pub(crate) start: u64,
    /// argv joined by spaces; empty when it cannot be read.
    pub(crate) command: String,
    /// `ZMX_SESSION` from its environment, read for processes outside any session tree (adopted
    /// by launchd or init) where the system lets it be read.
    pub(crate) session_tag: Option<String>,
}

/// A process to signal: its pid and the start time that proves it is still the same process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Member {
    pub(crate) pid: u32,
    pub(crate) start: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopSignal {
    /// Asks it to end (SIGTERM).
    Term,
    /// Ends it (SIGKILL), for what is still running after the grace period.
    Kill,
}

/// Whether this system has the process scan and signals the guard needs.
pub(crate) fn supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux"))
}

/// Every process of this user except gxserver itself.
pub(crate) fn snapshot_user_processes() -> Vec<ProcessRow> {
    let own = std::process::id();
    #[cfg(target_os = "macos")]
    let rows = super::processes_macos::user_processes();
    #[cfg(target_os = "linux")]
    let rows = super::processes_linux::user_processes();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let rows: Vec<ProcessRow> = Vec::new();
    rows.into_iter().filter(|row| row.pid != own).collect()
}

/// How much memory `pid` uses: macOS's phys_footprint (Activity Monitor's Memory column), resident
/// size on Linux; 0 when it cannot be read.
pub(crate) fn memory_bytes(pid: u32) -> u64 {
    #[cfg(target_os = "macos")]
    return super::processes_macos::footprint_bytes(pid).unwrap_or(0);
    #[cfg(target_os = "linux")]
    return super::processes_linux::resident_bytes(pid).unwrap_or(0);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        0
    }
}

fn start_of(pid: u32) -> Option<u64> {
    #[cfg(target_os = "macos")]
    return super::processes_macos::start_time(pid);
    #[cfg(target_os = "linux")]
    return super::processes_linux::start_time(pid);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

/// The members still running as the same processes.
pub(crate) fn alive_members(members: &[Member]) -> Vec<Member> {
    members
        .iter()
        .copied()
        .filter(|member| start_of(member.pid) == Some(member.start))
        .collect()
}

/// Sends `signal` to each member that is still the same process; the number signalled.
pub(crate) fn signal_processes(members: &[Member], signal: StopSignal) -> usize {
    let own = std::process::id();
    let mut signalled = 0;
    for member in alive_members(members) {
        if member.pid <= 1 || member.pid == own {
            continue;
        }
        #[cfg(unix)]
        {
            let number = match signal {
                StopSignal::Term => libc::SIGTERM,
                StopSignal::Kill => libc::SIGKILL,
            };
            if unsafe { libc::kill(member.pid as libc::pid_t, number) } == 0 {
                signalled += 1;
            }
        }
        #[cfg(not(unix))]
        let _ = signal;
    }
    signalled
}
