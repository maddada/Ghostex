/*
CDXC:AgentProviders 2026-09-26 DECISION:
User: every Hermes chat names the bot it talks to (Harry, not Hermes Agent), the
default profile reads Hermes, and Context details offers the rows Hermes can
back: context used, cost, tokens, session time and model. Context use comes from
the Hermes status line (`match_hermes_statusline`); everything else here comes
from the session's own row in its profile's session store, found by session id
(`session_chat_hermes::hermes_state_db_path`).
*/

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{Connection, Row};
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::server::read_runtime_text;
use crate::session_chat_hermes::{
    hermes_home, hermes_state_db_path, is_safe_hermes_session_id, open_read_only_state_db,
};
use crate::session_chat_options::SessionChatDetectedSelection;

/// The Hermes session row for this Ghostex session, as a detection layer
/// carrying `hermesStatus`. `None` until the hooks record the session id.
pub fn read_hermes_status_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatDetectedSelection> {
    let session = repository.get_session(project_id, session_id).ok()??;
    let (agent_session_id, state_db_path, connection) = open_hermes_session_store(&session)?;
    let (status, session_effort) = read_hermes_session_status(&connection, &agent_session_id)?;
    let model_catalog = status.get("name").and_then(Value::as_str).map(|bot| {
        let mut catalog = cached_hermes_model_catalog(&connection, &state_db_path, bot);
        if let Some(effort) =
            session_effort.filter(|effort| HERMES_PICKER_EFFORTS.contains(&effort.as_str()))
        {
            catalog["agents"]["hermes"]["defaultEffort"] = json!(effort);
        }
        catalog
    });
    let (model, effort) = gateway_model_and_effort(&session);
    Some(SessionChatDetectedSelection {
        hermes_status: Some(status),
        model_catalog,
        model,
        effort,
        ..SessionChatDetectedSelection::default()
    })
}

/// CDXC:HermesGateway 2026-11-05 WHY:
/// The model pill read Hermes's status bar, and 0.22.5 redrew it with `·` between its fields where
/// the reader looks for `│`, so the pill fell back to plain "Model". The gateway's `session.info`
/// names the model and effort the next turn runs on (the status bar only catches up after a turn),
/// so the pill reads that whenever a gateway answers (hermes_gateway.rs).
fn gateway_model_and_effort(
    session: &Value,
) -> (
    Option<crate::session_chat_options::SessionChatDetectedChoice>,
    Option<crate::session_chat_options::SessionChatDetectedChoice>,
) {
    use crate::session_chat_options::{SessionChatDetectedChoice, SessionChatOptionEvidence};
    let Some(info) = crate::hermes_gateway::session_info(session) else {
        return (None, None);
    };
    let choice = |key: &str| {
        info[key]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| SessionChatDetectedChoice {
                value: value.to_string(),
                label: value.to_string(),
                source: SessionChatOptionEvidence::Terminal,
            })
    };
    (choice("model"), choice("reasoning_effort"))
}

/// The Hermes session id the hooks recorded on this Ghostex session, and its profile's session
/// store, opened read-only.
fn open_hermes_session_store(session: &Value) -> Option<(String, PathBuf, Connection)> {
    let agent_session_id = read_runtime_text(session, "agentSessionId")?;
    if !is_safe_hermes_session_id(&agent_session_id) {
        return None;
    }
    let state_db_path = hermes_state_db_path(&hermes_home(), &agent_session_id);
    let connection = open_read_only_state_db(&state_db_path)?;
    Some((agent_session_id, state_db_path, connection))
}

/// The row's `hermesStatus` and the reasoning effort the session started on.
fn read_hermes_session_status(
    connection: &Connection,
    session_id: &str,
) -> Option<(Value, Option<String>)> {
    // Named columns: the row also holds the whole system prompt, and this runs every detect tick.
    connection
        .query_row(
            "SELECT profile_name, model, started_at, ended_at, input_tokens, cache_read_tokens, \
             cache_write_tokens, output_tokens, actual_cost_usd, estimated_cost_usd, model_config \
             FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| Ok((hermes_status_value(row), session_reasoning_effort(row))),
        )
        .ok()
}

/// CDXC:AgentProviders 2026-09-26 WHY:
/// Hermes never shows its reasoning effort, so the picker's effort row starts on the level the
/// session started with (`model_config.reasoning_config.effort`, resolved per model when the
/// session starts), else on the profile's configured level. A later `/model --reasoning` does not
/// rewrite this record, so the chat's own record of a pick wins over it.
fn session_reasoning_effort(row: &Row<'_>) -> Option<String> {
    let config = row
        .get::<_, Option<String>>("model_config")
        .ok()
        .flatten()?;
    let config = serde_json::from_str::<Value>(&config).ok()?;
    config
        .pointer("/reasoning_config/effort")?
        .as_str()
        .map(str::to_string)
}

/// The row's values the chat can show, camelCase, each dropped when absent.
fn hermes_status_value(row: &Row<'_>) -> Value {
    let mut status = Map::new();
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            status.insert(key.to_string(), value);
        }
    };
    let number = |column: &str| {
        row.get::<_, Option<f64>>(column)
            .ok()
            .flatten()
            .filter(|value| value.is_finite())
            .map(|value| json!(value))
    };
    put(
        "name",
        bot_name(row.get("profile_name").ok().flatten()).map(Value::String),
    );
    put(
        "model",
        row.get::<_, Option<String>>("model")
            .ok()
            .flatten()
            .filter(|model| !model.trim().is_empty())
            .map(Value::String),
    );
    put("startedAt", number("started_at"));
    put("endedAt", number("ended_at"));
    put("inputTokens", number("input_tokens"));
    put("cacheReadTokens", number("cache_read_tokens"));
    put("cacheWriteTokens", number("cache_write_tokens"));
    put("outputTokens", number("output_tokens"));
    // A provider's billed figure beats Hermes' own estimate when it has one.
    put(
        "costUsd",
        number("actual_cost_usd").or_else(|| number("estimated_cost_usd")),
    );
    Value::Object(status)
}

/// `harry` reads Harry; the root profile, `default`, reads Hermes.
fn bot_name(profile: Option<String>) -> Option<String> {
    let profile = profile?;
    let profile = profile.trim();
    if profile == "default" {
        return Some("Hermes".to_string());
    }
    let mut characters = profile.chars();
    let first = characters.next()?;
    Some(first.to_uppercase().chain(characters).collect())
}

/// The reasoning levels the picker's effort row offers, each typed as `--reasoning <level>`.
pub(crate) const HERMES_PICKER_EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// A catalog document needs a date; this lineup is rebuilt from the profile on every read.
const HERMES_CATALOG_UPDATED_AT: &str = "2026-09-26";

/// How long a profile's lineup is reused. Detection reads it about once a second while a session
/// works, and it only changes when a session starts on another model or the config is edited.
const HERMES_CATALOG_TTL: Duration = Duration::from_secs(30);

static HERMES_CATALOGS: Mutex<Option<HashMap<PathBuf, (Instant, Value)>>> = Mutex::new(None);

/// [`hermes_model_catalog`], rebuilt at most every [`HERMES_CATALOG_TTL`] per session store.
fn cached_hermes_model_catalog(connection: &Connection, state_db_path: &Path, bot: &str) -> Value {
    let cached = HERMES_CATALOGS.lock().ok().and_then(|catalogs| {
        let (built_at, catalog) = catalogs.as_ref()?.get(state_db_path)?;
        (built_at.elapsed() < HERMES_CATALOG_TTL).then(|| catalog.clone())
    });
    if let Some(catalog) = cached {
        return catalog;
    }
    let catalog = hermes_model_catalog(connection, state_db_path, bot);
    if let Ok(mut catalogs) = HERMES_CATALOGS.lock() {
        catalogs.get_or_insert_with(HashMap::new).insert(
            state_db_path.to_path_buf(),
            (Instant::now(), catalog.clone()),
        );
    }
    catalog
}

/// CDXC:AgentProviders 2026-09-26 DECISION:
/// User: the Hermes model picker lists the profile's default model (`model.default` in the `config.yaml` beside its session store) plus the distinct models that profile's sessions used, ordered by use, with a low, medium and high effort row; a pick types Hermes' own `/model <name> --reasoning <level>`, which is session-only, and `--global` is never sent. Hermes' internal model caches are never read.
/// SEE-ALSO: server/src/session_chat_provider_model_picker.rs types the command, packages/gx-chat-core/src/menus/option_catalog.rs builds the picker from this document.
fn hermes_model_catalog(connection: &Connection, state_db_path: &Path, bot: &str) -> Value {
    let config = profile_config(state_db_path);
    let default_model = hermes_default_model(&config);
    let default_effort = hermes_config_value(&config, "agent", "reasoning_effort")
        .filter(|effort| HERMES_PICKER_EFFORTS.contains(&effort.as_str()));
    let mut models: Vec<Value> = default_model
        .iter()
        .map(|model| {
            json!({
                "value": model,
                "label": model,
                "default": true,
                "description": format!("{bot}'s default model"),
            })
        })
        .collect();
    for (model, sessions) in hermes_used_models(connection) {
        if default_model.as_deref() == Some(model.as_str()) {
            continue;
        }
        let plural = if sessions == 1 { "" } else { "s" };
        models.push(json!({
            "value": model,
            "label": model,
            "description": format!("Used in {sessions} {bot} session{plural}"),
        }));
    }
    json!({
        "schemaVersion": 1,
        "updatedAt": HERMES_CATALOG_UPDATED_AT,
        "effortLabels": {},
        "agents": {
            "hermes": {
                "name": bot,
                "efforts": HERMES_PICKER_EFFORTS,
                "defaultEffort": default_effort,
                "models": models,
            },
        },
    })
}

/// Every model this profile's sessions ran, most used first, then most recently used.
fn hermes_used_models(connection: &Connection) -> Vec<(String, i64)> {
    let Ok(mut statement) = connection.prepare(
        "SELECT TRIM(model), COUNT(*) FROM sessions WHERE TRIM(COALESCE(model, '')) <> '' \
         GROUP BY TRIM(model) ORDER BY COUNT(*) DESC, MAX(started_at) DESC",
    ) else {
        return Vec::new();
    };
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// The `config.yaml` beside a profile's session store, or nothing when it cannot be read.
fn profile_config(state_db_path: &Path) -> String {
    state_db_path
        .parent()
        .and_then(|profile| fs::read_to_string(profile.join("config.yaml")).ok())
        .unwrap_or_default()
}

/// The profile's default model: `model.default`, or `model: <name>`, which Hermes reads the same.
fn hermes_default_model(config: &str) -> Option<String> {
    hermes_config_value(config, "model", "default").or_else(|| {
        config
            .lines()
            .find_map(|line| line.strip_prefix("model:").and_then(yaml_scalar))
    })
}

/// One scalar from a Hermes `config.yaml`: `key` directly under the top-level `section` mapping.
///
/// CDXC:AgentProviders 2026-09-26 WHY:
/// A line reader for three known keys rather than a YAML parser: gxserver carries no YAML crate
/// and needs nothing else from this file.
fn hermes_config_value(config: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    let mut child_indent = None;
    for line in config.lines() {
        let content = line.trim_end();
        let text = content.trim_start();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let indent = content.len() - text.len();
        if indent == 0 {
            in_section = text
                .strip_prefix(section)
                .and_then(|rest| rest.strip_prefix(':'))
                .is_some_and(|rest| yaml_scalar(rest).is_none());
            child_indent = None;
            continue;
        }
        if !in_section || *child_indent.get_or_insert(indent) != indent {
            continue;
        }
        if let Some(rest) = text
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(':'))
        {
            return yaml_scalar(rest);
        }
    }
    None
}

/// A plain or quoted YAML scalar with any trailing comment removed; `None` for an empty value or
/// anything that opens a block, list or map.
fn yaml_scalar(raw: &str) -> Option<String> {
    let value = raw.split(" #").next().unwrap_or_default().trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim();
    (!value.is_empty() && !value.starts_with(['|', '>', '{', '[', '&', '*']))
        .then(|| value.to_string())
}

/// What Hermes' status bar prints for a model id (`_get_status_bar_snapshot`): the part after the
/// last slash, cut to 23 characters and `...` past 26. Config model aliases are not mirrored, so
/// an aliased model never matches its id.
fn hermes_status_bar_model(model: &str) -> String {
    let short = model.rsplit('/').next().unwrap_or(model);
    let short = short.strip_suffix(".gguf").unwrap_or(short);
    if short.chars().count() > 26 {
        format!("{}...", short.chars().take(23).collect::<String>())
    } else {
        short.to_string()
    }
}

/// A model id without the variant tag Hermes shows when it resolved a picked id to the one declared
/// id that extends it (`claude-opus-5-5` runs as `claude-opus-5-5[1m]`, verified live 2026-09-26):
/// one trailing bracketed alphanumeric tag.
pub(crate) fn hermes_untagged_model(shown: &str) -> &str {
    shown
        .strip_suffix(']')
        .and_then(|rest| rest.rsplit_once('['))
        .filter(|(base, tag)| {
            !base.is_empty() && !tag.is_empty() && tag.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        .map_or(shown, |(base, _)| base)
}

/// Whether the status bar's model is `model`, tagged or not. Only the tag is ignored, so
/// `gpt-5.6-sol` never matches a status bar still showing `gpt-5.6-sol-900k`.
pub(crate) fn hermes_status_bar_shows(shown: &str, model: &str) -> bool {
    let short = hermes_status_bar_model(model);
    [shown, hermes_untagged_model(shown)]
        .into_iter()
        .any(|name| name == model || name == short)
}

/// CDXC:AgentProviders 2026-09-26 WHY:
/// The status bar shortens `deepseek/deepseek-v4-pro` to `deepseek-v4-pro`, a name neither the
/// picker's rows nor `/model` know. When the screen's model is the session row's model shortened,
/// the chat carries the row's full id; the screen stays the authority when they name different
/// models.
pub fn restore_hermes_model_id(selection: &mut SessionChatDetectedSelection) {
    let Some(full) = selection
        .hermes_status
        .as_ref()
        .and_then(|status| status.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    if let Some(model) = selection.model.as_mut() {
        if model.value != full && model.value == hermes_status_bar_model(&full) {
            model.value = full.clone();
            model.label = full;
        }
    }
}

/// The `--provider` a Hermes `/model` pick needs, or `None` when the model runs on the session's
/// own provider or its provider is unknown.
///
/// CDXC:AgentProviders 2026-09-26 WHY:
/// Hermes resolves a bare `/model <name>` against the session's current provider and refuses a
/// model from another one ("switch with `--provider <slug>`", verified live 2026-09-26), so a row
/// this profile ran elsewhere (Harry's `claude-opus-5-5`) carries that provider: `model.provider`
/// for the config default, else the provider its sessions billed most. The session's own provider
/// is the `provider` Hermes writes into `model_config` on every switch; `--provider` without
/// `--global` is session-only too.
pub(crate) fn hermes_switch_provider(session: &Value, model: &str) -> Option<String> {
    let (agent_session_id, state_db_path, connection) = open_hermes_session_store(session)?;
    let current = connection
        .query_row(
            "SELECT model_config, billing_provider FROM sessions WHERE id = ?1",
            rusqlite::params![agent_session_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .ok()
        .and_then(|(config, billing)| {
            config
                .and_then(|config| serde_json::from_str::<Value>(&config).ok())
                .and_then(|config| config.get("provider")?.as_str().map(str::to_string))
                .or(billing)
        });
    let config = profile_config(&state_db_path);
    let default_provider = if hermes_default_model(&config).as_deref() == Some(model) {
        hermes_config_value(&config, "model", "provider")
    } else {
        None
    };
    let provider = default_provider.or_else(|| {
        connection
            .query_row(
                "SELECT TRIM(billing_provider) FROM sessions WHERE TRIM(model) = ?1 \
                     AND TRIM(COALESCE(billing_provider, '')) <> '' \
                     GROUP BY TRIM(billing_provider) ORDER BY COUNT(*) DESC LIMIT 1",
                rusqlite::params![model],
                |row| row.get::<_, String>(0),
            )
            .ok()
    })?;
    (current.as_deref().map(str::trim) != Some(provider.as_str())).then_some(provider)
}
