//! Whether a hook was fired by an agent that another agent started from its tool.

/// Executable names (without extension) of agents that can start another agent from their tools.
/// Agents that run as a plain `node` or `python` process are missing on purpose: their name is the
/// runtime's.
const AGENT_EXECUTABLES: &[&str] = &[
    "claude",
    "openclaude",
    "codex",
    "grok",
    "opencode",
    "droid",
    "agy",
    "kiro-cli",
    "qodercli",
    "cursor-agent",
    "empryo",
    "zcode",
];

/// CDXC:SessionIdentity 2026-10-11 WHY:
/// An agent another agent starts from its tool (a thread's `claude -p` or `cswap run … -- -p`, a test Claude, a `codex exec`) inherits the terminal's GHOSTEX_GLOBAL_SESSION_REF and gxserver address, so its hooks posted into the session as if they were the session's own agent: the session took the nested run's conversation id, its chat showed the nested conversation, its activity flipped mid-turn, and an orchestrator thread's report became the nested run's answer (thread G0hhy on 2026-10-04, throwaway G9c61 on 2026-10-05, thread G8x2n on 2026-10-11 on Windows). The environment cannot tell them apart: Claude sets CLAUDE_CODE_CHILD_SESSION, CLAUDECODE and every other CLAUDE_* variable for the hooks of a top-level Claude too. The process tree can. The session's own agent hangs below the session's zmx or wmx daemon through shells and launchers only, so a hook counts only when the agent that fired it reaches the daemon with no other agent in between. Supersedes the 2026-10-05 rule, which dropped a hook only when it saw another `claude` above: on Windows, Git Bash running one MSYS program from another (`timeout 120 claude -p`, `env … claude`, a pipeline's subshell) ends the intermediate Windows process, so the nested Claude's parent chain broke before reaching the parent Claude and the hook counted as the session's (G8x2n). A chain that breaks before the daemon is now not the session's agent either.
/// The firing agent is `CLAUDE_PID` for Claude and OpenClaude (Claude's own subagents and side queries run inside that same process, so they keep their own rules in notify_runtime.rs), and for the other agents in AGENT_EXECUTABLES the nearest ancestor of the hook process with that agent's executable name. An agent not found that way keeps today's behaviour. A parent link counts only when the parent is not newer than the child, so a reused Windows pid can never graft the session's own agent below another tree.
/// SEE-ALSO: server/src/agent_hooks/notify_runtime.rs (drops the event), server/src/zmx/process_identity.rs `resolve_process_tree_agent_identity` (the same rule for the live process scan).
pub(crate) fn hook_agent_is_nested(agent_key: &str) -> bool {
    let claude = matches!(agent_key, "claude" | "openclaude");
    let executable = hook_agent_executable(agent_key);
    if !claude && executable.is_none() {
        return false;
    }
    let table = ProcessTable::read();
    let firing = if claude {
        std::env::var("CLAUDE_PID")
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 0)
    } else {
        executable.and_then(|name| nearest_ancestor_named(&table, std::process::id(), name))
    };
    let Some(firing) = firing else {
        return false;
    };
    !reaches_session_daemon(&table, firing)
}

/// CDXC:SessionIdentity 2026-10-30 WHY:
/// A program started from a Ghostex terminal (a desktop app, a dev server, a script) inherits the pane's GHOSTEX_GLOBAL_SESSION_REF and gxserver address, and the `claude -p` or Agent SDK Claude it spawns hangs below the pane's zmx through that program, with no agent in between, so `hook_agent_is_nested` counted it as the session's agent: the session took each run's conversation id, its chat showed the run's prompt and answer, and a plain terminal session became a Claude session bound to it (user report 2026-10-30, reproduced with throwaway G1iza and G9stb). A session's own Claude is always interactive; Claude sets `CLAUDE_CODE_ENTRYPOINT=sdk-cli` for `-p` (even over an inherited `cli`) and the SDKs set `sdk-ts`/`sdk-py`, so a hook from a headless Claude is never the session's. `/clear`, `/resume` and compaction run in the interactive process and keep moving the session; a daemon-hosted conversation and a fork keep their own rules in notify_runtime.rs.
pub(crate) fn hook_agent_runs_headless(agent_key: &str) -> bool {
    matches!(agent_key, "claude" | "openclaude")
        && is_headless_claude_entrypoint(std::env::var("CLAUDE_CODE_ENTRYPOINT").ok().as_deref())
}

fn is_headless_claude_entrypoint(entrypoint: Option<&str>) -> bool {
    entrypoint.is_some_and(|entrypoint| entrypoint.trim().starts_with("sdk-"))
}

/// The executable a hook's agent runs as, for the agents whose own process carries that name.
fn hook_agent_executable(agent_key: &str) -> Option<&'static str> {
    Some(match agent_key {
        "codex" => "codex",
        "grok" => "grok",
        "opencode" => "opencode",
        "droid" => "droid",
        "antigravity" => "agy",
        "kiro" => "kiro-cli",
        "qoder" => "qodercli",
        "cursor" => "cursor-agent",
        _ => return None,
    })
}

/// The closest process above `pid` named `name`, below the session daemon.
fn nearest_ancestor_named(table: &ProcessTable, pid: u32, name: &str) -> Option<u32> {
    let mut current = pid;
    for _ in 0..64 {
        let parent = table.parent(current)?;
        let stem = table.executable_stem(parent)?;
        if is_session_daemon(&stem) {
            return None;
        }
        if stem.eq_ignore_ascii_case(name) {
            return Some(parent);
        }
        current = parent;
    }
    None
}

/// Whether the walk up from `agent` reaches its zmx or wmx daemon without passing another agent.
/// An unknown or broken link ends the walk short of the daemon. A direct parent with the agent's
/// own name that really runs a script runtime is the agent's launcher (`node …/codex.js`
/// renaming itself `codex` before it starts the native `codex`), not a second agent.
fn reaches_session_daemon(table: &ProcessTable, agent: u32) -> bool {
    let agent_stem = table.executable_stem(agent);
    let mut current = agent;
    for depth in 0..64 {
        let Some(parent) = table.parent(current) else {
            return false;
        };
        let Some(stem) = table.executable_stem(parent) else {
            return false;
        };
        if is_session_daemon(&stem) {
            return true;
        }
        let launcher = depth == 0
            && agent_stem
                .as_deref()
                .is_some_and(|agent_stem| agent_stem.eq_ignore_ascii_case(&stem))
            && table.runs_script_runtime(parent);
        if !launcher
            && AGENT_EXECUTABLES
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&stem))
        {
            return false;
        }
        current = parent;
    }
    false
}

fn is_session_daemon(stem: &str) -> bool {
    stem.eq_ignore_ascii_case("zmx") || stem.eq_ignore_ascii_case("wmx")
}

#[cfg(windows)]
pub(super) struct ProcessTable {
    rows: std::collections::HashMap<u32, (u32, String)>,
}

#[cfg(windows)]
impl ProcessTable {
    pub(super) fn read() -> Self {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        };
        let mut rows = std::collections::HashMap::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Self { rows };
            }
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut found = Process32FirstW(snapshot, &mut entry) != 0;
            while found {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..length]);
                rows.insert(entry.th32ProcessID, (entry.th32ParentProcessID, exe));
                found = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        Self { rows }
    }

    /// The parent, when it still exists and was created no later than `pid`: Windows keeps a dead
    /// parent's pid and hands it to new processes.
    pub(super) fn parent(&self, pid: u32) -> Option<u32> {
        let (parent, _) = self.rows.get(&pid)?;
        let parent = *parent;
        if parent == 0 || parent == pid || !self.rows.contains_key(&parent) {
            return None;
        }
        (created(parent)? <= created(pid)?).then_some(parent)
    }

    pub(super) fn executable_stem(&self, pid: u32) -> Option<String> {
        let (_, exe) = self.rows.get(&pid)?;
        Some(exe.strip_suffix(".exe").unwrap_or(exe).to_string())
    }

    /// Windows names a process by its real executable, so a script runtime never carries an
    /// agent's name.
    fn runs_script_runtime(&self, _pid: u32) -> bool {
        false
    }
}

#[cfg(windows)]
fn created(pid: u32) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [creation, exit, kernel, user] = &mut times;
        let ok = GetProcessTimes(process, creation, exit, kernel, user) != 0;
        CloseHandle(process);
        ok.then(|| ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
    }
}

/// POSIX reparents an orphan to init, so a parent pid is never stale; each link is read on demand.
#[cfg(not(windows))]
pub(super) struct ProcessTable;

#[cfg(not(windows))]
impl ProcessTable {
    pub(super) fn read() -> Self {
        Self
    }

    pub(super) fn parent(&self, pid: u32) -> Option<u32> {
        posix::parent(pid).filter(|parent| *parent > 1 && *parent != pid)
    }

    pub(super) fn executable_stem(&self, pid: u32) -> Option<String> {
        posix::name(pid)
    }

    /// Whether the process's executable file is a script runtime, whatever name it gave itself.
    fn runs_script_runtime(&self, pid: u32) -> bool {
        posix::executable_file_name(pid).is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            matches!(name.as_str(), "node" | "bun" | "deno") || name.starts_with("python")
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod posix {
    pub(super) fn parent(pid: u32) -> Option<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name sits in parentheses and may contain spaces; the fields after it are
        // state, then the parent pid.
        let after_name = &stat[stat.rfind(')')? + 1..];
        after_name.split_whitespace().nth(1)?.parse().ok()
    }

    pub(super) fn name(pid: u32) -> Option<String> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }

    pub(super) fn executable_file_name(pid: u32) -> Option<String> {
        let path = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        Some(path.file_name()?.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
mod posix {
    fn info(pid: u32) -> Option<libc::proc_bsdinfo> {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let written = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        (written == size).then_some(info)
    }

    pub(super) fn parent(pid: u32) -> Option<u32> {
        Some(info(pid)?.pbi_ppid)
    }

    /// The short name, except that the native Claude install runs from
    /// `~/.local/share/claude/versions/<version>` through a `claude` symlink, so its short name
    /// can be the version; that executable path names it `claude`.
    pub(super) fn name(pid: u32) -> Option<String> {
        let info = info(pid)?;
        let bytes = info
            .pbi_comm
            .iter()
            .take_while(|byte| **byte != 0)
            .map(|byte| *byte as u8)
            .collect::<Vec<_>>();
        let comm = String::from_utf8_lossy(&bytes).into_owned();
        if executable_path(pid).is_some_and(|path| path.contains("/claude/versions/")) {
            return Some("claude".to_string());
        }
        Some(comm)
    }

    pub(super) fn executable_file_name(pid: u32) -> Option<String> {
        let path = executable_path(pid)?;
        Some(path.rsplit('/').next()?.to_string())
    }

    fn executable_path(pid: u32) -> Option<String> {
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let length = unsafe {
            libc::proc_pidpath(
                pid as libc::c_int,
                path.as_mut_ptr().cast(),
                path.len() as u32,
            )
        };
        (length > 0).then(|| String::from_utf8_lossy(&path[..length as usize]).into_owned())
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_os = "macos"
)))]
mod posix {
    pub(super) fn parent(_pid: u32) -> Option<u32> {
        None
    }

    pub(super) fn name(_pid: u32) -> Option<String> {
        None
    }

    pub(super) fn executable_file_name(_pid: u32) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::is_headless_claude_entrypoint;

    /// `claude -p` reports `sdk-cli`, the TypeScript and Python Agent SDKs `sdk-ts` and `sdk-py`;
    /// an interactive Claude reports `cli`.
    #[test]
    fn only_a_headless_claude_entrypoint_is_headless() {
        for headless in ["sdk-cli", "sdk-ts", "sdk-py"] {
            assert!(is_headless_claude_entrypoint(Some(headless)), "{headless}");
        }
        for interactive in [None, Some(""), Some("cli")] {
            assert!(
                !is_headless_claude_entrypoint(interactive),
                "{interactive:?}"
            );
        }
    }
}
