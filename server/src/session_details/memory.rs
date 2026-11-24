//! The Environment section's RAM row: how much memory the session's processes use right now.
//!
//! CDXC:SessionDetails 2026-11-02 WHY:
//! The number is the titlebar Resources panel's session row, measured where the session runs so the web build, the phone and remote computers see it too: every process of the session's zmx daemon (`zmx run|attach|watch-title <name>`) and everything below it (agent, shells, tools), summed from macOS's phys_footprint ledger (Activity Monitor's Memory column) and from rss elsewhere. It is read with the panel's own poll, never on a timer of its own.
//! SEE-ALSO: apps/desktop/src/app/titlebar/resources_snapshot.rs (the Resources panel's session rows), apps/desktop/src/app/helpers/titlebar/resource_monitor.rs (the same ledger read).

use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::sync::OnceLock;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use super::async_cache::{AsyncSectionCache, SectionLoad};

/// How long one `ps` process table snapshot is reused across sessions and panel polls.
#[cfg(unix)]
const PROCESS_TABLE_FRESH_FOR: Duration = Duration::from_secs(10);
/// Hard cap on a single `ps` invocation.
#[cfg(unix)]
const PS_TIMEOUT: Duration = Duration::from_secs(2);

/// One `ps -axo pid=,ppid=,rss=,command=` line.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Clone)]
struct Process {
    pid: u32,
    ppid: u32,
    rss_kb: u64,
    command: String,
}

#[cfg_attr(not(unix), allow(dead_code))]
fn parse_processes(output: &str) -> Vec<Process> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let ppid = fields.next()?.parse().ok()?;
            let rss_kb = fields.next()?.parse().ok()?;
            let command = fields.collect::<Vec<_>>().join(" ");
            (!command.is_empty()).then_some(Process {
                pid,
                ppid,
                rss_kb,
                command,
            })
        })
        .collect()
}

/// The zmx session a `zmx run|attach|watch-title <name>` process belongs to.
#[cfg_attr(not(unix), allow(dead_code))]
fn zmx_session_name(command: &str) -> Option<&str> {
    let mut args = command.split_whitespace();
    let executable = std::path::Path::new(args.next()?).file_name()?.to_str()?;
    if !matches!(executable, "zmx" | "zmx.exe")
        || !matches!(args.next()?, "run" | "attach" | "watch-title")
    {
        return None;
    }
    args.next()
}

/// The pids of zmx session `zmx_name` and of everything below them.
#[cfg_attr(not(unix), allow(dead_code))]
fn session_tree(processes: &[Process], zmx_name: &str) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        children.entry(process.ppid).or_default().push(process.pid);
    }
    let mut queue: Vec<u32> = processes
        .iter()
        .filter(|process| zmx_session_name(&process.command) == Some(zmx_name))
        .map(|process| process.pid)
        .collect();
    let mut seen = HashSet::new();
    while let Some(pid) = queue.pop() {
        if seen.insert(pid) {
            queue.extend(children.get(&pid).into_iter().flatten().copied());
        }
    }
    seen.into_iter().collect()
}

#[cfg(target_os = "macos")]
fn phys_footprint_bytes(pid: u32) -> Option<u64> {
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV0 {
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wkups: u64,
        ri_interrupt_wkups: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
    }

    unsafe extern "C" {
        fn proc_pid_rusage(
            pid: std::ffi::c_int,
            flavor: std::ffi::c_int,
            buffer: *mut std::ffi::c_void,
        ) -> std::ffi::c_int;
    }

    let mut info = RusageInfoV0::default();
    let status = unsafe {
        proc_pid_rusage(
            pid as std::ffi::c_int,
            0,
            (&raw mut info).cast::<std::ffi::c_void>(),
        )
    };
    (status == 0).then_some(info.ri_phys_footprint)
}

#[cfg(unix)]
fn process_table_cache() -> &'static AsyncSectionCache<(), Vec<Process>> {
    static CACHE: OnceLock<AsyncSectionCache<(), Vec<Process>>> = OnceLock::new();
    CACHE.get_or_init(|| AsyncSectionCache::new(1))
}

#[cfg(unix)]
fn fetch_process_table() -> Vec<Process> {
    let mut command = std::process::Command::new("ps");
    command
        .args(["-axo", "pid=,ppid=,rss=,command="])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + PS_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Vec::new();
                }
                break;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    let Ok(bytes) = reader.join() else {
        return Vec::new();
    };
    parse_processes(&String::from_utf8_lossy(&bytes))
}

#[cfg(unix)]
fn memory_bytes_from_table(processes: &[Process], zmx_name: &str) -> Option<u64> {
    let tree = session_tree(processes, zmx_name);
    if tree.is_empty() {
        return None;
    }
    let rss: HashMap<u32, u64> = processes
        .iter()
        .map(|process| (process.pid, process.rss_kb * 1024))
        .collect();
    Some(
        tree.iter()
            .map(|pid| {
                #[cfg(target_os = "macos")]
                if let Some(bytes) = phys_footprint_bytes(*pid) {
                    return bytes;
                }
                rss.get(pid).copied().unwrap_or_default()
            })
            .sum(),
    )
}

/// Non-blocking RAM lookup: returns cached value immediately (refreshing in background if stale)
/// or `None` while the first `ps` snapshot runs in the background.
#[cfg(unix)]
pub(super) fn read_session_memory_bytes_progressive(zmx_name: &str) -> Option<u64> {
    match process_table_cache().get_or_start(
        (),
        PROCESS_TABLE_FRESH_FOR,
        false,
        fetch_process_table,
    ) {
        SectionLoad::Ready(processes) => memory_bytes_from_table(&processes, zmx_name),
        SectionLoad::Pending => None,
    }
}

/// The bytes zmx session `zmx_name` uses, or `None` when none of its processes is running.
/// Cached process-wide for [`PROCESS_TABLE_FRESH_FOR`] and time-boxed.
#[cfg(unix)]
pub(super) fn read_session_memory_bytes(zmx_name: &str) -> Option<u64> {
    let processes = process_table_cache().get_or_wait(
        (),
        PROCESS_TABLE_FRESH_FOR,
        false,
        PS_TIMEOUT,
        fetch_process_table,
    )?;
    memory_bytes_from_table(&processes, zmx_name)
}

/// Native Windows sessions run under wmx, which this read does not walk yet; the row stays hidden.
#[cfg(not(unix))]
pub(super) fn read_session_memory_bytes_progressive(_zmx_name: &str) -> Option<u64> {
    None
}

/// Native Windows sessions run under wmx, which this read does not walk yet; the row stays hidden.
#[cfg(not(unix))]
pub(super) fn read_session_memory_bytes(_zmx_name: &str) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "\
  100     1  2048 /Applications/Ghostex.app/Contents/Resources/bin/zmx run gx-abc
  101   100 409600 claude --resume x
  102   101  1024 /bin/zsh -c npm test
  103     1  4096 /Applications/Ghostex.app/Contents/Resources/bin/zmx run gx-other
  104   103 999999 codex
  105     9   512 zmx attach gx-abc
";

    #[test]
    fn the_tree_is_the_sessions_zmx_processes_and_everything_below_them() {
        let processes = parse_processes(PS);
        let mut tree = session_tree(&processes, "gx-abc");
        tree.sort_unstable();
        assert_eq!(tree, vec![100, 101, 102, 105]);
    }

    #[test]
    fn a_session_with_no_processes_has_no_tree() {
        assert!(session_tree(&parse_processes(PS), "gx-gone").is_empty());
    }

    #[test]
    fn only_zmx_session_verbs_name_a_session() {
        assert_eq!(
            zmx_session_name("/x/zmx watch-title gx-abc"),
            Some("gx-abc")
        );
        assert_eq!(zmx_session_name("/x/zmx list"), None);
        assert_eq!(zmx_session_name("/x/notzmx run gx-abc"), None);
    }
}
