//! Whether the Grok Bot remote built-in extension is on (`grokBotHidden`, on by default;
//! CDXC:GrokBot 2026-11-19 in packages/settings-catalog/src/data/official_extensions.rs). While it
//! is off, `/api/grokbot` refuses connect, test, notes and the setup marks (status, setup and the
//! instructions still answer, so Settings > Grok Bot can show the switch), the delivery worker
//! stops, and agents' launch notes and coordinators' guides leave the Grok Bot line out. The saved
//! connection is kept, so turning it back on needs no new setup.
//!
//! CDXC:GrokBot 2026-11-19 WHY: the switch lives in the settings file, which the desktop's Settings
//! writes directly, so it is read fresh on every call and on every pass of the delivery worker, as
//! the voice agent's is (server/src/voice_agent/feature.rs): no restart and no watcher of its own.

use ghostex_settings_catalog::built_in_extensions::{self, GROK_BOT};
use serde_json::Value;

use crate::domain::DomainStateError;

/// The `code` `/api/grokbot` answers with while the extension is off.
pub(crate) const TURNED_OFF: &str = "turnedOff";

/// Whether Grok Bot remote is on in `settings` (the defaults when there are none: on).
pub(crate) fn on_in(settings: Option<&Value>) -> bool {
    built_in_extensions::on_with(GROK_BOT, |key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// Whether Grok Bot remote is on, read fresh from this gxserver's settings file.
pub(crate) fn on(paths: &crate::paths::GxserverPaths) -> bool {
    on_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}

/// What a client is told while it is off: where the switch is.
pub(crate) fn off_message() -> String {
    format!(
        "{} is turned off. Turn it on at the top of Settings > Grok Bot.",
        built_in_extensions::title(GROK_BOT)
    )
}

pub(crate) fn turned_off() -> DomainStateError {
    DomainStateError {
        code: TURNED_OFF,
        message: off_message(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn on_by_default_and_off_only_when_hidden() {
        assert!(on_in(None));
        assert!(on_in(Some(&json!({}))));
        assert!(on_in(Some(&json!({"grokBotHidden": false}))));
        assert!(!on_in(Some(&json!({"grokBotHidden": true}))));
    }

    #[test]
    fn the_refusal_names_the_page() {
        assert_eq!(
            off_message(),
            "Grok Bot remote is turned off. Turn it on at the top of Settings > Grok Bot."
        );
    }
}
