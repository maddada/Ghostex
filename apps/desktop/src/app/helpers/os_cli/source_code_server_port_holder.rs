//! The program holding the Code editor's local port: naming it on the Code view's card, and
//! ending it for the card's "Close it and retry" button. Only the process that listens on the
//! port is ever ended (plus its code-server wrapper parent), never a program found by name.

use std::time::{Duration, Instant};

use crate::app::helpers::*;
use crate::*;

/// One process listening on the editor's port.
struct PortListener {
    pid: u32,
    name: String,
    in_wsl: bool,
    /// Windows: it runs in another session than this app (an SSH logon or a service).
    other_session: bool,
    /// code-server's wrapper, the listening fork's parent; it exits with its child anyway.
    code_server_wrapper: Option<u32>,
}

/// What "Close it and retry" did.
pub(crate) enum SourceCodeServerPortHolderClose {
    /// The holder was ended, or nothing listened any more.
    Closed,
    /// Windows asked for permission and the user declined it.
    ElevationRefused,
    Failed(String),
}

/// The program on `port`, for the card's text; `None` when it cannot be found.
pub(crate) fn source_code_server_port_holder(port: u16) -> Option<SourceCodeServerPortHolder> {
    let listener = port_listeners(port).into_iter().next()?;
    let is_node = listener
        .name
        .trim_end_matches(".exe")
        .eq_ignore_ascii_case("node");
    Some(SourceCodeServerPortHolder {
        started_by_another_computer: listener.other_session && is_node,
        in_wsl: listener.in_wsl,
        name: listener.name,
        pid: listener.pid,
    })
}

/// Ends whatever listens on `port`, looked up now rather than when the card was drawn.
pub(crate) fn source_code_server_close_port_holders(port: u16) -> SourceCodeServerPortHolderClose {
    let listeners = port_listeners(port);
    if listeners.is_empty() {
        return SourceCodeServerPortHolderClose::Closed;
    }
    if listeners.iter().any(|listener| listener.in_wsl) {
        #[cfg(target_os = "windows")]
        return match windows_terminal_backend::end_source_code_server_port_owners(port) {
            Ok(_) => SourceCodeServerPortHolderClose::Closed,
            Err(message) => SourceCodeServerPortHolderClose::Failed(message),
        };
    }
    let mut pids = Vec::new();
    for listener in &listeners {
        for pid in std::iter::once(listener.pid).chain(listener.code_server_wrapper) {
            if !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }
    if let Err(close) = end_processes(&pids) {
        return close;
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if port_listeners(port)
            .iter()
            .all(|listener| !pids.contains(&listener.pid))
        {
            return SourceCodeServerPortHolderClose::Closed;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    SourceCodeServerPortHolderClose::Failed("It is still running.".to_string())
}

#[cfg(target_os = "windows")]
fn port_listeners(port: u16) -> Vec<PortListener> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    // In the WSL environment the editor listens inside WSL; Windows only shows WSL's relay.
    if windows_terminal_backend::current_preference()
        == windows_terminal_backend::WindowsTerminalBackendPreference::Wsl
        && let Ok(owners) = windows_terminal_backend::source_code_server_port_owners(port)
        && !owners.is_empty()
    {
        return owners
            .into_iter()
            .map(|(pid, name)| PortListener {
                pid,
                name,
                in_wsl: true,
                other_session: false,
                code_server_wrapper: None,
            })
            .collect();
    }
    let own_pid = std::process::id();
    let mut pids = gpui_read_windows_resource_servers()
        .unwrap_or_default()
        .into_iter()
        .filter(|server| server.port == port)
        .map(|server| server.pid & !WINDOWS_PROCESS_ID_BIT)
        .filter(|pid| *pid != own_pid && *pid > 4)
        .collect::<Vec<_>>();
    pids.dedup();
    if pids.is_empty() {
        return Vec::new();
    }
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let own_session = windows_session_id(own_pid);
    pids.into_iter()
        .filter_map(|pid| {
            let process = system.process(Pid::from_u32(pid))?;
            let name = std::path::Path::new(process.name())
                .file_name()?
                .to_string_lossy()
                .into_owned();
            if name.eq_ignore_ascii_case("wslrelay.exe") {
                return None;
            }
            let code_server_wrapper = process.parent().filter(|parent| {
                system.process(*parent).is_some_and(|parent| {
                    parent.cmd().iter().any(|argument| {
                        let argument = argument.to_string_lossy();
                        argument.ends_with("out\\node\\entry.js")
                            || argument.ends_with("out/node/entry.js")
                    })
                })
            });
            Some(PortListener {
                pid,
                name,
                in_wsl: false,
                other_session: own_session.is_some() && windows_session_id(pid) != own_session,
                code_server_wrapper: code_server_wrapper.map(|parent| parent.as_u32()),
            })
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_session_id(pid: u32) -> Option<u32> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ProcessIdToSessionId(pid: u32, session_id: *mut u32) -> i32;
    }
    let mut session_id = 0u32;
    (unsafe { ProcessIdToSessionId(pid, &mut session_id) } != 0).then_some(session_id)
}

/// Ends `pids`; a process this app may not end (today's case: a Code editor an SSH logon started
/// elevated) is ended by an elevated `taskkill /PID`, after the user approves the Windows prompt.
#[cfg(target_os = "windows")]
fn end_processes(pids: &[u32]) -> Result<(), SourceCodeServerPortHolderClose> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, GetLastError,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    let mut denied = Vec::new();
    for &pid in pids {
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if process.is_null() {
            match unsafe { GetLastError() } {
                ERROR_ACCESS_DENIED => denied.push(pid),
                // Already gone (the wrapper exits with its child).
                ERROR_INVALID_PARAMETER => {}
                error => {
                    return Err(SourceCodeServerPortHolderClose::Failed(format!(
                        "Ghostex couldn’t close it (Windows error {error})."
                    )));
                }
            }
            continue;
        }
        let ended = unsafe { TerminateProcess(process, 1) } != 0;
        let error = unsafe { GetLastError() };
        unsafe { CloseHandle(process) };
        if !ended && error == ERROR_ACCESS_DENIED {
            denied.push(pid);
        }
    }
    if denied.is_empty() {
        return Ok(());
    }
    end_processes_elevated(&denied)
}

#[cfg(target_os = "windows")]
fn end_processes_elevated(pids: &[u32]) -> Result<(), SourceCodeServerPortHolderClose> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |value: &str| value.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let verb = wide("runas");
    let file = wide("taskkill.exe");
    let parameters = wide(
        &std::iter::once("/F".to_string())
            .chain(pids.iter().map(|pid| format!("/PID {pid}")))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(if unsafe { GetLastError() } == ERROR_CANCELLED {
            SourceCodeServerPortHolderClose::ElevationRefused
        } else {
            SourceCodeServerPortHolderClose::Failed(
                "Windows couldn’t start the tool that closes it.".to_string(),
            )
        });
    }
    if !info.hProcess.is_null() {
        unsafe {
            WaitForSingleObject(info.hProcess, 30_000);
            CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn port_listeners(port: u16) -> Vec<PortListener> {
    let own_pid = std::process::id();
    let mut pids = gpui_read_native_resource_servers()
        .into_iter()
        .filter(|server| server.port == port && server.pid != own_pid)
        .map(|server| server.pid)
        .collect::<Vec<_>>();
    pids.dedup();
    pids.into_iter()
        .filter_map(|pid| {
            let (parent, name) = unix_process_parent_and_name(pid)?;
            let code_server_wrapper = (parent > 1)
                .then(|| unix_process_arguments(parent))
                .flatten()
                .filter(|arguments| arguments.contains("out/node/entry.js"))
                .map(|_| parent);
            Some(PortListener {
                pid,
                name,
                in_wsl: false,
                other_session: false,
                code_server_wrapper,
            })
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn unix_process_parent_and_name(pid: u32) -> Option<(u32, String)> {
    let output = std::process::Command::new("ps")
        .args(["-o", "ppid=,comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let output = String::from_utf8_lossy(&output.stdout);
    let (parent, command) = output.trim().split_once(char::is_whitespace)?;
    let name = std::path::Path::new(command.trim())
        .file_name()?
        .to_string_lossy()
        .into_owned();
    Some((parent.trim().parse().ok()?, name))
}

#[cfg(not(target_os = "windows"))]
fn unix_process_arguments(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(not(target_os = "windows"))]
fn end_processes(pids: &[u32]) -> Result<(), SourceCodeServerPortHolderClose> {
    let alive = |pid: u32| unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    for &pid in pids {
        if unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } != 0
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        {
            return Err(SourceCodeServerPortHolderClose::Failed(
                "It belongs to another user, so Ghostex can’t close it.".to_string(),
            ));
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && pids.iter().any(|pid| alive(*pid)) {
        std::thread::sleep(Duration::from_millis(100));
    }
    for &pid in pids.iter().filter(|pid| alive(**pid)) {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }
    Ok(())
}
