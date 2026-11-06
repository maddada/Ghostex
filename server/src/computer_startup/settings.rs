//! The three "When your computer starts" switches, read from the shared settings file.

use serde_json::Value;

use crate::paths::GxserverPaths;

pub(crate) const START_SERVER_AT_LOGIN: &str = "startServerAtLogin";
pub(crate) const OPEN_APP_AT_LOGIN: &str = "openAppAtLogin";
pub(crate) const RESUME_SESSIONS_AFTER_RESTART: &str = "resumeSessionsAfterRestart";

/// What the switches ask for once the cascade is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComputerStartupSettings {
    pub start_server: bool,
    pub open_app: bool,
    pub resume_sessions: bool,
}

impl ComputerStartupSettings {
    /// CDXC:ComputerStartup 2026-11-06 DECISION: User: "add a button to enable starting gxserver on start up + be able to launch the app itself on start up + continue sessions that were running when the pc shut down or lost power. all of these cascading in a nice intuitive way". Opening the app and resuming sessions both hang under Start Ghostex's server when you log in: either one counts only while that switch is on, whatever the file holds, so a hand-edited file or `ghostex settings set` cannot open the app at login without the server.
    pub(crate) fn from_settings(settings: Option<&Value>) -> Self {
        let flag = |key: &str| {
            settings
                .and_then(|settings| settings.get(key))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        let start_server = flag(START_SERVER_AT_LOGIN);
        Self {
            start_server,
            open_app: start_server && flag(OPEN_APP_AT_LOGIN),
            resume_sessions: start_server && flag(RESUME_SESSIONS_AFTER_RESTART),
        }
    }

    pub(crate) fn read(paths: &GxserverPaths) -> Self {
        Self::from_settings(crate::shared_settings_file::read_shared_settings(paths).as_ref())
    }
}
