use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_boolean, FlagValue, Flags};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};

use super::*;

/// Sidebar session tags accepted by `ghostex tag-session` (insertion order
/// matters: it is reproduced in error messages).
const SIDEBAR_SESSION_TAGS: [&str; 13] = [
    "favorite",
    "high-priority",
    "low-priority",
    "todo",
    "research",
    "in-progress",
    "testing",
    "blocked",
    "on-hold",
    "done",
    "bug",
    "feature",
    "design",
];

const CLEAR_SESSION_TAG_VALUES: [&str; 5] = ["", "clear", "none", "null", "unset"];

pub(super) fn parse_session_selector(rest: &[String], flags: &Flags) -> Map<String, Value> {
    let mut map = Map::new();
    if flags.contains("index") {
        map.insert("index".to_string(), flag_number_value(flags, "index"));
    }
    set_or_remove(
        &mut map,
        "sessionId",
        flag_json(flags, "sessionId").or_else(|| rest_string(rest, 0)),
    );
    if flags.contains("sessionNumber") {
        map.insert(
            "sessionNumber".to_string(),
            flag_number_value(flags, "sessionNumber"),
        );
    }
    map
}

pub(super) fn parse_rename(rest: &[String], flags: &Flags) -> Value {
    /*
    CDXC:Mobile 2026-05-17-13:23:
    Ghostex Android invokes remote rename through `ghostex rename-session
    --session-id <id> --title <title> --json` so SSH quoting can keep the
    stable session id and user-entered title as separate CLI arguments.
    */
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "title".to_string(),
        flag_json(flags, "title").unwrap_or_else(|| Value::String(join_rest(rest, 1))),
    );
    Value::Object(map)
}

/*
CDXC:Mobile 2026-08-01:
`/api/requestSessionRename` takes the rename-session payload plus the agent
identity hints and a title source. `titleSource` defaults to "user" here so
mobile does not have to send it on every rename; gxserver applies the same
default, but sending it keeps the CLI payload self-describing.
*/
pub(super) fn parse_rename_request(rest: &[String], flags: &Flags) -> Value {
    let mut map = parse_rename(rest, flags)
        .as_object()
        .cloned()
        .unwrap_or_default();
    set_or_remove(&mut map, "agentName", flag_json(flags, "agentName"));
    set_or_remove(
        &mut map,
        "agentSessionId",
        flag_json(flags, "agentSessionId"),
    );
    set_or_remove(
        &mut map,
        "agentSessionPath",
        flag_json(flags, "agentSessionPath"),
    );
    map.insert(
        "titleSource".to_string(),
        flag_json(flags, "titleSource").unwrap_or_else(|| Value::String("user".to_string())),
    );
    Value::Object(map)
}

/// `export-transcript`'s selector, and what the file holds when the caller says: without a flag
/// gxserver's default applies.
///
/// CDXC:TranscriptExport 2026-10-31 WHY:
/// User: Include starts off on every client, and the phone reaches gxserver only through this verb, which took no include options, so its file always held commands and file changes.
pub(super) fn parse_export_transcript(rest: &[String], flags: &Flags) -> Value {
    let mut map = parse_session_selector(rest, flags);
    for name in ["includeCommands", "includePatches", "includeReasoning"] {
        if let Some(raw) = flags.0.get(name) {
            map.insert(name.to_string(), Value::Bool(parse_boolean(raw)));
        }
    }
    // `--last-turns N` limits the file to the last N turns; anything not a positive number is all of it.
    if let Some(turns) = flag_json(flags, "lastTurns")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.trim().parse().ok())
        })
        .filter(|turns| *turns > 0)
    {
        map.insert("lastTurns".to_string(), json!(turns));
    }
    Value::Object(map)
}

pub(super) fn parse_session_boolean(name: &str, rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector =
        flags.contains("sessionId") || flags.contains("index") || flags.contains("sessionNumber");
    let positional_index = if has_flag_selector { 0 } else { 1 };
    let raw = flags
        .0
        .get(name)
        .cloned()
        .or_else(|| flags.0.get("value").cloned())
        .or_else(|| {
            rest.get(positional_index)
                .map(|text| FlagValue::Text(text.clone()))
        })
        .unwrap_or_else(|| FlagValue::Text("true".to_string()));
    let mut map = parse_session_selector(rest, flags);
    map.insert(name.to_string(), Value::Bool(parse_boolean(&raw)));
    Value::Object(map)
}

pub(super) fn parse_delayed_send(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let mut map = parse_session_selector(rest, flags);
    let send_when_agent_stops = flags.truthy("whenAgentFinishes");
    let send_when_all_project_sessions_stop = flags.truthy("whenAllAgentsFinish");
    let delay_ms = flags.number("delayMs").filter(|value| value.is_finite());

    let trigger_count = usize::from(delay_ms.is_some())
        + usize::from(send_when_agent_stops)
        + usize::from(send_when_all_project_sessions_stop);
    if trigger_count == 0 {
        return Err(CliError::Other(
            "Missing Delayed Send trigger. Use --delay-ms, --when-agent-finishes, or --when-all-agents-finish."
                .to_string(),
        ));
    }
    if trigger_count > 1 {
        return Err(CliError::Other(
            "Choose exactly one Delayed Send trigger: --delay-ms, --when-agent-finishes, or --when-all-agents-finish."
                .to_string(),
        ));
    }

    if let Some(delay_ms) = delay_ms {
        map.insert("delayMs".to_string(), js_number_to_value(delay_ms));
    } else if send_when_agent_stops {
        map.insert("sendWhenAgentStops".to_string(), Value::Bool(true));
    } else {
        map.insert(
            "sendWhenAllProjectSessionsStop".to_string(),
            Value::Bool(true),
        );
    }
    Ok(Value::Object(map))
}

pub(super) fn parse_session_tag(rest: &[String], flags: &Flags) -> CliResult<Value> {
    let has_flag_selector =
        flags.contains("sessionId") || flags.contains("index") || flags.contains("sessionNumber");
    let positional_index = if has_flag_selector { 0 } else { 1 };
    let raw_tag = flag_json(flags, "tag")
        .or_else(|| flag_json(flags, "sessionTag"))
        .or_else(|| flag_json(flags, "value"))
        .or_else(|| rest_string(rest, positional_index));
    let tag_list = SIDEBAR_SESSION_TAGS.join(", ");
    let Some(raw_tag) = raw_tag else {
        return Err(CliError::Other(format!(
            "Missing session tag. Use one of: {tag_list}, or none."
        )));
    };
    let raw_tag_string = js_string(&raw_tag);
    let normalized_tag = raw_tag_string.trim().to_lowercase();
    let mut map = parse_session_selector(rest, flags);
    /*
    CDXC:Sessions 2026-09-11 WHY:
    Built-in tags and clear words resolve here, but a custom tag is only known
    to the daemon catalog, and this parser has no rpc access. Anything that is
    neither is carried as `customSessionTagQuery` (the user's text, so the
    error can echo it) for the tagSession bridge branch to resolve against the
    catalog by name or id; the daemon never receives that key.
    */
    if CLEAR_SESSION_TAG_VALUES.contains(&normalized_tag.as_str()) {
        map.insert("isFavorite".to_string(), Value::Bool(false));
        map.insert("sessionTag".to_string(), Value::Null);
    } else if SIDEBAR_SESSION_TAGS.contains(&normalized_tag.as_str()) {
        map.insert(
            "isFavorite".to_string(),
            Value::Bool(normalized_tag == "favorite"),
        );
        map.insert("sessionTag".to_string(), Value::String(normalized_tag));
    } else {
        map.insert(
            "customSessionTagQuery".to_string(),
            Value::String(raw_tag_string.trim().to_string()),
        );
    }
    Ok(Value::Object(map))
}

/// Resolve a `customSessionTagQuery` left by `parse_session_tag` against the
/// daemon's custom tag catalog: an exact id match first, then a
/// case-insensitive name match in catalog order. Payloads without the query
/// key pass through untouched, so built-in tags cost no extra round trip.
pub(super) fn resolve_custom_session_tag_for_tag_session(
    payload: &Value,
    flags: &Flags,
) -> CliResult<Value> {
    let Some(query) = payload.get("customSessionTagQuery").and_then(Value::as_str) else {
        return Ok(payload.clone());
    };
    // Same daemon the session write goes to: a global session ref in the
    // payload can select a remote server, and its catalog is the one that
    // names the tag.
    let target = rpc::resolve_gxserver_server_target(flags, payload)?;
    let catalog =
        rpc::request_gxserver_rpc(&target, "/api/readCustomSessionTags", &json!({}), flags)?;
    let catalog = catalog.get("customSessionTags").unwrap_or(&Value::Null);
    let tags = catalog.get("tags").and_then(Value::as_object);
    let ordered_tags: Vec<&Value> = catalog
        .get("order")
        .and_then(Value::as_array)
        .map(|order| {
            order
                .iter()
                .filter_map(|id| tags?.get(id.as_str()?))
                .collect()
        })
        .unwrap_or_default();
    let query_lower = query.to_lowercase();
    let resolved = ordered_tags
        .iter()
        .find(|tag| tag.get("tagId").and_then(Value::as_str) == Some(query))
        .or_else(|| {
            ordered_tags.iter().find(|tag| {
                tag.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.to_lowercase() == query_lower)
            })
        })
        .and_then(|tag| tag.get("tagId").and_then(Value::as_str));
    let Some(tag_id) = resolved else {
        let mut choices: Vec<String> = SIDEBAR_SESSION_TAGS
            .iter()
            .map(|tag| (*tag).to_string())
            .collect();
        choices.extend(
            ordered_tags
                .iter()
                .filter_map(|tag| tag.get("name").and_then(Value::as_str))
                .map(|name| format!("\"{name}\"")),
        );
        return Err(CliError::Other(format!(
            "Unknown session tag \"{query}\". Use one of: {}, or none.",
            choices.join(", ")
        )));
    };
    let mut object = payload.as_object().cloned().unwrap_or_default();
    object.remove("customSessionTagQuery");
    object.insert("isFavorite".to_string(), Value::Bool(false));
    object.insert("sessionTag".to_string(), Value::String(tag_id.to_string()));
    Ok(Value::Object(object))
}

/*
CDXC:SessionNotes 2026-08-24:
`ghostex session-note save --session-id <id> --note <text> --json` keeps the
stable session id and the user-entered note as separate CLI arguments, exactly
like `rename-session` does with `--title`, so SSH quoting never has to reunite
them. An omitted or empty note clears the note.
*/
pub(super) fn parse_session_note(rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector =
        flags.contains("sessionId") || flags.contains("index") || flags.contains("sessionNumber");
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "note".to_string(),
        flag_json(flags, "note").unwrap_or_else(|| {
            Value::String(join_rest(rest, if has_flag_selector { 0 } else { 1 }))
        }),
    );
    Value::Object(map)
}

pub(super) fn parse_send_text(rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector = ["sessionId", "selector", "session", "sessionTitle", "target"]
        .iter()
        .any(|key| flags.truthy(key));
    let mut map = parse_session_selector(rest, flags);
    map.insert(
        "text".to_string(),
        flag_json(flags, "text").unwrap_or_else(|| {
            Value::String(join_rest(rest, if has_flag_selector { 0 } else { 1 }))
        }),
    );
    Value::Object(map)
}

pub(super) fn parse_send_key(rest: &[String], flags: &Flags) -> Value {
    let has_flag_selector = ["sessionId", "selector", "session", "sessionTitle", "target"]
        .iter()
        .any(|key| flags.truthy(key));
    let mut map = parse_session_selector(rest, flags);
    set_or_remove(
        &mut map,
        "key",
        flag_json(flags, "key")
            .or_else(|| rest_string(rest, if has_flag_selector { 0 } else { 1 })),
    );
    Value::Object(map)
}
