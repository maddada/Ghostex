use std::{ffi::OsStr, process::Command};

/// CDXC:PlatformSupport 2026-09-14 WHY:
/// Background Git and tool probes run in the interactive Windows session; without CREATE_NO_WINDOW every probe can open Windows Terminal.
pub(crate) fn background_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = &mut command;
    command
}

/// CDXC:RemoteMachines 2026-09-14 WHY:
/// A server started over Windows SSH must outlive the exec channel without keeping
/// that channel's inheritable handles open. Otherwise the CLI exits but the phone
/// waits forever for EOF. Spawn this detached control plane without handle inheritance.
#[cfg(windows)]
pub(crate) fn spawn_detached_server(executable: &OsStr) -> std::io::Result<u32> {
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
    for flags in [0x0100_0208, 0x0000_0208] {
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
