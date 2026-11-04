//! Open With on Windows: the apps the shell offers for a file type or a link scheme, opening a
//! file with one of them, and the system's own "How do you want to open this file?" dialog.
//!
//! CDXC:Docs 2026-10-09 DECISION:
//! User: "clicking on Open with in gpui files top bar doesn't do anything". The Open With menu lists apps on Windows as it does on macOS: the shell's recommended handlers for the file's type (the same list as Explorer's Open with), the browsers for a page's link, and "Choose another app…", which opens the Windows Open With dialog. A file opens through the shell's own handler, so Store apps work too; a link opens in the chosen browser's program.

use std::path::{Path, PathBuf};

use super::SpawnReaped;

use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, IDataObject,
};
use windows::Win32::UI::Shell::{
    ASSOC_FILTER_RECOMMENDED, BHID_DataObject, IAssocHandler, IEnumAssocHandlers, IShellItem,
    OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OAIF_REGISTER_EXT, OPENASINFO, SHAssocEnumHandlers,
    SHAssocEnumHandlersForProtocolByApplication, SHCreateItemFromParsingName, SHOpenWithDialog,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// The `app` path that stands for "Choose another app…" rather than an app.
pub(crate) const GPUI_OPEN_WITH_CHOOSER: &str = "::choose-another-app";

/// COM for the calling thread, released when it drops (only when this call initialized it).
struct ComScope(bool);

impl ComScope {
    fn enter() -> Self {
        // S_FALSE (already initialized) still needs the matching uninitialize; a thread already in
        // another apartment model fails here and is used as it is.
        Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn take_string(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let value = unsafe { text.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(text.0 as *const _)) };
    value.filter(|value| !value.is_empty())
}

/// The shell's handlers for a file extension (`"md"`) or, with `is_url`, for the scheme of a link.
fn handlers(target: &str, is_url: bool) -> Vec<IAssocHandler> {
    let found: windows::core::Result<IEnumAssocHandlers> = unsafe {
        if is_url {
            let scheme = target.split_once(':').map_or(target, |(scheme, _)| scheme);
            SHAssocEnumHandlersForProtocolByApplication(&HSTRING::from(scheme))
        } else {
            SHAssocEnumHandlers(
                &HSTRING::from(format!(".{}", target.trim_start_matches('.'))),
                ASSOC_FILTER_RECOMMENDED,
            )
        }
    };
    let Ok(found) = found else {
        return Vec::new();
    };
    let mut handlers = Vec::new();
    loop {
        let mut slot: [Option<IAssocHandler>; 1] = [None];
        let mut fetched = 0u32;
        if unsafe { found.Next(&mut slot, Some(&mut fetched)) }.is_err() || fetched == 0 {
            break;
        }
        let Some(handler) = slot[0].take() else {
            break;
        };
        handlers.push(handler);
    }
    handlers
}

fn handler_path(handler: &IAssocHandler) -> Option<PathBuf> {
    unsafe { handler.GetName() }
        .ok()
        .and_then(take_string)
        .map(PathBuf::from)
}

/// `(name, program)` for each app the shell offers, in its order, without repeats.
pub(crate) fn windows_open_with_applications(target: &str, is_url: bool) -> Vec<(String, PathBuf)> {
    let _com = ComScope::enter();
    let mut apps: Vec<(String, PathBuf)> = Vec::new();
    for handler in handlers(target, is_url) {
        let Some(path) = handler_path(&handler) else {
            continue;
        };
        let Some(name) = unsafe { handler.GetUIName() }.ok().and_then(take_string) else {
            continue;
        };
        if apps.iter().all(|(_, known)| known != &path) {
            apps.push((name, path));
        }
    }
    apps
}

/// Opens `target` (a file or a link) with `app`, an app `windows_open_with_applications` listed,
/// or shows the Open With dialog for a file when `app` is `GPUI_OPEN_WITH_CHOOSER`.
pub(crate) fn windows_open_with_application(
    app: &Path,
    target: &std::ffi::OsStr,
) -> Result<(), String> {
    let target_path = Path::new(target);
    let is_file = target_path.is_absolute() && target_path.exists();
    if app.as_os_str() == GPUI_OPEN_WITH_CHOOSER {
        if !is_file {
            return Err("Choose another app works on files on this computer.".to_string());
        }
        // The dialog is modal; it runs on a thread of its own so the app keeps drawing.
        let file = HSTRING::from(target_path.as_os_str());
        std::thread::spawn(move || {
            let _com = ComScope::enter();
            let info = OPENASINFO {
                pcszFile: PCWSTR(file.as_ptr()),
                pcszClass: PCWSTR::null(),
                oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT | OAIF_EXEC,
            };
            let _ = unsafe { SHOpenWithDialog(None, &info) };
        });
        return Ok(());
    }
    if is_file {
        let extension = target_path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _com = ComScope::enter();
        let handler = handlers(&extension, false)
            .into_iter()
            .find(|handler| handler_path(handler).as_deref() == Some(app));
        if let Some(handler) = handler {
            let invoked = unsafe {
                SHCreateItemFromParsingName::<_, _, IShellItem>(
                    &HSTRING::from(target_path.as_os_str()),
                    None,
                )
                .and_then(|item| item.BindToHandler::<_, IDataObject>(None, &BHID_DataObject))
                .and_then(|data| handler.Invoke(&data))
            };
            if invoked.is_ok() {
                return Ok(());
            }
        }
    }
    std::process::Command::new(app)
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_reaped()
        .map_err(|_| "The app could not open the file.".to_string())
}
