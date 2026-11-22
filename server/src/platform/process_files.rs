use std::{collections::HashMap, mem::size_of, path::PathBuf, ptr, time::Duration};
use windows_sys::{
    Wdk::System::Threading::{NtQueryInformationProcess, ProcessHandleInformation},
    Win32::{
        Foundation::{
            CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE,
            STATUS_INFO_LENGTH_MISMATCH,
        },
        Storage::FileSystem::{
            FILE_NAME_OPENED, FILE_TYPE_DISK, GetFileType, GetFinalPathNameByHandleW,
        },
        System::Threading::{
            GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE, PROCESS_QUERY_INFORMATION,
        },
    },
};

/// The hidden verb that runs `open_paths` in a child gxserver.
pub(crate) const OPEN_PATHS_COMMAND: &str = "process-open-files";
/// A walk normally takes milliseconds; a child that has not answered by then is ended.
const OPEN_PATHS_TIMEOUT: Duration = Duration::from_secs(10);

/// The open file paths of each process, read by a child gxserver (`process-open-files <pid>…`).
///
/// CDXC:SessionIdentity 2026-10-25 WHY:
/// The handle walk below duplicates every handle of another process into the caller and asks Windows about it, the only native work gxserver does for one agent. Its old `PssCaptureSnapshot` call killed the native Windows daemon with an access violation on every Codex session start and restore (GitHub #289, #292; see `open_paths`), so it runs in a short-lived child, one per probe for all the pids, and a fault there ends only the child: the daemon logs its exit status and goes on without those paths, the way a process it may not open is skipped. `lsof` ran out of process on macOS for the same lookup until 2026-10-23. This supersedes the 2026-10-24 note that the crash was not reproduced.
pub(crate) fn open_paths_isolated(process_ids: &[i64]) -> HashMap<i64, Vec<PathBuf>> {
    let pids: Vec<i64> = process_ids.iter().copied().filter(|pid| *pid > 0).collect();
    if pids.is_empty() {
        return HashMap::new();
    }
    match run_open_paths_child(&pids) {
        Ok(paths) => paths,
        Err(error) => {
            log_open_paths_failure(&pids, &error);
            HashMap::new()
        }
    }
}

/// The gxserver executable beside this one: the CLI (`ghostex.exe`) is a link to it, and only the gxserver name dispatches the hidden verb.
fn gxserver_executable() -> std::io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    let sibling = current.with_file_name("gxserver.exe");
    Ok(if sibling.is_file() { sibling } else { current })
}

fn run_open_paths_child(pids: &[i64]) -> Result<HashMap<i64, Vec<PathBuf>>, String> {
    use std::{io::Read, process::Stdio};
    let mut child = super::process::background_command(
        gxserver_executable().map_err(|error| format!("gxserver path unreadable: {error}"))?,
    )
    .arg(OPEN_PATHS_COMMAND)
    .args(pids.iter().map(i64::to_string))
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|error| format!("could not start: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = sender.send(text);
    });
    let text = match receiver.recv_timeout(OPEN_PATHS_TIMEOUT) {
        Ok(text) => text,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "did not answer within {} s",
                OPEN_PATHS_TIMEOUT.as_secs()
            ));
        }
    };
    let status = child
        .wait()
        .map_err(|error| format!("could not be waited on: {error}"))?;
    if !status.success() {
        return Err(match status.code() {
            Some(code) => format!("exited with status {:#010X}", code as u32),
            None => "exited without a status".to_string(),
        });
    }
    let parsed: HashMap<String, Vec<PathBuf>> = serde_json::from_str(text.trim())
        .map_err(|error| format!("answered unreadable output: {error}"))?;
    Ok(parsed
        .into_iter()
        .filter_map(|(pid, paths)| Some((pid.parse().ok()?, paths)))
        .collect())
}

fn log_open_paths_failure(pids: &[i64], error: &str) {
    use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
    static LOGGER: std::sync::OnceLock<GxserverLogger> = std::sync::OnceLock::new();
    let logger = LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "processOpenFilesProbeFailed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(format!(
            "Reading the open files of an agent process failed: the helper {error}."
        )),
        details: Some(serde_json::json!({ "processIds": pids })),
    });
}

/// `gxserver process-open-files <pid>…`: prints `{"<pid>": [paths…]}` for the daemon (`open_paths_isolated`).
pub(crate) fn print_open_paths(arguments: &[String]) -> anyhow::Result<()> {
    let answer: HashMap<String, Vec<PathBuf>> = arguments
        .iter()
        .filter_map(|pid| pid.parse::<i64>().ok())
        .filter_map(|pid| Some((pid.to_string(), open_paths(pid)?)))
        .collect();
    println!("{}", serde_json::to_string(&answer)?);
    Ok(())
}

/// CDXC:SessionIdentity 2026-09-14 WHY:
/// A newly started native Codex has no conversation ID in argv. Windows previously returned no open files, leaving chat at "starting" after the terminal had replied.
/// Read the exact process-owned rollout from the process's handle table, matching macOS lsof and Linux procfs without guessing from a shared project directory.
///
/// CDXC:SessionIdentity 2026-10-25 WHY:
/// The handle values come from `NtQueryInformationProcess(ProcessHandleInformation)`, not `PssCaptureSnapshot`. On Windows 11 22621 `PssCaptureSnapshot(PSS_CAPTURE_HANDLES)` raised an access violation inside Windows for every process it was pointed at (Codex, Explorer, wmx, PowerShell), while other capture flags and this query worked; that call was the gxserver crash on every Codex session start in GitHub #289 and #292. Do not bring the process-snapshot API back for this lookup.
fn open_paths(process_id: i64) -> Option<Vec<PathBuf>> {
    let pid = u32::try_from(process_id).ok().filter(|pid| *pid != 0)?;
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_DUP_HANDLE, 0, pid);
        if process.is_null() {
            return None;
        }
        let Some(handles) = handle_values(process) else {
            CloseHandle(process);
            return None;
        };
        let mut paths = Vec::new();
        for handle in handles {
            let mut duplicate = ptr::null_mut();
            if DuplicateHandle(
                process,
                handle,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ) == 0
            {
                continue;
            }
            // Query only duplicated disk handles; named pipes must never enter a potentially blocking path-name query.
            if GetFileType(duplicate) == FILE_TYPE_DISK {
                if let Some(path) = file_path(duplicate) {
                    paths.push(path);
                }
            }
            CloseHandle(duplicate);
        }
        CloseHandle(process);
        Some(paths)
    }
}

/// `PROCESS_HANDLE_SNAPSHOT_INFORMATION`, the head of the `ProcessHandleInformation` answer; windows-sys has the class but not its layout.
#[repr(C)]
#[allow(dead_code)]
struct HandleSnapshotHeader {
    number_of_handles: usize,
    reserved: usize,
}

/// `PROCESS_HANDLE_TABLE_ENTRY_INFO`, one entry after the head.
#[repr(C)]
#[allow(dead_code)]
struct HandleTableEntry {
    handle_value: HANDLE,
    handle_count: usize,
    pointer_count: usize,
    granted_access: u32,
    object_type_index: u32,
    handle_attributes: u32,
    reserved: u32,
}

/// The handle values open in `process`, or `None` when Windows would not list them.
unsafe fn handle_values(process: HANDLE) -> Option<Vec<HANDLE>> {
    // usize elements keep the buffer aligned for both structs.
    let mut buffer: Vec<usize> = vec![0; 8 * 1024];
    // The table can grow between the size answer and the next call, so retry a few times.
    for _ in 0..4 {
        let byte_length = buffer.len() * size_of::<usize>();
        let mut needed = 0_u32;
        let status = NtQueryInformationProcess(
            process,
            ProcessHandleInformation,
            buffer.as_mut_ptr().cast(),
            u32::try_from(byte_length).ok()?,
            &mut needed,
        );
        if status == STATUS_INFO_LENGTH_MISMATCH {
            let wanted =
                (needed as usize).max(byte_length * 2) + 64 * size_of::<HandleTableEntry>();
            buffer.resize(wanted.div_ceil(size_of::<usize>()), 0);
            continue;
        }
        if status < 0 {
            return None;
        }
        let header = &*buffer.as_ptr().cast::<HandleSnapshotHeader>();
        let room =
            (byte_length - size_of::<HandleSnapshotHeader>()) / size_of::<HandleTableEntry>();
        let entries = std::slice::from_raw_parts(
            buffer
                .as_ptr()
                .cast::<u8>()
                .add(size_of::<HandleSnapshotHeader>())
                .cast::<HandleTableEntry>(),
            header.number_of_handles.min(room),
        );
        return Some(entries.iter().map(|entry| entry.handle_value).collect());
    }
    None
}

fn file_path(handle: HANDLE) -> Option<PathBuf> {
    let mut buffer = vec![0_u16; 1024];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_OPENED,
            )
        } as usize;
        if length == 0 || length > 32_768 {
            return None;
        }
        if length >= buffer.len() {
            buffer.resize(length + 1, 0);
            continue;
        }
        let path = String::from_utf16_lossy(&buffer[..length]);
        return Some(if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else {
            PathBuf::from(path.strip_prefix(r"\\?\").unwrap_or(&path))
        });
    }
}
