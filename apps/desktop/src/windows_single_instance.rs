//! One Ghostex per data folder on Windows.
//!
//! CDXC:PlatformSupport 2026-09-28 WHY:
//! Chromium's profile lock used to make a second Ghostex hand its launch to the running one (`on_already_running_app_relaunch` in cef/shell/lifecycle.rs), because CEF started with the app. CEF now starts only when a web view is shown (CDXC:CefRuntime 2026-09-28 in app/helpers/web_runtime.rs), so opening Ghostex again from the desktop icon, Start menu or taskbar would start a second full app on the same sessions. A named mutex per CEF profile folder keeps the old behaviour without CEF: the second process brings the running Ghostex forward and exits.

use std::io::{Read as _, Write as _};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::FromRawHandle as _;

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, GetLastError, HWND, INVALID_HANDLE_VALUE, LPARAM,
    RECT,
};
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

/// How long a second launch keeps trying to pass its links to a Ghostex that is still starting.
const LINK_HANDOFF_ATTEMPTS: u32 = 30;
const LINK_HANDOFF_RETRY: std::time::Duration = std::time::Duration::from_millis(100);

/// FNV-1a over the lower-cased profile folder: mutex names cannot contain backslashes.
fn instance_hash() -> u64 {
    let profile = std::env::var_os("GHOSTEX_GPUI_CEF_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::shared_settings::ghostex_storage_paths().cef_cache_dir());
    profile
        .to_string_lossy()
        .to_lowercase()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// The pipe a running Ghostex takes session links on, one per data folder like the lock.
fn link_pipe_name() -> String {
    format!("\\\\.\\pipe\\Ghostex-links-{:016x}", instance_hash())
}

/// True when another Ghostex already runs on this data folder; it has been brought forward, it has
/// this launch's session links, and this process should exit.
pub(crate) fn hand_off_to_running_instance(links: &[String]) -> bool {
    let hash = instance_hash();
    let name = format!("Local\\Ghostex-{hash:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call. The handle is kept
    // open for the life of the process on purpose: it is the lock.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return false;
    }
    activate_running_instance();
    send_links(links);
    true
}

fn send_links(links: &[String]) {
    if links.is_empty() {
        return;
    }
    let (name, payload) = (
        link_pipe_name(),
        crate::session_link_handoff::encode_links(links),
    );
    for _ in 0..LINK_HANDOFF_ATTEMPTS {
        if let Ok(mut pipe) = std::fs::OpenOptions::new().write(true).open(&name) {
            let _ = pipe.write_all(&payload);
            return;
        }
        std::thread::sleep(LINK_HANDOFF_RETRY);
    }
}

/// Takes the session links of later launches for as long as the app runs. The pipe's default
/// security lets only this user write to it, and it refuses other computers.
pub(crate) fn serve_links(mut deliver: impl FnMut(String) + Send + 'static) {
    let name = link_pipe_name()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    let spawned = std::thread::Builder::new()
        .name("ghostex-session-links".into())
        .spawn(move || {
            loop {
                // SAFETY: `name` is NUL-terminated and outlives the call.
                let handle = unsafe {
                    CreateNamedPipeW(
                        name.as_ptr(),
                        PIPE_ACCESS_INBOUND,
                        PIPE_TYPE_BYTE
                            | PIPE_READMODE_BYTE
                            | PIPE_WAIT
                            | PIPE_REJECT_REMOTE_CLIENTS,
                        PIPE_UNLIMITED_INSTANCES,
                        0,
                        crate::session_link_handoff::MAX_HANDOFF_BYTES as u32,
                        0,
                        std::ptr::null(),
                    )
                };
                if handle == INVALID_HANDLE_VALUE {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
                // SAFETY: `handle` is the pipe just created; the file below owns and closes it.
                let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                let pipe = unsafe { std::fs::File::from_raw_handle(handle.cast()) };
                if !connected {
                    continue;
                }
                let mut payload = Vec::new();
                let _ = pipe
                    .take(crate::session_link_handoff::MAX_HANDOFF_BYTES)
                    .read_to_end(&mut payload);
                for link in crate::session_link_handoff::decode_links(&payload) {
                    deliver(link);
                }
            }
        });
    if let Err(error) = spawned {
        eprintln!("Ghostex could not listen for session links: {error}");
    }
}

/// Brings the other Ghostex's main window forward: the largest visible unowned top-level window of
/// another process running this same executable file. The full path, not just `Ghostex.exe`, so a
/// portable copy relaunched beside an installed Ghostex raises its own window (ghostex_paths::portable).
fn activate_running_instance() {
    struct Search {
        executable: Vec<u16>,
        best: (HWND, i64),
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `Search` below, alive for the whole EnumWindows call.
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == std::process::id()
            || unsafe { IsWindowVisible(hwnd) } == 0
            || !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null()
            || process_executable_path(pid).as_deref() != Some(search.executable.as_slice())
        {
            return 1;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe { GetWindowRect(hwnd, &mut rect) };
        let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
        if area > search.best.1 {
            search.best = (hwnd, area);
        }
        1
    }
    let Some(executable) = std::env::current_exe().ok().map(|path| path_key(&path)) else {
        return;
    };
    let mut search = Search {
        executable,
        best: (std::ptr::null_mut(), 0),
    };
    // SAFETY: `visit` only reads windows and writes through the pointer to `search`.
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    let window = search.best.0;
    if window.is_null() {
        return;
    }
    // SAFETY: `window` is a live top-level window found above.
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        SetForegroundWindow(window);
    }
}

/// The lower-cased full path of process `pid`'s executable.
fn process_executable_path(pid: u32) -> Option<Vec<u16>> {
    // SAFETY: the process handle is only queried and then closed; the buffer outlives the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = vec![0u16; 1024];
        let mut length = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        windows_sys::Win32::Foundation::CloseHandle(process);
        if ok == 0 {
            return None;
        }
        buffer.truncate(length as usize);
        let path = <std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(&buffer);
        Some(path_key(std::path::Path::new(&path)))
    }
}

/// One spelling per file (long names, one case), so both sides of the comparison agree.
fn path_key(path: &std::path::Path) -> Vec<u16> {
    lowercase_wide(
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .as_os_str(),
    )
}

fn lowercase_wide(name: &std::ffi::OsStr) -> Vec<u16> {
    std::ffi::OsString::from(name.to_string_lossy().to_lowercase())
        .encode_wide()
        .collect()
}
