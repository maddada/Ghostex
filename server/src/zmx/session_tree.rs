//! One session's own processes: its zmx daemon (on Windows its shell) and everything below it.
//!
//! CDXC:AgentProviders 2026-11-16 WHY:
//! Every Claude account switch on a machine with about 60 sessions failed with "Could not inspect
//! the agent while exiting it" and left the session holding an empty shell (GitHub #630). The
//! switch's check that the shell had the terminal back ran `ps -axo pid,ppid,pgid,tpgid,tty,command`
//! over the whole machine and gave up past 1 MiB of output
//! (`GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES`); each session's daemon carries its startup
//! script in its command line (about 13 KB), so 64 sessions made 1.19 MB. The check also ran only
//! after Ctrl-C had stopped the agent, so the switch gave up with the agent gone and no resume
//! typed. The switch now reads only the session it switches: the daemon is found among the
//! processes the kernel names `zmx` (macOS short records, Linux `/proc/<pid>/stat`; Windows reads
//! the shell pid from the session's endpoint file), and argv is read only below it. Nothing here
//! goes through a capped pipe, so the number of other sessions no longer matters.
//! SEE-ALSO: server/src/accounts/exit.rs (the switch's exit loop), server/src/accounts/restart.rs
//! (`plan`, the check before anything is stopped), process_table_macos.rs.

use super::process_identity::{
    group_processes_by_parent_pid, resolve_process_tree_agent_identity, ProcessRow,
};
use super::ZmxProcessIdentity;

/// Bounds the walk: a session tree wider or deeper than this is not one the switch can use.
#[cfg(not(target_os = "macos"))]
const MAX_TREE_PROCESSES: usize = 4096;

/// One process of a session's tree.
#[derive(Clone, Debug)]
pub(crate) struct SessionProcess {
    pub(crate) pid: i64,
    pub(crate) ppid: i64,
    /// Its process group and its terminal's foreground group; both 0 without a terminal (and
    /// always on Windows, which has neither).
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) group: i64,
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) foreground_group: i64,
    /// The terminal device as `ps -o tty` prints it (`ttys003`, `pts/3`).
    pub(crate) terminal: Option<String>,
    pub(crate) command: String,
}

/// The processes at and below a session's root.
#[derive(Clone, Debug)]
pub(crate) struct SessionTree {
    pub(crate) root: i64,
    pub(crate) processes: Vec<SessionProcess>,
}

impl SessionTree {
    /// The agent the session runs, as the full process scan names it.
    pub(crate) fn agent_identity(&self) -> Option<ZmxProcessIdentity> {
        let rows = self
            .processes
            .iter()
            .map(|process| {
                ProcessRow::new(
                    process.pid,
                    process.ppid,
                    process.terminal.clone(),
                    process.command.clone(),
                )
            })
            .collect::<Vec<_>>();
        let children = group_processes_by_parent_pid(&rows);
        resolve_process_tree_agent_identity(self.root, &rows, &children)
            .filter(|identity| identity.agent_id.is_some())
    }
}

/// The session's processes, `Ok(None)` when no daemon runs for it, `Err` when they cannot be read.
pub(crate) fn read_zmx_session_tree(zmx_name: &str) -> Result<Option<SessionTree>, ()> {
    platform::read(zmx_name)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use crate::zmx::process_table_macos as table;

    pub(super) fn read(zmx_name: &str) -> Result<Option<SessionTree>, ()> {
        let Some(root) = table::zmx_daemon_pid(zmx_name) else {
            return Ok(None);
        };
        let processes = table::process_tree_rows(&[root])
            .into_iter()
            .map(|row| {
                let (group, foreground_group) =
                    table::process_group_and_foreground(row.pid()).unwrap_or((0, 0));
                SessionProcess {
                    pid: row.pid(),
                    ppid: row.ppid(),
                    group,
                    foreground_group,
                    terminal: table::process_terminal_name(row.pid()),
                    command: row.command().to_string(),
                }
            })
            .collect::<Vec<_>>();
        // The daemon exited between the two reads.
        if !processes.iter().any(|process| process.pid == root) {
            return Ok(None);
        }
        Ok(Some(SessionTree { root, processes }))
    }
}

/// Linux reads `/proc` directly: a short `stat` line for every process, argv only for the
/// processes named `zmx` and the ones below the session's daemon.
#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::*;
    use std::collections::{HashMap, HashSet, VecDeque};

    struct Stat {
        name: String,
        ppid: i64,
        group: i64,
        terminal_device: i64,
        foreground_group: i64,
    }

    pub(super) fn read(zmx_name: &str) -> Result<Option<SessionTree>, ()> {
        let mut stats = HashMap::new();
        for entry in std::fs::read_dir("/proc").map_err(|_| ())?.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i64>().ok())
            else {
                continue;
            };
            if let Some(stat) = stat(pid) {
                stats.insert(pid, stat);
            }
        }
        let Some(root) = stats
            .iter()
            .filter(|(_, stat)| stat.name == "zmx")
            .find_map(|(pid, _)| {
                let command = command_line(*pid)?;
                (crate::zmx::process_identity::zmx_daemon_session_name(&command)? == zmx_name)
                    .then_some(*pid)
            })
        else {
            return Ok(None);
        };
        let mut children = HashMap::<i64, Vec<i64>>::new();
        for (pid, stat) in &stats {
            children.entry(stat.ppid).or_default().push(*pid);
        }
        let mut processes = Vec::new();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([root]);
        while let Some(pid) = queue.pop_front() {
            if processes.len() >= MAX_TREE_PROCESSES || !seen.insert(pid) {
                continue;
            }
            let Some(stat) = stats.get(&pid) else {
                continue;
            };
            processes.push(SessionProcess {
                pid,
                ppid: stat.ppid,
                group: if stat.foreground_group > 0 {
                    stat.group
                } else {
                    0
                },
                foreground_group: stat.foreground_group.max(0),
                terminal: terminal_name(stat.terminal_device),
                command: command_line(pid).unwrap_or_else(|| format!("({})", stat.name)),
            });
            if let Some(kids) = children.get(&pid) {
                queue.extend(kids.iter().copied());
            }
        }
        Ok(Some(SessionTree { root, processes }))
    }

    /// `/proc/<pid>/stat`: the name in parentheses (it may hold spaces), then state(3) ppid(4)
    /// pgrp(5) session(6) tty_nr(7) tpgid(8).
    fn stat(pid: i64) -> Option<Stat> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_stat(&text)
    }

    fn parse_stat(text: &str) -> Option<Stat> {
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        let name = text.get(open + 1..close)?.to_string();
        let fields = text
            .get(close + 1..)?
            .split_whitespace()
            .collect::<Vec<_>>();
        let number = |index: usize| fields.get(index)?.parse::<i64>().ok();
        Some(Stat {
            name,
            ppid: number(1)?,
            group: number(2)?,
            terminal_device: number(4)?,
            foreground_group: number(5)?,
        })
    }

    /// argv joined by spaces, as `ps -o command` prints it; `None` for a kernel thread or a process
    /// that is gone.
    fn command_line(pid: i64) -> Option<String> {
        let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let text = bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(String::from_utf8_lossy)
            .collect::<Vec<_>>()
            .join(" ");
        (!text.is_empty()).then(|| text.replace(['\n', '\r', '\t'], " "))
    }

    /// The terminal `tty_nr` names, as `ps -o tty` prints it: `pts/N` for a pseudo-terminal
    /// (majors 136 to 143), `ttyN` for a console.
    fn terminal_name(device: i64) -> Option<String> {
        if device <= 0 {
            return None;
        }
        let major = (device >> 8) & 0xfff;
        let minor = (device & 0xff) | ((device >> 12) & 0xfff00);
        match major {
            136..=143 => Some(format!("pts/{}", (major - 136) * 256 + minor)),
            4 if minor < 64 => Some(format!("tty{minor}")),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stat_reads_a_name_with_spaces_and_the_terminal_fields() {
            let stat =
                parse_stat("4242 (tmux: server) S 1 4242 4242 34817 4300 4194560 0").unwrap();
            assert_eq!(stat.name, "tmux: server");
            assert_eq!(stat.ppid, 1);
            assert_eq!(stat.group, 4242);
            assert_eq!(
                terminal_name(stat.terminal_device).as_deref(),
                Some("pts/1")
            );
            assert_eq!(stat.foreground_group, 4300);
        }

        #[test]
        fn this_process_tree_is_read_from_proc() {
            let me = i64::from(std::process::id());
            assert!(stat(me).is_some());
            assert!(command_line(me).is_some());
        }
    }
}

/// Windows has no process groups: the tree is the session's shell (the pid its wmx endpoint
/// record names, the same root `wmx process-snapshot` lists) and what runs below it, each parent
/// link checked against reused pids, and command lines read only for those processes.
#[cfg(windows)]
mod platform {
    use super::*;
    use crate::agent_hooks::nested_agent::ProcessTable;
    use crate::agent_hooks::wmx_pane::Process;
    use sha2::{Digest, Sha256};
    use std::collections::{HashSet, VecDeque};

    pub(super) fn read(zmx_name: &str) -> Result<Option<SessionTree>, ()> {
        // `protocol::endpoint` in wmx: `<directory>\<sha256(name)>.json`.
        let key = hex::encode(Sha256::digest(zmx_name.as_bytes()));
        let path =
            std::path::Path::new(&crate::zmx::session_directory()).join(format!("{key}.json"));
        let Ok(bytes) = std::fs::read(path) else {
            return Ok(None);
        };
        let endpoint: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
        if endpoint["name"].as_str() != Some(zmx_name) {
            return Ok(None);
        }
        let Some(root) = endpoint["shell_pid"]
            .as_u64()
            .or_else(|| endpoint["shellPid"].as_u64())
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 0)
        else {
            return Ok(None);
        };
        let table = ProcessTable::read();
        if table.executable_stem(root).is_none() {
            return Ok(None);
        }
        let mut processes = Vec::new();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([(root, 0_u32)]);
        while let Some((pid, parent)) = queue.pop_front() {
            if processes.len() >= MAX_TREE_PROCESSES || !seen.insert(pid) {
                continue;
            }
            let command = Process::open(pid)
                .and_then(|process| process.command_line())
                .map(|line| line.replace(['\r', '\n'], " "))
                .unwrap_or_default();
            processes.push(SessionProcess {
                pid: i64::from(pid),
                ppid: i64::from(parent),
                group: 0,
                foreground_group: 0,
                terminal: None,
                command,
            });
            for child in table.recorded_children(pid) {
                if table.parent(child) == Some(pid) {
                    queue.push_back((child, pid));
                }
            }
        }
        Ok(Some(SessionTree {
            root: i64::from(root),
            processes,
        }))
    }
}
