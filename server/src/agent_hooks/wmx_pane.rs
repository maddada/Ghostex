//! The wmx pane that owns an agent on native Windows, read from the process tree.
//!
//! CDXC:AgentHooks 2026-11-02 WHY:
//! On native Windows a pane's agent hangs below the pane's wmx host, `"…\wmx.exe" serve-encoded <base64 launch JSON>`, whose JSON `name` is the same `S..-P..-G..` session name a `zmx run <name>` carries, so the walk reads that instead of `ps`. Windows has no exec, so a launcher stays in the tree: a Ghostex Claude on 2026-11-02 (Claude Code 2.1.316) ran as `claude.exe` below `python.exe`, `python.exe`, `cswap.exe` and `pwsh.exe`, then the pane's `wmx.exe`. Shells and cswap's launcher may stand between the agent and the host; anything else (another agent above all) means the agent is not the pane's own. A parent link counts only when the parent is not newer than the child (`nested_agent::ProcessTable`), so a reused pid never grafts an agent below another pane.
//! SEE-ALSO: server/src/agent_hooks/ancestor_session.rs `zmx_session_owning_agent` (the zmx walk), .dependencies/wmx/src/launch.rs (the host's command line), server/src/hookless_activity/claude.rs (reads the session records this walk maps to panes).

use base64::{engine::general_purpose::STANDARD, Engine as _};

/// What the pane walk reads about a process. Windows answers from a Toolhelp snapshot and each
/// process's command line; the tests answer from a table.
pub(super) trait PaneProcesses {
    fn parent(&self, pid: u32) -> Option<u32>;
    fn executable_stem(&self, pid: u32) -> Option<String>;
    fn command_line(&self, pid: u32) -> Option<String>;
}

/// Executables that may stand between a pane's wmx host and its own agent: shells (PowerShell,
/// cmd, Git Bash and its MSYS helpers) and cswap, whose `cswap.exe` runs its Python twice.
const LAUNCHERS: &[&str] = &[
    "pwsh",
    "powershell",
    "cmd",
    "bash",
    "sh",
    "zsh",
    "fish",
    "dash",
    "nu",
    "env",
    "login",
    "cswap",
    "python",
    "pythonw",
];

fn is_launcher(stem: &str) -> bool {
    LAUNCHERS.contains(&stem) || stem.starts_with("python3")
}

/// The session name of the wmx pane whose own agent is `agent`.
pub(super) fn wmx_session_owning_agent_in(
    processes: &dyn PaneProcesses,
    agent: u32,
) -> Option<String> {
    let mut current = agent;
    for _ in 0..64 {
        let parent = processes.parent(current)?;
        let stem = processes.executable_stem(parent)?.to_ascii_lowercase();
        let stem = stem.strip_suffix(".exe").unwrap_or(&stem);
        if stem == "wmx" {
            return wmx_host_session_name(&processes.command_line(parent)?);
        }
        if !is_launcher(stem) {
            return None;
        }
        current = parent;
    }
    None
}

/// The session name a wmx pane host was started with. Any other wmx (an `attach` client, a
/// `start` that has not handed off yet) yields nothing.
pub(super) fn wmx_host_session_name(command_line: &str) -> Option<String> {
    let line = command_line.trim_start();
    let arguments = match line.strip_prefix('"') {
        Some(quoted) => &quoted[quoted.find('"')? + 1..],
        None => line.split_once(char::is_whitespace)?.1,
    };
    let mut words = arguments.split_whitespace();
    if words.next()? != "serve-encoded" {
        return None;
    }
    let launch = STANDARD.decode(words.next()?).ok()?;
    let launch: serde_json::Value = serde_json::from_slice(&launch).ok()?;
    launch["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Whether a process created at `created` (a FILETIME count) is the one a Claude session record
/// names: Claude writes the creation time there as `procStart`. A record without one (an older
/// Claude) matches any process with its pid.
pub(super) fn record_start_matches(created: Option<u64>, proc_start: Option<&str>) -> bool {
    match proc_start.and_then(|start| start.trim().parse::<u64>().ok()) {
        Some(start) => created == Some(start),
        None => true,
    }
}

#[cfg(windows)]
struct WindowsProcesses(super::nested_agent::ProcessTable);

#[cfg(windows)]
impl PaneProcesses for WindowsProcesses {
    fn parent(&self, pid: u32) -> Option<u32> {
        self.0.parent(pid)
    }

    fn executable_stem(&self, pid: u32) -> Option<String> {
        self.0.executable_stem(pid)
    }

    fn command_line(&self, pid: u32) -> Option<String> {
        let process = Process::open(pid)?;
        process.command_line()
    }
}

#[cfg(windows)]
pub(super) fn wmx_session_owning_agent(agent: u32) -> Option<String> {
    let processes = WindowsProcesses(super::nested_agent::ProcessTable::read());
    wmx_session_owning_agent_in(&processes, agent)
}

/// CDXC:AgentHooks 2026-11-02 WHY:
/// Windows hands a dead process's pid to new processes and Claude can leave its session record behind, so a record counts as live only while its pid runs and, when the record has `procStart`, was created at that time.
#[cfg(windows)]
pub(crate) fn claude_record_process_running(pid: u32, proc_start: Option<&str>) -> bool {
    Process::open(pid).is_some_and(|process| {
        process.running() && record_start_matches(process.created(), proc_start)
    })
}

/// Every running `claude.exe` with its command line and when it started (ms since the epoch),
/// for the `claude attach` clients the hook-free observer follows (hookless_activity/attach.rs).
#[cfg(windows)]
pub(crate) fn claude_processes() -> Vec<(u32, i64, String)> {
    let table = super::nested_agent::ProcessTable::read();
    table
        .pids_named("claude")
        .filter_map(|pid| {
            let process = Process::open(pid)?;
            Some((
                pid,
                filetime_to_epoch_ms(process.created()?),
                process.command_line()?,
            ))
        })
        .collect()
}

/// A FILETIME count (100 ns steps since 1601) in ms since the Unix epoch.
#[cfg(windows)]
fn filetime_to_epoch_ms(filetime: u64) -> i64 {
    const UNIX_EPOCH_AS_FILETIME_MS: i64 = 11_644_473_600_000;
    (filetime / 10_000) as i64 - UNIX_EPOCH_AS_FILETIME_MS
}

#[cfg(windows)]
struct Process(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for Process {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
impl Process {
    fn open(pid: u32) -> Option<Self> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        if pid == 0 {
            return None;
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        (!handle.is_null()).then_some(Self(handle))
    }

    fn running(&self) -> bool {
        const STILL_ACTIVE: u32 = 259;
        let mut code = 0u32;
        let ok =
            unsafe { windows_sys::Win32::System::Threading::GetExitCodeProcess(self.0, &mut code) };
        ok != 0 && code == STILL_ACTIVE
    }

    fn created(&self) -> Option<u64> {
        use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
        let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
        let [creation, exit, kernel, user] = &mut times;
        let ok = unsafe { GetProcessTimes(self.0, creation, exit, kernel, user) } != 0;
        ok.then(|| ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
    }

    /// The process's command line, the way .dependencies/wmx/src/process_snapshot.rs reads it.
    fn command_line(&self) -> Option<String> {
        use windows_sys::{
            Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation},
            Win32::Foundation::UNICODE_STRING,
        };
        let mut length = 0u32;
        unsafe {
            NtQueryInformationProcess(
                self.0,
                ProcessCommandLineInformation,
                std::ptr::null_mut(),
                0,
                &mut length,
            );
        }
        if (length as usize) < std::mem::size_of::<UNICODE_STRING>() {
            return None;
        }
        // u64 cells keep the UNICODE_STRING header at the alignment it is read with.
        let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
        let status = unsafe {
            NtQueryInformationProcess(
                self.0,
                ProcessCommandLineInformation,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut length,
            )
        };
        if status < 0 {
            return None;
        }
        let text = unsafe { &*(buffer.as_ptr() as *const UNICODE_STRING) };
        if text.Buffer.is_null() || text.Length == 0 {
            return Some(String::new());
        }
        let units = unsafe { std::slice::from_raw_parts(text.Buffer, text.Length as usize / 2) };
        Some(String::from_utf16_lossy(units))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// pid -> (parent, executable, command line)
    struct Table(HashMap<u32, (u32, &'static str, String)>);

    impl PaneProcesses for Table {
        fn parent(&self, pid: u32) -> Option<u32> {
            let parent = self.0.get(&pid)?.0;
            self.0.contains_key(&parent).then_some(parent)
        }

        fn executable_stem(&self, pid: u32) -> Option<String> {
            let exe = self.0.get(&pid)?.1;
            Some(exe.strip_suffix(".exe").unwrap_or(exe).to_string())
        }

        fn command_line(&self, pid: u32) -> Option<String> {
            Some(self.0.get(&pid)?.2.clone())
        }
    }

    fn host(name: &str) -> String {
        let launch = serde_json::json!({
            "name": name,
            "cwd": "C:\\dev\\mastro_ed",
            "shell": "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            "startup": null,
        });
        format!(
            "\"C:\\Users\\madda\\AppData\\Local\\Ghostex\\current\\resources\\native\\wmx.exe\" serve-encoded {}",
            STANDARD.encode(launch.to_string())
        )
    }

    fn table(rows: &[(u32, u32, &'static str, String)]) -> Table {
        Table(
            rows.iter()
                .map(|(pid, parent, exe, command)| (*pid, (*parent, *exe, command.clone())))
                .collect(),
        )
    }

    /// The chain read from the user's Windows machine on 2026-11-02.
    #[test]
    fn a_cswap_claude_below_pwsh_finds_its_wmx_pane() {
        let processes = table(&[
            (13300, 42728, "claude.exe", "claude.EXE --model opus".into()),
            (
                42728,
                66052,
                "python.exe",
                "python.exe cswap.exe run 2".into(),
            ),
            (
                66052,
                37912,
                "python.exe",
                "python.exe cswap.exe run 2".into(),
            ),
            (37912, 49528, "cswap.exe", "cswap.exe run 2".into()),
            (49528, 61288, "pwsh.exe", "pwsh.exe -NoLogo -NoExit".into()),
            (61288, 57372, "wmx.exe", host("S8g-P2uwo-G98u6")),
            (57372, 4, "explorer.exe", String::new()),
        ]);
        assert_eq!(
            wmx_session_owning_agent_in(&processes, 13300).as_deref(),
            Some("S8g-P2uwo-G98u6")
        );
    }

    #[test]
    fn a_claude_started_from_another_agent_is_not_the_panes() {
        let processes = table(&[
            (20, 10, "claude.exe", String::new()),
            (10, 9, "bash.exe", String::new()),
            (9, 2, "claude.exe", String::new()),
            (2, 1, "pwsh.exe", String::new()),
            (1, 0, "wmx.exe", host("S1-P1-G1")),
        ]);
        assert_eq!(wmx_session_owning_agent_in(&processes, 20), None);
        assert_eq!(
            wmx_session_owning_agent_in(&processes, 9).as_deref(),
            Some("S1-P1-G1")
        );
    }

    #[test]
    fn a_broken_chain_or_another_program_finds_no_pane() {
        let processes = table(&[
            (20, 10, "claude.exe", String::new()),
            (10, 99, "pwsh.exe", String::new()),
            (30, 31, "claude.exe", String::new()),
            (31, 2, "Code.exe", String::new()),
            (2, 1, "pwsh.exe", String::new()),
            (1, 0, "wmx.exe", host("S1-P1-G1")),
        ]);
        assert_eq!(wmx_session_owning_agent_in(&processes, 20), None);
        assert_eq!(wmx_session_owning_agent_in(&processes, 30), None);
    }

    #[test]
    fn only_a_wmx_pane_host_names_a_session() {
        let processes = table(&[
            (20, 2, "claude.exe", String::new()),
            (2, 1, "pwsh.exe", String::new()),
            (1, 0, "WMX.EXE", "\"C:\\wmx.exe\" attach S1-P1-G1".into()),
        ]);
        assert_eq!(wmx_session_owning_agent_in(&processes, 20), None);
        assert_eq!(
            wmx_host_session_name(&host("S1-P1-G1").replacen('"', "", 2)).as_deref(),
            Some("S1-P1-G1")
        );
        assert_eq!(wmx_host_session_name("wmx.exe serve-encoded !!"), None);
        assert_eq!(wmx_host_session_name("\"C:\\wmx.exe\" serve"), None);
    }

    #[test]
    fn a_record_matches_only_the_process_it_was_written_for() {
        assert!(record_start_matches(
            Some(134380359716877926),
            Some("134380359716877926")
        ));
        assert!(!record_start_matches(
            Some(134380359716877927),
            Some("134380359716877926")
        ));
        assert!(!record_start_matches(None, Some("134380359716877926")));
        assert!(record_start_matches(Some(1), None));
    }
}
