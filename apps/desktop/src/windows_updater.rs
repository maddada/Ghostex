use std::ffi::OsString;
use std::sync::mpsc::Sender;

use velopack::sources::GithubSource;
use velopack::{Error, UpdateCheck, UpdateInfo, UpdateManager, VelopackApp, VelopackAsset};

const GHOSTEX_RELEASE_REPOSITORY: &str = "https://github.com/maddada/Ghostex";

#[derive(Clone)]
pub(crate) struct WindowsUpdater {
    manager: UpdateManager,
}

#[derive(Clone)]
pub(crate) struct WindowsUpdate {
    info: UpdateInfo,
}

impl WindowsUpdate {
    pub(crate) fn version(&self) -> &str {
        &self.info.TargetFullRelease.Version
    }

    pub(crate) fn notes_markdown(&self) -> &str {
        &self.info.TargetFullRelease.NotesMarkdown
    }
}

pub(crate) enum WindowsUpdateCheck {
    NoUpdateAvailable,
    UpdateAvailable(WindowsUpdate),
}

pub(crate) fn run_startup_hooks() {
    // Velopack owns install/update lifecycle command-line invocations. This
    // must run before GPUI, CEF, logging, or any background threads start.
    VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .on_after_install_fast_callback(|_| crate::windows_cli_link::link_installed_cli())
        .on_after_update_fast_callback(|_| crate::windows_cli_link::link_installed_cli())
        .on_before_uninstall_fast_callback(|_| crate::windows_url_scheme::unregister_if_ours())
        .run();
    crate::windows_cli_link::link_installed_cli();
    adopt_release_app_user_model_id();
}

/// The id `VelopackApp::run` gives a release install (`velopack.<packId>`, packId Ghostex), which its Start Menu shortcut carries.
const RELEASE_APP_USER_MODEL_ID: &str = "velopack.Ghostex";

/// CDXC:Build 2026-10-07 WHY:
/// The local start installs into the release's per-user folder (`%LOCALAPPDATA%\Ghostex\current`) without Velopack's update manifest, so a dev build never updates itself from the release feed; Velopack then leaves the process without an AppUserModelID. A process in that folder takes the release's id itself, so its taskbar button, pins and toasts match the shortcut the install script writes and what users get. A `--machine` or portable copy keeps the fallback id in windows_notifications.rs.
/// SEE-ALSO: tooling/install-windows-gpui.ps1, tooling/xtask/src/start/windows.rs (resolve_install_paths).
fn adopt_release_app_user_model_id() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let Some(app_dir) = executable.parent() else {
        return;
    };
    // A real Velopack install has its manifest here and already got the id from VelopackApp::run.
    if app_dir.join("sq.version").is_file() {
        return;
    }
    let Some(per_user_app_dir) = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(|value| {
            std::path::PathBuf::from(value)
                .join("Ghostex")
                .join("current")
        })
    else {
        return;
    };
    let same_folder = app_dir
        .to_string_lossy()
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(per_user_app_dir.to_string_lossy().trim_end_matches('\\'));
    if !same_folder {
        return;
    }
    let id: Vec<u16> = RELEASE_APP_USER_MODEL_ID
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `id` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}

impl WindowsUpdater {
    pub(crate) fn new() -> Result<Self, Error> {
        // The channel embedded by `vpk pack` selects win-x64-stable or
        // win-arm64-stable. Stable builds intentionally ignore GitHub
        // prereleases, matching the stable release workflow.
        let source = GithubSource::new(GHOSTEX_RELEASE_REPOSITORY, None, false);
        UpdateManager::new(source, None, None).map(|manager| Self { manager })
    }

    pub(crate) fn is_portable(&self) -> bool {
        self.manager.get_is_portable()
    }

    pub(crate) fn pending_restart(&self) -> Option<VelopackAsset> {
        self.manager.get_update_pending_restart()
    }

    pub(crate) fn check_for_updates(&self) -> Result<WindowsUpdateCheck, Error> {
        match self.manager.check_for_updates()? {
            UpdateCheck::RemoteIsEmpty | UpdateCheck::NoUpdateAvailable => {
                Ok(WindowsUpdateCheck::NoUpdateAvailable)
            }
            UpdateCheck::UpdateAvailable(info) => {
                Ok(WindowsUpdateCheck::UpdateAvailable(WindowsUpdate {
                    info: *info,
                }))
            }
        }
    }

    pub(crate) fn download(
        &self,
        update: &WindowsUpdate,
        progress: Sender<i16>,
    ) -> Result<VelopackAsset, Error> {
        self.manager
            .download_updates(&update.info, Some(progress))?;
        Ok(update.info.TargetFullRelease.clone())
    }

    pub(crate) fn apply_after_exit(&self, asset: &VelopackAsset) -> Result<(), Error> {
        self.manager
            .wait_exit_then_apply_updates(asset, false, true, Vec::<OsString>::new())
    }
}

pub(crate) type WindowsReadyUpdate = VelopackAsset;
