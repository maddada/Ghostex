use super::{decode_messages, error, safe_id, Client};
use crate::{domain::DomainStateError, session_chat::*, session_chat_options::*};
use serde_json::{json, Value};
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
    /// `events::follow` when this snapshot was read.
    changes: u64,
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
            super::events::forget(&oldest);
        }
    }
    cache.entry(id.into()).or_default().clone()
}

pub(crate) fn invalidate(id: &str) {
    slot(id).lock().unwrap_or_else(|e| e.into_inner()).fetched = None;
}

/// How long a read stays current. While the event stream is connected it reports every change,
/// so the limit only bounds the cost of an event kind this reader does not know.
fn freshness() -> Duration {
    if super::events::connected() {
        Duration::from_secs(15)
    } else {
        Duration::from_millis(500)
    }
}

pub(crate) fn snapshot(id: &str) -> Result<Snapshot, DomainStateError> {
    if !safe_id(id) {
        return Err(error("Invalid OpenCode session ID."));
    }
    super::events::ensure_started();
    let changes = super::events::follow(id);
    let entry = slot(id);
    let mut cached = entry.lock().unwrap_or_else(|e| e.into_inner());
    if cached.changes == changes && cached.fetched.is_some_and(|at| at.elapsed() < freshness()) {
        if let Some(snapshot) = cached.snapshot.clone() {
            let (live, changed) = super::events::live(id, None);
            if changed {
                write_mirror(id, &snapshot, &live)
                    .map_err(|e| error(format!("Could not update the OpenCode transcript: {e}")))?;
            }
            return Ok(snapshot);
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
    let (live, _) = super::events::live(id, Some(&snapshot.messages));
    write_mirror(id, &snapshot, &live)
        .map_err(|e| error(format!("Could not update the OpenCode transcript: {e}")))?;
    cached.snapshot = Some(snapshot.clone());
    cached.fetched = Some(Instant::now());
    // Read before the requests, so a change reported while they ran makes the next read refetch.
    cached.changes = changes;
    Ok(snapshot)
}

pub(crate) fn mirror_path(id: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("opencode-chat-mirror")
        .join(format!("{id}.jsonl"))
}

fn write_mirror(
    id: &str,
    snapshot: &Snapshot,
    live: &super::events::LiveText,
) -> std::io::Result<()> {
    let mut output = Vec::new();
    let rows = if live.is_empty() {
        decode_messages(&snapshot.messages)
    } else {
        decode_messages(&live.apply(&snapshot.messages))
    };
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
        Err(_) if just_created(session) => {
            return problem(session).unwrap_or(SessionChatTerminalDetection {
                attempted: true,
                captured: true,
                ..Default::default()
            })
        }
        Err(e) => return problem(session).unwrap_or_else(|| unavailable(&e.message)),
    };
    let snapshot = match snapshot(&id) {
        Ok(snapshot) => snapshot,
        Err(e) => return problem(session).unwrap_or_else(|| unavailable(&e.message)),
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
    let model = detected_model(&snapshot.info, &snapshot.messages, catalog.as_ref());
    let mut context_usage = super::usage(&snapshot);
    if let Some(usage) = context_usage.as_mut() {
        usage.window_size = model
            .as_ref()
            .and_then(|model| catalog_row(catalog.as_ref(), &model.value))
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
        effort: model
            .as_ref()
            .and_then(|model| model.variant.as_deref())
            .map(|variant| choice(variant, variant)),
        model: model.map(|model| choice(&model.value, &model.label)),
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
        fleet: super::fleet(&id, &snapshot.messages).ok().flatten(),
        fleet_observed: true,
        options: Some(SessionChatDetectedOptions {
            selection,
            detected_at: chrono::Utc::now().to_rfc3339(),
        }),
        prompt: super::interactive_prompt(&snapshot),
        composer: composer(&snapshot),
        ..Default::default()
    }
}

pub(crate) struct DetectedModel {
    /// `provider/model`, the value the picker and `select` use.
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) variant: Option<String>,
}

fn catalog_row<'a>(catalog: Option<&'a Value>, value: &str) -> Option<&'a Value> {
    catalog?["agents"]["opencode"]["models"]
        .as_array()?
        .iter()
        .find(|row| row["value"] == value)
}

/// CDXC:SessionChat 2026-10-25 WHY:
/// User: "im using opencode and im using deepseek model but it's saying a different model in the chat view composer". The model is the conversation's own selection (`session.model`, set by every switch from the TUI, the picker or `/model`), else the model of its latest reply; a session that has neither has no model yet and the pill names OpenCode instead of guessing a default. Only an object counts, so a `null` selection falls through to the reply. The label is the catalog's display name for that exact provider and model; a model the catalog does not list (another provider's DeepSeek, a local model) shows its own id rather than any other model's name.
pub(crate) fn detected_model(
    info: &Value,
    messages: &[Value],
    catalog: Option<&Value>,
) -> Option<DetectedModel> {
    let reference = |model: &Value| -> Option<(String, String, Option<String>)> {
        let provider = model["providerID"].as_str().filter(|s| !s.is_empty())?;
        let id = model["id"]
            .as_str()
            .or_else(|| model["modelID"].as_str())
            .filter(|s| !s.is_empty())?;
        Some((
            format!("{provider}/{id}"),
            id.to_string(),
            model["variant"].as_str().map(str::to_string),
        ))
    };
    let (value, id, variant) = reference(&info["model"]).or_else(|| {
        messages
            .iter()
            .rev()
            .filter(|message| message["type"] == "assistant")
            .find_map(|message| reference(&message["model"]))
    })?;
    let label = catalog_row(catalog, &value)
        .and_then(|row| row["label"].as_str())
        .filter(|label| !label.trim().is_empty())
        .map(str::to_string)
        .unwrap_or(id);
    Some(DetectedModel {
        value,
        label,
        variant: variant.filter(|variant| variant != "default"),
    })
}

/// CDXC:SessionChat 2026-10-20 WHY:
/// OpenCode takes chat sends through its server API, so its input box is "ready" once the conversation answers idle with no permission or form waiting. A new chat's first message (and an orchestrator's skill line) is a startup send that waits for a Ready composer; this reading replaces the screen capture for OpenCode, and leaving it Unknown held those sends in the queue forever.
fn composer(snapshot: &Snapshot) -> crate::session_chat_composer::SessionChatComposerReadiness {
    use crate::session_chat_composer::{SessionChatComposerReadiness, SessionChatComposerState};
    let mut composer = SessionChatComposerReadiness::default();
    if !snapshot.working && snapshot.permissions.is_empty() && snapshot.forms.is_empty() {
        composer.state = SessionChatComposerState::Ready;
    }
    composer
}

/// CDXC:SessionChat 2026-10-20 WHY:
/// A new OpenCode TUI has no conversation until Ghostex's plugin creates one a few seconds after it starts (`ghostex-v2/tui.js`), so for its first minute a missing conversation is a start, not a screen someone must fix. Showing "OpenCode chat is not connected" then made an orchestrator report a fresh OpenCode thread as waiting for an answer.
fn just_created(session: &Value) -> bool {
    session["createdAt"]
        .as_str()
        .and_then(|created| chrono::DateTime::parse_from_rfc3339(created).ok())
        .is_some_and(|created| {
            chrono::Utc::now().signed_duration_since(created) < chrono::Duration::seconds(60)
        })
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

/// Why this session's chat cannot connect, when Ghostex can tell (agent_cli/opencode.rs): an OpenCode
/// before 2, an install that cannot start, none at all, or the OpenCode 2 install it started.
fn problem(session: &Value) -> Option<SessionChatTerminalDetection> {
    use crate::agent_cli::opencode::{self, InstallState, Problem};
    use crate::session_chat_notice::{
        SessionChatTerminalNoticeAction, SessionChatTerminalNoticeActionKind,
    };
    let action = |id: &str, label: &str| SessionChatTerminalNoticeAction {
        id: id.into(),
        label: label.into(),
        // Rides the answer every chat client already sends (session_chat_agent_restart.rs).
        kind: SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    };
    let installable = opencode::install_command().is_some();
    let install = |label: &str| {
        installable
            .then(|| action(opencode::ACTION_INSTALL, label))
            .into_iter()
            .collect::<Vec<_>>()
    };
    let no_installer = if installable {
        ""
    } else {
        " Install OpenCode 2 from Settings > Agents, then restart this session."
    };
    let (title, detail, actions) = match opencode::install_state() {
        Some(InstallState::Running) => (
            "Installing OpenCode 2…".to_string(),
            "Ghostex is running OpenCode's official installer and restarts this session on OpenCode 2 when it finishes.".to_string(),
            Vec::new(),
        ),
        state => {
            let home = crate::resume_lookup::home_dir();
            let zmx_name = session["zmxName"].as_str();
            let problem = opencode::diagnose_cached(zmx_name, &home)?;
            let restart = opencode::launch_executable(&home).is_ok();
            let mut found = match &problem {
                Problem::Older { version, path } if restart => (
                    format!("OpenCode {version} isn't supported"),
                    format!("This session runs OpenCode {version} ({path}). Ghostex's chat works with OpenCode 2, which is installed too. Restart this session on OpenCode 2 to use the chat."),
                    vec![action(opencode::ACTION_RESTART, "Restart on OpenCode 2")],
                ),
                Problem::Older { version, path } => (
                    format!("OpenCode {version} isn't supported"),
                    format!("This session runs OpenCode {version} ({path}). Ghostex's chat works with OpenCode 2 only. Install OpenCode 2 and Ghostex restarts this session on it; OpenCode {version} stays installed.{no_installer}"),
                    install("Install OpenCode 2"),
                ),
                Problem::Unusable { path } => (
                    "OpenCode can't start".to_string(),
                    format!("{path} is installed but doesn't start. An npm install with ignore-scripts turned on skips the step that downloads OpenCode 2's program. Install OpenCode 2 with its official installer and Ghostex restarts this session on it.{no_installer}"),
                    install("Install OpenCode 2"),
                ),
                Problem::Missing => (
                    "OpenCode 2 isn't installed".to_string(),
                    format!("Install OpenCode 2 and Ghostex restarts this session on it.{no_installer}"),
                    install("Install OpenCode 2"),
                ),
            };
            if let Some(InstallState::Failed(error)) = state {
                found = (
                    "OpenCode 2 could not be installed".to_string(),
                    format!("The official installer failed: {error}"),
                    install("Try again"),
                );
            }
            found
        }
    };
    let mut actions = actions;
    actions.push(SessionChatTerminalNoticeAction::switch_to_terminal(
        "Open terminal",
    ));
    Some(SessionChatTerminalDetection {
        attempted: true,
        captured: true,
        notice: Some(handoff(&title, &detail).with_actions(actions)),
        ..Default::default()
    })
}

fn unavailable(detail: &str) -> SessionChatTerminalDetection {
    SessionChatTerminalDetection {
        attempted: true,
        captured: true,
        notice: Some(handoff("OpenCode chat is not connected", detail)),
        ..Default::default()
    }
}
