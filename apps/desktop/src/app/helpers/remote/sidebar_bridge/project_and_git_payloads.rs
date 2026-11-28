use crate::*;

pub(crate) fn gpui_remote_sidebar_recent_projects_value(
    result: &serde_json::Value,
) -> Option<serde_json::Value> {
    let rows = result.get("recentProjects")?.as_array()?;
    Some(serde_json::Value::Array(
        rows.iter()
            .filter_map(|row| row.as_object())
            .filter_map(gpui_remote_sidebar_recent_project_payload)
            .collect(),
    ))
}

pub(crate) fn gpui_remote_sidebar_recent_project_payload(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let project_id = json_string_field(project, "projectId")?;
    let title = json_string_field(project, "title")?;
    let path = json_string_field(project, "path")?;
    let mut output = serde_json::Map::new();
    output.insert("path".to_string(), serde_json::json!(path));
    output.insert("projectId".to_string(), serde_json::json!(project_id));
    output.insert("title".to_string(), serde_json::json!(title));
    if let Some(icon) = project.get("icon").and_then(serde_json::Value::as_object) {
        output.insert("icon".to_string(), serde_json::Value::Object(icon.clone()));
    }
    if let Some(icon_data_url) = json_string_field(project, "iconDataUrl") {
        output.insert("iconDataUrl".to_string(), serde_json::json!(icon_data_url));
    }
    if let Some(recent_closed_at) = json_string_field(project, "recentClosedAt") {
        output.insert(
            "recentClosedAt".to_string(),
            serde_json::json!(recent_closed_at),
        );
    }
    if let Some(session_count) = json_u64_field(project, "sessionCount") {
        output.insert("sessionCount".to_string(), serde_json::json!(session_count));
    }
    if let Some(theme) = json_string_field(project, "theme") {
        output.insert("theme".to_string(), serde_json::json!(theme));
    }
    if let Some(theme_color) = json_string_field(project, "themeColor") {
        output.insert("themeColor".to_string(), serde_json::json!(theme_color));
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_presentation_project_payload(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    /*
    CDXC:RemoteMachines 2026-06-24-18:22:
    Remote project mutations may return only presentation-shaped project metadata plus sanitized Git preferences. Strip raw domain-only state such as custom commands, agents, launch settings, notifications, history, and board config before CEF receives the response.
    */
    let project_id = json_string_field(project, "projectId")?;
    let title = json_string_field(project, "title")
        .or_else(|| json_string_field(project, "name"))
        .unwrap_or("Project");
    let created_at = json_string_field(project, "createdAt").unwrap_or("");
    let updated_at = json_string_field(project, "updatedAt").unwrap_or(created_at);
    let sort_key = json_string_field(project, "sortKey").unwrap_or(updated_at);
    let mut output = serde_json::Map::new();
    output.insert("createdAt".to_string(), serde_json::json!(created_at));
    output.insert(
        "groupIds".to_string(),
        serde_json::json!(
            json_array_field(project, "groupIds")
                .cloned()
                .unwrap_or_default()
        ),
    );
    if let Some(git_config) = project
        .get("gitConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_git_config_payload)
    {
        output.insert("gitConfig".to_string(), git_config);
    }
    output.insert(
        "isFavorite".to_string(),
        serde_json::json!(json_bool_field(project, "isFavorite").unwrap_or(false)),
    );
    output.insert(
        "isPinned".to_string(),
        serde_json::json!(json_bool_field(project, "isPinned").unwrap_or(false)),
    );
    if let Some(path) = json_string_field(project, "path") {
        output.insert("path".to_string(), serde_json::json!(path));
    }
    output.insert("projectId".to_string(), serde_json::json!(project_id));
    output.insert("sortKey".to_string(), serde_json::json!(sort_key));
    output.insert("title".to_string(), serde_json::json!(title));
    output.insert("updatedAt".to_string(), serde_json::json!(updated_at));
    if let Some(worktree) = project
        .get("worktree")
        .and_then(serde_json::Value::as_object)
    {
        output.insert(
            "worktree".to_string(),
            serde_json::Value::Object(worktree.clone()),
        );
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_git_config_payload(
    source: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut output = serde_json::Map::new();
    if let Some(confirm_commit) = json_bool_field(source, "confirmCommit") {
        output.insert(
            "confirmCommit".to_string(),
            serde_json::json!(confirm_commit),
        );
    }
    if let Some(generate_commit_body) = json_bool_field(source, "generateCommitBody") {
        output.insert(
            "generateCommitBody".to_string(),
            serde_json::json!(generate_commit_body),
        );
    }
    if let Some(primary_action) = json_string_field(source, "primaryAction")
        .filter(|value| gpui_remote_sidebar_git_action_allowed(value))
    {
        output.insert(
            "primaryAction".to_string(),
            serde_json::json!(primary_action),
        );
    }
    (!output.is_empty()).then(|| serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_git_action_allowed(value: &str) -> bool {
    matches!(
        value,
        "commit" | "push" | "pr" | "syncRemote" | "syncMain" | "multiRelease" | "release"
    )
}

pub(crate) fn gpui_remote_sidebar_typed_operation_response_payload(
    mut result: serde_json::Value,
) -> serde_json::Value {
    if let Some(object) = result.as_object_mut() {
        object.remove("command");
    }
    result
}

pub(crate) fn gpui_remote_sidebar_generate_commit_message_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(subject) = result.get("subject").and_then(serde_json::Value::as_str) {
        response.insert("subject".to_string(), serde_json::json!(subject));
    }
    if let Some(body) = result.get("body").and_then(serde_json::Value::as_str) {
        response.insert("body".to_string(), serde_json::json!(body));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_create_pull_request_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(ok) = result.get("ok").and_then(serde_json::Value::as_bool) {
        response.insert("ok".to_string(), serde_json::json!(ok));
    }
    if let Some(created) = result.get("created").and_then(serde_json::Value::as_bool) {
        response.insert("created".to_string(), serde_json::json!(created));
    }
    if let Some(reason) = result.get("reason").and_then(serde_json::Value::as_str) {
        response.insert("reason".to_string(), serde_json::json!(reason));
    }
    if let Some(provider) = result
        .get("provider")
        .and_then(serde_json::Value::as_str)
        .and_then(ghostex_gx_protocol::git_provider::GitHostProvider::from_wire_name)
    {
        response.insert(
            "provider".to_string(),
            serde_json::json!(provider.wire_name()),
        );
    }
    if let Some(pr) = result.get("pr").and_then(serde_json::Value::as_object) {
        let mut sanitized_pr = serde_json::Map::new();
        if let Some(state) = pr.get("state").and_then(serde_json::Value::as_str) {
            sanitized_pr.insert("state".to_string(), serde_json::json!(state));
        }
        if let Some(number) = pr.get("number").and_then(serde_json::Value::as_u64) {
            sanitized_pr.insert("number".to_string(), serde_json::json!(number));
        }
        if let Some(provider) = pr
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .and_then(ghostex_gx_protocol::git_provider::GitHostProvider::from_wire_name)
        {
            sanitized_pr.insert(
                "provider".to_string(),
                serde_json::json!(provider.wire_name()),
            );
        }
        if !sanitized_pr.is_empty() {
            response.insert("pr".to_string(), serde_json::Value::Object(sanitized_pr));
        }
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_delete_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let warnings = result
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(|warning| {
                    let kind = warning.get("kind").and_then(serde_json::Value::as_str)?;
                    Some(serde_json::json!({ "kind": kind }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = serde_json::Map::new();
    response.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    serde_json::Value::Object(response)
}
