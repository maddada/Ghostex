use super::{Client, Snapshot, error};
use crate::{
    domain::DomainStateError,
    session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent},
    session_chat_agent_tasks::{SessionChatAgentTask, SessionChatAgentTasks},
    session_chat_options::SessionChatContextUsage,
};
use serde_json::Value;

pub(crate) fn usage(snapshot: &Snapshot) -> Option<SessionChatContextUsage> {
    let message = snapshot
        .messages
        .iter()
        .rev()
        .take_while(|m| !(m["type"] == "compaction" && m["status"] == "completed"))
        .find(|m| m["type"] == "assistant" && m["tokens"].is_object())?;
    let t = &message["tokens"];
    let used = [
        &t["input"],
        &t["output"],
        &t["reasoning"],
        &t["cache"]["read"],
        &t["cache"]["write"],
    ]
    .into_iter()
    .filter_map(Value::as_u64)
    .fold(0u64, u64::saturating_add);
    (used > 0).then_some(SessionChatContextUsage {
        used_tokens: Some(used),
        ..Default::default()
    })
}

pub(crate) fn tasks(snapshot: &Snapshot) -> Option<SessionChatAgentTasks> {
    let todos = snapshot
        .messages
        .iter()
        .rev()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten().rev())
        .find(|p| {
            p["type"] == "tool" && p["name"] == "todowrite" && p["state"]["status"] == "completed"
        })?["state"]["input"]["todos"]
        .as_array()?;
    let tasks = todos
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            Some(SessionChatAgentTask {
                id: t["id"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| i.to_string()),
                subject: t["content"].as_str()?.into(),
                status: t["status"].as_str().unwrap_or("pending").into(),
                active_form: None,
                blocked_by: vec![],
            })
        })
        .collect::<Vec<_>>();
    (!tasks.is_empty()).then_some(SessionChatAgentTasks { tasks })
}

pub(crate) fn fleet(id: &str) -> Result<Option<SessionChatAgentFleet>, DomainStateError> {
    let client = Client::discover()?;
    let active = client.request("GET", "/api/session/active", None)?;
    let response = client.request(
        "GET",
        &format!("/api/session?parentID={id}&limit=200"),
        None,
    )?;
    let children = response["data"]
        .as_array()
        .ok_or_else(|| error("Invalid OpenCode child sessions."))?;
    let now = chrono::Utc::now().timestamp_millis();
    let agents = children
        .iter()
        .filter_map(|child| {
            let id = child["id"].as_str()?;
            let started = child["time"]["created"].as_i64()?;
            let working = active["data"].get(id).is_some();
            Some(SessionChatSubAgent {
                id: Some(id.into()),
                started_at: started,
                working,
                name: child["agent"].as_str().unwrap_or("OpenCode agent").into(),
                task: child["title"].as_str().map(str::to_string),
                model: child["model"]["id"].as_str().map(str::to_string),
                effort: child["model"]["variant"].as_str().map(str::to_string),
                elapsed_seconds: Some(
                    (if working {
                        now
                    } else {
                        child["time"]["idle"].as_i64().unwrap_or(started)
                    })
                    .saturating_sub(started)
                    .max(0) as u64
                        / 1000,
                ),
                tokens: None,
                nested: None,
            })
        })
        .collect::<Vec<_>>();
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}

/// A child must prove its ancestry before a client can open it from the lead's chat.
pub(crate) fn child(root: &str, selector: &str) -> Result<Value, DomainStateError> {
    let client = Client::discover()?;
    let info = client.session(selector, "", "GET", None)?["data"].clone();
    let mut parent = info["parentID"].as_str().map(str::to_string);
    let mut visited = std::collections::HashSet::new();
    for _ in 0..64 {
        let Some(id) = parent else {
            break;
        };
        if id == root {
            return Ok(info);
        }
        if !visited.insert(id.clone()) {
            break;
        }
        parent = client.session(&id, "", "GET", None)?["data"]["parentID"]
            .as_str()
            .map(str::to_string);
    }
    Err(error(
        "This OpenCode subagent does not belong to the current conversation.",
    ))
}
