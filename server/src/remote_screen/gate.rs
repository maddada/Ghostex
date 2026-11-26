//! Whether remote screen is on for this computer.

use serde_json::Value;

use crate::paths::GxserverPaths;

/// The switch in the shared settings file. It has no Settings row, no Help entry and no catalog
/// default yet: a missing key is off.
pub(crate) const REMOTE_SCREEN_SETTING: &str = "remoteScreenEnabled";

/// Read fresh from the settings file on every call, so turning it off ends new starts at once.
///
/// CDXC:RemoteScreen 2026-11-26 DECISION:
/// User (Q5115): build the spike "behind an off-by-default setting so nothing reaches users until the user approves". Until then the switch is a hidden key set by hand in the settings file of a test computer; it gets a Settings row and a Help entry only once the user approves the feature.
pub(crate) fn remote_screen_enabled(paths: &GxserverPaths) -> bool {
    crate::shared_settings_file::read_shared_settings(paths)
        .as_ref()
        .and_then(|settings| settings.get(REMOTE_SCREEN_SETTING))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
