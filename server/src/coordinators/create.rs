//! Creating a coordinator: `/api/createAgentSession` with a `coordinator` object.

use ghostex_gx_chat_core::protocol::{
    apply_coordinator_start_settings, coordinator_start_effort, coordinator_start_from_settings,
    coordinator_start_model, COORDINATOR_AGENT_DEFAULT, COORDINATOR_EFFORT_SETTING,
    COORDINATOR_MODEL_SETTING,
};
use rusqlite::Connection;
use serde_json::{Map, Value};

use super::records::{
    insert_coordinator, COORDINATOR_GOAL_MAX_CHARS, COORDINATOR_INSTRUCTIONS_MAX_CHARS,
};
use crate::domain::DomainStateError;

/// The `coordinator` object of a create request: `{ goal?, instructions?, firstRequest? }`.
/// `firstRequest` follows the skill line in the orchestrator's first message
/// (`orchestrator_first_message`); a client that queues its first request itself instead (the
/// phone before 2026-10-17) gets it as the second message.
pub struct CoordinatorCreateRequest {
    pub goal: String,
    pub instructions: String,
    pub first_request: String,
}

pub fn coordinator_create_request(
    params: &Map<String, Value>,
) -> Result<Option<CoordinatorCreateRequest>, DomainStateError> {
    let Some(object) = params.get("coordinator") else {
        return Ok(None);
    };
    if object.is_null() {
        return Ok(None);
    }
    let object = object.as_object().ok_or_else(|| {
        DomainStateError::bad_request("coordinator must be an object with an optional goal.")
    })?;
    let field = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let goal = field("goal");
    let instructions = field("instructions");
    if goal.chars().count() > COORDINATOR_GOAL_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Keep the goal under {COORDINATOR_GOAL_MAX_CHARS} characters; put details in the instructions."
        )));
    }
    if instructions.chars().count() > COORDINATOR_INSTRUCTIONS_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Standing instructions are limited to {COORDINATOR_INSTRUCTIONS_MAX_CHARS} characters."
        )));
    }
    Ok(Some(CoordinatorCreateRequest {
        goal,
        instructions,
        first_request: field("firstRequest"),
    }))
}

/// Stores the coordinator row for a session `/api/createAgentSession` just created.
pub fn register_created_coordinator(
    db: &Connection,
    params: &Map<String, Value>,
    session: &Value,
) -> Result<bool, DomainStateError> {
    let Some(request) = coordinator_create_request(params)? else {
        return Ok(false);
    };
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    insert_coordinator(
        db,
        &text("projectId"),
        &text("sessionId"),
        &request.goal,
        &request.instructions,
    )?;
    super::playbook::mark_new_coordinator(db, &(text("projectId"), text("sessionId")))?;
    Ok(true)
}

fn row_text<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

/// The orchestrator family of a launcher row from `/api/readSidebarHud`
/// (`COORDINATOR_AGENT_FAMILIES`): by its icon, its agent id, or the executable its command runs.
/// SEE-ALSO: packages/gx-core/src/session_create/agents.rs (`coordinator_launcher_family`, the clients' copy a handoff reads).
pub fn coordinator_launcher_family(row: &Value) -> Option<&'static str> {
    let by_icon =
        crate::agents::default_agent_icon_to_id(row_text(row, "icon")).map(str::to_string);
    let by_id = Some(row_text(row, "agentId").to_string());
    let by_command =
        crate::agents::infer_agent_id_from_command_executable(row_text(row, "command"));
    [by_icon, by_id, by_command]
        .into_iter()
        .flatten()
        .find_map(|family| {
            super::COORDINATOR_AGENT_FAMILIES
                .into_iter()
                .find(|supported| *supported == family)
        })
}

/// The first configured launcher a coordinator can run on, in `COORDINATOR_AGENT_FAMILIES` order
/// (Claude first).
pub fn default_coordinator_launcher(rows: &[Value]) -> Option<String> {
    super::COORDINATOR_AGENT_FAMILIES.iter().find_map(|family| {
        rows.iter()
            .find(|row| coordinator_launcher_family(row) == Some(*family))
            .map(|row| row_text(row, "agentId").to_string())
    })
}

/// `{ agentCommand, icon }` of a launcher from `/api/readSidebarHud`, the way the desktop sends
/// it: custom agents are listed per machine, so a project that does not list one still launches it.
pub fn launcher_launch_settings(rows: &[Value], agent_id: &str) -> Value {
    let mut settings = Map::new();
    if let Some(row) = rows.iter().find(|row| row_text(row, "agentId") == agent_id) {
        for (from, to) in [("command", "agentCommand"), ("icon", "icon")] {
            let value = row_text(row, from);
            if !value.is_empty() {
                settings.insert(to.to_string(), Value::String(value.to_string()));
            }
        }
    }
    Value::Object(settings)
}

/// A new coordinator as `ghostex coordinator create` and the voice agent's `start_coordinator`
/// ask for it.
pub struct CoordinatorLaunch<'a> {
    /// The launchers of `/api/readSidebarHud` (plus a hidden one the caller allowed).
    pub agent_rows: &'a [Value],
    pub project_id: &'a str,
    pub agent_id: &'a str,
    /// `None`: the placeholder "Coordinator" the agent's first name may replace.
    pub title: Option<String>,
    pub goal: String,
    /// Follows the skill line in its first message.
    pub task: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The shared settings, for the Coordinator model and Coordinator effort a create without
    /// `model`/`effort` starts on (`Value::Null` reads as their defaults).
    pub settings: &'a Value,
}

/// The `/api/createAgentSession` params of a new coordinator, with the defaults `create` applies:
/// the Coordinator model and Coordinator effort settings (Opus 5.5 at High unless changed).
pub fn coordinator_create_params(launch: CoordinatorLaunch<'_>) -> Value {
    let family = launch
        .agent_rows
        .iter()
        .find(|row| row_text(row, "agentId") == launch.agent_id)
        .and_then(coordinator_launcher_family);
    // CDXC:Coordinators 2026-10-03 SEE-ALSO: keeps_its_given_title in server/src/coordinators/title.rs; an unnamed coordinator's "Coordinator" is a placeholder the agent's first name may replace.
    let title_source = if launch.title.is_some() {
        "user"
    } else {
        "placeholder"
    };
    let mut params = serde_json::json!({
        "projectId": launch.project_id,
        "agentId": launch.agent_id,
        "launchSettings": launcher_launch_settings(launch.agent_rows, launch.agent_id),
        "title": launch.title.unwrap_or_else(|| "Coordinator".to_string()),
        "runtimeSettings": { "titleSource": title_source },
        "coordinator": { "goal": launch.goal, "firstRequest": launch.task },
    });
    // CDXC:Coordinators 2026-11-17 SEE-ALSO: `coordinator_start_model` in packages/gx-protocol/src/coordinator.rs (the user's decision: Opus 5.5 at High, or the agent's own default). ZCode takes no effort choice; its model rides agentModel and gxserver queues it as a `/model` line ahead of the first message.
    let chosen_model = launch.model.is_some();
    let chosen_effort = launch.effort.is_some();
    if let Some(model) = launch.model {
        params["agentModel"] = Value::String(model);
    }
    if let Some(effort) = launch.effort {
        params["agentEffort"] = Value::String(effort);
    }
    if let (Some(family), Some(object)) = (family, params.as_object_mut()) {
        apply_coordinator_start_settings(
            object,
            family,
            launch.settings,
            chosen_model,
            chosen_effort,
        );
    }
    params
}

/// `/api/createAgentSession` for a coordinator: the model and effort the sender took from its own
/// Coordinator settings are taken again from this computer's (`settings`), so a coordinator started
/// here from another computer or the CLI starts on the settings of the computer it runs on. Values
/// the user chose are kept. `family` is the agent's family as this computer launches it (its
/// project or global agent config, so a custom launcher another computer names only by id
/// resolves too); without one the create keeps what it was sent.
pub(crate) fn apply_destination_coordinator_start(
    params: &mut Map<String, Value>,
    settings: &Value,
    family: Option<&str>,
) {
    let (from_model, from_effort) = coordinator_start_from_settings(params);
    if !from_model && !from_effort {
        return;
    }
    if let Some(family) = family {
        apply_coordinator_start_settings(params, family, settings, !from_model, !from_effort);
    }
}

/// `/api/readCoordinatorStart`: the Coordinator model and Coordinator effort settings as Settings
/// shows them (`opus`/`agent`; `agent`/`low`/`medium`/`high`/`xhigh`), for the phone's Settings ›
/// Chat, which changes them through `updateSettingsPatch` like any settings write
/// (apps/mobile/app/src/screens/settings/CoordinatorStartRows.tsx).
pub(crate) fn read_start_endpoint_value(paths: &crate::paths::GxserverPaths) -> Value {
    let settings = crate::shared_settings_file::read_shared_settings(paths).unwrap_or(Value::Null);
    let setting = |key: &str| settings.get(key).and_then(Value::as_str);
    let model = match coordinator_start_model("claude", setting(COORDINATOR_MODEL_SETTING)) {
        Some(_) => "opus",
        None => COORDINATOR_AGENT_DEFAULT,
    };
    let effort = coordinator_start_effort("claude", setting(COORDINATOR_EFFORT_SETTING))
        .unwrap_or(COORDINATOR_AGENT_DEFAULT);
    serde_json::json!({ "model": model, "effort": effort })
}
