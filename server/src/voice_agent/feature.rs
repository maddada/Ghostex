//! Whether the voice agent built-in extension is on (`voiceAgentHidden` on Settings > Extensions,
//! on by default; CDXC:VoiceMode 2026-10-25 in
//! packages/settings-catalog/src/data/official_extensions.rs). While it is off `/api/voice`
//! refuses conversations (new ones and reattaching), the announcement watcher ends the ones that
//! are open, and the remembered conversation of a call cut short by a restart is not summarized.
//! Dictate (`/api/voice?mode=dictation`, the recordings routes) and the keys stay, because Dictate
//! is a separate feature that uses the same xAI key.
//!
//! CDXC:VoiceMode 2026-10-25 WHY: the switch lives in the settings file, which the desktop's Settings and `ghostex settings` write directly, so it is read fresh on every conversation start and on every two-second pass of the watcher that already runs while a call is open (`announcements.rs`): turning it off ends a call within seconds, and turning it back on works at once, without a restart or a watcher of its own.

use ghostex_settings_catalog::built_in_extensions::{self, VOICE_AGENT};
use serde_json::Value;

/// The `error` code `/api/voice` answers while the voice agent is off, and the `ended` reason of a
/// call it ends.
pub(super) const TURNED_OFF: &str = "turnedOff";

/// Whether the voice agent is on in `settings` (the defaults when there are none: on).
pub(crate) fn voice_agent_on_in(settings: Option<&Value>) -> bool {
    built_in_extensions::on_with(VOICE_AGENT, |key| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    })
}

/// Whether the voice agent is on, read fresh from this gxserver's settings file.
pub(crate) fn voice_agent_on(paths: &crate::paths::GxserverPaths) -> bool {
    voice_agent_on_in(crate::session_lifecycle::read_sidebar_settings(paths).as_ref())
}

/// What a client is told while the voice agent is off, with the one command that turns it on.
pub(crate) fn voice_agent_off_message() -> String {
    format!(
        "{} From a terminal: ghostex settings set voiceAgentHidden false",
        built_in_extensions::turned_off_message(VOICE_AGENT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn on_without_settings_and_off_only_when_hidden() {
        assert!(voice_agent_on_in(None));
        assert!(voice_agent_on_in(Some(&json!({}))));
        assert!(voice_agent_on_in(Some(&json!({"voiceAgentHidden": false}))));
        assert!(!voice_agent_on_in(Some(&json!({"voiceAgentHidden": true}))));
        // Not an experimental feature: on by default with nothing saved.
        assert!(voice_agent_on_in(Some(&json!({}))));
    }

    #[test]
    fn the_refusal_names_the_switch() {
        let message = voice_agent_off_message();
        assert!(message.starts_with("Voice agent is turned off."));
        assert!(message.contains("Settings > Extensions"));
        assert!(message.contains("ghostex settings set voiceAgentHidden false"));
    }
}
