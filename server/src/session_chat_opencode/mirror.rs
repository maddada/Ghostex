use super::{Client, decode_messages, error, safe_id};
use crate::{domain::DomainStateError, session_chat::*, session_chat_options::*};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(crate) struct Snapshot {
    pub(crate) info: Value,
    pub(crate) messages: Vec<Value>,
    pub(crate) permissions: Vec<Value>,
    pub(crate) forms: Vec<Value>,
    pub(crate) working: bool,
}

#[derive(Default)]
struct Cached {
    snapshot: Option<Snapshot>,
    fetched: Option<Instant>,
}

type Cache = Mutex<HashMap<String, Arc<Mutex<Cached>>>>;
static CACHE: OnceLock<Cache> = OnceLock::new();

fn slot(id: &str) -> Arc<Mutex<Cached>> {
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !cache.contains_key(id) && cache.len() >= 32 {
        let oldest = cache
            .iter()
            .filter(|(_, entry)| Arc::strong_count(entry) == 1)
            .filter_map(|(key, entry)| {
                entry
                    .try_lock()
                    .ok()
                    .map(|entry| (key.clone(), entry.fetched))
            })
            .min_by_key(|(_, at)| *at)
            .map(|(key, _)| key);
        if let Some(oldest) = oldest {
            cache.remove(&oldest);
        }
    }
    cache.entry(id.into()).or_default().clone()
}

pub(crate) fn invalidate(id: &str) {
    slot(id).lock().unwrap_or_else(|e| e.into_inner()).fetched = None;
}

pub(crate) fn snapshot(id: &str) -> Result<Snapshot, DomainStateError> {
    if !safe_id(id) {
        return Err(error("Invalid OpenCode session ID."));
    }
    let entry = slot(id);
    let mut cached = entry.lock().unwrap_or_else(|e| e.into_inner());
    if cached
        .fetched
        .is_some_and(|at| at.elapsed() < Duration::from_millis(500))
    {
        if let Some(snapshot) = &cached.snapshot {
            return Ok(snapshot.clone());
        }
    }
    let client = Client::discover()?;
    let info = client.session(id, "", "GET", None)?["data"].clone();
    if info["id"].as_str() != Some(id) {
        return Err(error("OpenCode returned a different session."));
    }
    let messages = client.messages(id)?;
    let permissions = client.session(id, "/permission", "GET", None)?["data"]
        .as_array()
        .cloned()
        .ok_or_else(|| error("Invalid OpenCode permissions response."))?;
    let forms = client.session(id, "/form", "GET", None)?["data"]
        .as_array()
        .cloned()
        .ok_or_else(|| error("Invalid OpenCode forms response."))?;
    let active = client.request("GET", "/api/session/active", None)?;
    let working = active["data"].get(id).is_some()
        || messages
            .iter()
            .any(|m| m["type"] == "shell" && m["status"] == "running");
    let snapshot = Snapshot {
        info,
        messages,
        permissions,
        forms,
        working,
    };
    write_mirror(id, &snapshot)
        .map_err(|e| error(format!("Could not update the OpenCode transcript: {e}")))?;
    cached.snapshot = Some(snapshot.clone());
    cached.fetched = Some(Instant::now());
    Ok(snapshot)
}

pub(crate) fn mirror_path(id: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("opencode-chat-mirror")
        .join(format!("{id}.jsonl"))
}

fn write_mirror(id: &str, snapshot: &Snapshot) -> std::io::Result<()> {
    let mut output = Vec::new();
    let rows = decode_messages(&snapshot.messages);
    for row in &rows {
        serde_json::to_writer(&mut output, row)?;
        output.push(b'\n');
    }
    if let Some(turn) = rows
        .iter()
        .rev()
        .find(|row| row.role == SessionChatRole::User)
    {
        let idle = snapshot.messages.iter().rev().find(|m| m["type"] == "idle");
        serde_json::to_writer(
            &mut output,
            &json!({"opencodeLifecycle": {
                "state": if snapshot.working || snapshot.info["time"]["idle"].as_i64().zip(turn.timestamp).is_none_or(|(idle, sent)|idle < sent) { "working" } else if idle.is_some_and(|m|m["outcome"] == "interrupted") { "interrupted" } else { "completed" },
                "turnId": turn.id, "timestamp": snapshot.info["time"]["idle"].as_i64().or(turn.timestamp)
            }}),
        )?;
        output.push(b'\n');
    }
    let path = mirror_path(id);
    let previous = fs::read(&path).unwrap_or_default();
    if previous == output && path.is_file() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().unwrap())?;
    // Mutable parts and revert alter earlier rows. Atomic replacement makes the existing
    // follower publish one replacement snapshot instead of duplicating streaming text.
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    temp.write_all(&output)?;
    temp.persist(&path).map_err(|e| e.error)?;
    Ok(())
}

pub(crate) fn resolve_transcript(id: &str) -> Option<PathBuf> {
    if !safe_id(id) {
        return None;
    }
    let path = mirror_path(id);
    if snapshot(id).is_ok() || path.is_file() {
        Some(path)
    } else {
        None
    }
}

pub(crate) fn refresh_for_path(path: &Path) {
    if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
        let _ = snapshot(id);
    }
}

pub(crate) fn detect(session: &Value) -> SessionChatTerminalDetection {
    let id = match super::session_id(session) {
        Ok(id) => id,
        Err(e) => return unavailable(&e.message),
    };
    let snapshot = match snapshot(&id) {
        Ok(snapshot) => snapshot,
        Err(e) => return unavailable(&e.message),
    };
    let choice = |value: &str, label: &str| SessionChatDetectedChoice {
        value: value.into(),
        label: label.into(),
        source: SessionChatOptionEvidence::Transcript,
    };
    let catalog = Client::discover()
        .and_then(|client| super::model_catalog(&client, &snapshot.info))
        .ok();
    let last_reply = snapshot
        .messages
        .iter()
        .rev()
        .find(|message| message["type"] == "assistant");
    let model = snapshot
        .info
        .get("model")
        .or_else(|| last_reply.and_then(|m| m.get("model")))
        .unwrap_or(&Value::Null);
    let mut context_usage = super::usage(&snapshot);
    if let Some(usage) = context_usage.as_mut() {
        let value = format!(
            "{}/{}",
            model["providerID"].as_str().unwrap_or(""),
            model["id"].as_str().unwrap_or("")
        );
        usage.window_size = catalog
            .as_ref()
            .and_then(|c| c["agents"]["opencode"]["models"].as_array())
            .and_then(|rows| rows.iter().find(|row| row["value"] == value))
            .and_then(|row| row["contextWindow"].as_u64())
            .filter(|size| *size > 0);
        usage.used_percentage = usage
            .used_tokens
            .zip(usage.window_size)
            .map(|(used, size)| {
                ((used as f64 / size as f64 * 100.0).round()).clamp(0.0, 100.0) as u32
            });
    }
    let selection = SessionChatDetectedSelection {
        model_catalog: catalog,
        context_usage,
        model: model["providerID"]
            .as_str()
            .zip(model["id"].as_str())
            .map(|(provider, id)| choice(&format!("{provider}/{id}"), id)),
        effort: model["variant"]
            .as_str()
            .map(|variant| choice(variant, variant)),
        mode: snapshot.info["agent"]
            .as_str()
            .or_else(|| last_reply.and_then(|m| m["agent"].as_str()))
            .map(|mode| choice(mode, mode)),
        ..Default::default()
    };
    SessionChatTerminalDetection {
        captured: true,
        attempted: true,
        notice: (!snapshot.forms.is_empty() && super::interactive_prompt(&snapshot).is_none())
            .then(|| {
                handoff(
                    "Complete this OpenCode form in the terminal",
                    "This form includes conditional fields or an external sign-in step.",
                )
            }),
        tasks: super::tasks(&snapshot),
        fleet: super::fleet(&id).ok().flatten(),
        fleet_observed: true,
        options: Some(SessionChatDetectedOptions {
            selection,
            detected_at: chrono::Utc::now().to_rfc3339(),
        }),
        prompt: super::interactive_prompt(&snapshot),
        ..Default::default()
    }
}

fn handoff(title: &str, detail: &str) -> crate::session_chat_notice::SessionChatTerminalNotice {
    use crate::session_chat_notice::*;
    SessionChatTerminalNotice::new(
        "opencodeInputBlocked",
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Screen,
        title,
    )
    .with_detail(detail)
    .with_input_blocking(true)
    .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
        "Open terminal",
    )])
}

fn unavailable(detail: &str) -> SessionChatTerminalDetection {
    SessionChatTerminalDetection {
        attempted: true,
        captured: true,
        notice: Some(handoff("OpenCode chat is not connected", detail)),
        ..Default::default()
    }
}
