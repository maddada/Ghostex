//! Dictate's settings on this computer: which engine transcribes (Hex, Superwhisper, OpenAI or xAI), the Hex
//! model and the language hint. gxserver keeps them in its own file
//! (`<gxserver state>/voice-agent/dictate-settings.json`) and is their only writer.
//!
//! CDXC:Dictate 2026-11-05 WHY:
//! These belong to the computer whose gxserver transcribes, and the phone changes them per computer (Q1275), including on a computer that runs no desktop app (a Linux server). So they are not in the desktop's settings file, whose writes go through the running app (CDXC:Settings 2026-09-09); gxserver writes this file itself, atomically, and every client reads it through `/api/readDictateSettings`.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::languages;
use crate::paths::GxserverPaths;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Engine {
    #[serde(rename = "openai")]
    OpenAi,
    Xai,
    /// Hex 3's local API on this computer (`hex.rs`).
    Hex,
    /// Superwhisper on this Linux computer (`superwhisper.rs`).
    Superwhisper,
}

impl Engine {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Engine::OpenAi => "openai",
            Engine::Xai => "xai",
            Engine::Hex => "hex",
            Engine::Superwhisper => "superwhisper",
        }
    }

    fn from_id(id: &str) -> Option<Engine> {
        match id {
            "openai" => Some(Engine::OpenAi),
            "xai" => Some(Engine::Xai),
            "hex" => Some(Engine::Hex),
            "superwhisper" => Some(Engine::Superwhisper),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DictateSettings {
    pub(crate) engine: Engine,
    /// The Hex model id (`GET /models`); `None` uses Hex's first ready model.
    #[serde(default)]
    pub(crate) hex_model: Option<String>,
    pub(crate) language: String,
}

/// Keys of settings Dictate no longer has (`cleanup.rs`), which files saved by older versions still
/// hold and older clients still send in a patch: read and patched without complaint, never kept.
pub(crate) const RETIRED_KEYS: &[&str] = &[
    "polish",
    "dictionary",
    "addProjectNames",
    "removeFillers",
    "localRetiredNotice",
    "localRetiredDictationNotice",
];

impl DictateSettings {
    /// A new install's settings: the engine is the saved key's (xAI first, then OpenAI), else
    /// Superwhisper where it is installed (Linux), else Hex.
    ///
    /// CDXC:Dictate 2026-11-14 DECISION:
    /// User (Q1259 "default to b", Q1272): the engine is an explicit choice with no automatic switching. A new install picks it once, the key when one exists and otherwise Hex, the recommended engine since our local model was retired (Q1709, `retired.rs`), and then keeps it: adding a key later doesn't switch. User (Q22, 2026-11-14): "a new Linux install with Superwhisper installed starts on Superwhisper; existing installs keep what they picked", so a keyless new install on a computer with Superwhisper picks it instead of Hex. Supersedes the 2026-11-08 version, which picked Hex there.
    pub(crate) fn first_run(
        xai_key: bool,
        openai_key: bool,
        superwhisper: bool,
    ) -> DictateSettings {
        DictateSettings {
            engine: if xai_key {
                Engine::Xai
            } else if openai_key {
                Engine::OpenAi
            } else if superwhisper {
                Engine::Superwhisper
            } else {
                Engine::Hex
            },
            hex_model: None,
            language: "auto".to_string(),
        }
    }

    /// The language hint recognizers get (`None` for auto-detect).
    pub(crate) fn language_hint(&self) -> Option<&str> {
        (self.language != "auto").then_some(self.language.as_str())
    }

    /// Applies a client's patch: each top-level key replaces its value. Answers a sentence saying
    /// what is wrong instead.
    pub(crate) fn patched(&self, patch: &Map<String, Value>) -> Result<DictateSettings, String> {
        let mut next = self.clone();
        for (key, value) in patch {
            match key.as_str() {
                "engine" => {
                    next.engine = value.as_str().and_then(Engine::from_id).ok_or(
                        if value.as_str() == Some("local") {
                            "Ghostex no longer has its own speech model: pick hex, superwhisper, openai or xai."
                        } else {
                            "engine must be hex, superwhisper, openai or xai."
                        },
                    )?;
                }
                "localModel" => {
                    return Err(
                        "Ghostex no longer has its own speech model: pick Hex as the engine."
                            .to_string(),
                    )
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
                    if code != "auto" && !languages::LANGUAGES.contains(&code.as_str()) {
                        return Err(format!("{code} is not a language Dictate knows."));
                    }
                    next.language = code;
                }
                // Phones at 11.2.1 and desktops at 11.2.2 still send these with every change on
                // Settings › Dictation; refusing one would fail the whole change.
                retired if RETIRED_KEYS.contains(&retired) => {}
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

/// Saved settings, with a retired engine moved to Hex: `(settings, moved)`.
///
/// A file whose engine is `local` (our retired engine, `retired.rs`) moves to Hex, never to a
/// key's paid engine. Its `localModel` and the [`RETIRED_KEYS`] older versions saved are
/// dropped; the next save leaves them out.
pub(crate) fn parse(text: &str) -> Option<(DictateSettings, bool)> {
    let mut value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object_mut()?;
    let moved = object.get("engine").and_then(Value::as_str) == Some("local");
    if moved {
        object.insert("engine".to_string(), Value::String("hex".to_string()));
    }
    object.remove("localModel");
    for key in RETIRED_KEYS {
        object.remove(*key);
    }
    let settings = serde_json::from_value::<DictateSettings>(value).ok()?;
    Some((settings, moved))
}

/// Whether the saved settings pick Hex, without writing a new install's settings.
pub(crate) fn uses_hex(paths: &GxserverPaths) -> bool {
    fs::read_to_string(file(paths))
        .ok()
        .and_then(|text| parse(&text))
        .is_some_and(|(settings, _)| settings.engine == Engine::Hex)
}

/// The saved settings, or a new install's (written at once, so the first choice is kept).
/// `new_install` answers whether an xAI key and an OpenAI key are saved and whether Superwhisper is
/// installed; it is asked only on the first read.
pub(crate) fn read(
    paths: &GxserverPaths,
    new_install: impl FnOnce() -> (bool, bool, bool),
) -> DictateSettings {
    let _guard = lock();
    if let Some((saved, moved)) = fs::read_to_string(file(paths))
        .ok()
        .and_then(|text| parse(&text))
    {
        if moved {
            let _ = write_locked(paths, &saved);
        }
        return saved;
    }
    let (xai, openai, superwhisper) = new_install();
    let settings = DictateSettings::first_run(xai, openai, superwhisper);
    let _ = write_locked(paths, &settings);
    settings
}

/// Saves a client's patch over the saved settings and answers the result.
pub(crate) fn update(
    paths: &GxserverPaths,
    patch: &Map<String, Value>,
    new_install: impl FnOnce() -> (bool, bool, bool),
) -> Result<DictateSettings, String> {
    let current = read(paths, new_install);
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
