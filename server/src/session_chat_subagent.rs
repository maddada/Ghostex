use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::session_chat::{
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    SessionChatTranscriptAgent,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_tail::{read_session_chat_tail_page, SessionChatTailPage};
use crate::storage::open_gxserver_database;

#[path = "session_chat_subagent_fleet.rs"]
mod fleet;
#[path = "session_chat_subagent_model.rs"]
pub(crate) mod model;

struct ChildTranscript {
    id: String,
    name: String,
    path: PathBuf,
    agent_type: Option<String>,
}

fn codex_child(root: &Path, root_id: &str, selector: &str) -> anyhow::Result<ChildTranscript> {
    let home = root
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "sessions"))
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("Codex transcript is outside its sessions directory."))?;
    let db = Connection::open_with_flags(
        home.join("state_5.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    // CDXC:SessionChat 2026-09-07 WHY:
    // The spawn graph proves a transcript belongs to this conversation. Include closed edges and archived children so completed subagents remain readable.
    let mut query = db.prepare(
        "WITH RECURSIVE descendants(id) AS (
        SELECT child_thread_id FROM thread_spawn_edges WHERE parent_thread_id = ?1
        UNION SELECT edge.child_thread_id FROM thread_spawn_edges edge
        JOIN descendants ON edge.parent_thread_id = descendants.id
    ) SELECT threads.id, threads.rollout_path, threads.source FROM descendants
    JOIN threads ON threads.id = descendants.id WHERE threads.id != ?1",
    )?;
    let rows = query.query_map([root_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut matches = Vec::new();
    for row in rows {
        let (id, path, source) = row?;
        let source: Value = serde_json::from_str(&source)?;
        let spawn = source.pointer("/subagent/thread_spawn");
        let agent_path = spawn
            .and_then(|v| v.get("agent_path"))
            .and_then(Value::as_str);
        let nickname = spawn
            .and_then(|v| v.get("agent_nickname"))
            .and_then(Value::as_str);
        let name = agent_path.or(nickname).unwrap_or(&id).to_string();
        let child = ChildTranscript {
            id: id.clone(),
            name,
            path: PathBuf::from(path),
            agent_type: spawn
                .and_then(|spawn| spawn.get("agent_role"))
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        if id == selector {
            return Ok(child);
        }
        if agent_path == Some(selector)
            || nickname == Some(selector)
            || agent_path.and_then(|p| p.rsplit('/').next()) == Some(selector)
        {
            matches.push(child);
        }
    }
    anyhow::ensure!(
        matches.len() <= 1,
        "Several subagents share this name. Open the agent from its spawn result."
    );
    matches.pop().ok_or_else(|| {
        anyhow::anyhow!("This subagent's transcript is not available in this conversation.")
    })
}

fn safe_agent_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn claude_child(root: &Path, selector: &str) -> anyhow::Result<ChildTranscript> {
    let directory = root.with_extension("").join("subagents");
    if safe_agent_id(selector) {
        let path = directory.join(format!("agent-{selector}.jsonl"));
        if path.is_file() {
            return Ok(ChildTranscript {
                id: selector.to_string(),
                name: selector.to_string(),
                path,
                agent_type: None,
            });
        }
    }
    let mut calls = HashSet::new();
    let mut children = HashMap::new();
    let file = File::open(root)?;
    let length = file.metadata()?.len();
    let mut reader = BufReader::new(file.take(length));
    let mut line = String::new();
    let record_limit = crate::session_chat::MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES;
    while reader
        .by_ref()
        .take(record_limit as u64 + 1)
        .read_line(&mut line)?
        != 0
    {
        anyhow::ensure!(
            line.len() <= record_limit,
            "A transcript record is too large to resolve this subagent by name."
        );
        let record = serde_json::from_str::<Value>(&line).ok();
        line.clear();
        let Some(record) = record else {
            continue;
        };
        if let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) {
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("tool_use")
                    && matches!(
                        block.get("name").and_then(Value::as_str),
                        Some("Agent" | "Task")
                    )
                    && ["name", "description"].iter().any(|key| {
                        block
                            .get("input")
                            .and_then(|v| v.get(key))
                            .and_then(Value::as_str)
                            == Some(selector)
                    })
                {
                    if let Some(id) = block.get("id").and_then(Value::as_str) {
                        calls.insert(id.to_string());
                    }
                }
            }
        }
        if let Some(id) = record
            .pointer("/toolUseResult/agentId")
            .and_then(Value::as_str)
        {
            let matched = record
                .pointer("/message/content")
                .and_then(Value::as_array)
                .is_some_and(|blocks| {
                    blocks.iter().any(|b| {
                        b.get("tool_use_id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| calls.contains(id))
                    })
                });
            if matched {
                children.insert(id.to_string(), selector.to_string());
            }
        }
        if record
            .get("parentToolUseID")
            .and_then(Value::as_str)
            .is_some_and(|id| calls.contains(id))
        {
            if let Some(id) = record.pointer("/data/agentId").and_then(Value::as_str) {
                children.insert(id.to_string(), selector.to_string());
            }
        }
    }
    anyhow::ensure!(
        children.len() <= 1,
        "Several subagents share this name. Open the agent from its spawn result."
    );
    let (id, name) = children
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("This subagent's transcript is not available yet."))?;
    anyhow::ensure!(
        safe_agent_id(&id),
        "Invalid subagent identity in the transcript."
    );
    Ok(ChildTranscript {
        path: directory.join(format!("agent-{id}.jsonl")),
        id,
        name,
        agent_type: None,
    })
}

pub(crate) async fn handle_read_subagent(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    project_id: &str,
    session_id: &str,
    selector: &str,
    limit: usize,
    before_offset: Option<u64>,
) -> RoutedResponse {
    let resolved = (|| {
        let db = open_gxserver_database(&state.paths).map_err(|e| anyhow::anyhow!("{e}"))?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(project_id, session_id)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?
            .ok_or_else(|| anyhow::anyhow!("The session no longer exists."))?;
        let agent = session_chat_agent_for_session(&session);
        let family = resolve_session_chat_transcript_agent(agent.as_deref())
            .filter(|a| {
                matches!(
                    a,
                    SessionChatTranscriptAgent::Codex | SessionChatTranscriptAgent::Claude
                )
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Subagent transcripts are supported for Codex and Claude.")
            })?;
        let root_id = read_runtime_text(&session, "agentSessionId")
            .ok_or_else(|| anyhow::anyhow!("The session has no transcript identity yet."))?;
        let root_path = read_runtime_text(&session, "agentSessionPath");
        Ok::<_, anyhow::Error>((family, root_id, root_path))
    })();
    let selector = selector.trim().to_string();
    let result = tokio::task::spawn_blocking(move || {
        anyhow::ensure!(!selector.is_empty(), "A subagent name or id is required.");
        let (family, root_id, root_path) = resolved?;
        let root = resolve_session_chat_transcript_path(family, Some(&root_id), root_path.as_deref())
            .ok_or_else(|| anyhow::anyhow!("The main transcript is not available yet."))?;
        let selector = if let Some(snapshot) = selector.strip_prefix("fleet:") {
            fleet::resolve_fleet_selector(&root, family, snapshot)?
        } else {
            selector
        };
        let child = match family {
            SessionChatTranscriptAgent::Codex => codex_child(&root, &root_id, &selector)?,
            _ => claude_child(&root, &selector)?,
        };
        match read_session_chat_tail_page(family, &child.path, limit, before_offset)? {
            SessionChatTailPage::NotFound => anyhow::bail!("This subagent's transcript is not available yet."),
            SessionChatTailPage::Page { messages, lifecycle, has_more, before_offset, .. } => {
                let mut details = serde_json::to_value(model::read(&child.path, family)?)?;
                details["id"] = json!(child.id);
                details["name"] = json!(child.name);
                let agent_type = if family == SessionChatTranscriptAgent::Claude {
                    match File::open(child.path.with_extension("meta.json")) {
                        Ok(file) => serde_json::from_reader::<_, Value>(file.take(64 * 1024))?
                            .get("agentType").and_then(Value::as_str).map(str::to_string),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => return Err(error.into()),
                    }
                } else { child.agent_type };
                if let Some(agent_type) = agent_type { details["agentType"] = json!(agent_type); }
                let mut result = json!({
                    "status": if messages.is_empty() { "empty" } else { "ready" },
                    "messages": messages, "hasMore": has_more, "beforeOffset": before_offset,
                    "epoch": 0, "seq": 0,
                    "agent": if family == SessionChatTranscriptAgent::Codex { "codex" } else { "claude" },
                    "agentSessionId": child.id, "subagent": details,
                });
                if let Some(lifecycle) = lifecycle { result["lifecycle"] = serde_json::to_value(lifecycle)?; }
                Ok(result)
            }
        }
    }).await;
    match result {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        error => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "subagentTranscriptUnavailable",
                message: match error {
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => error.to_string(),
                    _ => unreachable!(),
                },
            },
        ),
    }
}
