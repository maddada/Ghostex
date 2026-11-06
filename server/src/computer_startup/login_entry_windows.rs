//! Windows: a value under the user's `Run` key, which Explorer starts at every sign-in.

use std::{os::windows::ffi::OsStrExt, path::Path};

use anyhow::{anyhow, Result};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
    System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    },
};

use super::{remove_file_if_present, write_if_changed, LoginEntryPlace, AT_LOGIN_COMMAND};
use crate::paths::GxserverPaths;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Ghostex Server";
const PROFILE_FILE: &str = "run-key.txt";

/// CDXC:ComputerStartup 2026-11-06 WHY: Explorer starts Run entries in the user's desktop session as its own children, in no job, which is where gxserver must run (CDXC:PlatformSupport 2026-10-04 in server/src/platform/desktop_session.rs). The entry goes through `conhost.exe --headless` for the reason that comment gives: a console program started there opens a Windows Terminal window when Terminal is the default console host.
pub(super) fn install(
    place: &LoginEntryPlace,
    gxserver: &Path,
    _paths: &GxserverPaths,
) -> Result<()> {
    let conhost = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join(r"System32\conhost.exe");
    let command = format!(
        "\"{}\" --headless \"{}\" {AT_LOGIN_COMMAND}",
        conhost.display(),
        gxserver.display()
    );
    match place {
        LoginEntryPlace::Profile(folder) => write_if_changed(
            &folder.join(PROFILE_FILE),
            &format!("HKCU\\{RUN_KEY}\n{VALUE_NAME}={command}\n"),
        ),
        LoginEntryPlace::System => {
            if read_value().as_deref() == Some(command.as_str()) {
                return Ok(());
            }
            let key = wide(RUN_KEY);
            let name = wide(VALUE_NAME);
            let data = wide(&command);
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(anyhow!(
                    "could not write HKCU\\{RUN_KEY}\\{VALUE_NAME} (error {status})"
                ));
            }
            Ok(())
        }
    }
}

pub(super) fn remove(place: &LoginEntryPlace) -> Result<()> {
    match place {
        LoginEntryPlace::Profile(folder) => remove_file_if_present(&folder.join(PROFILE_FILE)),
        LoginEntryPlace::System => {
            let key = wide(RUN_KEY);
            let name = wide(VALUE_NAME);
            let status =
                unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
            if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                return Err(anyhow!(
                    "could not remove HKCU\\{RUN_KEY}\\{VALUE_NAME} (error {status})"
                ));
            }
            Ok(())
        }
    }
}

fn read_value() -> Option<String> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE_NAME);
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
