//! What Dictate will do on this computer: the settings with their first-run default, the words the
//! recognizer is told to expect, and the engine and model that will transcribe (the mic tooltip).

use serde_json::{json, Value};

use super::catalog;
use super::hex;
use super::models;
use super::settings::{self, DictateSettings, Engine};
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

/// Always a hint: recognizers otherwise write "Gostex".
const BUILT_IN_WORDS: &[&str] = &["Ghostex"];

/// The project names on this computer, as dictionary words.
pub(crate) fn project_words(state: &AppState) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for project in crate::project_git_state::list_projects(state).unwrap_or_default() {
        let Some(name) = project.get("name").and_then(Value::as_str) else {
            continue;
        };
        let name = name.trim();
        if name.chars().count() >= 3
            && name.chars().count() <= 64
            && !words.iter().any(|seen| seen.eq_ignore_ascii_case(name))
        {
            words.push(name.to_string());
        }
    }
    words
}

/// The words every engine is told to expect: Ghostex, the user's words, then project names.
pub(crate) fn hint_words(settings: &DictateSettings, projects: &[String]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut add = |word: &str| {
        if !words.iter().any(|seen| seen.eq_ignore_ascii_case(word)) {
            words.push(word.to_string());
        }
    };
    BUILT_IN_WORDS.iter().for_each(|word| add(word));
    settings
        .dictionary
        .iter()
        .for_each(|entry| add(&entry.word));
    if settings.add_project_names {
        projects.iter().for_each(|word| add(word));
    }
    words
}

/// (word, alias) for every alias in the dictionary.
pub(crate) fn alias_pairs(settings: &DictateSettings) -> Vec<(String, String)> {
    settings
        .dictionary
        .iter()
        .flat_map(|entry| {
            entry
                .aliases
                .iter()
                .map(move |alias| (entry.word.clone(), alias.clone()))
        })
        .collect()
}

pub(crate) fn provider_of(engine: Engine) -> Option<Provider> {
    match engine {
        Engine::Local | Engine::Hex => None,
        Engine::OpenAi => Some(Provider::OpenAi),
        Engine::Xai => Some(Provider::Xai),
    }
}

/// The tooltip's first line after "Dictate · ".
pub(crate) fn engine_label(settings: &DictateSettings) -> String {
    match settings.engine {
        Engine::Local => format!(
            "Local, {}",
            catalog::model(&settings.local_model).map_or("model", |spec| spec.short_title)
        ),
        Engine::OpenAi => "OpenAI gpt-4o-transcribe".to_string(),
        Engine::Xai => "xAI speech to text".to_string(),
        Engine::Hex => match &settings.hex_model {
            Some(model) => format!("Hex, {model}"),
            None => "Hex".to_string(),
        },
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

fn gigabytes(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / 1e9)
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
        return hex::problem(state, settings.hex_model.as_deref());
    }
    match provider_of(settings.engine) {
        None => {
            let spec = catalog::model(&settings.local_model)?;
            if models::is_ready(paths, spec) {
                return None;
            }
            Some(match models::progress(spec) {
                Some(percent) => (
                    "modelDownloading",
                    format!("{} is downloading ({percent}%).", spec.short_title),
                ),
                None => (
                    "modelMissing",
                    format!(
                        "Not downloaded yet. Click to download ({})",
                        gigabytes(spec.download_bytes())
                    ),
                ),
            })
        }
        Some(provider) => match keys::key_state(paths, provider) {
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
        },
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
        "modelId": (settings.engine == Engine::Local).then_some(settings.local_model.as_str()),
        "ready": problem.is_none(),
        "problem": problem.map(|(code, message)| json!({"code": code, "message": message})),
        "polish": settings.polish.enabled && super::cleanup::polish_available(state),
    })
}
