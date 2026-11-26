//! The language a voice call speaks (`voiceAgentLanguage`) and the turn words that end a spoken
//! turn in it (`voiceAgentTurnWords`): what `/api/readVoiceAgent` lists, what
//! `/api/setVoiceAgentSettings` accepts, and what a call starts with (`for_call`).
//!
//! CDXC:VoiceMode 2026-11-25 DECISION:
//! User: the language the agent speaks is a setting whose default follows the language of the device that places the call, with "nicht nur deutsch und englisch. die sprachen, die die modelle auch unterstützen. beispielsweise spanisch, italienisch, französisch etc. alles gängige." Turn words are set per language ("mehrere Wörter festlegen können pro Sprache"): English defaults to "reply" and "go ahead", German to "fertig" and "antworte", and a language without its own words uses the English defaults until the user sets some.

use serde_json::{json, Map, Value};

use super::speech::languages::LANGUAGE_LABELS;

pub(super) const LANGUAGE_SETTING: &str = "voiceAgentLanguage";
pub(super) const TURN_WORDS_SETTING: &str = "voiceAgentTurnWords";
/// `voiceAgentLanguage` when the call speaks the language of the device that places it.
pub(super) const DEVICE: &str = "device";

/// The languages a call can speak, in the order the pickers list them, as ISO 639-1 codes: xAI's
/// documented list for its realtime voice model, kept where OpenAI's speech-output list has them too.
///
/// CDXC:VoiceMode 2026-11-26 DECISION: User: keep the 15 languages, with OpenAI's speech-output list as the named substitute source.
/// CDXC:VoiceMode 2026-11-25 WHY: the sources, read on 2026-11-25:
/// - xAI documents the languages of its realtime voice model, https://docs.x.ai/developers/model-capabilities/audio/speech-to-speech, "Supported Languages": English, Arabic (three regional variants), Bengali, Chinese (Simplified), French, German, Hindi, Indonesian, Italian, Japanese, Korean, Portuguese (Brazil, Portugal), Russian, Spanish (Mexico, Spain), Turkish, Vietnamese.
/// - OpenAI publishes no language list for GPT Realtime (https://developers.openai.com/api/docs/models/gpt-realtime-2.1 has none). Its speech-output list stands in for it: https://developers.openai.com/api/docs/guides/text-to-speech, "Supported languages" (the Whisper languages), which has every xAI language above except Bengali.
/// So the list is not a documented intersection of what both realtime models support: OpenAI's side is a substitute source, and how well GPT Realtime speaks each language is not established here.
pub(super) const LANGUAGES: [&str; 15] = [
    "en", "de", "fr", "es", "it", "pt", "ru", "zh", "ja", "ko", "ar", "hi", "tr", "vi", "id",
];

/// The turn words a language has until the user sets their own.
const ENGLISH_TURN_WORDS: &[&str] = &["reply", "go ahead"];
const GERMAN_TURN_WORDS: &[&str] = &["fertig", "antworte"];

/// How many turn words a language can have, and how long each can be (in characters).
const MAX_TURN_WORDS: usize = 8;
const MAX_TURN_WORD_CHARS: usize = 40;

/// The supported code `code` names, if it names one.
pub(super) fn supported(code: &str) -> Option<&'static str> {
    LANGUAGES.into_iter().find(|known| *known == code)
}

/// The English name of a supported language (Dictate's picker labels, `speech/languages.rs`).
pub(super) fn label(code: &str) -> &'static str {
    LANGUAGE_LABELS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, label)| *label)
        .unwrap_or("English")
}

/// The built-in turn words of `code`: German has its own, every other language the English ones.
pub(super) fn default_turn_words(code: &str) -> &'static [&'static str] {
    match code {
        "de" => GERMAN_TURN_WORDS,
        _ => ENGLISH_TURN_WORDS,
    }
}

/// The saved `voiceAgentLanguage`: `device` or a supported code (`device` when unset or unknown).
pub(super) fn saved_language(settings: &Value) -> &'static str {
    settings
        .get(LANGUAGE_SETTING)
        .and_then(Value::as_str)
        .and_then(supported)
        .unwrap_or(DEVICE)
}

/// `words` as turn words to save: 1 to 8 entries, each trimmed to 1 to 40 characters.
pub(super) fn checked_turn_words(words: &[Value]) -> Option<Vec<String>> {
    if words.is_empty() || words.len() > MAX_TURN_WORDS {
        return None;
    }
    words
        .iter()
        .map(|word| {
            let word = word.as_str()?.trim();
            let length = word.chars().count();
            (1..=MAX_TURN_WORD_CHARS)
                .contains(&length)
                .then(|| word.to_string())
        })
        .collect()
}

/// The user's own turn words for `code`, when they saved some that are still valid.
fn saved_turn_words(settings: &Value, code: &str) -> Option<Vec<String>> {
    let words = settings.get(TURN_WORDS_SETTING)?.get(code)?.as_array()?;
    checked_turn_words(words)
}

/// The turn words a call in `code` listens for: the user's own, or else the built-in ones.
pub(super) fn turn_words(settings: &Value, code: &str) -> Vec<String> {
    saved_turn_words(settings, code).unwrap_or_else(|| {
        default_turn_words(code)
            .iter()
            .map(|word| word.to_string())
            .collect()
    })
}

/// What a call speaks and which words end a turn in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallLanguage {
    pub(crate) code: &'static str,
    pub(crate) turn_words: Vec<String>,
}

/// The call's language and turn words from the saved settings and the call's `language` query
/// parameter: a saved code wins; with `device`, a supported parameter is used, and no parameter or
/// an unsupported one gives English (what every call spoke before the setting existed).
pub(crate) fn for_call(settings: &Value, language_parameter: Option<&str>) -> CallLanguage {
    let code = match saved_language(settings) {
        DEVICE => language_parameter.and_then(supported).unwrap_or("en"),
        code => code,
    };
    CallLanguage {
        code,
        turn_words: turn_words(settings, code),
    }
}

/// `readVoiceAgent`'s `language`, `languages`, `turnWords` and `turnWordDefaults`.
pub(super) fn client_view(settings: &Value) -> Map<String, Value> {
    let mut view = Map::new();
    view.insert("language".into(), json!(saved_language(settings)));
    view.insert(
        "languages".into(),
        Value::Array(
            LANGUAGES
                .iter()
                .map(|code| json!({"code": code, "label": label(code)}))
                .collect(),
        ),
    );
    view.insert(
        "turnWords".into(),
        Value::Object(
            LANGUAGES
                .iter()
                .map(|code| (code.to_string(), json!(turn_words(settings, code))))
                .collect(),
        ),
    );
    view.insert(
        "turnWordDefaults".into(),
        Value::Object(
            LANGUAGES
                .iter()
                .map(|code| (code.to_string(), json!(default_turn_words(code))))
                .collect(),
        ),
    );
    view
}

/// The accepted values of `setVoiceAgentSettings`' `language`, for its error message.
pub(super) fn accepted_languages() -> String {
    format!("{DEVICE}, {}", LANGUAGES.join(", "))
}
