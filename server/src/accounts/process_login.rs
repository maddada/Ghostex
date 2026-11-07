use super::{model::Provider, session_identity::login_identity};
use std::path::{Path, PathBuf};

/// CDXC:AgentProviders 2026-10-04 WHY:
/// A process without the config variable runs on the provider's default login: cswap launches plain `claude` when the selected account is already the default, so treating a missing variable as unverifiable failed every switch to that account. Only an environment that cannot be read stays unverified.
pub(crate) fn identity(process_id: i64, agent: &str, home: &Path) -> Option<String> {
    let (provider, key, default_root) = match agent {
        "claude" => (
            Provider::Claude,
            "CLAUDE_CONFIG_DIR",
            ghostex_paths::agent_homes::claude_default_home(&home),
        ),
        "codex" => (
            Provider::Codex,
            "CODEX_HOME",
            ghostex_paths::agent_homes::codex_default_home(home),
        ),
        _ => return None,
    };
    let root = match environment_value(process_id, key)? {
        Some(value) => absolute_root(value)?,
        None => default_root,
    };
    login_identity(provider, &root, home)
}

pub(super) fn configured_home(process_id: i64, key: &str) -> Option<PathBuf> {
    absolute_root(environment_value(process_id, key)??)
}

/// `None` when the environment cannot be read, `Some(None)` when the variable is unset or empty.
fn environment_value(process_id: i64, key: &str) -> Option<Option<String>> {
    let environment = process_environment(process_id)?;
    let value = environment.split(|byte| *byte == 0).find_map(|entry| {
        let split = entry.iter().position(|byte| *byte == b'=')?;
        let (name, value) = (&entry[..split], &entry[split + 1..]);
        // Windows environment names are case-insensitive.
        let matches = if cfg!(windows) {
            name.eq_ignore_ascii_case(key.as_bytes())
        } else {
            name == key.as_bytes()
        };
        matches.then_some(value)
    });
    Some(
        value
            .filter(|value| !value.is_empty())
            .and_then(|value| std::str::from_utf8(value).ok())
            .map(str::to_string),
    )
}

fn absolute_root(value: String) -> Option<PathBuf> {
    let root = PathBuf::from(value);
    root.is_absolute().then_some(root)
}

#[cfg(target_os = "linux")]
fn process_environment(process_id: i64) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(format!("/proc/{process_id}/environ"))
        .ok()?
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

#[cfg(target_os = "macos")]
fn process_environment(process_id: i64) -> Option<Vec<u8>> {
    process_arguments_and_environment(process_id).map(|(_, environment)| environment)
}

/// The process's argv and its NUL-separated environment, both from one `KERN_PROCARGS2` read.
#[cfg(target_os = "macos")]
fn process_arguments_and_environment(process_id: i64) -> Option<(Vec<String>, Vec<u8>)> {
    let pid = i32::try_from(process_id).ok().filter(|pid| *pid > 0)?;
    let mut argmax = 0_i32;
    let mut size = std::mem::size_of_val(&argmax);
    let mut limit_mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    // macOS rejects PROCARGS2 buffers larger than kern.argmax with EINVAL.
    let result = unsafe {
        libc::sysctl(
            limit_mib.as_mut_ptr(),
            limit_mib.len() as u32,
            (&mut argmax as *mut i32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || argmax <= 0 {
        return None;
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut bytes = vec![0_u8; argmax as usize];
    let mut size = bytes.len();
    // KERN_PROCARGS2 returns argc, the executable path, padding, argv, then NUL-separated environment entries.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    if argc < 0 {
        return None;
    }
    let mut offset = 4;
    offset += bytes.get(offset..)?.iter().position(|byte| *byte == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut arguments = Vec::with_capacity(argc as usize);
    for _ in 0..argc {
        let length = bytes.get(offset..)?.iter().position(|byte| *byte == 0)?;
        arguments.push(String::from_utf8_lossy(&bytes[offset..offset + length]).into_owned());
        offset += length + 1;
    }
    Some((arguments, bytes.get(offset..)?.to_vec()))
}

#[cfg(target_os = "linux")]
fn process_arguments_and_environment(process_id: i64) -> Option<(Vec<String>, Vec<u8>)> {
    let arguments = std::fs::read(format!("/proc/{process_id}/cmdline"))
        .ok()?
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect();
    Some((arguments, process_environment(process_id)?))
}

/// Windows gives no other process's argv here, so only the environment is returned.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_arguments_and_environment(process_id: i64) -> Option<(Vec<String>, Vec<u8>)> {
    Some((Vec::new(), process_environment(process_id)?))
}

/// What a running process was started with: its argv (empty where the platform does not say) and
/// its environment.
pub(crate) struct ProcessLaunch {
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}

/// The argv and environment of `process_id`, or `None` when it cannot be read. The Session details
/// panel starts a read-only copy of a session's Claude with them (server/src/session_details/probe.rs).
pub(crate) fn process_launch(process_id: i64) -> Option<ProcessLaunch> {
    let (arguments, environment) = process_arguments_and_environment(process_id)?;
    let environment = environment
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let split = entry.iter().position(|byte| *byte == b'=')?;
            (split > 0).then(|| {
                (
                    String::from_utf8_lossy(&entry[..split]).into_owned(),
                    String::from_utf8_lossy(&entry[split + 1..]).into_owned(),
                )
            })
        })
        .collect();
    Some(ProcessLaunch {
        arguments,
        environment,
    })
}

/// CDXC:AgentProviders 2026-10-04 WHY:
/// Windows has no public API for another process's environment, so it is read from the target's PEB: ProcessParameters at PEB+0x20, then Environment (+0x80) and EnvironmentSize (+0x3F0) in RTL_USER_PROCESS_PARAMETERS (x64 layout). Without it every native Windows account switch ended in "Could not verify the new agent process's account login."
#[cfg(all(windows, target_pointer_width = "64"))]
fn process_environment(process_id: i64) -> Option<Vec<u8>> {
    use std::{ffi::c_void, mem::size_of};
    use windows_sys::{
        Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation},
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::{
                Diagnostics::Debug::ReadProcessMemory,
                Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ},
            },
        },
    };
    // PROCESS_BASIC_INFORMATION; windows-sys gates it behind the unrelated Win32_System_Kernel feature.
    #[repr(C)]
    #[allow(dead_code)]
    struct BasicInformation {
        exit_status: i32,
        peb_base_address: usize,
        affinity_mask: usize,
        base_priority: i32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }
    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
    let pid = u32::try_from(process_id).ok().filter(|pid| *pid > 0)?;
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if raw.is_null() {
        return None;
    }
    let process = Process(raw);
    let read = |address: usize, buffer: &mut [u8]| {
        let mut read = 0usize;
        let ok = unsafe {
            ReadProcessMemory(
                process.0,
                address as *const c_void,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut read,
            )
        };
        ok != 0 && read == buffer.len()
    };
    let read_pointer = |address: usize| {
        let mut bytes = [0_u8; size_of::<usize>()];
        read(address, &mut bytes).then(|| usize::from_ne_bytes(bytes))
    };
    let mut info: BasicInformation = unsafe { std::mem::zeroed() };
    let mut length = 0_u32;
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessBasicInformation,
            (&mut info as *mut BasicInformation).cast(),
            size_of::<BasicInformation>() as u32,
            &mut length,
        )
    };
    if status < 0 || info.peb_base_address == 0 {
        return None;
    }
    let parameters = read_pointer(info.peb_base_address + 0x20)?;
    let environment = read_pointer(parameters + 0x80)?;
    let size = read_pointer(parameters + 0x3F0)?.min(4 * 1024 * 1024);
    if environment == 0 || size == 0 {
        return None;
    }
    let mut block = vec![0_u8; size & !1];
    if !read(environment, &mut block) {
        return None;
    }
    let units: Vec<u16> = block
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let mut bytes = Vec::new();
    for entry in units
        .split(|unit| *unit == 0)
        .take_while(|entry| !entry.is_empty())
    {
        bytes.extend_from_slice(String::from_utf16_lossy(entry).as_bytes());
        bytes.push(0);
    }
    Some(bytes)
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    all(windows, target_pointer_width = "64")
)))]
fn process_environment(_process_id: i64) -> Option<Vec<u8>> {
    None
}
