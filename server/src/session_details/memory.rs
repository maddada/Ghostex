//! The Environment section's RAM row: how much memory the session's processes use right now.
//!
//! CDXC:SessionDetails 2026-11-02 WHY:
//! The number is the titlebar Resources panel's session row, measured where the session runs so the web build, the phone and remote computers see it too: every process of the session's zmx daemon (`zmx run|attach|watch-title <name>`) and everything below it (agent, shells, tools), summed from macOS's phys_footprint ledger (Activity Monitor's Memory column) and from rss elsewhere. It is read with the panel's own poll, never on a timer of its own.
//! SEE-ALSO: apps/desktop/src/app/titlebar/resources_snapshot.rs (the Resources panel's session rows), apps/desktop/src/app/helpers/titlebar/resource_monitor.rs (the same ledger read).

use std::collections::{HashMap, HashSet};

/// One `ps -axo pid=,ppid=,rss=,command=` line.
#[cfg_attr(not(unix), allow(dead_code))]
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

/// The bytes zmx session `zmx_name` uses, or `None` when none of its processes is running.
#[cfg(unix)]
pub(super) fn read_session_memory_bytes(zmx_name: &str) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,command="])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let processes = parse_processes(&String::from_utf8_lossy(&output.stdout));
    let tree = session_tree(&processes, zmx_name);
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
