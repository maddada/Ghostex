use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_boolean, Flags};
use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

/*
CDXC:KeepAwake 2026-08-19:
The payload is deliberately a list of `{projectId, sessionId}` pairs the caller
already knows from `ghostex sessions --json --mobile-summary`, so no lease
renewal has to re-resolve a bare session id through the daemon inventory.
*/
/*
The body is the flat `{"event", "properties"}` shape `/api/recordClientEvent`
reads, not the `{"params"}` envelope. Only `client_os` is required here; the
daemon re-validates every field against its taxonomy and decides what is sent.
*/
pub(super) fn parse_client_hello(flags: &Flags) -> CliResult<Value> {
    let client = flags.text("client").unwrap_or_default();
    if client.trim() != "mobile" {
        return Err(CliError::Other(
            "client-hello requires --client mobile.".to_string(),
        ));
    }
    let os = flags.text("os").unwrap_or_default();
    if os.trim().is_empty() {
        return Err(CliError::Other(
            "client-hello requires --os <android|ios>.".to_string(),
        ));
    }
    let mut properties = Map::new();
    properties.insert(
        "client_os".to_string(),
        Value::String(os.trim().to_string()),
    );
    set_or_remove(
        &mut properties,
        "client_os_version",
        flag_json(flags, "osVersion"),
    );
    set_or_remove(
        &mut properties,
        "client_app_version",
        flag_json(flags, "appVersion"),
    );
    let mut map = Map::new();
    map.insert(
        "event".to_string(),
        Value::String("client.connected".to_string()),
    );
    map.insert("properties".to_string(), Value::Object(properties));
    Ok(Value::Object(map))
}

/*
CDXC:Telemetry 2026-10-27:
A phone screen opened, for "which views are used the most". Same flat
`/api/recordClientEvent` body as the hello; the daemon accepts only the
`mobile_*` members of its surface table with `client: mobile` and throttles each
to one event an hour.
*/
pub(super) fn parse_client_view(flags: &Flags) -> CliResult<Value> {
    if flags.text("client").unwrap_or_default().trim() != "mobile" {
        return Err(CliError::Other(
            "client-view requires --client mobile.".to_string(),
        ));
    }
    let view = flags.text("view").unwrap_or_default();
    if view.trim().is_empty() {
        return Err(CliError::Other(
            "client-view requires --view <name>.".to_string(),
        ));
    }
    Ok(json!({
        "event": "surface.opened",
        "properties": { "surface": view.trim(), "client": "mobile" },
    }))
}

pub(super) fn parse_keep_sessions_awake(flags: &Flags) -> CliResult<Value> {
    let sessions_text = flags.text("sessionsJson").unwrap_or_default();
    if sessions_text.trim().is_empty() {
        return Err(CliError::Other(
            "hold-sessions-awake requires --sessions-json '[{\"projectId\":\"...\",\"sessionId\":\"...\"}]'."
                .to_string(),
        ));
    }
    let sessions: Value = serde_json::from_str(&sessions_text)
        .map_err(|error| CliError::Other(format!("Invalid --sessions-json: {error}")))?;
    if !sessions.is_array() {
        return Err(CliError::Other(
            "Invalid --sessions-json: expected a JSON array.".to_string(),
        ));
    }
    let mut map = Map::new();
    map.insert("sessions".to_string(), sessions);
    if flags.contains("ttlMs") {
        map.insert("ttlMs".to_string(), flag_number_value(flags, "ttlMs"));
    }
    set_or_remove(&mut map, "holderId", flag_json(flags, "holderId"));
    if flags.truthy("release") {
        map.insert("release".to_string(), Value::Bool(true));
    }
    Ok(Value::Object(map))
}

pub(super) fn parse_create_session(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "cwd", flag_json(flags, "cwd"));
    set_or_remove(&mut map, "groupId", flag_json(flags, "groupId"));
    map.insert(
        "input".to_string(),
        flag_json(flags, "input").unwrap_or_else(|| Value::String(join_rest(rest, 1))),
    );
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    if let Some(value) = flags.0.get("start") {
        map.insert("start".to_string(), Value::Bool(parse_boolean(value)));
    }
    set_or_remove(
        &mut map,
        "title",
        flag_json(flags, "title").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_agent(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "agentId",
        flag_json(flags, "agentId").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(
        &mut map,
        "firstInputDraft",
        flag_json(flags, "firstInputDraft"),
    );
    set_or_remove(&mut map, "groupId", flag_json(flags, "groupId"));
    set_or_remove(&mut map, "agentModel", flag_json(flags, "model"));
    set_or_remove(&mut map, "agentEffort", flag_json(flags, "effort"));
    set_or_remove(&mut map, "runOn", flag_json(flags, "runOn"));
    set_or_remove(&mut map, "prompt", flag_json(flags, "prompt"));
    Value::Object(map)
}

pub(super) fn parse_command_button(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "commandId",
        flag_json(flags, "commandId").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_click_button(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "id",
        flag_json(flags, "id").or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(
        &mut map,
        "kind",
        flag_json(flags, "kind").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_save_agent(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "acceptAllMode", flag_json(flags, "acceptAllMode"));
    set_or_remove(
        &mut map,
        "agentId",
        flag_json(flags, "agentId").or_else(|| rest_string(rest, 0)),
    );
    map.insert(
        "command".to_string(),
        flag_json(flags, "command").unwrap_or_else(|| Value::String(join_rest(rest, 2))),
    );
    set_or_remove(&mut map, "icon", flag_json(flags, "icon"));
    set_or_remove(
        &mut map,
        "name",
        flag_json(flags, "name").or_else(|| rest_string(rest, 1)),
    );
    Value::Object(map)
}

pub(super) fn parse_save_command(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    map.insert(
        "actionType".to_string(),
        flag_json(flags, "actionType")
            .or_else(|| flag_json(flags, "type"))
            .unwrap_or_else(|| json!("terminal")),
    );
    map.insert(
        "closeTerminalOnExit".to_string(),
        Value::Bool(
            flags
                .0
                .get("closeTerminalOnExit")
                .map(parse_boolean)
                .unwrap_or(false),
        ),
    );
    map.insert(
        "command".to_string(),
        flag_json(flags, "command").unwrap_or_else(|| Value::String(join_rest(rest, 2))),
    );
    set_or_remove(
        &mut map,
        "commandId",
        flag_json(flags, "commandId").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(&mut map, "icon", flag_json(flags, "icon"));
    set_or_remove(
        &mut map,
        "name",
        flag_json(flags, "name").or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(&mut map, "path", flag_json(flags, "path"));
    map.insert(
        "playCompletionSound".to_string(),
        Value::Bool(
            flags
                .0
                .get("playCompletionSound")
                .map(parse_boolean)
                .unwrap_or(true),
        ),
    );
    map.insert(
        "showOnProjectRow".to_string(),
        Value::Bool(
            flags
                .0
                .get("showOnProjectRow")
                .map(parse_boolean)
                .unwrap_or(false),
        ),
    );
    set_or_remove(&mut map, "url", flag_json(flags, "url"));
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    set_or_remove(&mut map, "projectName", flag_json(flags, "projectName"));
    Value::Object(map)
}

pub(super) fn parse_group(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "groupId",
        flag_json(flags, "groupId").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_project(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "name", flag_json(flags, "name"));
    set_or_remove(
        &mut map,
        "path",
        flag_json(flags, "path").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    Value::Object(map)
}

pub(super) fn parse_project_move(rest: &[String], flags: &Flags) -> Value {
    /*
    CDXC:Mobile 2026-05-18-16:13:
    Ghostex Android reorders project groups through the Mac CLI, not local
    phone state. The desktop sidebar remains the source of truth and later
    inventory calls return the persisted order to mobile.
    */
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "direction",
        flag_json(flags, "direction")
            .or_else(|| flag_json(flags, "dir"))
            .or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(
        &mut map,
        "projectId",
        flag_json(flags, "projectId").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_project_path(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    /*
    CDXC:AddProject 2026-07-30:
    Ghostex mobile speaks the CLI, not the wire protocol, so the Add Project
    flow's "create this folder and add it" affordance reaches gxserver as
    `add-project --create-if-missing`. The flag is only sent when the caller
    passed it, so every existing `add-project` invocation keeps the old
    missing-path rejection.
    */
    if flags.contains("createIfMissing") {
        map.insert(
            "createIfMissing".to_string(),
            Value::Bool(parse_boolean(
                flags.0.get("createIfMissing").expect("flag present"),
            )),
        );
    }
    set_or_remove(&mut map, "name", flag_json(flags, "name"));
    set_or_remove(
        &mut map,
        "path",
        flag_json(flags, "path").or_else(|| rest_string(rest, 0)),
    );
    // `--workspace <name|id>`: gxserver resolves either; without it the project stays in Personal.
    set_or_remove(
        &mut map,
        "workspaceId",
        flag_json(flags, "workspace").or_else(|| flag_json(flags, "workspaceId")),
    );
    Value::Object(map)
}

pub(super) fn parse_project_collection(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "name", flag_json(flags, "name"));
    set_or_remove(&mut map, "path", flag_json(flags, "path"));
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    // A bare first positional is a project id. Paths and names stay explicit
    // so automation never guesses which registered project the user meant.
    if !map.contains_key("projectId") && !map.contains_key("path") && !map.contains_key("name") {
        set_or_remove(&mut map, "projectId", rest_string(rest, 0));
    }
    set_or_remove(
        &mut map,
        "collectionTitle",
        flag_json(flags, "group")
            .or_else(|| flag_json(flags, "collection"))
            .or_else(|| rest_string(rest, 1)),
    );
    Value::Object(map)
}

pub(super) fn parse_browse_directories(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "cwd", flag_json(flags, "cwd"));
    if flags.contains("limit") {
        map.insert("limit".to_string(), flag_number_value(flags, "limit"));
    }
    set_or_remove(
        &mut map,
        "partialPath",
        flag_json(flags, "partialPath").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_lookup_repository(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "cwd", flag_json(flags, "cwd"));
    set_or_remove(
        &mut map,
        "provider",
        flag_json(flags, "provider").or_else(|| rest_string(rest, 0)),
    );
    set_or_remove(
        &mut map,
        "repository",
        flag_json(flags, "repository").or_else(|| rest_string(rest, 1)),
    );
    Value::Object(map)
}

pub(super) fn parse_clone_repository(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "branchName", flag_json(flags, "branchName"));
    if let Some(value) = flags.0.get("cloneMainOnly") {
        map.insert(
            "cloneMainOnly".to_string(),
            Value::Bool(parse_boolean(value)),
        );
    }
    set_or_remove(
        &mut map,
        "destinationPath",
        flag_json(flags, "destinationPath").or_else(|| rest_string(rest, 1)),
    );
    set_or_remove(
        &mut map,
        "remoteUrl",
        flag_json(flags, "remoteUrl").or_else(|| rest_string(rest, 0)),
    );
    if let Some(value) = flags.0.get("shallowClone") {
        map.insert(
            "shallowClone".to_string(),
            Value::Bool(parse_boolean(value)),
        );
    }
    // `--workspace <name|id>` puts the cloned project there, like add-project's.
    set_or_remove(
        &mut map,
        "workspaceId",
        flag_json(flags, "workspace").or_else(|| flag_json(flags, "workspaceId")),
    );
    Value::Object(map)
}
