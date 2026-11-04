//! `ghostex://` on Windows: the per-user registration that makes the operating system start
//! Ghostex for a link, written at every launch so it follows the install, as macOS reads the same
//! claim from the app bundle's Info.plist. A new key under `HKCU\Software\Classes` needs no admin
//! rights. SEE-ALSO: apps/desktop/src/session_link_handoff.rs.
//!
//! CDXC:OsIntegration 2026-10-31 WHY:
//! `%1` is text a web page wrote. CEF and Chromium read the process's raw command line, so a link holding a quote could end the argument and pass a switch such as `--renderer-cmd-prefix`, which runs a program; `--` ends their switches, so whatever follows it stays plain arguments, and the app itself takes only session links from them. The registered program is the launcher, `Ghostex.exe`, which sets up what the runtime needs and passes its arguments on (src/bin/ghostex_gpui_cef_bootstrap.rs); this file is compiled into both, the runtime for the registration and the launcher for Velopack's uninstall hook.

use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
};

/// The command Windows runs for a link.
fn open_command(executable: &str) -> String {
    format!("\"{executable}\" -- \"%1\"")
}

/// The program a link starts: the launcher beside the running executable, when there is one (a
/// local start runs the runtime alone).
fn link_executable(current: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let launcher = current.with_file_name("Ghostex.exe");
    if exists(&launcher) {
        launcher
    } else {
        current.to_path_buf()
    }
}

#[cfg(target_os = "windows")]
const SCHEME_KEY: &str = "Software\\Classes\\ghostex";
#[cfg(target_os = "windows")]
const COMMAND_KEY: &str = "Software\\Classes\\ghostex\\shell\\open\\command";

#[cfg(target_os = "windows")]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

#[cfg(target_os = "windows")]
fn executable() -> Option<String> {
    let current = std::env::current_exe().ok()?;
    let executable = link_executable(&current, |path| path.is_file());
    Some(executable.to_str()?.to_string())
}

#[cfg(target_os = "windows")]
fn set_value(key: &str, name: Option<&str>, value: &str) -> bool {
    let (key, name, value) = (wide(key), name.map(wide), wide(value));
    // SAFETY: every string is NUL-terminated and outlives the call; the size is in bytes.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr()),
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        )
    };
    status == ERROR_SUCCESS
}

/// The command the registration holds now.
#[cfg(target_os = "windows")]
fn registered_command() -> Option<String> {
    let key = wide(COMMAND_KEY);
    let mut buffer = vec![0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer and its size in bytes describe the same allocation.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            std::ptr::null(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = buffer.iter().position(|&unit| unit == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buffer[..end]))
}

#[cfg(target_os = "windows")]
fn our_command() -> Option<String> {
    executable().map(|path| open_command(&path))
}

/// Whether a link starts this copy of Ghostex.
#[cfg(target_os = "windows")]
pub(crate) fn is_registered() -> bool {
    registered_command().is_some_and(|command| Some(command) == our_command())
}

/// Makes this copy the one a link starts.
#[cfg(target_os = "windows")]
pub(crate) fn register() -> bool {
    let (Some(executable), Some(command)) = (executable(), our_command()) else {
        return false;
    };
    set_value(SCHEME_KEY, None, "URL:Ghostex")
        && set_value(SCHEME_KEY, Some("URL Protocol"), "")
        && set_value(
            &format!("{SCHEME_KEY}\\DefaultIcon"),
            None,
            &format!("{executable},0"),
        )
        && set_value(COMMAND_KEY, None, &command)
}

/// At launch. A portable copy runs beside an installed Ghostex and takes links only when nothing
/// else does; Settings' button still hands them to it.
#[cfg(target_os = "windows")]
pub(crate) fn register_at_launch() {
    if ghostex_paths::portable::current().is_some() && registered_command().is_some() {
        return;
    }
    if !is_registered() {
        register();
    }
}

/// When this install goes away, unless another copy has taken the links since.
#[cfg(target_os = "windows")]
pub(crate) fn unregister_if_ours() {
    if is_registered() {
        let key = wide(SCHEME_KEY);
        // SAFETY: the key name is NUL-terminated and outlives the call.
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_stays_behind_the_switch_terminator() {
        assert_eq!(
            open_command(r"C:\Users\A B\AppData\Local\Ghostex\current\Ghostex.exe"),
            r#""C:\Users\A B\AppData\Local\Ghostex\current\Ghostex.exe" -- "%1""#
        );
    }

    #[test]
    fn links_start_the_launcher_when_it_is_there() {
        let runtime = Path::new("current").join("ghostex-gpui-runtime.exe");
        let launcher = Path::new("current").join("Ghostex.exe");
        assert_eq!(link_executable(&runtime, |_| true), launcher);
        assert_eq!(link_executable(&runtime, |_| false), runtime);
        assert_eq!(link_executable(&launcher, |_| true), launcher);
    }
}
