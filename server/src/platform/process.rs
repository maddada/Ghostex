use std::{ffi::OsStr, process::Command};

#[path = "process_run.rs"]
mod run;
pub(crate) use run::*;

/// The child gets a console of its own that is never shown.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The child leads its own Ctrl+C group, so a console signal to the parent does not reach it.
#[cfg(windows)]
pub(crate) const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// CDXC:PlatformSupport 2026-10-09 WHY:
/// Every console program Ghostex starts for its own work (git, gh, wmx helpers, PowerShell probes, agent CLIs, tailcat) runs in the user's desktop session. A console program whose parent has no console (the desktop app is a GUI program; a daemon started without one) gets a fresh console, and with Windows Terminal as the default terminal that console opens as a Terminal window that flashes on the user's screen; `-WindowStyle Hidden` hides it only after it has appeared, and piping the output does not stop it. So every background start opts out when the process is created, through this one helper. Starts the user asked to see (UAC prompts, installers, an editor or terminal they opened) do not use it.
pub(crate) trait NoConsoleWindow {
    /// Starts the child with CREATE_NO_WINDOW on Windows; a no-op elsewhere. `creation_flags` replaces earlier flags, so a caller that needs more passes `CREATE_NO_WINDOW | …` itself.
    fn no_console_window(&mut self) -> &mut Self;
}

impl NoConsoleWindow for Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl NoConsoleWindow for tokio::process::Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        self.creation_flags(CREATE_NO_WINDOW);
        self
    }
}

/// A `Command` for background work that never shows a console window (`NoConsoleWindow`).
pub(crate) fn background_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.no_console_window();
    command
}

/// CDXC:PlatformSupport 2026-10-06 WHY:
/// Every helper the daemon starts (wmx `watch-title`/`history`/`list`, git, probes) inherits its error mode. While Windows shuts down, a helper that starts can no longer initialise user32 and fails with 0xc0000142, and with the default mode Windows showed an "unable to start correctly" dialog for each one that the user had to click through. Helpers are invisible background work, so their load failures and crashes end quietly instead; session shells opt back into the default mode (`process` in zmx/scripts_windows.rs).
#[cfg(windows)]
pub(crate) fn suppress_helper_error_dialogs() {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        GetErrorMode, SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX,
    };
    unsafe {
        SetErrorMode(GetErrorMode() | SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    }
}

/// Ends the helper `pid` when this process ends, however it ends; a no-op outside Windows.
///
/// CDXC:PlatformSupport 2026-10-24 WHY:
/// Windows does not end a child with its parent, so every gxserver that died left its `wmx watch-title` helpers running (they exit only when a title write fails), and with each start adding more, a reporter counted 26 wmx processes (GitHub #289). A helper that must die with the daemon joins a kill-on-close job that only this process holds: when the daemon exits or crashes, Windows closes the job and ends its members. Only helpers that start nothing that must outlive the daemon join it; session hosts never do, so agents keep running across restarts.
pub(crate) fn end_with_server(pid: u32) {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
                Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
            },
        };
        // The handle is never closed: the kernel closes it when this process ends.
        static HELPER_JOB: OnceLock<usize> = OnceLock::new();
        let job = *HELPER_JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return 0;
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                CloseHandle(job);
                return 0;
            }
            job as usize
        });
        if job == 0 || pid == 0 {
            return;
        }
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return;
            }
            AssignProcessToJobObject(job as _, process);
            CloseHandle(process);
        }
    }
    #[cfg(not(windows))]
    let _ = pid;
}

/// A command started as the root of a process tree, so that `end` stops it and every process it started.
///
/// CDXC:PlatformSupport 2026-10-29 DECISION:
/// This is the shared runner the time-boxed `git`/`gh` commands end through on a timeout, with everything they started, and the one the coming command source will use. The repository owner: "Yes to the job-object runner, and yes please to the separate `git`/`gh` timeout PR for Windows; land it first so the command source uses the same runner."
///
/// CDXC:PlatformSupport 2026-10-29 WHY:
/// Windows does not end a child's children with it, and `taskkill /T` follows parent pids, so a grandchild whose parent had already exited (a `gh` child `git`, a credential helper) survived a timeout and kept the output pipe open. On Windows the command starts suspended, joins a job of its own and only then runs, so nothing it starts can escape before it joins. It is resumed with `NtResumeProcess`, which is undocumented but has been stable since Windows XP: stable `std` does not expose the main thread's handle, and the documented route, a system-wide Toolhelp thread snapshot to find that thread, measured about 7 ms per spawn on a CI runner. The job has no kill-on-close, so a helper a finished command left behind (git's fsmonitor daemon) keeps running as it does on Unix, and breakaway stays allowed so a child that asks to leave the job still starts. On Unix the command leads its own process group.
pub(crate) struct ProcessTree {
    child: std::process::Child,
    #[cfg(unix)]
    leads_group: bool,
    #[cfg(windows)]
    job: std::os::windows::io::OwnedHandle,
}

impl ProcessTree {
    /// Spawns `command` as the root of its own tree. On Windows this sets the creation flags (CREATE_NO_WINDOW included), replacing any the caller set.
    pub(crate) fn spawn(command: &mut Command) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            Ok(Self {
                child: command.spawn()?,
                leads_group: true,
            })
        }
        #[cfg(windows)]
        {
            spawn_in_job(command)
        }
    }

    pub(crate) fn child(&mut self) -> &mut std::process::Child {
        &mut self.child
    }

    /// Stops every process still in the tree and reaps the root.
    pub(crate) fn end(&mut self) {
        #[cfg(unix)]
        if self.leads_group {
            if let Ok(group) = libc::pid_t::try_from(self.child.id()) {
                if group > 0 {
                    unsafe {
                        libc::kill(-group, libc::SIGKILL);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let ended = unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(
                    self.job.as_raw_handle() as _,
                    1,
                )
            };
            if ended == 0 {
                log_process_tree_failure(
                    "processTreeEndFailed",
                    &job_step_error(
                        "could not end the command's job",
                        std::io::Error::last_os_error(),
                    ),
                );
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// How long the parent waits for the output-draining thread after the command
/// has exited. A command cannot exit before it has written its output, so at
/// most a pipe buffer is ever still in flight and the thread normally finishes
/// at once. An expired grace shows only that the reader did not finish in time:
/// usually something the command left behind still holds the write end of the
/// pipe, but a starved thread or a slow log sink can delay it too.
pub(crate) const COMMAND_READER_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Waits until `deadline` for `reader`, a thread draining a command's pipe, and returns what it read. `None` when the reader had not finished by `deadline` (the thread is left to finish on its own) or panicked; either is logged under the command's `executable`.
pub(crate) fn join_reader_within<T>(
    reader: std::thread::JoinHandle<T>,
    deadline: std::time::Instant,
    executable: &str,
) -> Option<T> {
    while !reader.is_finished() {
        if std::time::Instant::now() >= deadline {
            log_reader_problem(
                "commandReaderAbandoned",
                "The thread reading the command's output did not finish within its grace after the command exited, so Ghostex stopped waiting for it.",
                executable,
            );
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let joined = reader.join();
    if joined.is_err() {
        log_reader_problem(
            "commandReaderPanicked",
            "The thread reading the command's output panicked, so its output was lost.",
            executable,
        );
    }
    joined.ok()
}

/// The log these helpers write to. Tests drive the reader timeout on purpose, so under test it lives in a temporary home instead of the user's own log.
fn process_logger() -> &'static crate::logging::GxserverLogger {
    static LOGGER: std::sync::OnceLock<crate::logging::GxserverLogger> = std::sync::OnceLock::new();
    LOGGER.get_or_init(|| {
        #[cfg(test)]
        let home = Some(
            std::env::temp_dir().join(format!("gxserver-process-tests-{}", std::process::id())),
        );
        #[cfg(not(test))]
        let home = None;
        crate::logging::GxserverLogger::new(crate::paths::get_gxserver_paths(home))
    })
}

fn log_reader_problem(event: &str, message: &str, executable: &str) {
    use crate::logging::{GxserverLogInput, LogLevel};
    let _ = process_logger().log(GxserverLogInput {
        level: LogLevel::Warn,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: Some(serde_json::json!({ "executable": executable })),
    });
}

/// `error` with the step that failed in front. It keeps the kind, and the OS code only as text in the message: `raw_os_error()` on the result is `None`.
#[cfg(windows)]
fn job_step_error(step: &str, error: std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), format!("{step}: {error}"))
}

/// Logs a job failure where it happens, so it never reads as "the tool is not installed" with nothing in the log.
#[cfg(windows)]
fn log_process_tree_failure(event: &str, error: &std::io::Error) {
    use crate::logging::{GxserverLogInput, LogLevel};
    let _ = process_logger().log(GxserverLogInput {
        level: LogLevel::Error,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: None,
    });
}

#[cfg(windows)]
#[link(name = "ntdll.dll", kind = "raw-dylib", modifiers = "+verbatim")]
extern "system" {
    /// CDXC:Build 2026-10-29 WHY:
    /// ntdll is linked with `raw-dylib`, the way windows-sys links, so building for Windows needs no `ntdll.lib` import library.
    fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
}

#[cfg(windows)]
fn spawn_in_job(command: &mut Command) -> std::io::Result<ProcessTree> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;
    let job = command_job().inspect_err(|error| {
        log_process_tree_failure("processTreeStartFailed", error);
    })?;
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    // An ordinary start failure (the program is not installed) is the caller's answer, not a log line.
    let mut child = command.spawn()?;
    if let Err(error) = join_and_resume(&job, &child) {
        log_process_tree_failure("processTreeStartFailed", &error);
        // It never ran a line of its own, so nothing else is left to stop.
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(ProcessTree { child, job })
}

/// A job of its own for one command: no kill-on-close, breakaway allowed (see `ProcessTree`).
#[cfg(windows)]
fn command_job() -> std::io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    };
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(job_step_error(
            "could not create a job for the command",
            std::io::Error::last_os_error(),
        ));
    }
    let job = unsafe { OwnedHandle::from_raw_handle(job as _) };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_BREAKAWAY_OK;
    let limited = unsafe {
        SetInformationJobObject(
            job.as_raw_handle() as _,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if limited == 0 {
        return Err(job_step_error(
            "could not set the command job's limits",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(job)
}

/// Puts the suspended `child` in `job`, then lets it run.
#[cfg(windows)]
fn join_and_resume(
    job: &std::os::windows::io::OwnedHandle,
    child: &std::process::Child,
) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
    let joined =
        unsafe { AssignProcessToJobObject(job.as_raw_handle() as _, child.as_raw_handle() as _) };
    if joined == 0 {
        return Err(job_step_error(
            "could not put the command in its job",
            std::io::Error::last_os_error(),
        ));
    }
    let status = unsafe { NtResumeProcess(child.as_raw_handle() as _) };
    if status < 0 {
        return Err(std::io::Error::other(format!(
            "could not resume the command: NtResumeProcess returned NTSTATUS {status:#010x}"
        )));
    }
    Ok(())
}

/// The process's creation time as a Windows FILETIME (100ns ticks since 1601), or `None` once it has exited or cannot be opened.
#[cfg(windows)]
pub(crate) fn process_creation_filetime(process_id: i64) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    let pid = u32::try_from(process_id).ok().filter(|pid| *pid != 0)?;
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let read = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(process);
        (read != 0)
            .then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}

/// Whether `pid` names a process that has not exited: `runtime::is_process_running` on Unix, and
/// on Windows (where that answers `false` for every pid) the process's exit code still being
/// STILL_ACTIVE.
pub(crate) fn process_is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, STILL_ACTIVE},
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        if pid == 0 {
            return false;
        }
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return false;
            }
            let mut code = 0u32;
            let read = GetExitCodeProcess(process, &mut code);
            CloseHandle(process);
            read != 0 && code == STILL_ACTIVE as u32
        }
    }
    #[cfg(not(windows))]
    {
        crate::runtime::is_process_running(pid)
    }
}

/// Whether `pid` runs an executable named `gxserver.exe` (wherever it lives, including a `.retired-native` copy), so a recorded pid that Windows has since reused is never mistaken for a daemon.
#[cfg(windows)]
pub(crate) fn process_runs_gxserver(pid: u32) -> bool {
    process_image_path(pid).is_some_and(|image| {
        image
            .file_name()
            .is_some_and(|file| file.eq_ignore_ascii_case("gxserver.exe"))
    })
}

/// The full path of the executable `pid` runs, or `None` when it has exited or cannot be opened (another user's or an elevated process).
#[cfg(windows)]
pub(crate) fn process_image_path(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    if pid == 0 {
        return None;
    }
    let mut name = vec![0u16; 32_768];
    let mut length = name.len() as u32;
    let read = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let read =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, name.as_mut_ptr(), &mut length);
        CloseHandle(process);
        read
    };
    (read != 0)
        .then(|| std::path::PathBuf::from(std::ffi::OsString::from_wide(&name[..length as usize])))
}

/// Every process id running now, from one Toolhelp snapshot.
#[cfg(windows)]
pub(crate) fn process_ids() -> Vec<u32> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
    };
    let mut ids = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return ids;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            ids.push(entry.th32ProcessID);
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    ids
}

/// Ends `pid` at once and waits up to `wait` for it to be gone; `true` once it has exited.
#[cfg(windows)]
pub(crate) fn terminate_process(pid: u32, wait: std::time::Duration) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{
            OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
            PROCESS_TERMINATE,
        },
    };
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return !process_is_alive(pid);
        }
        TerminateProcess(process, 1);
        let waited = WaitForSingleObject(process, wait.as_millis().min(u32::MAX as u128) as u32);
        CloseHandle(process);
        waited == WAIT_OBJECT_0
    }
}

/// Whether this process runs an executable image that is no longer the installed file at its path.
///
/// CDXC:ServerDaemon 2026-09-28 WHY:
/// The desktop refreshes the managed Windows package by renaming the old binaries into `.retired-native` while they may still be running. A gxserver started during that refresh kept executing the retired image, but `build-identity.json` already named the new build, so health reported the new identity and the app never restarted it: a day of installed fixes never ran (the chat Subagents card stayed empty). Compare the file mapped as this image with the installed path so a superseded daemon reports a mismatched identity and the app restarts it.
///
/// CDXC:ServerDaemon 2026-10-27 WHY:
/// The package ships gxserver.exe as a hard link to ghostex.exe in `resources\native`, and the kernel names the mapped image after the link the section was first created through (ghostex.exe), while `current_exe` is gxserver.exe. Comparing the two paths called every daemon superseded, so it stopped about 10 s after each start and the app relaunched it in a loop, and no agent session could start (GitHub #312). Two paths now count as one file when their volume and file id match, so a hard link, another spelling or a different case of the same file is never superseded; only a different file at the startup path ends it.
/// SEE-ALSO: `refresh` in apps/desktop/src/windows_terminal_backend/native_package.rs; `gpui_probe_local_gxserver_health_with_diagnostics` in apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs.
#[cfg(windows)]
pub(crate) fn running_image_superseded() -> bool {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::{
        ProcessStatus::K32GetMappedFileNameW, Threading::GetCurrentProcess,
    };
    // `current_exe` is the path the loader recorded at startup, not where the image lives now.
    let Ok(installed) = std::env::current_exe() else {
        return false;
    };
    let mut name = vec![0u16; 32_768];
    // Any address inside this executable names the file its image section maps, renames included.
    let length = unsafe {
        K32GetMappedFileNameW(
            GetCurrentProcess(),
            running_image_superseded as *const std::ffi::c_void,
            name.as_mut_ptr(),
            name.len() as u32,
        )
    } as usize;
    if length == 0 {
        return false;
    }
    let mut mapped = std::ffi::OsString::from(r"\\?\GLOBALROOT");
    mapped.push(std::ffi::OsString::from_wide(&name[..length]));
    image_superseded_at(
        std::path::Path::new(&mapped),
        &installed,
        windows_file_identity,
    )
}

/// The volume and file id of one file, the same through every hard link, symlink and spelling of its path.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    volume: u64,
    file: u64,
}

/// Whether `running` (the file the image is mapped from) and `installed` (the startup path) are known to be different files. A path that cannot be identified (the installed file is mid-refresh, the image cannot be opened) is not proof, so it never counts as superseded; the next check decides.
#[cfg(any(windows, test))]
fn image_superseded_at(
    running: &std::path::Path,
    installed: &std::path::Path,
    identify: impl Fn(&std::path::Path) -> Option<FileIdentity>,
) -> bool {
    match (identify(running), identify(installed)) {
        (Some(running), Some(installed)) => running != installed,
        _ => false,
    }
}

/// The volume serial number and file index of `path`, opened for its attributes only so a running or shared image opens too.
#[cfg(windows)]
fn windows_file_identity(path: &std::path::Path) -> Option<FileIdentity> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_READ_ATTRIBUTES,
    };
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .open(path)
        .ok()?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let read = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    (read != 0).then(|| FileIdentity {
        volume: u64::from(info.dwVolumeSerialNumber),
        file: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

/// CDXC:ServerDaemon 2026-09-29 WHY:
/// Linux marks the executable of a process whose file was replaced (a reinstall or update that kept gxserver running) as `<path> (deleted)`, and `current_exe` returns that path. gxserver writes its own path into session restore scripts and agent hooks, so a wake ran `…/gxserver (deleted) resume-lookup` and failed with "Unable to restore Codex session". Such a daemon reports a superseded identity, like Windows above, so the app restarts it on the installed binary.
#[cfg(target_os = "linux")]
pub(crate) fn running_image_superseded() -> bool {
    std::fs::read_link("/proc/self/exe")
        .is_ok_and(|path| path.to_string_lossy().ends_with(" (deleted)"))
}

/// CDXC:RemoteMachines 2026-09-14 WHY:
/// A server started over Windows SSH must outlive the exec channel without keeping
/// that channel's inheritable handles open. Otherwise the CLI exits but the phone
/// waits forever for EOF. Spawn this detached control plane without handle inheritance.
///
/// CDXC:PlatformSupport 2026-09-28 WHY:
/// The server gets its own windowless console (CREATE_NO_WINDOW), not DETACHED_PROCESS: with no console at all, every console child it started without its own flag opened a Windows Terminal window. The new console is still separate from the launcher's, so it outlives an SSH channel or terminal too.
///
/// CDXC:PlatformSupport 2026-10-04 WHY:
/// Since the user's "Prevent and cure" decision (`server_placement` in platform/desktop_session.rs), a caller outside the desktop spawns gxserver itself only when nobody is signed in to the desktop; a caller in the desktop session (an administrator terminal, still given standard rights) spawns it here too.
/// SEE-ALSO: `gpui_spawn_local_gxserver_daemon` in apps/desktop/src/app/helpers/board_gxserver/gxserver_health_and_daemon.rs.
#[cfg(windows)]
pub(crate) fn spawn_server(executable: &OsStr) -> std::io::Result<u32> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            CreateProcessAsUserW, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
        },
    };
    let standard_user = super::standard_user::standard_user_token_if_elevated()?;
    let application: Vec<u16> = executable.encode_wide().chain(Some(0)).collect();
    let mut arguments = vec![u16::from(b'"')];
    arguments.extend(executable.encode_wide());
    arguments.extend("\" --foreground".encode_utf16());
    arguments.push(0);
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // CREATE_BREAKAWAY_FROM_JOB (dropped when the job denies it) | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP.
    for flags in [0x0900_0200, 0x0800_0200] {
        let mut command_line = arguments.clone();
        let created = unsafe {
            match &standard_user {
                Some(token) => CreateProcessAsUserW(
                    token.as_raw_handle(),
                    application.as_ptr(),
                    command_line.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    flags,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut process,
                ),
                None => CreateProcessW(
                    application.as_ptr(),
                    command_line.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    flags,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut process,
                ),
            }
        };
        if created != 0 {
            unsafe {
                CloseHandle(process.hThread);
                CloseHandle(process.hProcess);
            }
            return Ok(process.dwProcessId);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(5) {
            return Err(error);
        }
    }
    Err(std::io::Error::last_os_error())
}

#[cfg(all(test, windows))]
mod process_tree_tests {
    use super::{process_is_alive, ProcessTree};
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    /// Ends the tree when the test leaves early, so a failed assertion does not leave PowerShell and `ping` running.
    struct EndOnDrop(ProcessTree);

    impl Drop for EndOnDrop {
        fn drop(&mut self) {
            self.0.end();
        }
    }

    /// `text` as a single-quoted PowerShell string.
    fn single_quoted(text: &str) -> String {
        format!("'{}'", text.replace('\'', "''"))
    }

    fn wait_until(limit: Duration, done: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        while !done() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }

    // The case `taskkill /T` misses: the root starts a parent, the parent starts a grandchild and exits, so nothing links the grandchild to the root any more.
    #[test]
    fn end_stops_a_grandchild_whose_parent_already_exited() {
        // A space and an apostrophe, as a `%TEMP%` under a user name like "Jane O'Neil" has.
        let temp = tempfile::Builder::new()
            .prefix("tree test o'neil ")
            .tempdir()
            .expect("tempdir");
        let pids = temp.path().join("pids.txt");
        let parent_script = temp.path().join("parent.ps1");
        let root_script = temp.path().join("root.ps1");
        std::fs::write(
            &parent_script,
            format!(
                "$grandchild = Start-Process ping -ArgumentList '-n','120','127.0.0.1' -NoNewWindow -PassThru\r\nSet-Content -LiteralPath {} -Value \"$PID $($grandchild.Id)\"\r\n",
                single_quoted(&pids.display().to_string())
            ),
        )
        .unwrap();
        // Start-Process joins -ArgumentList with spaces and adds no quotes, so the script path carries its own.
        std::fs::write(
            &root_script,
            format!(
                "Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File',{} -NoNewWindow\r\nStart-Sleep -Seconds 120\r\n",
                single_quoted(&format!("\"{}\"", parent_script.display()))
            ),
        )
        .unwrap();
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&root_script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut tree = EndOnDrop(ProcessTree::spawn(&mut command).expect("spawn"));

        let read_pids = || -> Option<(u32, u32)> {
            let text = std::fs::read_to_string(&pids).ok()?;
            let mut fields = text.split_whitespace().map(str::parse::<u32>);
            Some((fields.next()?.ok()?, fields.next()?.ok()?))
        };
        assert!(
            wait_until(Duration::from_secs(60), || read_pids().is_some()),
            "the parent never recorded its grandchild"
        );
        let (parent, grandchild) = read_pids().unwrap();
        assert!(
            wait_until(Duration::from_secs(30), || !process_is_alive(parent)),
            "the parent must have exited before the tree ends"
        );
        assert!(process_is_alive(grandchild));

        tree.0.end();
        assert!(
            wait_until(Duration::from_secs(10), || !process_is_alive(grandchild)),
            "the orphaned grandchild must end with the tree"
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{image_superseded_at, FileIdentity};
    use std::{os::unix::fs::MetadataExt, path::Path};

    // The same comparison Windows makes, fed the device and inode the Unix filesystem keeps for each file.
    fn unix_file_identity(path: &Path) -> Option<FileIdentity> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(FileIdentity {
            volume: metadata.dev(),
            file: metadata.ino(),
        })
    }

    fn native_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let native = temp.path().join("resources").join("native");
        std::fs::create_dir_all(&native).expect("native dir");
        (temp, native)
    }

    #[test]
    fn hard_linked_server_is_not_superseded_by_its_other_name() {
        let (_temp, native) = native_dir();
        let ghostex = native.join("ghostex.exe");
        let gxserver = native.join("gxserver.exe");
        std::fs::write(&ghostex, b"build A").unwrap();
        std::fs::hard_link(&ghostex, &gxserver).unwrap();
        // The kernel names the image after ghostex.exe; the process started as gxserver.exe (GitHub #312).
        assert!(!image_superseded_at(
            &ghostex,
            &gxserver,
            unix_file_identity
        ));
    }

    #[test]
    fn symlinked_or_respelled_path_is_not_superseded() {
        let (temp, native) = native_dir();
        let gxserver = native.join("gxserver.exe");
        std::fs::write(&gxserver, b"build A").unwrap();
        let link = temp.path().join("gxserver-link.exe");
        std::os::unix::fs::symlink(&gxserver, &link).unwrap();
        assert!(!image_superseded_at(&gxserver, &link, unix_file_identity));
        let respelled = native.join("..").join("native").join("gxserver.exe");
        assert!(!image_superseded_at(
            &gxserver,
            &respelled,
            unix_file_identity
        ));
        // A different case names the same file wherever the volume ignores case (the Windows and default macOS case).
        let upper = native.join("GXSERVER.EXE");
        if upper.exists() {
            assert!(!image_superseded_at(&gxserver, &upper, unix_file_identity));
        }
    }

    #[test]
    fn image_renamed_away_with_a_new_build_installed_is_superseded() {
        let (temp, native) = native_dir();
        let gxserver = native.join("gxserver.exe");
        std::fs::write(&gxserver, b"build A").unwrap();
        let retired = temp.path().join(".retired-native");
        std::fs::create_dir_all(&retired).unwrap();
        let running = retired.join("gxserver.exe");
        std::fs::rename(&gxserver, &running).unwrap();
        std::fs::write(&gxserver, b"build B").unwrap();
        assert!(image_superseded_at(&running, &gxserver, unix_file_identity));
    }

    #[test]
    fn hard_linked_pair_replaced_by_a_new_build_is_superseded() {
        let (temp, native) = native_dir();
        let ghostex = native.join("ghostex.exe");
        let gxserver = native.join("gxserver.exe");
        std::fs::write(&ghostex, b"build A").unwrap();
        std::fs::hard_link(&ghostex, &gxserver).unwrap();
        let retired = temp.path().join(".retired-native");
        std::fs::create_dir_all(&retired).unwrap();
        let running = retired.join("ghostex.exe");
        std::fs::rename(&ghostex, &running).unwrap();
        std::fs::remove_file(&gxserver).unwrap();
        std::fs::write(&ghostex, b"build B").unwrap();
        std::fs::hard_link(&ghostex, &gxserver).unwrap();
        assert!(image_superseded_at(&running, &gxserver, unix_file_identity));
    }

    #[test]
    fn a_path_that_cannot_be_identified_is_not_superseded() {
        let (_temp, native) = native_dir();
        let gxserver = native.join("gxserver.exe");
        std::fs::write(&gxserver, b"build A").unwrap();
        let missing = native.join("missing.exe");
        // The installed file is mid-refresh, or the image cannot be opened.
        assert!(!image_superseded_at(
            &gxserver,
            &missing,
            unix_file_identity
        ));
        assert!(!image_superseded_at(
            &missing,
            &gxserver,
            unix_file_identity
        ));
    }
}
