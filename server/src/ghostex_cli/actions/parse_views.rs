use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_boolean, FlagValue, Flags};
use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

pub(super) fn parse_visible_count(rest: &[String], flags: &Flags) -> Value {
    let count = if flags.contains("count") {
        flag_number_value(flags, "count")
    } else if let Some(first) = rest.first() {
        crate::ghostex_cli::args::js_number(first)
            .map(js_number_to_value)
            .unwrap_or(Value::Null)
    } else {
        // Number(undefined) is NaN, which JSON-serializes to null.
        Value::Null
    };
    json!({ "count": count })
}

pub(super) fn parse_view_mode(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "mode",
        flag_json(flags, "mode").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_url(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(
        &mut map,
        "url",
        flag_json(flags, "url").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_browser_open(rest: &[String], flags: &Flags) -> Value {
    let mut map = Map::new();
    set_or_remove(&mut map, "groupId", flag_json(flags, "groupId"));
    set_or_remove(&mut map, "projectId", flag_json(flags, "projectId"));
    set_or_remove(
        &mut map,
        "projectName",
        flag_json(flags, "projectName").or_else(|| flag_json(flags, "name")),
    );
    let project_path = flag_json(flags, "projectPath")
        .or_else(|| flag_json(flags, "path"))
        .or_else(|| {
            let active_project = flags
                .0
                .get("activeProject")
                .map(parse_boolean)
                .unwrap_or(false);
            if active_project {
                None
            } else {
                Some(Value::String(cwd_string()))
            }
        });
    set_or_remove(&mut map, "projectPath", project_path);
    map.insert(
        "reuse".to_string(),
        if flags.truthy("new") {
            json!("none")
        } else {
            flag_json(flags, "reuse").unwrap_or_else(|| json!("similar"))
        },
    );
    set_or_remove(
        &mut map,
        "url",
        flag_json(flags, "url").or_else(|| rest_string(rest, 0)),
    );
    Value::Object(map)
}

pub(super) fn parse_open_paths(rest: &[String], flags: &Flags) -> Value {
    let targets: Vec<Value> = if !rest.is_empty() {
        rest.iter()
            .map(|value| Value::String(value.clone()))
            .collect()
    } else if flags.truthy("path") {
        vec![flags.0.get("path").expect("truthy path").as_json()]
    } else {
        Vec::new()
    };
    json!({
        "mode": "open",
        "targets": targets
            .iter()
            .map(|target| parse_open_path_target(target, false))
            .collect::<Vec<Value>>(),
    })
}

pub(super) fn parse_edit_paths(rest: &[String], flags: &Flags) -> Value {
    let wait_consumed_target = flags.string_value("wait").map(str::to_string);
    let targets: Vec<Value> = if !rest.is_empty() {
        rest.iter()
            .map(|value| Value::String(value.clone()))
            .collect()
    } else if flags.truthy("goto") {
        vec![flags.0.get("goto").expect("truthy goto").as_json()]
    } else if flags.truthy("path") {
        vec![flags.0.get("path").expect("truthy path").as_json()]
    } else if matches!(&wait_consumed_target, Some(target) if !target.is_empty()) {
        vec![Value::String(
            wait_consumed_target.clone().expect("checked wait target"),
        )]
    } else {
        Vec::new()
    };
    let wait = flags.0.get("wait") == Some(&FlagValue::Bool(true))
        || wait_consumed_target.is_some()
        || flags.0.get("wait").map(parse_boolean).unwrap_or(false);
    json!({
        "mode": "edit",
        "targets": targets
            .iter()
            .map(|target| parse_open_path_target(target, wait))
            .collect::<Vec<Value>>(),
        "wait": wait,
    })
}

pub(super) fn parse_quick_terminal(rest: &[String], flags: &Flags) -> Value {
    let command_separator_index = rest.iter().position(|arg| arg == "--");
    let command_rest: &[String] = match command_separator_index {
        Some(index) => &rest[index + 1..],
        None => rest,
    };
    let mut map = Map::new();
    if !command_rest.is_empty() {
        map.insert("command".to_string(), Value::String(command_rest.join(" ")));
    }
    set_or_remove(
        &mut map,
        "cwd",
        flag_json(flags, "cwd").or_else(|| flag_json(flags, "path")),
    );
    set_or_remove(
        &mut map,
        "title",
        flag_json(flags, "title").or_else(|| flag_json(flags, "name")),
    );
    Value::Object(map)
}

pub(super) fn parse_open_path_target(value: &Value, wait: bool) -> Value {
    let raw = string_or_empty(Some(value)).trim().to_string();
    let (parsed_path, line, column) = parse_vs_code_path_position(&raw);
    let mut target = Map::new();
    if let Some(column) = column {
        target.insert("column".to_string(), json!(column));
    }
    if let Some(line) = line {
        target.insert("line".to_string(), json!(line));
    }
    target.insert("path".to_string(), json!(node_path_resolve(&parsed_path)));
    target.insert("raw".to_string(), json!(raw));
    if wait {
        /*
        CDXC:OsIntegration 2026-05-27-18:06:
        `ghostex edit --wait` waits for a concrete opened editor item, so each
        target carries a stable per-command wait token across the native
        bridge.
        */
        target.insert(
            "waitToken".to_string(),
            json!(format!(
                "wait-{}-{}",
                to_base36(chrono::Utc::now().timestamp_millis().max(0) as u128),
                random_base36(8)
            )),
        );
    }
    Value::Object(target)
}

/// parseVsCodePathPosition: VS Code-style `file:line:column` targets. Mirrors
/// `/^(?<path>.+?)(?::(?<line>[1-9]\d*))?(?::(?<column>[1-9]\d*))?$/u`.
pub(super) fn parse_vs_code_path_position(value: &str) -> (String, Option<i64>, Option<i64>) {
    if value.is_empty() {
        // The regex requires a non-empty path; JS falls back to { path: value }.
        return (value.to_string(), None, None);
    }
    fn is_positive_int(candidate: &str) -> bool {
        let mut chars = candidate.chars();
        matches!(chars.next(), Some(first) if ('1'..='9').contains(&first))
            && chars.all(|c| c.is_ascii_digit())
    }
    let mut numbers: Vec<i64> = Vec::new();
    let mut current = value;
    for _ in 0..2 {
        let Some(index) = current.rfind(':') else {
            break;
        };
        let (head, tail) = current.split_at(index);
        let digits = &tail[1..];
        if head.is_empty() || !is_positive_int(digits) {
            break;
        }
        numbers.push(digits.parse().expect("validated digits"));
        current = head;
    }
    match numbers.len() {
        1 => (current.to_string(), Some(numbers[0]), None),
        2 => (current.to_string(), Some(numbers[1]), Some(numbers[0])),
        _ => (current.to_string(), None, None),
    }
}

pub(super) fn parse_assert_card(rest: &[String], flags: &Flags) -> Map<String, Value> {
    let mut map = parse_session_selector(rest, flags);
    set_or_remove(&mut map, "agentIcon", flag_json(flags, "agentIcon"));
    set_or_remove(&mut map, "agentName", flag_json(flags, "agentName"));
    if let Some(value) = flags.0.get("visible") {
        map.insert("visible".to_string(), Value::Bool(parse_boolean(value)));
    }
    map
}

pub(super) fn parse_wait_for(rest: &[String], flags: &Flags) -> Value {
    let mut map = parse_assert_card(rest, flags);
    if flags.contains("intervalMs") {
        map.insert(
            "intervalMs".to_string(),
            flag_number_value(flags, "intervalMs"),
        );
    }
    if flags.contains("timeoutMs") {
        map.insert(
            "timeoutMs".to_string(),
            flag_number_value(flags, "timeoutMs"),
        );
    }
    Value::Object(map)
}

pub(super) fn parse_sidebar_project_collections_state(
    rest: &[String],
    flags: &Flags,
) -> CliResult<Value> {
    /*
    CDXC:Projects 2026-07-18-00:00:
    Mobile edits durable sidebar project collections by SSH-exec'ing `ghostex
    update-sidebar-project-collections --state-json '<json>'` for a full
    read-modify-write of the collections state. The CLI passes the whole state
    through untouched; gxserver owns normalization (order authority, one
    project per collection, limits) and the normalized result is printed back
    for the client to adopt. `--state-json` mirrors automation-save's
    `--definition-json` ergonomics; `--json` stays the CLI output flag.
    */
    let state_json = flag_json(flags, "stateJson")
        .or_else(|| flag_json(flags, "state"))
        .unwrap_or_else(|| Value::String(join_rest(rest, 0)));
    let state_text = match state_json {
        Value::String(text) => text,
        _ => String::new(),
    };
    if state_text.trim().is_empty() {
        return Err(CliError::Other(
            "update-sidebar-project-collections requires --state-json '<json>' with the full collections state.".to_string(),
        ));
    }
    let state: Value = serde_json::from_str(&state_text)
        .map_err(|error| CliError::Other(format!("Invalid --state-json: {error}")))?;
    if !state.is_object() {
        return Err(CliError::Other(
            "update-sidebar-project-collections --state-json must be a JSON object with collections, order, and nextCollectionNumber.".to_string(),
        ));
    }
    Ok(json!({ "state": state }))
}

pub(super) fn parse_sidebar_spaces_state(rest: &[String], flags: &Flags) -> CliResult<Value> {
    /*
    CDXC:Spaces 2026-08-27:
    Mobile edits durable sidebar Spaces by SSH-exec'ing `ghostex
    update-sidebar-spaces --state-json '<json>'` for a full read-modify-write of
    the Space document, exactly like the project-collections command beside it.
    The CLI passes the whole state through untouched; gxserver owns
    normalization (order authority, member dedupe, grouped-project exclusion,
    limits) and the normalized result is printed back for the client to adopt.
    */
    let state_json = flag_json(flags, "stateJson")
        .or_else(|| flag_json(flags, "state"))
        .unwrap_or_else(|| Value::String(join_rest(rest, 0)));
    let state_text = match state_json {
        Value::String(text) => text,
        _ => String::new(),
    };
    if state_text.trim().is_empty() {
        return Err(CliError::Other(
            "update-sidebar-spaces requires --state-json '<json>' with the full spaces state."
                .to_string(),
        ));
    }
    let state: Value = serde_json::from_str(&state_text)
        .map_err(|error| CliError::Other(format!("Invalid --state-json: {error}")))?;
    if !state.is_object() {
        return Err(CliError::Other(
            "update-sidebar-spaces --state-json must be a JSON object with spaces and order."
                .to_string(),
        ));
    }
    Ok(json!({ "state": state }))
}

/// `ghostex update-custom-session-tags --state-json '<json>'`: a full
/// read-modify-write of the custom tag catalog, passed through untouched so
/// gxserver owns normalization exactly as it does for Spaces.
pub(super) fn parse_custom_session_tags_state(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let state_json = flag_json(flags, "stateJson")
        .or_else(|| flag_json(flags, "state"))
        .unwrap_or_else(|| Value::String(join_rest(rest, 0)));
    let state_text = match state_json {
        Value::String(text) => text,
        _ => String::new(),
    };
    if state_text.trim().is_empty() {
        return Err(CliError::Other(
            "update-custom-session-tags requires --state-json '<json>' with the full custom session tags state."
                .to_string(),
        ));
    }
    let state: Value = serde_json::from_str(&state_text)
        .map_err(|error| CliError::Other(format!("Invalid --state-json: {error}")))?;
    if !state.is_object() {
        return Err(CliError::Other(
            "update-custom-session-tags --state-json must be a JSON object with tags and order."
                .to_string(),
        ));
    }
    Ok(json!({ "state": state }))
}
