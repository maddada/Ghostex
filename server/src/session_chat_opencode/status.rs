use super::{error, Client, Snapshot};
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

/// The lead's children; `messages` are the lead's own, which say which child tasks finished.
pub(crate) fn fleet(
    id: &str,
    messages: &[Value],
) -> Result<Option<SessionChatAgentFleet>, DomainStateError> {
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
    let agents = roster(
        children,
        &active["data"],
        messages,
        chrono::Utc::now().timestamp_millis(),
    );
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}

fn roster(
    children: &[Value],
    active: &Value,
    messages: &[Value],
    now: i64,
) -> Vec<SessionChatSubAgent> {
    let finished = finished_tasks(messages);
    children
        .iter()
        .filter_map(|child| {
            let id = child["id"].as_str()?;
            let started = child["time"]["created"].as_i64()?;
            let working = active.get(id).is_some();
            Some(SessionChatSubAgent {
                id: Some(id.into()),
                started_at: started,
                working,
                done: !working && finished.contains(id),
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
        .collect()
}

/// CDXC:SessionStatus 2026-11-04 WHY:
/// An idle OpenCode child is done only when the lead's latest `task` call for it completed: a child asking a question or waiting on an approval is idle too, but its task call is still running, and a task resumed with the same child reopens it.
fn finished_tasks(messages: &[Value]) -> std::collections::HashSet<&str> {
    let mut latest = std::collections::HashMap::new();
    for part in messages
        .iter()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .filter(|p| p["type"] == "tool" && p["name"] == "task")
    {
        if let Some(child) = part["state"]["metadata"]["sessionId"].as_str() {
            latest.insert(child, part["state"]["status"] == "completed");
        }
    }
    latest
        .into_iter()
        .filter_map(|(child, completed)| completed.then_some(child))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn child(id: &str) -> Value {
        json!({"id": id, "agent": "general", "title": id, "time": {"created": 1_000, "idle": 31_000}})
    }

    fn task(child: &str, status: &str) -> Value {
        json!({"type": "assistant", "content": [{"type": "tool", "name": "task", "state": {
            "status": status, "input": {"description": child}, "metadata": {"sessionId": child},
        }}]})
    }

    /// GitHub #403: three finished children read "Idle" with stopped clocks.
    #[test]
    fn a_child_whose_task_completed_is_done() {
        let agents = roster(
            &[child("ses_a")],
            &json!({}),
            &[task("ses_a", "completed")],
            60_000,
        );
        assert!(!agents[0].working);
        assert!(agents[0].done);
        assert_eq!(agents[0].elapsed_seconds, Some(30));
    }

    /// A child asking a question is idle in OpenCode, but the lead's task call for it still runs.
    #[test]
    fn a_child_waiting_on_a_question_is_not_done() {
        let agents = roster(
            &[child("ses_a")],
            &json!({}),
            &[task("ses_a", "running")],
            60_000,
        );
        assert!(!agents[0].working);
        assert!(!agents[0].done);
    }

    #[test]
    fn a_resumed_task_reopens_its_child_and_a_working_child_is_never_done() {
        let resumed = roster(
            &[child("ses_a")],
            &json!({}),
            &[task("ses_a", "completed"), task("ses_a", "running")],
            60_000,
        );
        assert!(!resumed[0].done);
        let working = roster(
            &[child("ses_a")],
            &json!({"ses_a": {}}),
            &[task("ses_a", "completed")],
            60_000,
        );
        assert!(working[0].working);
        assert!(!working[0].done);
    }

    #[test]
    fn the_done_flag_is_sent_beside_the_idle_status() {
        let fleet = SessionChatAgentFleet::new(roster(
            &[child("ses_a"), child("ses_b")],
            &json!({}),
            &[task("ses_a", "completed"), task("ses_b", "running")],
            60_000,
        ));
        let value = fleet.to_value();
        assert_eq!(value["agents"][0]["status"], "idle");
        assert_eq!(value["agents"][0]["done"], true);
        assert_eq!(value["agents"][1]["status"], "idle");
        assert!(value["agents"][1].get("done").is_none());
    }
}
