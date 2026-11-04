//! Where a program started right now would look for commands.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// CDXC:PlatformSupport 2026-09-28 WHY:
/// gxserver outlives the app and keeps the PATH it started with. On Windows an agent CLI installed while it ran (the official installers append to the user `Path`) stayed "not found" in hook status, the Agents page and the account helpers until the app and background service were restarted. Read the registry's Machine then User `Path` first, which is what a program launched from Explorer or a new terminal gets, then the entries only this process had. This needs no PowerShell start per lookup.
/// SEE-ALSO: .dependencies/wmx/src/daemon.rs `session_path` gives a new terminal the same order.
pub(crate) fn directories() -> Vec<PathBuf> {
    #[cfg(windows)]
    let registry = windows::registry_path_directories();
    #[cfg(not(windows))]
    let registry: Vec<PathBuf> = Vec::new();
    let process = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut unique: Vec<PathBuf> = Vec::new();
    for directory in registry.into_iter().chain(process) {
        if directory.is_absolute() && !unique.iter().any(|known| same_directory(known, &directory))
        {
            unique.push(directory);
        }
    }
    unique
}

/// The registry's Machine then User `Path`: what a new terminal searches, without the folders only
/// gxserver's own PATH has (such as the tool folders `managed_tools::paths::extend_process_path` appends).
#[cfg(windows)]
pub(crate) fn new_program_directories() -> Vec<PathBuf> {
    windows::registry_path_directories()
}

/// `directories()` joined, for a child process that must find tools installed after gxserver started.
pub(crate) fn value() -> OsString {
    std::env::join_paths(directories())
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

/// The first executable named `name` in `directories()`, then in `extra`. On Windows a bare name matches
/// its PATHEXT spellings and finally `.ps1`, so an npm install resolves to its `.cmd` shim before the
/// PowerShell one that a Restricted execution policy refuses to run.
///
/// CDXC:PlatformSupport 2026-11-04 WHY:
/// A lookup reads the registry PATH and checks every folder in it for each PATHEXT spelling, and gxserver repeats the same few lookups (`git`, `claude`, `node`) for every session on every sessions read: 11% of its CPU samples on Windows (measured 2026-11-04). An answer without `extra` folders is kept for five seconds, so a CLI installed while gxserver runs is still found within seconds.
pub(crate) fn find(name: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::{Duration, Instant},
    };
    const KEPT_FOR: Duration = Duration::from_secs(5);
    static RECENT: OnceLock<Mutex<HashMap<String, (Instant, Option<PathBuf>)>>> = OnceLock::new();
    let search = || {
        directories()
            .into_iter()
            .chain(extra.iter().cloned())
            .flat_map(|directory| candidates(&directory, name))
            .find(|path| is_executable(path))
    };
    // Tests install and remove fake programs within milliseconds of each other.
    if cfg!(test) || !extra.is_empty() {
        return search();
    }
    let recent = RECENT.get_or_init(Default::default);
    if let Some((at, found)) = recent
        .lock()
        .ok()
        .and_then(|recent| recent.get(name).cloned())
    {
        if at.elapsed() < KEPT_FOR {
            return found;
        }
    }
    let found = search();
    if let Ok(mut recent) = recent.lock() {
        recent.retain(|_, (at, _)| at.elapsed() < KEPT_FOR);
        recent.insert(name.to_string(), (Instant::now(), found.clone()));
    }
    found
}

/// The first executable named `name` in exactly these folders, whether or not they are on PATH.
pub(crate) fn find_in(name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    directories
        .iter()
        .flat_map(|directory| candidates(directory, name))
        .find(|path| is_executable(path))
}

/// CDXC:PlatformSupport 2026-10-07 WHY:
/// npm (and mise's `npm:` backend, whose `mise which` prints it) puts an extensionless POSIX shell shim next to `name.cmd` and `name.ps1` in `node_modules\.bin`. Windows cannot start the extensionless one (os error 193), so a version check on it failed and the Agents page called an installed ZCode "not installed". On Windows a path another tool reported is mapped to the sibling Windows can run, in PATHEXT order and `.ps1` last; macOS and Linux keep the path as given.
pub(crate) fn runnable(path: &str) -> String {
    #[cfg(windows)]
    {
        let given = Path::new(path);
        if let (Some(directory), Some(name)) = (
            given.parent(),
            given.file_name().and_then(|name| name.to_str()),
        ) {
            if let Some(found) = candidates(directory, name)
                .into_iter()
                .find(|candidate| is_executable(candidate))
            {
                return found.to_string_lossy().into_owned();
            }
        }
    }
    path.to_string()
}

/// Adds `dir` to the user PATH for programs started from now on (Windows only).
#[cfg(windows)]
pub(crate) fn add_user_path_directory(dir: &Path) -> Result<(), String> {
    windows::add_user_path_directory(dir)
}

fn same_directory(left: &Path, right: &Path) -> bool {
    fn key(path: &Path) -> String {
        let text = path.to_string_lossy();
        let text = text.trim_end_matches(['\\', '/']);
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text.to_string()
        }
    }
    key(left) == key(right)
}

#[cfg(not(windows))]
fn candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    vec![directory.join(name)]
}

#[cfg(windows)]
fn candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let mut extensions = extensions
        .split(';')
        .map(str::trim)
        .filter(|extension| extension.starts_with('.'))
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    extensions.push(".ps1".into());
    let has_known_extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&format!(".{}", extension.to_lowercase())));
    if has_known_extension {
        return vec![directory.join(name)];
    }
    // An extensionless file (npm's shell shim for Git Bash) is not something Windows can start.
    extensions
        .iter()
        .map(|extension| directory.join(format!("{name}{extension}")))
        .collect()
}

fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|metadata| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            metadata.is_file()
        }
    })
}

#[cfg(windows)]
mod windows {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
    use windows_sys::Win32::{
        System::Registry::{
            RegCloseKey, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_EXPAND_SZ, REG_VALUE_TYPE,
            RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
        },
        UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        },
    };

    const USER_ENVIRONMENT: &str = "Environment";
    const MACHINE_ENVIRONMENT: &str =
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

    /// The Machine then User `Path` a newly launched program inherits, which an installer can have changed since gxserver started.
    pub(super) fn registry_path_directories() -> Vec<PathBuf> {
        [
            (HKEY_LOCAL_MACHINE, MACHINE_ENVIRONMENT),
            (HKEY_CURRENT_USER, USER_ENVIRONMENT),
        ]
        .into_iter()
        .filter_map(|(root, subkey)| read_value(root, subkey, "Path"))
        .flat_map(|value| {
            std::env::split_paths(&value)
                .filter(|path| path.is_absolute())
                .collect::<Vec<_>>()
        })
        .collect()
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn read_value(root: HKEY, subkey: &str, name: &str) -> Option<OsString> {
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the key handle is closed on every path out of this function.
        if unsafe { RegOpenKeyExW(root, wide(subkey).as_ptr(), 0, KEY_READ, &mut key) } != 0 {
            return None;
        }
        let value = read_string(key, name);
        // SAFETY: `key` was opened successfully above and is not used afterwards.
        unsafe { RegCloseKey(key) };
        value
    }

    /// Appends `dir` to the user `Path` (kept as REG_EXPAND_SZ so existing `%VAR%` entries survive) and tells
    /// running programs such as Explorer that the environment changed.
    pub(super) fn add_user_path_directory(dir: &std::path::Path) -> Result<(), String> {
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the key handle is closed below on every path.
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(USER_ENVIRONMENT).as_ptr(),
                0,
                KEY_READ | KEY_SET_VALUE,
                &mut key,
            )
        } != 0
        {
            return Err("Could not open your user environment in the registry.".into());
        }
        let current = read_raw(key, "Path")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let current = current.trim_end_matches(';');
        let value = if current.is_empty() {
            dir.display().to_string()
        } else {
            format!("{current};{}", dir.display())
        };
        let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `data` is a NUL-terminated UTF-16 buffer of exactly the byte length passed.
        let status = unsafe {
            RegSetValueExW(
                key,
                wide("Path").as_ptr(),
                0,
                REG_EXPAND_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        // SAFETY: `key` was opened successfully above and is not used afterwards.
        unsafe { RegCloseKey(key) };
        if status != 0 {
            return Err(format!("Could not update your user PATH (error {status})."));
        }
        let area = wide("Environment");
        let mut result = 0;
        // SAFETY: `area` outlives the call; SMTO_ABORTIFHUNG bounds the wait on unresponsive windows.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                area.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                &mut result,
            )
        };
        Ok(())
    }

    fn read_string(key: HKEY, name: &str) -> Option<OsString> {
        read_raw(key, name).map(|value| expand(&value))
    }

    fn read_raw(key: HKEY, name: &str) -> Option<OsString> {
        // Read REG_EXPAND_SZ unexpanded (RRF_NOEXPAND): Windows rejects RRF_RT_REG_EXPAND_SZ without it, and `expand` below resolves %USERPROFILE% style entries.
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let name = wide(name);
        let mut kind: REG_VALUE_TYPE = 0;
        let mut bytes: u32 = 0;
        // SAFETY: a null buffer with a zero length asks only for the required size.
        if unsafe {
            RegGetValueW(
                key,
                std::ptr::null(),
                name.as_ptr(),
                flags,
                &mut kind,
                std::ptr::null_mut(),
                &mut bytes,
            )
        } != 0
            || bytes == 0
        {
            return None;
        }
        let mut buffer = vec![0_u16; bytes as usize / 2 + 1];
        let mut length = bytes;
        // SAFETY: `buffer` holds at least `length` bytes and `length` is updated to what was written.
        if unsafe {
            RegGetValueW(
                key,
                std::ptr::null(),
                name.as_ptr(),
                flags,
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut length,
            )
        } != 0
        {
            return None;
        }
        let characters = (length as usize / 2).min(buffer.len());
        let text = &buffer[..characters];
        let text = &text[..text
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(characters)];
        (!text.is_empty()).then(|| OsString::from_wide(text))
    }

    fn expand(value: &OsString) -> OsString {
        let Some(text) = value.to_str() else {
            return value.clone();
        };
        let mut expanded = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('%') {
            let Some(end) = rest[start + 1..].find('%').map(|index| start + 1 + index) else {
                break;
            };
            let name = &rest[start + 1..end];
            // A literal `%` pair with no matching variable stays as written rather than becoming an empty path entry.
            match std::env::var(name) {
                Ok(replacement) if !name.is_empty() => {
                    expanded.push_str(&rest[..start]);
                    expanded.push_str(&replacement);
                }
                _ => expanded.push_str(&rest[..=end]),
            }
            rest = &rest[end + 1..];
        }
        expanded.push_str(rest);
        OsString::from(expanded)
    }
}
