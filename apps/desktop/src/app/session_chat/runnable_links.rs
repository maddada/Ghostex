//! Chat and terminal file links that could launch a program: an app bundle is shown in the file
//! manager, and a runnable file asks before the system opens it.

use std::path::{Path, PathBuf};

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

/// Folder extensions macOS opens as an app, a plug-in or an installer rather than as a folder.
const LAUNCHING_BUNDLE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "prefpane",
    "saver",
    "qlgenerator",
    "mdimporter",
    "plugin",
    "bundle",
    "kext",
    "workflow",
    "action",
    "service",
    "xpc",
    "pkg",
    "mpkg",
];

/// File extensions the system opener runs, installs or follows rather than shows.
const RUNNABLE_EXTENSIONS: &[&str] = &[
    // macOS and Linux scripts, installers and launchers.
    "command",
    "tool",
    "sh",
    "bash",
    "zsh",
    "csh",
    "ksh",
    "fish",
    "terminal",
    "jar",
    "pkg",
    "mpkg",
    "webloc",
    "inetloc",
    "fileloc",
    "workflow",
    "desktop",
    "appimage",
    "run",
    "deb",
    "rpm",
    // Windows programs, scripts, installers and shortcuts.
    "exe",
    "com",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "msi",
    "msp",
    "scr",
    "pif",
    "lnk",
    "url",
    "hta",
    "cpl",
    "msc",
    "reg",
    "appref-ms",
];

fn extension_in(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| extensions.contains(&extension.as_str()))
}

/// A folder that macOS would launch or install when opened, rather than show.
pub(crate) fn gpui_path_is_launching_bundle(path: &Path) -> bool {
    cfg!(target_os = "macos") && path.is_dir() && extension_in(path, LAUNCHING_BUNDLE_EXTENSIONS)
}

/// A file the system opener would run, install or follow: a known runnable extension, or (on
/// macOS and Linux) an executable file that starts like a program (Mach-O, ELF or `#!`).
pub(crate) fn gpui_path_is_runnable_file(path: &Path) -> bool {
    if extension_in(path, RUNNABLE_EXTENSIONS) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::io::Read as _;
        use std::os::unix::fs::PermissionsExt as _;
        let executable = std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0);
        if !executable {
            return false;
        }
        let mut head = [0_u8; 4];
        let read = std::fs::File::open(path)
            .and_then(|mut file| file.read(&mut head))
            .unwrap_or(0);
        let head = &head[..read];
        return head.starts_with(b"#!")
            || head == [0x7f, b'E', b'L', b'F']
            || matches!(
                head,
                [0xfe, 0xed, 0xfa, 0xce]
                    | [0xfe, 0xed, 0xfa, 0xcf]
                    | [0xce, 0xfa, 0xed, 0xfe]
                    | [0xcf, 0xfa, 0xed, 0xfe]
                    | [0xca, 0xfe, 0xba, 0xbe]
            );
    }
    #[allow(unreachable_code)]
    false
}

#[cfg(target_os = "macos")]
const REVEAL_BUTTON: &str = "Show in Finder";
#[cfg(target_os = "windows")]
const REVEAL_BUTTON: &str = "Show in Explorer";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const REVEAL_BUTTON: &str = "Show in Folder";

impl GhostexGpuiApp {
    /// Opens a runnable file from a chat or terminal link only after the reader agrees.
    ///
    /// CDXC:SessionChat 2026-10-25 DECISION:
    /// User (security audit, Q7 "reveal and prompt"): a chat or terminal file link to an app bundle shows it in the file manager instead of launching it, a runnable file (a program, a script, an installer or a shortcut) asks before the system opens it, and a `ghostex://terminal` link that carries a command asks before running it. A model's reply or a file in a repository chooses these links, so a click must never run something the reader did not see. The safe button is the default, so a stray Return never runs anything. Narrows the 2026-09-23 folder-link and the 2026-10-17 (Q218) binary-file decisions for these files only.
    /// SEE-ALSO: apps/desktop/src/app/sidebar_dispatch/folder_pickers_and_os_integration.rs (`ghostex://terminal`), apps/desktop/src/app/native_docs/entry.rs (`local_file_open_target`).
    pub(crate) fn open_runnable_file_after_confirming(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        if gpui_os_integration_path_is_script(&path) {
            // The same Run / Edit / Cancel prompt Finder's Open With uses for a script.
            self.present_gpui_os_integration_script_dialogs(vec![path], window, cx);
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let detail = format!(
            "{}\n\nOpening it can run a program on this computer. Show it first to check where it came from, or open it anyway.",
            path.display()
        );
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Open {name}?"),
            Some(&detail),
            &[REVEAL_BUTTON, "Open Anyway", "Cancel"],
            cx,
        );
        cx.spawn(async move |app, cx| {
            let failure = match answer.await {
                Ok(0) => gpui_reveal_path_in_finder(&path).err(),
                Ok(1) => gpui_open_path(&path)
                    .err()
                    .map(|_| "The operating system could not open that file.".to_string()),
                _ => None,
            };
            if let Some(message) = failure {
                let _ = app.update(cx, |app, cx| {
                    app.report_session_chat_file_open_failure(&message, cx);
                });
            }
        })
        .detach();
    }
}
