//! Starting gxserver in the signed-in user's desktop session when the caller runs outside it.

use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    ptr,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        EqualSid, GetSidIdentifierAuthority, GetSidSubAuthority, GetSidSubAuthorityCount,
        TokenSessionId, TokenUser, PSID, TOKEN_QUERY, TOKEN_USER,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

use super::{
    desktop_shell::{shell_execute_from_desktop, ShowWindow},
    launch_context,
    process::background_command,
    standard_user::{current_process_token, token_information},
};

/// The hidden gxserver verb the scheduled task runs in the desktop session.
pub(crate) const DESKTOP_LAUNCH_COMMAND: &str = "desktop-launch";

/// How long Task Scheduler may take to run the hand-off before the call gives up.
const TASK_START_PATIENCE: Duration = Duration::from_secs(30);

/// Where a gxserver this process starts must run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServerPlacement {
    /// This process already runs in a desktop session with a normal logon.
    Here,
    /// This process runs in session 0 or with a network logon, and this user is signed in to the desktop session `session_id`.
    UserDesktop { session_id: u32 },
    /// Nobody is signed in as this user: the server starts detached in the background session, as before, until the app replaces it.
    NobodySignedIn,
}

/// CDXC:PlatformSupport 2026-10-04 DECISION:
/// The user chose "Prevent and cure" (Q15 a): Ghostex must never host the user's sessions from a gxserver in Windows' background session. A server that SSH (session 0, network logon, administrators deny-only, sometimes RedirectionGuard) started broke Codex and cua-driver (junction PATH folders), CIM queries and administrator prompts on 2026-10-03. So a caller outside the desktop starts gxserver in the signed-in user's desktop session instead, and the app replaces any background server when it opens (`gpui_local_gxserver_limited_launch` in apps/desktop). When nobody is signed in, the user decided to "keep it running like before, so remote connections don't break" (Q26 c): a remote desktop runs `ghostex server start --json` and opens its tunnel afterwards, so a server that ended with the SSH call would never be reached.
pub(crate) fn server_placement() -> io::Result<ServerPlacement> {
    if !launch_context::current().is_background() {
        return Ok(ServerPlacement::Here);
    }
    Ok(match user_desktop_session()? {
        Some(session_id) => ServerPlacement::UserDesktop { session_id },
        None => ServerPlacement::NobodySignedIn,
    })
}

/// The desktop session where this token's user has Explorer running as their shell, if any.
fn user_desktop_session() -> io::Result<Option<u32>> {
    let own = current_process_token()?;
    let own_user = token_information(&own, TokenUser)?;
    let own_sid = unsafe { (*own_user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut more = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) } != 0;
    while more {
        let length = entry
            .szExeFile
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..length]);
        if name.eq_ignore_ascii_case("explorer.exe") {
            // Another user's Explorer cannot be opened; that is the expected answer, not an error.
            if let Some((session_id, same_user)) =
                process_session_and_user(entry.th32ProcessID, own_sid)
            {
                if session_id != 0 && same_user {
                    return Ok(Some(session_id));
                }
            }
        }
        more = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    Ok(None)
}

fn process_session_and_user(pid: u32, user: PSID) -> Option<(u32, bool)> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    let mut token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let session_id = token_information(&token, TokenSessionId).ok()?[0] as u32;
    let owner = token_information(&token, TokenUser).ok()?;
    let owner_sid = unsafe { (*owner.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    Some((session_id, unsafe { EqualSid(owner_sid, user) } != 0))
}

/// The `S-1-…` form of this token's user, which Task Scheduler takes as a principal.
fn current_user_sid_string() -> io::Result<String> {
    let token = current_process_token()?;
    let user = token_information(&token, TokenUser)?;
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let authority = unsafe { (*GetSidIdentifierAuthority(sid)).Value };
    let authority = authority
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
    let mut text = format!("S-1-{authority}");
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    for index in 0..u32::from(count) {
        text.push_str(&format!("-{}", unsafe { *GetSidSubAuthority(sid, index) }));
    }
    Ok(text)
}

/// A run-once Task Scheduler task, deleted when dropped.
struct HandOffTask {
    name: String,
}

impl Drop for HandOffTask {
    fn drop(&mut self) {
        let _ = background_command("schtasks.exe")
            .args(["/Delete", "/F", "/TN", &self.name])
            .output();
    }
}

fn schtasks(arguments: &[&std::ffi::OsStr], doing: &str) -> io::Result<()> {
    let output = background_command("schtasks.exe")
        .args(arguments)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "Could not {doing}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// CDXC:PlatformSupport 2026-10-04 WHY:
/// A session-0 or SSH process cannot create a process on the user's desktop itself, and gxserver must not be a Task Scheduler process: that job forbids breakaway, so Codex refused to run under it (see `standard_user_token_if_elevated`), and Task Scheduler ends a task's processes at its time limit. So a run-once interactive task (InteractiveToken, the user's own desktop token) runs only `gxserver desktop-launch`, which asks Explorer to start gxserver: gxserver becomes Explorer's child, in no job, with the user's normal desktop token and environment. The task goes through `conhost.exe --headless` because a console program started by an interactive task opens a Windows Terminal window when Terminal is the default console host; Explorer starts gxserver hidden for the same reason.
/// SEE-ALSO: `hand_off_install_to_desktop` in tooling/xtask/src/start/windows.rs, the same hand-off for the dev start.
pub(crate) fn start_server_in_desktop_session(
    executable: &Path,
    result_file: &Path,
) -> io::Result<()> {
    let _ = std::fs::remove_file(result_file);
    if let Some(parent) = result_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conhost = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\conhost.exe");
    let arguments = format!(
        "--headless \"{}\" {DESKTOP_LAUNCH_COMMAND} \"{}\"",
        executable.display(),
        result_file.display()
    );
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Starts the Ghostex background service in your desktop session. Removed right after it runs.</Description></RegistrationInfo>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>Parallel</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowStartOnDemand>true</AllowStartOnDemand><ExecutionTimeLimit>PT2M</ExecutionTimeLimit><Hidden>true</Hidden><Enabled>true</Enabled></Settings>
  <Actions Context="Author"><Exec><Command>{command}</Command><Arguments>{arguments}</Arguments></Exec></Actions>
</Task>
"#,
        user = current_user_sid_string()?,
        command = xml_text(&conhost.display().to_string()),
        arguments = xml_text(&arguments),
    );
    let xml_file = result_file.with_extension("task.xml");
    let mut encoded = vec![0xFF, 0xFE];
    encoded.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&xml_file, encoded)?;
    let task = HandOffTask {
        name: format!(r"\Ghostex\gxserver start {}", std::process::id()),
    };
    let created = schtasks(
        &[
            "/Create".as_ref(),
            "/F".as_ref(),
            "/TN".as_ref(),
            task.name.as_ref(),
            "/XML".as_ref(),
            xml_file.as_os_str(),
        ],
        "create the task that starts gxserver in your desktop session",
    );
    let _ = std::fs::remove_file(&xml_file);
    created?;
    schtasks(
        &["/Run".as_ref(), "/TN".as_ref(), task.name.as_ref()],
        "run the task that starts gxserver in your desktop session",
    )?;
    let started = Instant::now();
    loop {
        if let Ok(result) = std::fs::read_to_string(result_file) {
            let _ = std::fs::remove_file(result_file);
            let result = result.trim();
            return match result.strip_prefix("error: ") {
                Some(error) => Err(io::Error::other(format!(
                    "Starting gxserver in your desktop session failed: {error}"
                ))),
                None => Ok(()),
            };
        }
        if started.elapsed() > TASK_START_PATIENCE {
            return Err(io::Error::other(
                "Windows did not run the task that starts gxserver in your desktop session within 30 seconds.",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `gxserver desktop-launch <result-file>`: runs inside the task in the user's desktop session, asks Explorer to start `gxserver --foreground`, and writes `ok` or `error: …` to the result file the caller waits on.
pub(crate) fn run_desktop_launch(arguments: &[String]) -> io::Result<()> {
    let result_file = arguments
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("desktop-launch needs the result file path"))?;
    let outcome = (|| {
        let context = launch_context::current();
        if context.is_background() {
            return Err(io::Error::other(format!(
                "the task ran outside the desktop (Windows session {:?})",
                context.session_id
            )));
        }
        let executable = std::env::current_exe()?;
        let directory = executable
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        shell_execute_from_desktop(&executable, "--foreground", &directory, ShowWindow::Hidden)
    })();
    let text = match &outcome {
        Ok(()) => "ok".to_string(),
        Err(error) => format!("error: {error}"),
    };
    std::fs::write(&result_file, text)?;
    outcome
}
