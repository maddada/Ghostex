//! This user's processes on Linux, read from `/proc`.

use super::processes::ProcessRow;

pub(super) fn user_processes() -> Vec<ProcessRow> {
    let uid = unsafe { libc::getuid() };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let pid: u32 = entry.ok()?.file_name().to_str()?.parse().ok()?;
            if pid <= 1 || owner(pid)? != uid {
                return None;
            }
            let (ppid, start) = stat_fields(pid)?;
            let command = std::fs::read(format!("/proc/{pid}/cmdline"))
                .map(|bytes| {
                    bytes
                        .split(|byte| *byte == 0)
                        .filter(|part| !part.is_empty())
                        .map(|part| String::from_utf8_lossy(part).into_owned())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            // Only a process init (or a subreaper) adopted can have left its session's tree.
            let session_tag = (ppid == 1)
                .then(|| environment_value(pid, "ZMX_SESSION"))
                .flatten();
            Some(ProcessRow {
                pid,
                ppid,
                start,
                command,
                session_tag,
            })
        })
        .collect()
}

/// Resident memory (VmRSS).
pub(super) fn resident_bytes(pid: u32) -> Option<u64> {
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(pages * u64::try_from(page).ok()?)
}

/// When `pid` started, in clock ticks since boot (field 22 of `/proc/<pid>/stat`).
pub(super) fn start_time(pid: u32) -> Option<u64> {
    stat_fields(pid).map(|(_, start)| start)
}

fn owner(pid: u32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// (ppid, start time) from `/proc/<pid>/stat`, read after the command name, which may hold spaces
/// and parentheses.
fn stat_fields(pid: u32) -> Option<(u32, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let fields = rest.split_whitespace().collect::<Vec<_>>();
    // `rest` starts at field 3 (state): ppid is field 4, starttime field 22.
    Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}

fn environment_value(pid: u32, name: &str) -> Option<String> {
    let bytes = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{name}=");
    bytes
        .split(|byte| *byte == 0)
        .find_map(|entry| entry.strip_prefix(prefix.as_bytes()))
        .map(|value| String::from_utf8_lossy(value).into_owned())
}
