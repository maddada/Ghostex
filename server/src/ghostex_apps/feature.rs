//! Whether Ghostex Apps is on: its switch (`ghostexAppsEnabled`, Settings > Extensions, a Beta
//! card, off by default). Read fresh from the settings file on every call, so turning it off
//! refuses the next publish, open or bridge call without a restart.

use ghostex_settings_catalog::built_in_extensions::{self, GHOSTEX_APPS};
use serde_json::Value;

use crate::paths::GxserverPaths;

/// Whether the Ghostex Apps switch is on in `settings`.
fn on_in(settings: Option<&Value>) -> bool {
    built_in_extensions::on_with(GHOSTEX_APPS, |key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// Whether Ghostex Apps is on, read fresh from this gxserver's settings file.
pub(crate) fn apps_on(paths: &GxserverPaths) -> bool {
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    on_in(settings.as_ref())
}

/// Called by the settings watch (server/shared_settings_watch.rs) with every settings read that
/// changed something: while Ghostex Apps is off no launch token survives, so turning it off and on
/// again never brings back a window opened before (review finding 7).
pub(crate) fn settings_read(settings: Option<&Value>) {
    if !on_in(settings) {
        super::tokens::revoke_all();
    }
}

/// What a caller is told while Ghostex Apps is off.
pub(crate) fn apps_off_message() -> String {
    let computer = crate::connected_computers::this_computer_name();
    built_in_extensions::turned_off_on_message(GHOSTEX_APPS, &computer)
}

/// The line every agent session's launch note gets while Ghostex Apps is on.
pub(crate) const AGENT_NOTE: &str = "When the user asks for an app, a dashboard, a tracker or a presentation they will keep, build a Ghostex App (one HTML file they can install, with its own saved data) instead of a Live page: read `ghostex app guide` first.";

/// [`AGENT_NOTE`] while Ghostex Apps is on; nothing while it is off.
pub(crate) fn agent_note(paths: &GxserverPaths) -> Option<String> {
    apps_on(paths).then(|| AGENT_NOTE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn off_unless_switched_on() {
        assert!(!on_in(None));
        assert!(!on_in(Some(&json!({}))));
        assert!(!on_in(Some(&json!({"ghostexAppsEnabled": false}))));
        assert!(on_in(Some(&json!({"ghostexAppsEnabled": true}))));
    }
}
