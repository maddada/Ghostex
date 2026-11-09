//! The `ghostex` verbs of a whole-feature built-in extension (Actions) answer "turned off" while the
//! extension is off, instead of quietly running what the app no longer shows. The rule is the
//! settings catalog's (`ghostex_settings_catalog::built_in_extensions`), read from this machine's
//! settings file, so the CLI, the desktop and the web build agree.
//!
//! CDXC:Extensions 2026-10-01 SEE-ALSO: packages/settings-catalog/src/built_in_extensions.rs (the user decision and the one rule).

use ghostex_settings_catalog::built_in_extensions;
use serde_json::{Map, Value};

use super::rpc::{CliError, CliResult};

/// The saved settings, or none when the file is missing or unreadable (the defaults then apply).
fn saved_settings() -> Map<String, Value> {
    super::settings::read_settings_file().unwrap_or_default()
}

/// Whether built-in extension `id` is on for this machine.
pub(super) fn built_in_extension_enabled(id: &str) -> bool {
    built_in_extensions::enabled(&saved_settings(), id)
}

/// The ids of the whole-feature built-in extensions that are on for this machine.
pub(super) fn enabled_built_in_features() -> Vec<&'static str> {
    let settings = saved_settings();
    built_in_extensions::enabled_features_with(|key| settings.get(key).and_then(Value::as_bool))
}

/// The ids of the whole-feature built-in extensions that are off for this machine.
pub(super) fn disabled_built_in_features() -> Vec<&'static str> {
    let settings = saved_settings();
    built_in_extensions::disabled_features_with(|key| settings.get(key).and_then(Value::as_bool))
}

/// `Ok` while `id` is on; otherwise the "turned off" error naming the switch to flip.
pub(super) fn require_built_in_extension(id: &str) -> CliResult<()> {
    if built_in_extension_enabled(id) {
        return Ok(());
    }
    let mut message = built_in_extensions::turned_off_message(id);
    if let Some((key, enables)) = built_in_extensions::switch_key(id) {
        message.push_str(&built_in_extensions::terminal_hint(key, enables));
    }
    Err(CliError::Other(message))
}
