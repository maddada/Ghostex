//! Read aloud's speech engine on the desktop: one speaker for the whole app. The chat host
//! (`gx_chat/read_aloud.rs`) performs the core's `speak` and `stopSpeaking` through it and hears
//! the end back as `speechEnded`; the controls are the chat's (`native_chat/message_actions.rs`,
//! `native_chat/actions.rs`). The GPUI web build has its own twin of this folder
//! (`apps/gpui-web/src/app/read_aloud/`, the browser's `speechSynthesis`).
//!
//! - `macos.rs`: AVSpeechSynthesizer through `native/macos/GpuiReadAloud.m`.
//! - `voices.rs`: the Voice row's choices on Settings > Chat.
//! - `on_screen.rs`: which chat is on screen, told to the chat host from the window's render.

#[cfg(target_os = "macos")]
mod macos;
mod on_screen;
mod voices;

pub(crate) use voices::voice_options;

/// Whether this computer has a voice to read with.
///
/// CDXC:VoiceMode 2026-10-22 WHY: only macOS speaks for now. Read aloud is experimental, and the Windows speaker (WinRT `SpeechSynthesizer`, spec section 8.2) is not built yet, so a Windows computer tells its chats `available: false` and draws no Read aloud control. Linux never has a voice (CDXC:VoiceMode 2026-10-21 DECISION in packages/settings-catalog/src/general/chat.rs).
pub(crate) const HAS_VOICE: bool = cfg!(target_os = "macos");

/// Read aloud is on in this app's settings (its switch in Settings > Extensions > Experimental)
/// and this computer has a voice; contract section 2. A chat of a remote computer reads the same:
/// it is this app's feature and this app's voice.
pub(crate) fn available() -> bool {
    if !HAS_VOICE {
        return false;
    }
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let object = settings.object();
    ghostex_settings_catalog::built_in_extensions::on_with(
        ghostex_settings_catalog::built_in_extensions::READ_ALOUD,
        |key| object.get(key).and_then(serde_json::Value::as_bool),
    )
}

/// Speaks `texts` in order under `token`, replacing anything speaking, with `voice` (an engine
/// voice id, `""` for the system voice) at `speed` (a multiple of the engine's normal rate).
pub(crate) fn speak(token: u64, texts: Vec<String>, voice: &str, speed: f64) {
    #[cfg(target_os = "macos")]
    macos::speak(token, &texts, voice, macos::rate_for_speed(speed));
    #[cfg(not(target_os = "macos"))]
    {
        // No speaker here (`HAS_VOICE`), so the chat never asks; an answer keeps it from waiting.
        let _ = (texts, voice, speed);
        crate::app::gx_chat::read_aloud_speech_ended(token, true);
    }
}

/// Stops whatever is speaking. The end is not reported: the chat already forgot the utterance.
pub(crate) fn stop() {
    #[cfg(target_os = "macos")]
    macos::stop();
}

/// The browser's autoplay rule needs this (the web twin); a desktop speaker may always speak.
pub(crate) fn prime() {}
