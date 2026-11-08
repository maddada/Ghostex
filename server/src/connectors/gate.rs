//! Whether Connectors is on: its built-in extension switch (`connectorsHidden`, off by default;
//! CDXC:Extensions 2026-10-19 and 2026-11-09 in packages/settings-catalog/src/data/official_extensions.rs).
//! The CLI hides and refuses its verbs, the routes refuse, and the Executor's idle-stop thread
//! starts only once a Connectors route is actually served.

use ghostex_settings_catalog::built_in_extensions::{self, CONNECTORS};
use serde_json::Value;

/// What the CLI and the routes answer while Connectors is off: where its switch is.
pub(crate) fn connectors_off_message_in(_settings: Option<&Value>) -> String {
    format!(
        "{} From a terminal: ghostex settings set connectorsHidden false",
        built_in_extensions::turned_off_message(CONNECTORS)
    )
}

/// [`connectors_off_message_in`] for this gxserver's settings file.
pub(crate) fn connectors_off_message(paths: &crate::paths::GxserverPaths) -> String {
    connectors_off_message_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}

/// CDXC:Connectors 2026-10-15 DECISION:
/// User: Connectors should be on main "but should be experimental so not shown without that toggle enabled". While it is off, `ghostex connectors` is left out of `ghostex --help` and refuses, the Connectors routes refuse, and nothing Connectors-related runs in the background. A future Connectors page in the desktop or phone app must follow this same gate.
///
/// CDXC:Extensions 2026-11-09 DECISION:
/// User (Q2043): Enable Experimental Features (`showBetaFeatures`) is gone and Connectors is its
/// own switch in Settings > Extensions > Experimental, off by default. Someone who had the old
/// switch on keeps Connectors as it was: settings reads apply
/// `migrate_experimental_features_switch` (shared_settings_file.rs).
pub(crate) fn connectors_enabled_in(settings: Option<&Value>) -> bool {
    built_in_extensions::enabled_in_value(settings, CONNECTORS)
}

/// Whether Connectors is on, read fresh from this gxserver's settings file, so flipping the switch
/// takes effect without a restart.
pub(crate) fn connectors_enabled(paths: &crate::paths::GxserverPaths) -> bool {
    connectors_enabled_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}
