use serde_json::{json, Map, Value};

use crate::domain::{
    bool_field, insert_optional_string, insert_optional_trimmed_string, insert_optional_value,
    insert_parsed_optional_object, normalize_domain_lifecycle_state,
    normalize_optional_session_tag, normalize_session_kind, normalize_settled_override,
    normalize_zmx_provider_state, optional_string, parse_object, parse_object_array,
    parse_object_map, parse_string_array, required_string, resolve_surface,
    stringify_domain_json_field, DomainResult, DomainStateError,
};
use crate::ids::{create_global_session_ref, create_zmx_session_name};

#[derive(Debug)]
pub(crate) struct ProjectRow {
    attention_rules_json: String,
    completion_rules_json: String,
    created_at: String,
    custom_agent_order_json: String,
    custom_agents_json: String,
    custom_command_order_json: String,
    custom_commands_json: String,
    default_command: Option<String>,
    deleted_default_command_ids_json: String,
    git_config_json: String,
    identity_icon_json: String,
    is_favorite: i64,
    is_pinned: i64,
    is_recent_project: i64,
    launch_settings_json: String,
    name: String,
    notification_rules_json: String,
    path: Option<String>,
    previous_session_history_json: String,
    project_board_config_json: String,
    pub(crate) project_id: String,
    recent_closed_at: Option<String>,
    runtime_settings_json: String,
    system_kind: Option<String>,
    updated_at: String,
    visibility: String,
    worktree_json: String,
}

pub(crate) fn project_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        project_id: row.get("projectId")?,
        name: row.get("name")?,
        path: row.get("path")?,
        identity_icon_json: row.get("identityIconJson")?,
        is_pinned: row.get("isPinned")?,
        is_favorite: row.get("isFavorite")?,
        is_recent_project: row.get("isRecentProject")?,
        recent_closed_at: row.get("recentClosedAt")?,
        visibility: row.get("visibility")?,
        system_kind: row.get("systemKind")?,
        default_command: row.get("defaultCommand")?,
        worktree_json: row.get("worktreeJson")?,
        custom_agents_json: row.get("customAgentsJson")?,
        custom_agent_order_json: row.get("customAgentOrderJson")?,
        custom_commands_json: row.get("customCommandsJson")?,
        custom_command_order_json: row.get("customCommandOrderJson")?,
        deleted_default_command_ids_json: row.get("deletedDefaultCommandIdsJson")?,
        launch_settings_json: row.get("launchSettingsJson")?,
        runtime_settings_json: row.get("runtimeSettingsJson")?,
        completion_rules_json: row.get("completionRulesJson")?,
        attention_rules_json: row.get("attentionRulesJson")?,
        notification_rules_json: row.get("notificationRulesJson")?,
        git_config_json: row.get("gitConfigJson")?,
        project_board_config_json: row.get("projectBoardConfigJson")?,
        previous_session_history_json: row.get("previousSessionHistoryJson")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

pub(crate) fn project_from_row(row: ProjectRow) -> DomainResult<Value> {
    let mut project = Map::new();
    project.insert(
        "attentionRules".to_string(),
        parse_object(
            &row.attention_rules_json,
            "attentionRulesJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "completionRules".to_string(),
        parse_object(
            &row.completion_rules_json,
            "completionRulesJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert("createdAt".to_string(), Value::String(row.created_at));
    project.insert(
        "customAgentOrder".to_string(),
        parse_string_array(
            &row.custom_agent_order_json,
            "customAgentOrderJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "customAgents".to_string(),
        parse_object_array(
            &row.custom_agents_json,
            "customAgentsJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "customCommandOrder".to_string(),
        parse_string_array(
            &row.custom_command_order_json,
            "customCommandOrderJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "customCommands".to_string(),
        parse_object_array(
            &row.custom_commands_json,
            "customCommandsJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_optional_string(&mut project, "defaultCommand", row.default_command);
    project.insert(
        "deletedDefaultCommandIds".to_string(),
        parse_string_array(
            &row.deleted_default_command_ids_json,
            "deletedDefaultCommandIdsJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "gitConfig".to_string(),
        parse_object(
            &row.git_config_json,
            "gitConfigJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_parsed_optional_object(
        &mut project,
        "identityIcon",
        &row.identity_icon_json,
        "identityIconJson",
        "project",
        &row.project_id,
    )?;
    project.insert("isFavorite".to_string(), Value::Bool(row.is_favorite == 1));
    project.insert("isPinned".to_string(), Value::Bool(row.is_pinned == 1));
    project.insert(
        "isRecentProject".to_string(),
        Value::Bool(row.is_recent_project == 1),
    );
    project.insert(
        "launchSettings".to_string(),
        parse_object(
            &row.launch_settings_json,
            "launchSettingsJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert("name".to_string(), Value::String(row.name));
    project.insert(
        "notificationRules".to_string(),
        parse_object(
            &row.notification_rules_json,
            "notificationRulesJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_optional_string(&mut project, "path", row.path);
    project.insert(
        "previousSessionHistory".to_string(),
        parse_object_array(
            &row.previous_session_history_json,
            "previousSessionHistoryJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "projectBoardConfig".to_string(),
        parse_object(
            &row.project_board_config_json,
            "projectBoardConfigJson",
            "project",
            &row.project_id,
        )?,
    );
    project.insert(
        "projectId".to_string(),
        Value::String(row.project_id.clone()),
    );
    insert_optional_string(&mut project, "recentClosedAt", row.recent_closed_at);
    project.insert(
        "runtimeSettings".to_string(),
        parse_object(
            &row.runtime_settings_json,
            "runtimeSettingsJson",
            "project",
            &row.project_id,
        )?,
    );
    insert_optional_string(&mut project, "systemKind", row.system_kind);
    project.insert("updatedAt".to_string(), Value::String(row.updated_at));
    project.insert("visibility".to_string(), Value::String(row.visibility));
    insert_parsed_optional_object(
        &mut project,
        "worktree",
        &row.worktree_json,
        "worktreeJson",
        "project",
        &row.project_id,
    )?;
    Ok(Value::Object(project))
}

pub(crate) fn recent_project_from_project(
    project: &Value,
    session_count: usize,
) -> DomainResult<Value> {
    let object = project.as_object().ok_or_else(|| {
        DomainStateError::corrupt_state("Project row did not decode as an object.")
    })?;
    let project_id = required_string(object, "projectId")?;
    let title = required_string(object, "name")?;
    let path = optional_string(object, "path")
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| {
            DomainStateError::corrupt_state(format!(
                "Recent project {project_id} did not have a path."
            ))
        })?;
    let mut recent_project = Map::new();
    recent_project.insert("path".to_string(), Value::String(path));
    recent_project.insert("projectId".to_string(), Value::String(project_id));
    recent_project.insert(
        "sessionCount".to_string(),
        Value::Number(serde_json::Number::from(session_count)),
    );
    recent_project.insert("title".to_string(), Value::String(title));
    insert_optional_string(
        &mut recent_project,
        "recentClosedAt",
        optional_string(object, "recentClosedAt"),
    );
    if let Some(identity_icon) = object.get("identityIcon").and_then(Value::as_object) {
        insert_optional_value(
            &mut recent_project,
            "icon",
            identity_icon.get("icon").cloned(),
        );
        insert_optional_string(
            &mut recent_project,
            "iconDataUrl",
            identity_icon
                .get("iconDataUrl")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
        insert_optional_string(
            &mut recent_project,
            "theme",
            identity_icon
                .get("theme")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
        insert_optional_string(
            &mut recent_project,
            "themeColor",
            identity_icon
                .get("themeColor")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
    }
    Ok(Value::Object(recent_project))
}

#[derive(Debug)]
pub(crate) struct SessionRow {
    agent_id: Option<String>,
    attention_rules_json: String,
    command_id: Option<String>,
    completion_rules_json: String,
    created_at: String,
    cwd: Option<String>,
    is_favorite: i64,
    is_pinned: i64,
    kind: String,
    last_active_at: Option<String>,
    launch_settings_json: String,
    lifecycle_state: String,
    notification_rules_json: String,
    project_id: String,
    provider_state_json: String,
    restored_from_history_id: Option<String>,
    restored_from_session_id: Option<String>,
    runtime_settings_json: String,
    session_id: String,
    session_tag: Option<String>,
    settled_at: Option<String>,
    settled_override: Option<String>,
    settled_override_at: Option<String>,
    sidebar_order: Option<f64>,
    snoozed_at: Option<String>,
    snoozed_until: Option<String>,
    title: String,
    updated_at: String,
    worktree_json: String,
    zmx_name: String,
}

pub(crate) fn session_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        project_id: row.get("projectId")?,
        session_id: row.get("sessionId")?,
        kind: row.get("kind")?,
        title: row.get("title")?,
        lifecycle_state: row.get("lifecycleState")?,
        provider_state_json: row.get("providerStateJson")?,
        zmx_name: row.get("zmxName")?,
        cwd: row.get("cwd")?,
        agent_id: row.get("agentId")?,
        command_id: row.get("commandId")?,
        is_pinned: row.get("isPinned")?,
        is_favorite: row.get("isFavorite")?,
        restored_from_session_id: row.get("restoredFromSessionId")?,
        restored_from_history_id: row.get("restoredFromHistoryId")?,
        launch_settings_json: row.get("launchSettingsJson")?,
        runtime_settings_json: row.get("runtimeSettingsJson")?,
        completion_rules_json: row.get("completionRulesJson")?,
        attention_rules_json: row.get("attentionRulesJson")?,
        notification_rules_json: row.get("notificationRulesJson")?,
        worktree_json: row.get("worktreeJson")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
        last_active_at: row.get("lastActiveAt")?,
        sidebar_order: row.get("sidebarOrder")?,
        session_tag: row.get("sessionTag")?,
        settled_at: row.get("settledAt")?,
        settled_override: row.get("settledOverride")?,
        settled_override_at: row.get("settledOverrideAt")?,
        snoozed_at: row.get("snoozedAt")?,
        snoozed_until: row.get("snoozedUntil")?,
    })
}

pub(crate) fn session_from_row(server_id: &str, row: SessionRow) -> DomainResult<Value> {
    let row_id = format!("{}/{}", row.project_id, row.session_id);
    let zmx_name = create_zmx_session_name(server_id, &row.project_id, &row.session_id);
    let mut provider_state = parse_object_map(
        &row.provider_state_json,
        "providerStateJson",
        "session",
        &row_id,
    )?;
    provider_state = normalize_zmx_provider_state(provider_state, &zmx_name);
    let launch_settings = parse_object_map(
        &row.launch_settings_json,
        "launchSettingsJson",
        "session",
        &row_id,
    )?;
    let runtime_settings = parse_object_map(
        &row.runtime_settings_json,
        "runtimeSettingsJson",
        "session",
        &row_id,
    )?;
    let worktree = parse_object_map(&row.worktree_json, "worktreeJson", "session", &row_id)?;
    /*
    CDXC:SessionTags 2026-06-22-05:58:
    Stored sessionTag values are durable gxserver metadata, so Rust row hydration must reject the same invalid non-empty values as TypeScript instead of silently hiding corrupt or retired tags from clients.
    Legacy rows that only have isFavorite still hydrate as the Favorite tag so old state.db files keep the expanded tag model after migration.
    */
    let tag = match row.session_tag.as_deref() {
        Some(value) => normalize_optional_session_tag(Some(&Value::String(value.to_string())))?,
        None if row.is_favorite == 1 => Some("favorite".to_string()),
        None => None,
    };
    let mut session = Map::new();
    insert_optional_string(&mut session, "agentId", row.agent_id);
    session.insert(
        "attentionRules".to_string(),
        parse_object(
            &row.attention_rules_json,
            "attentionRulesJson",
            "session",
            &row_id,
        )?,
    );
    insert_optional_string(&mut session, "commandId", row.command_id);
    session.insert(
        "completionRules".to_string(),
        parse_object(
            &row.completion_rules_json,
            "completionRulesJson",
            "session",
            &row_id,
        )?,
    );
    session.insert("createdAt".to_string(), Value::String(row.created_at));
    insert_optional_string(&mut session, "cwd", row.cwd);
    session.insert(
        "globalRef".to_string(),
        Value::String(create_global_session_ref(
            server_id,
            &row.project_id,
            &row.session_id,
        )),
    );
    let mut hidden = Map::new();
    insert_optional_string(
        &mut hidden,
        "restoredFromHistoryId",
        row.restored_from_history_id,
    );
    insert_optional_string(
        &mut hidden,
        "restoredFromSessionId",
        row.restored_from_session_id,
    );
    session.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    session.insert(
        "isFavorite".to_string(),
        Value::Bool(tag.as_deref() == Some("favorite") || row.is_favorite == 1),
    );
    session.insert("isPinned".to_string(), Value::Bool(row.is_pinned == 1));
    session.insert(
        "kind".to_string(),
        Value::String(normalize_session_kind(Some(&Value::String(row.kind)))),
    );
    insert_optional_string(&mut session, "lastActiveAt", row.last_active_at);
    session.insert(
        "launchSettings".to_string(),
        Value::Object(launch_settings.clone()),
    );
    session.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_domain_lifecycle_state(Some(&Value::String(
            row.lifecycle_state,
        )))),
    );
    session.insert(
        "notificationRules".to_string(),
        parse_object(
            &row.notification_rules_json,
            "notificationRulesJson",
            "session",
            &row_id,
        )?,
    );
    session.insert(
        "projectId".to_string(),
        Value::String(row.project_id.clone()),
    );
    session.insert("providerState".to_string(), Value::Object(provider_state));
    session.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings.clone()),
    );
    session.insert("sessionId".to_string(), Value::String(row.session_id));
    if let Some(tag) = tag {
        session.insert("sessionTag".to_string(), Value::String(tag));
    }
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
    Settle/snooze columns are absent (NULL) on every state.db written before
    migration 0016, and the whole lifecycle is "no state" in that case. Hydrate
    them as omitted keys rather than explicit nulls so presentation, the CLI
    contract, and the client predicates all see the same "never settled, never
    snoozed" shape old rows already produce.
    */
    insert_optional_trimmed_string(&mut session, "settledAt", row.settled_at);
    insert_optional_string(
        &mut session,
        "settledOverride",
        normalize_settled_override(row.settled_override.as_deref()),
    );
    insert_optional_trimmed_string(&mut session, "settledOverrideAt", row.settled_override_at);
    if let Some(order) = row.sidebar_order.filter(|value| value.is_finite()) {
        session.insert("sidebarOrder".to_string(), json!(order));
    }
    insert_optional_trimmed_string(&mut session, "snoozedAt", row.snoozed_at);
    insert_optional_trimmed_string(&mut session, "snoozedUntil", row.snoozed_until);
    session.insert(
        "surface".to_string(),
        Value::String(resolve_surface(None, &launch_settings, &runtime_settings)),
    );
    session.insert("title".to_string(), Value::String(row.title));
    session.insert("updatedAt".to_string(), Value::String(row.updated_at));
    if !worktree.is_empty() {
        session.insert("worktree".to_string(), Value::Object(worktree));
    }
    session.insert("zmxName".to_string(), Value::String(zmx_name));
    let _ = row.zmx_name;
    Ok(Value::Object(session))
}

pub(crate) fn project_insert_params(
    project: &Value,
) -> DomainResult<rusqlite::ParamsFromIter<Vec<rusqlite::types::Value>>> {
    let object = project
        .as_object()
        .ok_or_else(|| DomainStateError::bad_request("Project must be an object."))?;
    let values = vec![
        sql_text(required_string(object, "projectId")?),
        sql_text(required_string(object, "name")?),
        sql_optional_text(optional_string(object, "path")),
        sql_text(stringify_domain_json_field(
            "identityIcon",
            object.get("identityIcon").unwrap_or(&json!({})),
        )?),
        sql_i64(bool_field(object, "isPinned") as i64),
        sql_i64(bool_field(object, "isFavorite") as i64),
        sql_i64(bool_field(object, "isRecentProject") as i64),
        sql_optional_text(optional_string(object, "recentClosedAt")),
        sql_optional_text(optional_string(object, "defaultCommand")),
        sql_text(stringify_domain_json_field(
            "worktree",
            object.get("worktree").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "customAgents",
            object.get("customAgents").unwrap_or(&json!([])),
        )?),
        sql_text(
            serde_json::to_string(object.get("customAgentOrder").unwrap_or(&json!([]))).unwrap(),
        ),
        sql_text(stringify_domain_json_field(
            "customCommands",
            object.get("customCommands").unwrap_or(&json!([])),
        )?),
        sql_text(
            serde_json::to_string(object.get("customCommandOrder").unwrap_or(&json!([]))).unwrap(),
        ),
        sql_text(
            serde_json::to_string(object.get("deletedDefaultCommandIds").unwrap_or(&json!([])))
                .unwrap(),
        ),
        sql_text(stringify_domain_json_field(
            "launchSettings",
            object.get("launchSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "runtimeSettings",
            object.get("runtimeSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "completionRules",
            object.get("completionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "attentionRules",
            object.get("attentionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "notificationRules",
            object.get("notificationRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "gitConfig",
            object.get("gitConfig").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "projectBoardConfig",
            object.get("projectBoardConfig").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "previousSessionHistory",
            object.get("previousSessionHistory").unwrap_or(&json!([])),
        )?),
        sql_text(required_string(object, "createdAt")?),
        sql_text(required_string(object, "updatedAt")?),
        sql_text(required_string(object, "visibility")?),
        sql_optional_text(optional_string(object, "systemKind")),
    ];
    Ok(rusqlite::params_from_iter(values))
}

pub(crate) fn session_insert_params(
    session: &Value,
) -> DomainResult<rusqlite::ParamsFromIter<Vec<rusqlite::types::Value>>> {
    let object = session
        .as_object()
        .ok_or_else(|| DomainStateError::bad_request("Session must be an object."))?;
    let hidden = object
        .get("hiddenMetadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let values = vec![
        sql_text(required_string(object, "projectId")?),
        sql_text(required_string(object, "sessionId")?),
        sql_text(required_string(object, "kind")?),
        sql_text(required_string(object, "title")?),
        sql_text(required_string(object, "lifecycleState")?),
        sql_text(stringify_domain_json_field(
            "providerState",
            object.get("providerState").unwrap_or(&json!({})),
        )?),
        sql_text(required_string(object, "zmxName")?),
        sql_optional_text(optional_string(object, "cwd")),
        sql_optional_text(optional_string(object, "agentId")),
        sql_optional_text(optional_string(object, "commandId")),
        sql_i64(bool_field(object, "isPinned") as i64),
        sql_i64(bool_field(object, "isFavorite") as i64),
        sql_optional_text(optional_string(object, "sessionTag")),
        sql_optional_text(optional_string(&hidden, "restoredFromSessionId")),
        sql_optional_text(optional_string(&hidden, "restoredFromHistoryId")),
        sql_text(stringify_domain_json_field(
            "launchSettings",
            object.get("launchSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "runtimeSettings",
            object.get("runtimeSettings").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "completionRules",
            object.get("completionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "attentionRules",
            object.get("attentionRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "notificationRules",
            object.get("notificationRules").unwrap_or(&json!({})),
        )?),
        sql_text(stringify_domain_json_field(
            "worktree",
            object.get("worktree").unwrap_or(&json!({})),
        )?),
        sql_text(required_string(object, "createdAt")?),
        sql_text(required_string(object, "updatedAt")?),
        sql_optional_text(optional_string(object, "lastActiveAt")),
        match object
            .get("sidebarOrder")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
        {
            Some(value) => rusqlite::types::Value::Real(value),
            None => rusqlite::types::Value::Null,
        },
        sql_optional_text(optional_string(object, "settledAt")),
        sql_optional_text(normalize_settled_override(
            optional_string(object, "settledOverride").as_deref(),
        )),
        sql_optional_text(optional_string(object, "settledOverrideAt")),
        sql_optional_text(optional_string(object, "snoozedAt")),
        sql_optional_text(optional_string(object, "snoozedUntil")),
    ];
    Ok(rusqlite::params_from_iter(values))
}

fn sql_text(value: String) -> rusqlite::types::Value {
    rusqlite::types::Value::Text(value)
}

fn sql_i64(value: i64) -> rusqlite::types::Value {
    rusqlite::types::Value::Integer(value)
}

fn sql_optional_text(value: Option<String>) -> rusqlite::types::Value {
    value.map(sql_text).unwrap_or(rusqlite::types::Value::Null)
}
