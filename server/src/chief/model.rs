//! Which agent, model and effort Chief of Staff starts on: the user's default agent, on that
//! agent's best model at High, picked from the live model list the chat's model picker shows.
//!
//! CDXC:Chief 2026-11-21 DECISION:
//! User: "use the agent that the user picked as their default as the main one for chief of staff pls. if they pick claude then pick opus (whatever is latest version of it) at high effort. And if they pick codex then do Sol 6.2 with high effort. And if they pick antigravity ... then pick Neon (latest version of it) high". Chief of Staff no longer asks which agent on first open (supersedes Q1740): it runs on the default agent (Settings > Agents, chosen in onboarding), or the first agent that can run a coordinator when the default can't. "Latest" is read from the agent's live lineup (agent_model_catalog.rs), never hard-coded: the newest Opus, the newest GPT Sol, the newest Gemini Neon; other agents get their default model (the lineup's `default` row, else its first). A model or effort the user picks by hand in a Chief of Staff chat is kept for its next chats on that agent (`ChiefRecord.model`).
//! SEE-ALSO: server/src/chief/session.rs (`create_chief_session`), server/src/session_chat_codex_picker.rs (where a hand pick is remembered).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A model and effort Chief of Staff runs on, for one agent family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChiefModel {
    pub family: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Picked by the user in Chief of Staff's chat, not by Ghostex.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_hand: bool,
}

/// The words a model's family must have to be the one Chief of Staff prefers on `family`.
fn preferred_family_words(family: &str) -> Option<&'static [&'static str]> {
    match family {
        "claude" => Some(&["opus"]),
        "codex" => Some(&["gpt", "sol"]),
        "antigravity" => Some(&["gemini", "neon"]),
        _ => None,
    }
}

fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_legacy(row: &Value) -> bool {
    text(row, "group") == Some("legacy")
}

/// The newest row whose name has every word of `words` in its family (by
/// `family_and_version`); the first in the lineup's order among equals.
fn newest_of<'a>(rows: &'a [Value], words: &[&str]) -> Option<&'a Value> {
    let mut best: Option<(&Value, Vec<u32>)> = None;
    for row in rows {
        let Some(label) = text(row, "label").or_else(|| text(row, "value")) else {
            continue;
        };
        let Some((family, version)) =
            ghostex_gx_chat_core::menus::catalog::family_and_version(label)
        else {
            continue;
        };
        let family_words: Vec<&str> = family.split_whitespace().collect();
        if !words.iter().all(|word| family_words.contains(word)) {
            continue;
        }
        if best.as_ref().is_none_or(|(_, newest)| version > *newest) {
            best = Some((row, version));
        }
    }
    best.map(|(row, _)| row)
}

/// The model Chief of Staff starts on for `family`, from that agent's entry in the model catalog
/// (`agent_model_catalog::catalog_agent`), with High effort where the model offers it (else the
/// level a coordinator starts on). `None` when the catalog lists no model for it: the agent then
/// starts on its own default, as before.
pub(crate) fn pick_from(family: &str, entry: Option<&Value>) -> Option<ChiefModel> {
    let rows: Vec<Value> = entry
        .and_then(|entry| entry.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let current: Vec<Value> = rows.iter().filter(|row| !is_legacy(row)).cloned().collect();
    let row = preferred_family_words(family)
        .and_then(|words| newest_of(&current, words).or_else(|| newest_of(&rows, words)))
        .or_else(|| {
            current
                .iter()
                .find(|row| row.get("default").and_then(Value::as_bool) == Some(true))
        })
        .or_else(|| current.first());
    let model = match row.and_then(|row| text(row, "value")) {
        Some(model) => model.to_string(),
        // The catalog has nothing for Claude yet (no lineup read): the coordinator default,
        // today's Opus.
        None if family == "claude" => {
            crate::coordinators::DEFAULT_CLAUDE_COORDINATOR_MODEL.to_string()
        }
        None => return None,
    };
    let effort = ghostex_gx_chat_core::protocol::coordinator_launch_takes_effort(family)
        .then(|| {
            let levels: Vec<String> = row
                .and_then(|row| row.get("efforts"))
                .or_else(|| entry.and_then(|entry| entry.get("efforts")))
                .and_then(Value::as_array)
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if levels.is_empty() {
                return crate::coordinators::default_orchestrator_effort(family)
                    .map(str::to_string);
            }
            if levels.iter().any(|level| level == "high") {
                return Some("high".to_string());
            }
            ghostex_gx_chat_core::menus::orchestrator_effort::default_orchestrator_effort(&levels)
        })
        .flatten();
    Some(ChiefModel {
        family: family.to_string(),
        model,
        effort,
        by_hand: false,
    })
}

/// [`pick_from`] over the live catalog.
pub(crate) fn pick(family: &str) -> Option<ChiefModel> {
    pick_from(
        family,
        crate::agent_model_catalog::catalog_agent(family).as_ref(),
    )
}

/// The user's default agent (`defaultPromptAgentId`: the settings file, which onboarding and
/// Settings > Agents write, then gxserver's own copy) when it can run Chief of Staff, else the
/// first agent that can, in the coordinator families' order.
pub(crate) fn default_agent(
    paths: &crate::paths::GxserverPaths,
    db: &rusqlite::Connection,
    agent_rows: &[Value],
) -> Option<String> {
    let runnable = |id: &str| {
        agent_rows
            .iter()
            .any(|row| row.get("agentId").and_then(Value::as_str) == Some(id))
    };
    let from_file = crate::shared_settings_file::read_shared_settings(paths)
        .as_ref()
        .and_then(|settings| text(settings, "defaultPromptAgentId").map(str::to_string));
    let from_server = crate::agents::read_agent_settings(db)
        .ok()
        .and_then(|settings| {
            settings
                .get("defaultPromptAgentId")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    [from_file, from_server]
        .into_iter()
        .flatten()
        .find(|id| runnable(id))
        .or_else(|| crate::coordinators::default_coordinator_launcher(agent_rows))
}

/// What a new Chief of Staff chat on `family` starts on: the user's hand pick from an earlier chat
/// on the same agent family, else [`pick`].
pub(crate) fn for_new_chat(family: &str, earlier: Option<&ChiefModel>) -> Option<ChiefModel> {
    earlier
        .filter(|earlier| earlier.by_hand && earlier.family == family)
        .cloned()
        .or_else(|| pick(family))
}

/// A model or effort the user picked in a chat of session `project_id`/`session_id`: when that is
/// Chief of Staff's current chat, its record keeps the pick for the chats after it. An effort-only
/// pick keeps the model it was on.
pub(crate) fn remember_hand_pick(
    paths: &crate::paths::GxserverPaths,
    project_id: &str,
    session_id: &str,
    family: &str,
    model: &str,
    effort: &str,
) {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return;
    };
    let Some(mut record) = super::record::read_record(&db) else {
        return;
    };
    if record.project_id != project_id || record.session_id != session_id || family.is_empty() {
        return;
    }
    let earlier = record
        .model
        .take()
        .filter(|earlier| earlier.family == family);
    let model = Some(model.trim())
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .or_else(|| earlier.as_ref().map(|earlier| earlier.model.clone()));
    let Some(model) = model else {
        record.model = earlier;
        return;
    };
    let effort = Some(effort.trim())
        .filter(|effort| !effort.is_empty())
        .map(str::to_string)
        .or_else(|| earlier.and_then(|earlier| earlier.effort));
    record.model = Some(ChiefModel {
        family: family.to_string(),
        model,
        effort,
        by_hand: true,
    });
    let _ = super::record::write_record(&db, paths, &record);
}

/// Session `project_id`/`session_id` switched to agent `agent_id` before its first message was
/// sent (`/api/switchDraftAgent`): when it is Chief of Staff's current chat, its record follows,
/// so New Chat and the sidebar name the agent it runs on now.
pub(crate) fn follow_agent_switch(
    paths: &crate::paths::GxserverPaths,
    project_id: &str,
    session_id: &str,
    agent_id: &str,
) {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return;
    };
    let Some(mut record) = super::record::read_record(&db) else {
        return;
    };
    if record.project_id != project_id
        || record.session_id != session_id
        || record.agent_id == agent_id
        || agent_id.trim().is_empty()
    {
        return;
    }
    record.agent_id = agent_id.to_string();
    // The model Ghostex picked belongs to the old agent; a hand pick names its own agent family
    // and only applies to chats on that family (`for_new_chat`).
    if record.model.as_ref().is_some_and(|model| !model.by_hand) {
        record.model = None;
    }
    let _ = super::record::write_record(&db, paths, &record);
}
