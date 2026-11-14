//! What Dictate will do on this computer: the settings with their first-run default, the word the
//! recognizer is told to expect, and the engine and model that will transcribe (the mic tooltip).

use serde_json::{json, Value};

use super::settings::{self, DictateSettings, Engine};
use super::{hex, superwhisper};
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::voice_agent::keys::{self, KeyState};
use crate::voice_agent::provider::Provider;

/// The settings, picking a new install's engine from the saved keys.
pub(crate) fn current_settings(paths: &GxserverPaths) -> DictateSettings {
    settings::read(paths, || saved_keys(paths))
}

pub(crate) fn saved_keys(paths: &GxserverPaths) -> (bool, bool) {
    (
        keys::key_state(paths, Provider::Xai) == KeyState::Saved,
        keys::key_state(paths, Provider::OpenAi) == KeyState::Saved,
    )
}

/// The words OpenAI and xAI are told to expect: recognizers otherwise write "Gostex". The user's
/// dictionary and project names are no longer hints (CDXC:Dictate 2026-11-10 in transcribe.rs).
pub(crate) fn hint_words() -> Vec<String> {
    vec!["Ghostex".to_string()]
}

pub(crate) fn provider_of(engine: Engine) -> Option<Provider> {
    match engine {
        Engine::Hex | Engine::Superwhisper => None,
        Engine::OpenAi => Some(Provider::OpenAi),
        Engine::Xai => Some(Provider::Xai),
    }
}

/// The tooltip's first line after "Dictate · ".
pub(crate) fn engine_label(settings: &DictateSettings) -> String {
    match settings.engine {
        Engine::OpenAi => "OpenAI gpt-4o-transcribe".to_string(),
        Engine::Xai => "xAI speech to text".to_string(),
        Engine::Hex => match &settings.hex_model {
            Some(model) => format!("Hex, {model}"),
            None => "Hex".to_string(),
        },
        Engine::Superwhisper => "Superwhisper".to_string(),
    }
}

/// [`engine_label`], with the Hex model's own name when Hex answered.
fn engine_label_with(settings: &DictateSettings, hex_state: Option<&hex::State>) -> String {
    if let (Engine::Hex, Some(hex::State::Ready { models, .. })) = (settings.engine, hex_state) {
        if let Ok(model) = hex::chosen_model(models, settings.hex_model.as_deref()) {
            return format!("Hex, {}", model.name);
        }
    }
    engine_label(settings)
}

/// Hex's state when the engine is Hex (a few local requests: call it off the async runtime).
pub(crate) fn hex_state_for(
    paths: &GxserverPaths,
    settings: &DictateSettings,
) -> Option<hex::State> {
    (settings.engine == Engine::Hex).then(|| hex::state(&paths.home_dir))
}

/// Why the chosen engine can't transcribe right now: `(code, sentence)`. `hex_state` is Hex's
/// state when the caller already asked it (it is asked here otherwise, blocking).
pub(crate) fn engine_problem(
    paths: &GxserverPaths,
    settings: &DictateSettings,
    hex_state: Option<&hex::State>,
) -> Option<(&'static str, String)> {
    if settings.engine == Engine::Hex {
        let asked;
        let state = match hex_state {
            Some(state) => state,
            None => {
                asked = hex::state(&paths.home_dir);
                &asked
            }
        };
        // A closed Hex is opened by the transcription itself (CDXC:Dictate 2026-11-13 in hex.rs).
        if matches!(state, hex::State::NotRunning) && hex::opens_by_itself(&paths.home_dir) {
            return None;
        }
        return hex::problem(state, settings.hex_model.as_deref());
    }
    if settings.engine == Engine::Superwhisper {
        return superwhisper::problem(&superwhisper::state(&paths.home_dir));
    }
    let provider = provider_of(settings.engine)?;
    match keys::key_state(paths, provider) {
        KeyState::Saved => None,
        KeyState::Unreadable => Some((
            "keyUnreadable",
            format!(
                "The saved {} API key can't be read anymore. Paste it again in Settings.",
                provider.label()
            ),
        )),
        KeyState::Missing => Some((
            "noKey",
            format!("Add your {} API key in Settings.", provider.label()),
        )),
    }
}

/// `DictateEngine` in the API contract. `hex_state` is [`hex_state_for`]'s answer.
pub(crate) fn engine_view(
    state: &AppState,
    settings: &DictateSettings,
    hex_state: Option<&hex::State>,
) -> Value {
    let problem = engine_problem(&state.paths, settings, hex_state);
    json!({
        "engine": settings.engine.id(),
        "label": engine_label_with(settings, hex_state),
        "ready": problem.is_none(),
        "problem": problem.map(|(code, message)| json!({"code": code, "message": message})),
        // Phones at 11.2.1 add ", then AI polish" to the engine line when this is true.
        "polish": false,
    })
}
