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
    VelopackApp::build().set_auto_apply_on_startup(false).run();
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
