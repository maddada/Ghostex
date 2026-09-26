use crate::ghostex_cli::{
    args::Flags,
    rpc::{self, CliError, CliResult},
    sessions,
};
use serde_json::{json, Value};

pub(super) fn text<'a>(session: &'a Value, key: &str) -> &'a str {
    session
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

pub(super) fn is_agent(session: &Value) -> bool {
    !text(session, "agentId").is_empty()
}

pub(super) fn inventory_flags(flags: &Flags, reference: &str) -> CliResult<Flags> {
    let mut flags = flags.clone();
    if rpc::is_gxserver_global_session_ref(reference) {
        let target = rpc::resolve_gxserver_server_target(&flags, &json!({"globalRef": reference}))?;
        if target.kind == "local" {
            flags.insert_text("server", "local");
        } else if let Some(profile) = target.profile_id {
            flags.insert_text("server", &profile);
        } else {
            return Err(CliError::Other(format!(
                "No connection profile for {reference}."
            )));
        }
    }
    Ok(flags)
}

/// CDXC:SessionIdentity 2026-09-17 DECISION:
/// User: agents must obtain their own session and agent identifiers from the CLI for message headers. Resolve exact environment identifiers, never the focused pane or a matching title.
pub(super) fn caller() -> CliResult<Value> {
    let (key, reference) = ["GHOSTEX_GLOBAL_SESSION_REF", "GHOSTEX_NATIVE_SESSION_ID", "GHOSTEX_SESSION_ID", "ZMX_SESSION"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()).map(|value| (key, value.trim().to_owned())))
        .ok_or_else(|| CliError::Other("Cannot identify the caller. Run inside a Ghostex agent session with GHOSTEX_GLOBAL_SESSION_REF or GHOSTEX_SESSION_ID set.".into()))?;
    let flags = inventory_flags(&Flags::default(), &reference)?;
    let rows = sessions::fetch_session_list(&flags, false)?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| {
            if key == "GHOSTEX_GLOBAL_SESSION_REF" {
                text(row, "globalRef") == reference
            } else if key == "GHOSTEX_NATIVE_SESSION_ID" {
                format!("{}:{}", text(row, "projectId"), text(row, "sessionId")) == reference
            } else {
                ["sessionId", "globalRef", "providerSessionName"]
                    .iter()
                    .any(|field| text(row, field) == reference)
            }
        })
        .collect();
    if matches.len() != 1 || !is_agent(matches[0]) || text(matches[0], "globalRef").is_empty() {
        return Err(CliError::Other(format!("Cannot identify one agent session from {key}={reference}. Run ghostex sessions --json to inspect the session; identity was not guessed.")));
    }
    let mut caller = matches[0].clone();
    resolve_names(std::slice::from_mut(&mut caller), &flags);
    Ok(caller)
}

pub(crate) fn resolve_names(rows: &mut [Value], flags: &Flags) {
    if let Ok(hud) = rpc::call_gxserver_rpc("/api/readSidebarHud", &json!({}), flags) {
        if let Some(agents) = hud["agents"].as_array() {
            for row in rows {
                if let Some(agent) = agents
                    .iter()
                    .find(|agent| text(agent, "agentId") == text(row, "agentId"))
                {
                    if !text(agent, "name").is_empty() {
                        row["agentName"] = agent["name"].clone();
                    }
                }
            }
        }
    }
}

pub(super) fn summary(row: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "globalRef",
        "sessionId",
        "title",
        "projectId",
        "projectName",
        "projectPath",
        "agentName",
        "agentId",
        "agentSessionId",
        "activity",
        "lifecycleState",
    ] {
        result.insert(key.to_owned(), row.get(key).cloned().unwrap_or(Value::Null));
    }
    if text(row, "agentName").is_empty() {
        result.insert("agentName".into(), json!(text(row, "agentId")));
    }
    Value::Object(result)
}

fn header_value(row: &Value, key: &str) -> String {
    let value = text(row, key);
    if value.is_empty() {
        return "unavailable".into();
    }
    value
        .chars()
        .map(|c| {
            if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// CDXC:Cli 2026-09-17 DECISION:
/// User: prepend the sender's CLI-resolved identity to every agent message. Assemble the header before enqueueing so delayed delivery retains the original sender.
/// CDXC:Cli 2026-09-18 DECISION:
/// User: the header must not render as a heading. The old `MESSAGE FROM` header ended in a dashed line, which Markdown reads as a setext underline, so the chat turned the whole header into an h2. A blank line now separates header and body.
/// SEE-ALSO: packages/gx-chat-core/src/transcript/agent_message.rs parses this header (and the old dashed one) into the chat's message card.
pub(super) fn message(sender: &Value, body: &str) -> String {
    let sender = summary(sender);
    format!("Message from another agent\nAgent: {}\nSession: {}\nSession ID: {}\nAgent ID: {}\nAgent Session ID: {}\nReply to: {}\n\n{}",
        header_value(&sender, "agentName"), header_value(&sender, "title"), header_value(&sender, "sessionId"),
        header_value(&sender, "agentId"), header_value(&sender, "agentSessionId"), header_value(&sender, "globalRef"), body)
}
