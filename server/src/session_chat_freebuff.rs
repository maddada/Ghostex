//! CDXC:SessionChat 2026-10-04 DECISION:
//! User: add Freebuff CLI support with simple chat support, "not super deep": sending messages, answering its questions and reading its output in Chat; configuration and similar things stay in the terminal.
//! Freebuff keeps each conversation as one `chat-messages.json` that it rewrites whole (`~/.config/manicode/projects/<folder name>/chats/<chat id>/`), so chat follows a JSONL mirror of that file, the way ZCode's database is followed.
//! SEE-ALSO: server/src/session_chat_decode_freebuff.rs reads the mirror rows; server/src/session_chat_freebuff_question.rs reads and answers its question form.

use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

const MESSAGES_FILE: &str = "chat-messages.json";

/// A mirror's source file and the (size, mtime) it was last mirrored from.
struct Mirror {
    messages: PathBuf,
    synced: Option<(u64, SystemTime)>,
}

static MIRRORS: Mutex<Option<HashMap<PathBuf, Mirror>>> = Mutex::new(None);

/// Freebuff rewrites the whole file and it can grow to many MB, so the follower's every-second
/// drain re-reads it only when its size or mtime moved.
fn messages_stamp(messages: &Path) -> Option<(u64, SystemTime)> {
    let metadata = fs::metadata(messages).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

/// Freebuff's own config directory: `FREEBUFF_CONFIG_DIR` when it is absolute (the CLI refuses a relative one), else `~/.config/manicode`.
pub(crate) fn freebuff_config_dir() -> PathBuf {
    std::env::var("FREEBUFF_CONFIG_DIR")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            crate::resume_lookup::home_dir()
                .join(".config")
                .join("manicode")
        })
}

/// Chat ids are the CLI's start time with `:` replaced (`2026-10-04T06-34-56.232Z`).
pub(crate) fn is_safe_freebuff_chat_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn chat_id_ms(id: &str) -> Option<i64> {
    let (date, time) = id.split_once('T')?;
    let time = time.replacen('-', ":", 2);
    crate::session_status::parse_iso_ms(&format!("{date}T{time}"))
}

fn messages_path_for_chat(id: &str, supplied: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = supplied.filter(|path| {
        path.file_name().and_then(|name| name.to_str()) == Some(MESSAGES_FILE)
            && path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some(id)
    }) {
        return Some(path.to_path_buf());
    }
    let projects = fs::read_dir(freebuff_config_dir().join("projects")).ok()?;
    projects
        .flatten()
        .map(|project| project.path().join("chats").join(id).join(MESSAGES_FILE))
        .filter(|path| path.is_file())
        .max_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
}

pub(crate) fn resolve_freebuff_chat_transcript_path(
    id: &str,
    supplied: Option<&Path>,
) -> Option<PathBuf> {
    if !is_safe_freebuff_chat_id(id) {
        return None;
    }
    let messages = messages_path_for_chat(id, supplied)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    messages.hash(&mut hasher);
    let path = ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("freebuff-chat-mirror")
        .join(format!("{:016x}", hasher.finish()))
        .join(format!("{id}.jsonl"));
    let mut guard = MIRRORS.lock().ok()?;
    let synced = messages_stamp(&messages);
    sync_mirror(&messages, &path)?;
    guard
        .get_or_insert_with(HashMap::new)
        .insert(path.clone(), Mirror { messages, synced });
    Some(path)
}

pub(crate) fn sync_freebuff_transcript_mirror_for_path(path: &Path) {
    let Ok(mut guard) = MIRRORS.lock() else {
        return;
    };
    let Some(mirror) = guard.as_mut().and_then(|map| map.get_mut(path)) else {
        return;
    };
    let stamp = messages_stamp(&mirror.messages);
    if stamp.is_some() && stamp == mirror.synced && path.is_file() {
        return;
    }
    mirror.synced = sync_mirror(&mirror.messages, path).and(stamp);
}

/// How long a Freebuff CLI may take to start after its session row is created.
const FREEBUFF_STARTUP_MS: i64 = 60_000;

/// CDXC:SessionIdentity 2026-10-04 WHY:
/// Freebuff has no hooks and names its chat itself (`--continue <id>` only resumes, and an unknown id silently opens the newest chat), so a new session learns its chat id from the chat folder: the oldest chat of this session's folder that started after the session was created, that no other live session owns, and that no other live Freebuff session of the same folder could have started. gxserver cannot read a process's cwd or start time on Windows, so the session rows' own `cwd` and `createdAt` are the evidence. Freebuff keys the folder by its base name only and writes a chat only after its first message, so an idle older session would otherwise take a newer session's chat; when two sessions could both own a chat, neither takes it and the chat stays in the terminal.
pub(crate) fn discover_freebuff_chat_for_session(
    repository: &crate::domain::DomainRepository<'_>,
    session: &Value,
    sessions: &[Value],
) -> Option<(String, PathBuf)> {
    let (cwd, created_ms) = freebuff_session_context(repository, session)?;
    let folder = cwd.file_name()?.to_os_string();
    let key = |row: &Value| (row.get("projectId").cloned(), row.get("sessionId").cloned());
    let mut claimed = Vec::new();
    let mut rivals = Vec::new();
    for other in sessions {
        if key(other) == key(session) || !crate::agents::is_active_identity_owner(other) {
            continue;
        }
        let bound = crate::presentation::util::read_runtime_text(other, "agentSessionId");
        if other.get("agentId").and_then(Value::as_str) == Some("freebuff") {
            if let Some((other_cwd, other_ms)) = freebuff_session_context(repository, other) {
                if other_cwd.file_name() == Some(folder.as_os_str()) {
                    rivals.push((other_ms, bound.is_some()));
                }
            }
        }
        claimed.extend(bound);
    }
    discover_freebuff_chat(&cwd, created_ms, &rivals, &claimed)
}

/// `rivals` are the other live Freebuff sessions of the same folder: (created at, has a chat).
fn discover_freebuff_chat(
    cwd: &Path,
    since_ms: i64,
    rivals: &[(i64, bool)],
    claimed: &[String],
) -> Option<(String, PathBuf)> {
    let folder = cwd.file_name()?.to_str()?;
    let chats = freebuff_config_dir()
        .join("projects")
        .join(folder)
        .join("chats");
    // The chat id is stamped when the CLI starts, a moment after the session row.
    let could_start = |created_ms: i64, started: i64| created_ms <= started + 2_000;
    fs::read_dir(chats)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_str()?.to_string();
            let started = chat_id_ms(&id)?;
            let messages = entry.path().join(MESSAGES_FILE);
            let contested = rivals.iter().any(|&(rival_ms, rival_bound)| {
                could_start(rival_ms, started)
                    && (rival_ms > since_ms
                        || (!rival_bound && rival_ms + FREEBUFF_STARTUP_MS >= started))
            });
            (is_safe_freebuff_chat_id(&id)
                && could_start(since_ms, started)
                && !contested
                && !claimed.contains(&id)
                && messages.is_file())
            .then_some((started, id, messages))
        })
        .min()
        .map(|(_, id, messages)| (id, messages))
}

/// The folder a session runs in (its own `cwd`, else its project's path) and its creation time.
pub(crate) fn freebuff_session_context(
    repository: &crate::domain::DomainRepository<'_>,
    session: &Value,
) -> Option<(PathBuf, i64)> {
    let project = session
        .get("projectId")
        .and_then(Value::as_str)
        .and_then(|project_id| repository.get_project(project_id).ok().flatten());
    let cwd = crate::session_git_status::effective_session_git_cwd(session, project.as_ref())?;
    let created_ms = session
        .get("createdAt")
        .and_then(Value::as_str)
        .and_then(crate::session_status::parse_iso_ms)?;
    Some((PathBuf::from(cwd), created_ms))
}

fn id_ms(id: &str) -> Option<i64> {
    let digits = id.split('-').nth(1)?;
    (digits.len() >= 12)
        .then(|| digits.parse::<i64>().ok())
        .flatten()
}

fn text_of(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn ask_user_answer_text(block: &Map<String, Value>) -> String {
    if block.get("skipped").and_then(Value::as_bool) == Some(true) {
        return "Skipped".to_string();
    }
    let questions = block
        .get("questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    block
        .get("answers")
        .and_then(Value::as_array)
        .map(|answers| {
            answers
                .iter()
                .filter_map(|answer| {
                    let question = answer.get("questionIndex").and_then(Value::as_u64)? as usize;
                    let mut parts: Vec<String> = Vec::new();
                    if let Some(selected) = text_of(answer.get("selectedOption")) {
                        parts.push(selected.to_string());
                    }
                    if let Some(selected) = answer.get("selectedOptions").and_then(Value::as_array)
                    {
                        parts.extend(
                            selected
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string),
                        );
                    }
                    if let Some(other) = text_of(answer.get("otherText")) {
                        parts.push(other.to_string());
                    }
                    let title = questions
                        .get(question)
                        .and_then(|question| text_of(question.get("question")))
                        .unwrap_or("Answer");
                    Some(format!(
                        "{title}: {}",
                        if parts.is_empty() {
                            "Skipped".to_string()
                        } else {
                            parts.join(", ")
                        }
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// One assistant message's blocks as mirror rows. Subagent blocks become one tool row each, with
/// their final text as the result; their own inner tool calls stay in the terminal.
fn push_block_records(
    append: &mut dyn FnMut(Value),
    message_id: &str,
    blocks: &[Value],
    turn_id: Option<&str>,
    timestamp: Option<i64>,
) {
    for (index, block) in blocks.iter().enumerate() {
        let Some(block) = block.as_object() else {
            continue;
        };
        let id = format!("{message_id}:{index}");
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let Some(text) = text_of(block.get("content")) else {
                    continue;
                };
                let role = if block.get("textType").and_then(Value::as_str) == Some("reasoning") {
                    "reasoning"
                } else {
                    "assistant"
                };
                append(
                    json!({"kind":"message","id":id,"role":role,"text":text,"turnId":turn_id,"timestamp":timestamp}),
                );
            }
            Some("plan") => {
                if let Some(text) = text_of(block.get("content")) {
                    append(
                        json!({"kind":"message","id":id,"role":"assistant","text":text,"turnId":turn_id,"timestamp":timestamp}),
                    );
                }
            }
            Some("tool") => {
                let name = block
                    .get("toolName")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                if block.get("includeToolCall").and_then(Value::as_bool) == Some(false)
                    || name == "end_turn"
                {
                    continue;
                }
                append(
                    json!({"kind":"tool","id":id,"name":name,"input":block.get("input").cloned().unwrap_or(Value::Null),"output":block.get("output"),"turnId":turn_id,"timestamp":timestamp}),
                );
            }
            Some("ask-user") => {
                let questions = block.get("questions").cloned().unwrap_or(Value::Null);
                let answered = block
                    .get("answers")
                    .is_some_and(|answers| !answers.is_null())
                    || block.get("skipped").and_then(Value::as_bool) == Some(true);
                append(
                    json!({"kind":"tool","id":id,"name":"ask_user","input":{"questions":questions},"output":answered.then(|| ask_user_answer_text(block)),"turnId":turn_id,"timestamp":timestamp}),
                );
            }
            Some("agent") => {
                let name = text_of(block.get("agentName"))
                    .or_else(|| text_of(block.get("agentType")))
                    .unwrap_or("Agent");
                let status = block.get("status").and_then(Value::as_str);
                let output =
                    matches!(status, Some("complete" | "failed" | "cancelled")).then(|| {
                        text_of(block.get("content"))
                            .unwrap_or(status.unwrap_or_default())
                            .to_string()
                    });
                append(
                    json!({"kind":"tool","id":id,"name":"spawn_agent","input":{"agent":name,"prompt":block.get("initialPrompt")},"output":output,"isError":status == Some("failed"),"turnId":turn_id,"timestamp":timestamp}),
                );
            }
            _ => {}
        }
    }
}

fn transcript_records(messages: &[Value]) -> String {
    let mut content = String::new();
    let mut append = |record: Value| {
        content.push_str(&record.to_string());
        content.push('\n');
    };
    let mut turn_id: Option<String> = None;
    for message in messages {
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            continue;
        };
        let timestamp = id_ms(id);
        let blocks = message
            .get("blocks")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        match message.get("variant").and_then(Value::as_str) {
            Some("user") => {
                let Some(text) = text_of(message.get("content")) else {
                    continue;
                };
                turn_id = Some(id.to_string());
                append(
                    json!({"kind":"message","id":id,"role":"user","text":text,"turnId":id,"timestamp":timestamp}),
                );
                append(
                    json!({"kind":"lifecycle","state":"working","turnId":id,"timestamp":timestamp}),
                );
            }
            Some("error") => {
                if let Some(text) = text_of(message.get("content")) {
                    append(
                        json!({"kind":"message","id":id,"role":"system","text":text,"turnId":turn_id,"timestamp":timestamp}),
                    );
                }
            }
            Some("ai" | "agent") => {
                if blocks.is_empty() {
                    if let Some(text) = text_of(message.get("content")) {
                        append(
                            json!({"kind":"message","id":id,"role":"assistant","text":text,"turnId":turn_id,"timestamp":timestamp}),
                        );
                    }
                } else {
                    push_block_records(&mut append, id, blocks, turn_id.as_deref(), timestamp);
                }
                let error = text_of(message.get("userError"));
                if let Some(error) = error {
                    append(
                        json!({"kind":"message","id":format!("{id}:error"),"role":"system","text":error,"turnId":turn_id,"timestamp":timestamp}),
                    );
                }
                if id.starts_with("ai-")
                    && message.get("isComplete").and_then(Value::as_bool) == Some(true)
                {
                    let state = if error.is_some() {
                        "interrupted"
                    } else {
                        "completed"
                    };
                    append(
                        json!({"kind":"lifecycle","state":state,"turnId":turn_id.as_deref().unwrap_or(id),"timestamp":timestamp}),
                    );
                }
            }
            _ => {}
        }
    }
    content
}

fn sync_mirror(messages_path: &Path, path: &Path) -> Option<()> {
    // A torn read mid-rewrite fails to parse; the next drain picks up the finished file.
    let raw = fs::read_to_string(messages_path).ok()?;
    let messages: Vec<Value> = serde_json::from_str(&raw).ok()?;
    let content = transcript_records(&messages);
    let previous = fs::read_to_string(path).ok();
    if previous.as_deref() == Some(content.as_str()) {
        return Some(());
    }
    fs::create_dir_all(path.parent()?).ok()?;
    if let Some(previous) = previous.filter(|previous| content.starts_with(previous.as_str())) {
        fs::OpenOptions::new()
            .append(true)
            .open(path)
            .ok()?
            .write_all(content[previous.len()..].as_bytes())
            .ok()?;
    } else {
        let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temp, content).ok()?;
        if fs::rename(&temp, path).is_err() {
            let _ = fs::remove_file(temp);
            return None;
        }
    }
    Some(())
}
