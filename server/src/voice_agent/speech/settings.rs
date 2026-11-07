//! Dictate's settings on this computer: which engine transcribes, the local model, the language
//! hint, the dictionary and the clean-up switches. gxserver keeps them in its own file
//! (`<gxserver state>/voice-agent/dictate-settings.json`) and is their only writer.
//!
//! CDXC:Dictate 2026-11-05 WHY:
//! These belong to the computer whose gxserver transcribes, and the phone changes them per computer (Q1275), including on a computer that runs no desktop app (a Linux server). So they are not in the desktop's settings file, whose writes go through the running app (CDXC:Settings 2026-09-09); gxserver writes this file itself, atomically, and every client reads it through `/api/readDictateSettings`.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::catalog;
use crate::paths::GxserverPaths;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Engine {
    Local,
    #[serde(rename = "openai")]
    OpenAi,
    Xai,
    /// Hex 3's local API on this computer (`hex.rs`).
    Hex,
}

impl Engine {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Engine::Local => "local",
            Engine::OpenAi => "openai",
            Engine::Xai => "xai",
            Engine::Hex => "hex",
        }
    }

    fn from_id(id: &str) -> Option<Engine> {
        match id {
            "local" => Some(Engine::Local),
            "openai" => Some(Engine::OpenAi),
            "xai" => Some(Engine::Xai),
            "hex" => Some(Engine::Hex),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DictionaryWord {
    pub(crate) word: String,
    #[serde(default)]
    pub(crate) aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Polish {
    pub(crate) enabled: bool,
    pub(crate) model: String,
    /// `None` uses the built-in prompt (`cleanup::POLISH_PROMPT`).
    #[serde(default)]
    pub(crate) prompt: Option<String>,
}

pub(crate) const POLISH_MODEL: &str = "claude-sonnet-5-5";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DictateSettings {
    pub(crate) engine: Engine,
    pub(crate) local_model: String,
    /// The Hex model id (`GET /models`); `None` uses Hex's first ready model.
    #[serde(default)]
    pub(crate) hex_model: Option<String>,
    pub(crate) language: String,
    #[serde(default)]
    pub(crate) dictionary: Vec<DictionaryWord>,
    #[serde(default = "yes")]
    pub(crate) add_project_names: bool,
    #[serde(default = "yes")]
    pub(crate) remove_fillers: bool,
    pub(crate) polish: Polish,
}

fn yes() -> bool {
    true
}

const MAX_WORDS: usize = 200;
const MAX_ALIASES: usize = 8;

impl DictateSettings {
    /// A new install's settings: the engine is the saved key's (xAI first, then OpenAI), else the
    /// local model.
    ///
    /// CDXC:Dictate 2026-11-05 DECISION:
    /// User (Q1259 "default to b", Q1272): the engine is an explicit choice of Local, OpenAI or xAI with no automatic switching. A new install picks it once, the key when one exists and the local model otherwise, and then keeps it: adding a key later doesn't switch. AI polish is on for a new install, with Claude Sonnet 5.5 (Q1273). Supersedes CDXC:VoiceMode 2026-10-30 (Q781, "the xAI key when it is set, otherwise the OpenAI key" at every transcription): that order now only picks a new install's engine.
    pub(crate) fn first_run(xai_key: bool, openai_key: bool) -> DictateSettings {
        DictateSettings {
            engine: if xai_key {
                Engine::Xai
            } else if openai_key {
                Engine::OpenAi
            } else {
                Engine::Local
            },
            local_model: catalog::PARAKEET_ID.to_string(),
            hex_model: None,
            language: "auto".to_string(),
            dictionary: Vec::new(),
            add_project_names: true,
            remove_fillers: true,
            polish: Polish {
                enabled: true,
                model: POLISH_MODEL.to_string(),
                prompt: None,
            },
        }
    }

    /// The language hint recognizers get (`None` for auto-detect).
    pub(crate) fn language_hint(&self) -> Option<&str> {
        (self.language != "auto").then_some(self.language.as_str())
    }

    /// Applies a client's patch: each top-level key replaces its value, `polish` merges its own
    /// keys. Answers a sentence saying what is wrong instead.
    pub(crate) fn patched(&self, patch: &Map<String, Value>) -> Result<DictateSettings, String> {
        let mut next = self.clone();
        for (key, value) in patch {
            match key.as_str() {
                "engine" => {
                    next.engine = value
                        .as_str()
                        .and_then(Engine::from_id)
                        .ok_or("engine must be local, openai, xai or hex.")?;
                }
                "localModel" => {
                    let id = value.as_str().unwrap_or_default();
                    if catalog::model(id).is_none() {
                        return Err(format!("{id} is not a local model Dictate offers."));
                    }
                    next.local_model = id.to_string();
                }
                "hexModel" => {
                    next.hex_model = match value {
                        Value::Null => None,
                        Value::String(id) if valid_hex_model(id) => Some(id.clone()),
                        _ => return Err("hexModel must be a Hex model id, or null.".to_string()),
                    };
                }
                "language" => {
                    let code = value
                        .as_str()
                        .unwrap_or_default()
                        .trim()
                        .to_ascii_lowercase();
                    if code != "auto" && !catalog::WHISPER_LANGUAGES.contains(&code.as_str()) {
                        return Err(format!("{code} is not a language Dictate knows."));
                    }
                    next.language = code;
                }
                "dictionary" => {
                    let words: Vec<DictionaryWord> = serde_json::from_value(value.clone())
                        .map_err(|_| "dictionary must be a list of {word, aliases}.".to_string())?;
                    next.dictionary = clean_dictionary(words)?;
                }
                "addProjectNames" => next.add_project_names = flag(key, value)?,
                "removeFillers" => next.remove_fillers = flag(key, value)?,
                "polish" => {
                    let polish = value.as_object().ok_or("polish must be an object.")?;
                    for (key, value) in polish {
                        match key.as_str() {
                            "enabled" => next.polish.enabled = flag("polish.enabled", value)?,
                            "model" => {
                                if value.as_str() != Some(POLISH_MODEL) {
                                    return Err(format!("polish.model must be {POLISH_MODEL}."));
                                }
                            }
                            "prompt" => {
                                next.polish.prompt = match value {
                                    Value::Null => None,
                                    Value::String(text) if text.trim().is_empty() => None,
                                    Value::String(text) if text.len() <= 8_000 => {
                                        Some(text.trim().to_string())
                                    }
                                    _ => {
                                        return Err(
                                            "polish.prompt must be text of at most 8,000 characters, or null."
                                                .to_string(),
                                        )
                                    }
                                };
                            }
                            other => return Err(format!("polish has no setting named {other}.")),
                        }
                    }
                }
                other => return Err(format!("Dictate has no setting named {other}.")),
            }
        }
        Ok(next)
    }
}

/// A Hex model id as Hex names them (`parakeet_v3`, `custom-…`): it is sent in a query string.
fn valid_hex_model(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'@' | b'/')
        })
}

fn flag(key: &str, value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{key} must be true or false."))
}

/// Trims every word and alias, drops empty and repeated ones, and checks the limits.
fn clean_dictionary(words: Vec<DictionaryWord>) -> Result<Vec<DictionaryWord>, String> {
    let mut cleaned: Vec<DictionaryWord> = Vec::new();
    for entry in words {
        let word = entry.word.split_whitespace().collect::<Vec<_>>().join(" ");
        if word.is_empty() {
            continue;
        }
        if word.chars().count() > 64 {
            return Err(format!("\"{word}\" is longer than 64 characters."));
        }
        if cleaned
            .iter()
            .any(|seen| seen.word.eq_ignore_ascii_case(&word))
        {
            continue;
        }
        let mut aliases: Vec<String> = Vec::new();
        for alias in entry.aliases {
            let alias = alias.split_whitespace().collect::<Vec<_>>().join(" ");
            if alias.is_empty() || aliases.iter().any(|seen| seen.eq_ignore_ascii_case(&alias)) {
                continue;
            }
            let length = alias.chars().count();
            if !(3..=64).contains(&length) {
                return Err(format!(
                    "The alias \"{alias}\" must be 3 to 64 characters long."
                ));
            }
            aliases.push(alias);
        }
        if aliases.len() > MAX_ALIASES {
            return Err(format!("\"{word}\" has more than {MAX_ALIASES} aliases."));
        }
        cleaned.push(DictionaryWord { word, aliases });
    }
    if cleaned.len() > MAX_WORDS {
        return Err(format!("The dictionary holds at most {MAX_WORDS} words."));
    }
    Ok(cleaned)
}

pub(crate) fn file(paths: &GxserverPaths) -> PathBuf {
    paths
        .root_dir
        .join("voice-agent")
        .join("dictate-settings.json")
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the saved settings pick Hex, without writing a new install's settings.
pub(crate) fn uses_hex(paths: &GxserverPaths) -> bool {
    fs::read_to_string(file(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<DictateSettings>(&text).ok())
        .is_some_and(|settings| settings.engine == Engine::Hex)
}

/// The saved settings, or a new install's (written at once, so the first choice is kept).
/// `has_keys` answers whether an xAI and an OpenAI key are saved; it is asked only on the first
/// read.
pub(crate) fn read(
    paths: &GxserverPaths,
    has_keys: impl FnOnce() -> (bool, bool),
) -> DictateSettings {
    let _guard = lock();
    if let Some(saved) = fs::read_to_string(file(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<DictateSettings>(&text).ok())
    {
        return saved;
    }
    let (xai, openai) = has_keys();
    let settings = DictateSettings::first_run(xai, openai);
    let _ = write_locked(paths, &settings);
    settings
}

/// Saves a client's patch over the saved settings and answers the result.
pub(crate) fn update(
    paths: &GxserverPaths,
    patch: &Map<String, Value>,
    has_keys: impl FnOnce() -> (bool, bool),
) -> Result<DictateSettings, String> {
    let current = read(paths, has_keys);
    let _guard = lock();
    let next = current.patched(patch)?;
    write_locked(paths, &next)
        .map_err(|error| format!("Couldn't save Dictate's settings: {error}"))?;
    Ok(next)
}

fn write_locked(paths: &GxserverPaths, settings: &DictateSettings) -> std::io::Result<()> {
    let path = file(paths);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    fs::write(&temp, text)?;
    fs::rename(&temp, &path)
}
