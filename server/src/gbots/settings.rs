//! The three settings GBots reads from the desktop's settings file: the extension switch, what the
//! Bots space shows, and the computer the bots run on.

use serde_json::Value;

use crate::paths::GxserverPaths;

/// The GBots extension is on (`gbotsEnabled`, off by default).
pub(crate) fn gbots_enabled(paths: &GxserverPaths) -> bool {
    crate::session_lifecycle::read_sidebar_settings(paths)
        .and_then(|settings| settings.get("gbotsEnabled").and_then(Value::as_bool))
        == Some(true)
}

/// The server id of the computer the bots run on; `None` for this computer.
pub(crate) fn gbots_computer(paths: &GxserverPaths) -> Option<String> {
    crate::session_lifecycle::read_sidebar_settings(paths).and_then(|settings| {
        settings
            .get("gbotsComputer")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// Whether this gxserver is the one that runs the bots: GBots is on, and the chosen computer is
/// this one (no choice means this computer).
pub(crate) fn runs_here(paths: &GxserverPaths, server_id: &str) -> bool {
    gbots_enabled(paths)
        && gbots_computer(paths).is_none_or(|computer| computer.eq_ignore_ascii_case(server_id))
}
