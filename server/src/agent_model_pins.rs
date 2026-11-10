//! The model and effort Ghostex starts Claude, Codex and Cursor sessions on.
//!
//! CDXC:AgentProviders 2026-10-07 DECISION:
//! User: the chat's model pill must show the model at once, because "if we don't have the model name showing and show a skeleton, the user will be afraid to hit Enter". Ghostex pins `--model`/`--effort` at create, resume and wake: "we pin based on what the user last selected basically u know? (unless he picks a model just for a session then we don't save that one). Also for cursor u can do last used I think they dont have selecting model for 1 session only." When the agent's own settings file changes outside Ghostex, the next launch is not pinned and the agent's own report becomes the new default. The pin is launch evidence the agent obeys, so the pill names the model the agent runs; nothing is guessed from settings files (CDXC:AgentScreenDetection 2026-10-04), whose values are only compared to see that they changed.
//! SEE-ALSO: agents/launch_plan.rs (create), agents/resume_plan.rs (resume and wake), session_chat_codex_picker.rs (a chat pick records the default), session_chat_options/detector.rs (a live report teaches it), session_chat_options/launch_selection.rs (the pill's launch value and labels).

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::session_chat_options::{
    SessionChatDetectedChoice, SessionChatOptionEvidence, SessionChatTerminalDetection,
};

/// `runtimeSettings` key recording why a session runs the model it was started on.
pub(crate) const SESSION_PIN_KEY: &str = "agentModelPin";
/// Started on the user's default (the pin), or moved to a new default by a chat pick.
pub(crate) const ORIGIN_DEFAULT: &str = "default";
/// A model chosen for this session alone: never taught to the default.
pub(crate) const ORIGIN_SESSION: &str = "session";
/// Started without a pin, so the agent's own report is the default.
pub(crate) const ORIGIN_LEARN: &str = "learn";

const STORE_FILE: &str = "agent-model-pins.json";
const MAX_SESSION_READINGS: usize = 2000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPin {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort_label: Option<String>,
}

impl ModelPin {
    fn same_choice(&self, other: &ModelPin) -> bool {
        self.model == other.model && self.effort == other.effort
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultPin {
    pin: ModelPin,
    /// The model-related values of the agent's settings file when this default was learned.
    #[serde(default)]
    settings: Value,
    #[serde(default)]
    recorded_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionReading {
    #[serde(default)]
    family: String,
    pin: ModelPin,
    #[serde(default)]
    recorded_at: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PinFile {
    #[serde(default)]
    defaults: BTreeMap<String, DefaultPin>,
    #[serde(default)]
    sessions: BTreeMap<String, SessionReading>,
}

struct PinStore {
    path: Option<PathBuf>,
    /// Where Claude's statusline payloads land, read by [`session_choice`].
    hook_state_directory: Option<PathBuf>,
    file: PinFile,
}

fn store() -> &'static Mutex<PinStore> {
    static STORE: OnceLock<Mutex<PinStore>> = OnceLock::new();
    STORE.get_or_init(|| {
        Mutex::new(PinStore {
            path: None,
            hook_state_directory: None,
            file: PinFile::default(),
        })
    })
}

/// Loads the remembered defaults; until this runs (unit tests) nothing is pinned.
pub(crate) fn init(paths: &crate::paths::GxserverPaths) {
    let path = paths.app_state_dir.join(STORE_FILE);
    let mut file = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<PinFile>(&text).ok())
        .unwrap_or_default();
    let healed = drop_unlaunchable_pins(&mut file);
    if let Ok(mut store) = store().lock() {
        store.path = Some(path);
        store.hook_state_directory =
            Some(crate::session_chat_options::session_chat_hook_state_directory(paths));
        store.file = file;
        if healed {
            save(&store);
        }
    }
}

/// Whether `model` is something the family's CLI takes as `--model`.
///
/// CDXC:AgentProviders 2026-11-05 WHY:
/// GitHub #422: with Cursor on Auto with the Balance setting, its footer says "Auto Balance", a name the model catalog does not list, so the screen reading kept the name itself as the model and Ghostex pinned it: every new Cursor session ran `cursor-agent --model "Auto Balance"`, which exits with "Cannot use this model: Auto Balance. Available models: auto, …", and the next session that did start re-learned the name. Cursor's model ids (`--list-models`) are lowercase words without spaces (`auto`, `grok-4.7`, `claude-opus-4-8[context=1m]`); its names have capitals and spaces ("Auto Balance", "Grok 4.7  High Fast" with zero-width spaces). Only an id is pinned. A name is not translated: `auto-smart`, the id `cli-config.json` keeps for Auto Balance, is refused by `--model` too (2026.10.22 and 2026.11.05), `--model auto` would drop the Balance setting, and any `--model` launch rewrites Cursor's saved default (`cli-config.json` `model`, measured 2026-11-05). Without a pin Cursor starts on that saved default, which is the model the user last used, so nothing is lost.
/// Claude and Codex readings already map to their CLI values, so they keep every pin.
pub(crate) fn launchable_model(family: &str, model: &str) -> bool {
    match family {
        "cursor" => {
            !model.is_empty()
                && model
                    .chars()
                    .all(|ch| ch.is_ascii_graphic() && !ch.is_ascii_uppercase())
        }
        _ => !model.trim().is_empty(),
    }
}

/// Drops remembered models the agent's CLI refuses, so a pin learned before
/// [`launchable_model`] (an "Auto Balance" default) stops breaking every launch. Answers whether
/// anything was dropped.
fn drop_unlaunchable_pins(file: &mut PinFile) -> bool {
    let defaults = file.defaults.len();
    let sessions = file.sessions.len();
    file.defaults
        .retain(|family, default| launchable_model(family, &default.pin.model));
    file.sessions.retain(|_, reading| {
        reading.family.is_empty() || launchable_model(&reading.family, &reading.pin.model)
    });
    file.defaults.len() != defaults || file.sessions.len() != sessions
}

fn save(store: &PinStore) {
    let Some(path) = store.path.as_ref() else {
        return;
    };
    let Ok(text) = serde_json::to_string_pretty(&store.file) else {
        return;
    };
    let _ = ghostex_paths::durable_file::write_durably(path, text.as_bytes());
}

/// Every agent's remembered default (model, effort and their labels) as settings sync carries it.
pub(crate) fn default_pins() -> BTreeMap<String, Value> {
    let Ok(store) = store().lock() else {
        return BTreeMap::new();
    };
    store
        .file
        .defaults
        .iter()
        .filter_map(|(family, default)| {
            Some((family.clone(), serde_json::to_value(&default.pin).ok()?))
        })
        .collect()
}

/// Another computer's remembered default for `family`, brought by settings sync (`None` forgets
/// this computer's). The agent's own settings file is snapshotted here, as for a pick made here.
pub(crate) fn set_synced_default(family: &str, pin: Option<&Value>) {
    match pin.and_then(|pin| serde_json::from_value::<ModelPin>(pin.clone()).ok()) {
        Some(pin) => record_default(family, pin),
        None => {
            let Ok(mut store) = store().lock() else {
                return;
            };
            if store.file.defaults.remove(family).is_some() {
                save(&store);
            }
        }
    }
}

/// The agent families whose launch Ghostex pins.
pub(crate) fn pin_family(family: &str) -> Option<&'static str> {
    match family {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "cursor" => Some("cursor"),
        _ => None,
    }
}

fn session_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

/// The default a new session of this family starts on, or `None` when none is known yet or the
/// agent's settings file changed since it was learned (that launch runs the agent's own default
/// and teaches the new one).
pub(crate) fn launch_default(family: &str) -> Option<ModelPin> {
    let family = pin_family(family)?;
    let remembered = store().lock().ok()?.file.defaults.get(family).cloned()?;
    (remembered.settings == settings_snapshot(family)
        && launchable_model(family, &remembered.pin.model))
    .then_some(remembered.pin)
}

/// The model and effort this conversation runs next: what it last ran, or what the user last
/// picked for it. Its resume and wake are launched on it and its pill shows it, so both agree
/// before the agent reports anything.
///
/// CDXC:SessionChat 2026-10-08 DECISION:
/// User: "we should cache the last used model and effort in each conversation fam for the composer since we basically keep that when we resume right?" Each conversation keeps its own last model and effort, here in gxserver so desktop, web and phone read the same value: the last live reading, a chat pick for it (a session-only pick included) the moment it is applied, and a `/model` typed in its terminal ("follow the outside change"). It beats the `--model`/`--effort` its saved command was launched with, which only a chat pick rewrote, so before this a slept conversation could come back on its launch model. New sessions still start on the agent-wide default (CDXC:AgentProviders 2026-10-07 above).
/// SEE-ALSO: agents/resume_plan.rs `with_resume_model_pin` (the relaunch), session_chat_options/launch_selection.rs (the pill), session_chat_codex_picker.rs (a pick records it).
pub(crate) fn session_choice(
    project_id: &str,
    session_id: &str,
    family: &str,
    agent_session_id: Option<&str>,
) -> Option<ModelPin> {
    let family = pin_family(family)?;
    let (reading, hook_state_directory) = {
        let store = store().lock().ok()?;
        let reading = store
            .file
            .sessions
            .get(&session_key(project_id, session_id))
            .filter(|reading| reading.family.is_empty() || reading.family == family)
            .cloned();
        (reading, store.hook_state_directory.clone())
    };
    // Claude writes its statusline on every model or effort change, also while no chat follows the
    // session (a `/model` typed in the terminal pane), so a payload newer than the reading wins.
    let statusline = (family == "claude")
        .then_some(())
        .and(hook_state_directory.zip(agent_session_id))
        .and_then(|(directory, agent_session_id)| {
            crate::session_chat_options::read_claude_statusline_reading(
                &directory,
                agent_session_id,
                false,
            )
        })
        .filter(|(_, written_at)| {
            reading
                .as_ref()
                .is_none_or(|reading| written_at.as_str() > reading.recorded_at.as_str())
        })
        .and_then(|(selection, _)| live_pin(&selection.model, &selection.effort));
    match (statusline, reading) {
        (Some(mut newer), reading) => {
            if newer.effort.is_none() {
                newer.effort = reading
                    .filter(|reading| reading.pin.model == newer.model)
                    .and_then(|reading| reading.pin.effort);
            }
            Some(newer)
        }
        (None, reading) => reading.map(|reading| reading.pin),
    }
    .filter(|pin| launchable_model(family, &pin.model))
}

/// A pick applied in the chat is this conversation's choice at once, so a session slept before
/// the agent repaints still comes back on it.
pub(crate) fn record_session_choice(
    project_id: &str,
    session_id: &str,
    family: &str,
    pin: ModelPin,
) {
    let Some(family) = pin_family(family) else {
        return;
    };
    let Ok(mut store) = store().lock() else {
        return;
    };
    let key = session_key(project_id, session_id);
    let old = store
        .file
        .sessions
        .get(&key)
        .map(|reading| reading.pin.clone());
    store.file.sessions.insert(
        key,
        SessionReading {
            family: family.to_string(),
            pin: with_known_labels(pin, old.as_ref()),
            recorded_at: crate::agents::now_iso(),
        },
    );
    prune_sessions(&mut store.file.sessions);
    save(&store);
}

/// A closed session restored from Previous Sessions comes back on the model its conversation last
/// ran, which is remembered under the closed session's key.
pub(crate) fn carry_session_choice(from: (&str, &str), to: (&str, &str)) {
    let Ok(mut store) = store().lock() else {
        return;
    };
    let to_key = session_key(to.0, to.1);
    if store.file.sessions.contains_key(&to_key) {
        return;
    }
    let Some(reading) = store
        .file
        .sessions
        .get(&session_key(from.0, from.1))
        .cloned()
    else {
        return;
    };
    store.file.sessions.insert(to_key, reading);
    save(&store);
}

/// The label the agent itself shows for this model, so a launch value reads exactly like the
/// report that later confirms it.
pub(crate) fn remembered_model_label(family: &str, model: &str) -> Option<String> {
    remembered_label(family, |pin| {
        (pin.model == model)
            .then(|| pin.model_label.clone())
            .flatten()
    })
}

pub(crate) fn remembered_effort_label(family: &str, effort: &str) -> Option<String> {
    remembered_label(family, |pin| {
        (pin.effort.as_deref() == Some(effort))
            .then(|| pin.effort_label.clone())
            .flatten()
    })
}

fn remembered_label(family: &str, pick: impl Fn(&ModelPin) -> Option<String>) -> Option<String> {
    let family = pin_family(family)?;
    let store = store().lock().ok()?;
    store
        .file
        .defaults
        .get(family)
        .and_then(|default| pick(&default.pin))
        .or_else(|| {
            store
                .file
                .sessions
                .values()
                .filter(|reading| reading.family == family)
                .find_map(|reading| pick(&reading.pin))
        })
}

/// A chat pick saved as the agent's default.
pub(crate) fn record_default(family: &str, pin: ModelPin) {
    let Some(family) = pin_family(family) else {
        return;
    };
    let settings = settings_snapshot(family);
    let Ok(mut store) = store().lock() else {
        return;
    };
    let labels = store.file.defaults.get(family).map(|old| old.pin.clone());
    let pin = with_known_labels(pin, labels.as_ref());
    store.file.defaults.insert(
        family.to_string(),
        DefaultPin {
            pin,
            settings,
            recorded_at: crate::agents::now_iso(),
        },
    );
    save(&store);
}

/// Keep labels a pick does not carry when the choice itself is unchanged.
fn with_known_labels(mut pin: ModelPin, old: Option<&ModelPin>) -> ModelPin {
    if let Some(old) = old {
        if pin.model_label.is_none() && old.model == pin.model {
            pin.model_label = old.model_label.clone();
        }
        if pin.effort_label.is_none() && old.effort == pin.effort {
            pin.effort_label = old.effort_label.clone();
        }
    }
    pin
}

/// The `runtimeSettings` marker for a session started on `pin` (or on nothing, to learn).
pub(crate) fn session_marker(origin: &str, pin: Option<&ModelPin>) -> Value {
    let mut marker = Map::new();
    marker.insert("origin".to_string(), json!(origin));
    if let Some(pin) = pin {
        marker.insert("model".to_string(), json!(pin.model));
        if let Some(effort) = pin.effort.as_deref() {
            marker.insert("effort".to_string(), json!(effort));
        }
    }
    Value::Object(marker)
}

/// A live reading (the agent's statusline or screen) teaches the session's last model, and the
/// default when the session runs the agent's own choice. Cheap when nothing changed: the session
/// row is read only for a reading that differs from the remembered one.
pub(crate) fn observe_detection(
    paths: &crate::paths::GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    agent: Option<&str>,
    detected: &SessionChatTerminalDetection,
) {
    let Some(family) = agent.map(str::trim).and_then(pin_family) else {
        return;
    };
    if !detected.attempted {
        return;
    }
    let Some(pin) = detected
        .options
        .as_ref()
        .and_then(|options| live_pin(&options.selection.model, &options.selection.effort))
    else {
        return;
    };
    let key = session_key(project_id, session_id);
    // A model the CLI would refuse is never remembered (the 2026-11-05 WHY on `launchable_model`),
    // and it replaces what was: kept, the old model would come back on resume, and as Cursor's
    // default on every new session.
    if !launchable_model(family, &pin.model) {
        let (has_reading, has_default) = store().lock().ok().map_or((false, false), |store| {
            (
                store.file.sessions.contains_key(&key),
                store.file.defaults.contains_key(family),
            )
        });
        if !has_reading && !has_default {
            return;
        }
        let forget_default = has_default
            && teaches_default(
                family,
                session_pin_marker(paths, server_id, project_id, session_id).as_ref(),
                &pin,
            );
        let Ok(mut store) = store().lock() else {
            return;
        };
        store.file.sessions.remove(&key);
        if forget_default {
            store.file.defaults.remove(family);
        }
        save(&store);
        return;
    }
    let changed = store().lock().ok().is_some_and(|store| {
        store
            .file
            .sessions
            .get(&key)
            .is_none_or(|reading| reading.pin != pin)
    });
    if !changed {
        return;
    }
    let marker = session_pin_marker(paths, server_id, project_id, session_id);
    let teaches_default = teaches_default(family, marker.as_ref(), &pin);
    let settings = teaches_default.then(|| settings_snapshot(family));
    let Ok(mut store) = store().lock() else {
        return;
    };
    store.file.sessions.insert(
        key,
        SessionReading {
            family: family.to_string(),
            pin: pin.clone(),
            recorded_at: crate::agents::now_iso(),
        },
    );
    prune_sessions(&mut store.file.sessions);
    if let Some(settings) = settings {
        store.file.defaults.insert(
            family.to_string(),
            DefaultPin {
                pin,
                settings,
                recorded_at: crate::agents::now_iso(),
            },
        );
    }
    save(&store);
}

/// The session's `runtimeSettings` record of what it was started on.
fn session_pin_marker(
    paths: &crate::paths::GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Option<Value> {
    crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| {
            crate::domain::DomainRepository::new(&db, server_id)
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .and_then(|session| {
            session
                .pointer(&format!("/runtimeSettings/{SESSION_PIN_KEY}"))
                .cloned()
        })
}

/// Claude and Codex learn only from a session started without a pin; a pick in the chat records
/// its own default. Cursor has no session-only pick, so its last used model is the default:
/// any session whose report moved away from what it was started on teaches it.
fn teaches_default(family: &str, marker: Option<&Value>, pin: &ModelPin) -> bool {
    let origin = marker
        .and_then(|marker| marker.get("origin"))
        .and_then(Value::as_str);
    if origin == Some(ORIGIN_LEARN) {
        return true;
    }
    if family != "cursor" {
        return false;
    }
    let Some(marker) = marker else {
        return true;
    };
    let started = ModelPin {
        model: marker
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        effort: marker
            .get("effort")
            .and_then(Value::as_str)
            .map(str::to_string),
        ..ModelPin::default()
    };
    !started.same_choice(pin)
}

fn live_pin(
    model: &Option<SessionChatDetectedChoice>,
    effort: &Option<SessionChatDetectedChoice>,
) -> Option<ModelPin> {
    let live = |choice: &SessionChatDetectedChoice| {
        matches!(
            choice.source,
            SessionChatOptionEvidence::Terminal | SessionChatOptionEvidence::Statusline
        )
    };
    let model = model.as_ref().filter(|choice| live(choice))?;
    let effort = effort.as_ref().filter(|choice| live(choice));
    Some(ModelPin {
        model: model.value.clone(),
        effort: effort.map(|choice| choice.value.clone()),
        model_label: Some(model.label.clone()).filter(|label| !label.is_empty()),
        effort_label: effort
            .map(|choice| choice.label.clone())
            .filter(|label| !label.is_empty()),
    })
}

fn prune_sessions(sessions: &mut BTreeMap<String, SessionReading>) {
    while sessions.len() > MAX_SESSION_READINGS {
        let Some(oldest) = sessions
            .iter()
            .min_by(|left, right| left.1.recorded_at.cmp(&right.1.recorded_at))
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        sessions.remove(&oldest);
    }
}

/// The model-related values of the agent's own settings, compared only to notice a change made
/// outside Ghostex (never shown).
pub(crate) fn settings_snapshot(family: &str) -> Value {
    let home = crate::accounts::launch::home().ok();
    let dir = |variable: &str, folder: &str| {
        std::env::var_os(variable)
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(folder)))
    };
    match family {
        "claude" => {
            let settings = dir("CLAUDE_CONFIG_DIR", ".claude")
                .and_then(|dir| fs::read_to_string(dir.join("settings.json")).ok())
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .unwrap_or(Value::Null);
            json!({
                "model": settings.get("model"),
                "effortLevel": settings.get("effortLevel"),
            })
        }
        "codex" => {
            let config = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    home.as_ref()
                        .map(|home| ghostex_paths::agent_homes::codex_default_home(home))
                })
                .and_then(|dir| fs::read_to_string(dir.join("config.toml")).ok())
                .and_then(|text| text.parse::<toml_edit::DocumentMut>().ok());
            let text = |table: &toml_edit::Item, key: &str| {
                table
                    .get(key)
                    .and_then(|item| item.as_str())
                    .map(str::to_string)
            };
            let Some(config) = config else {
                return Value::Null;
            };
            let root = config.as_item();
            let profile = text(root, "profile");
            let profile_table = profile
                .as_deref()
                .and_then(|name| root.get("profiles").and_then(|profiles| profiles.get(name)));
            json!({
                "model": text(root, "model"),
                "effort": text(root, "model_reasoning_effort"),
                "profile": profile,
                "profileModel": profile_table.and_then(|table| text(table, "model")),
                "profileEffort": profile_table.and_then(|table| text(table, "model_reasoning_effort")),
            })
        }
        "cursor" => {
            let config = home
                .as_ref()
                .and_then(|home| fs::read_to_string(home.join(".cursor/cli-config.json")).ok())
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .unwrap_or(Value::Null);
            let model = config
                .pointer("/model/modelId")
                .cloned()
                .unwrap_or(Value::Null);
            let parameters = model
                .as_str()
                .and_then(|id| config.pointer(&format!("/modelParameters/{id}")))
                .cloned()
                .unwrap_or(Value::Null);
            json!({ "model": model, "parameters": parameters })
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(model: &str, effort: Option<&str>) -> ModelPin {
        ModelPin {
            model: model.to_string(),
            effort: effort.map(str::to_string),
            ..ModelPin::default()
        }
    }

    #[test]
    fn claude_and_codex_learn_only_from_sessions_started_without_a_pin() {
        let learn = session_marker(ORIGIN_LEARN, None);
        let default = session_marker(ORIGIN_DEFAULT, Some(&pin("opus[1m]", Some("high"))));
        let session = session_marker(ORIGIN_SESSION, Some(&pin("sonnet", Some("low"))));
        for family in ["claude", "codex"] {
            assert!(teaches_default(family, Some(&learn), &pin("opus", None)));
            assert!(!teaches_default(
                family,
                Some(&default),
                &pin("sonnet", None)
            ));
            assert!(!teaches_default(family, Some(&session), &pin("opus", None)));
            assert!(!teaches_default(family, None, &pin("opus", None)));
        }
    }

    #[test]
    fn cursor_learns_its_last_used_model() {
        let started = session_marker(ORIGIN_DEFAULT, Some(&pin("grok-4.7", Some("high"))));
        assert!(!teaches_default(
            "cursor",
            Some(&started),
            &pin("grok-4.7", Some("high"))
        ));
        assert!(teaches_default(
            "cursor",
            Some(&started),
            &pin("gpt-6", Some("high"))
        ));
        assert!(teaches_default(
            "cursor",
            Some(&started),
            &pin("grok-4.7", Some("low"))
        ));
        assert!(teaches_default("cursor", None, &pin("grok-4.7", None)));
    }

    #[test]
    fn a_cursor_model_name_is_never_launched_as_its_model() {
        for name in [
            "Auto Balance",
            "Grok 4.7  High Fast\u{200b}\u{200b}",
            "Auto",
            "",
        ] {
            assert!(!launchable_model("cursor", name), "{name:?}");
        }
        for id in [
            "auto",
            "grok-4.7",
            "composer-2.5",
            "claude-opus-4-8[context=1m,effort=high,fast=false]",
        ] {
            assert!(launchable_model("cursor", id), "{id}");
        }
        assert!(launchable_model("claude", "opus[1m]"));
        assert!(launchable_model("codex", "gpt-5.6-sol"));
    }

    /// GitHub #422's `agent-model-pins.json`: the Auto Balance name learned as Cursor's default
    /// and a session's reading, beside pins that launch.
    #[test]
    fn loading_drops_the_pins_the_cli_refuses() {
        let mut file: PinFile = serde_json::from_value(json!({
            "defaults": {
                "cursor": { "pin": { "model": "Auto Balance", "modelLabel": "Auto Balance" },
                            "settings": { "model": "auto-smart", "parameters": [{ "id": "optimize_for", "value": "balanced" }] } },
                "claude": { "pin": { "model": "opus[1m]", "effort": "high" } }
            },
            "sessions": {
                "P1:G1": { "family": "cursor", "pin": { "model": "Auto Balance", "modelLabel": "Auto Balance" } },
                "P1:G2": { "family": "cursor", "pin": { "model": "grok-4.7", "effort": "medium" } },
                "P1:G3": { "family": "codex", "pin": { "model": "gpt-5.6-sol", "effort": "high" } }
            }
        }))
        .expect("the reported file parses");
        assert!(drop_unlaunchable_pins(&mut file));
        assert_eq!(file.defaults.keys().collect::<Vec<_>>(), ["claude"]);
        assert_eq!(file.sessions.keys().collect::<Vec<_>>(), ["P1:G2", "P1:G3"]);
        assert!(!drop_unlaunchable_pins(&mut file));
    }

    /// The footer the reporter's Cursor drew: its reading keeps the name, which is never pinned.
    #[test]
    fn the_auto_balance_footer_makes_no_launchable_pin() {
        let selection = crate::session_chat_options::match_cursor_statusline(
            "  New Agent \u{b7} Auto Balance \u{b7} 0 used",
        )
        .expect("the footer reads");
        let pin = live_pin(&selection.model, &selection.effort).expect("a live reading");
        assert_eq!(pin.model, "Auto Balance");
        assert!(!launchable_model("cursor", &pin.model));
    }

    #[test]
    fn only_live_evidence_makes_a_reading() {
        let choice = |source| SessionChatDetectedChoice {
            value: "opus[1m]".to_string(),
            label: "Opus 5.5 (1M)".to_string(),
            source,
        };
        assert!(live_pin(&Some(choice(SessionChatOptionEvidence::Launch)), &None).is_none());
        assert!(live_pin(&Some(choice(SessionChatOptionEvidence::Transcript)), &None).is_none());
        let reading = live_pin(&Some(choice(SessionChatOptionEvidence::Statusline)), &None)
            .expect("statusline reading");
        assert_eq!(reading.model_label.as_deref(), Some("Opus 5.5 (1M)"));
    }
}
