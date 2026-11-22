//! CDXC:SessionChat 2026-09-15 DECISION:
//! User: add ZCode chat support like Codex, using the CLI's actual implementation.
//! ZCode uses Pi's TUI but stores messages and mutable parts in SQLite, not Pi JSONL.
//! Its hook transcript_path is a temporary Claude-compatible export, so chat must follow the database instead.

use crate::domain::DomainRepository;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

static MIRRORS: Mutex<Option<HashMap<PathBuf, PathBuf>>> = Mutex::new(None);

pub(crate) fn is_safe_zcode_session_id(id: &str) -> bool {
    id.starts_with("sess_")
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub(crate) fn zcode_database_path(supplied: Option<&Path>) -> PathBuf {
    if let Some(path) = supplied.filter(|p| {
        matches!(
            p.extension().and_then(|s| s.to_str()),
            Some("sqlite" | "db")
        )
    }) {
        return path.to_path_buf();
    }
    let home = crate::resume_lookup::home_dir();
    for key in ["ZCODE_SESSION_DB_PATH", "ZCODE_SESSION_DB"] {
        if let Ok(path) = std::env::var(key) {
            if !path.trim().is_empty() {
                return crate::resume_lookup::expand_home(path.trim());
            }
        }
    }
    crate::agent_hooks::zcode_settings_files(&home)
        .iter()
        .find_map(|file| {
            fs::read_to_string(file)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|config| {
                    config
                        .pointer("/storage/sessionDbPath")
                        .and_then(Value::as_str)
                        .map(crate::resume_lookup::expand_home)
                })
        })
        .unwrap_or_else(|| home.join(".zcode/cli/db/db.sqlite"))
}

pub(crate) fn resolve_zcode_chat_transcript_path(
    id: &str,
    supplied: Option<&Path>,
) -> Option<PathBuf> {
    if !is_safe_zcode_session_id(id) {
        return None;
    }
    let db = zcode_database_path(supplied);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    db.hash(&mut hasher);
    let path = ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("zcode-chat-mirror")
        .join(format!("{:016x}", hasher.finish()))
        .join(format!("{id}.jsonl"));
    let mut guard = MIRRORS.lock().ok()?;
    sync_mirror(&db, id, &path)?;
    guard
        .get_or_insert_with(HashMap::new)
        .insert(path.clone(), db);
    Some(path)
}

pub(crate) fn sync_zcode_transcript_mirror_for_path(path: &Path) {
    let Ok(guard) = MIRRORS.lock() else {
        return;
    };
    let Some(db) = guard.as_ref().and_then(|map| map.get(path)) else {
        return;
    };
    if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
        sync_mirror(db, id, path);
    }
}

/// Read one consistent database snapshot. Row counts and database mtime alone
/// miss WAL commits and in-place text/tool updates, including same-size edits.
fn transcript_records(connection: &Connection, id: &str) -> rusqlite::Result<String> {
    let mut messages = connection.prepare("SELECT id, data, time_created FROM message WHERE session_id = ?1 ORDER BY sequence, time_created, id")?;
    let mut parts = connection.prepare("SELECT id, data FROM part WHERE session_id = ?1 AND message_id = ?2 ORDER BY sequence, time_created, id")?;
    let mut content = String::new();
    let mut append = |record: Value| {
        content.push_str(&record.to_string());
        content.push('\n');
    };
    for row in messages.query_map([id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })? {
        let (message_id, data, timestamp) = row?;
        let Ok(info) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if info
            .pointer("/semantics/uiVisibility")
            .and_then(Value::as_str)
            == Some("hidden")
        {
            continue;
        }
        if info.get("role").and_then(Value::as_str) == Some("user") {
            append(
                json!({"kind":"lifecycle", "messageId":message_id, "info":info, "timestamp":timestamp}),
            );
        }
        for part in parts.query_map(rusqlite::params![id, message_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (part_id, data) = part?;
            let Ok(part) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            append(
                json!({"kind":"part", "id":part_id, "messageId":message_id, "role":info.get("role"), "turnId":info.pointer("/anchor/turnId"), "timestamp":timestamp, "part":part}),
            );
        }
        if info.get("role").and_then(Value::as_str) == Some("assistant") {
            append(
                json!({"kind":"lifecycle", "messageId":message_id, "info":info, "timestamp":timestamp}),
            );
        }
    }
    Ok(content)
}

fn sync_mirror(db: &Path, id: &str, path: &Path) -> Option<()> {
    let mut connection = ghostex_find::agent_store::open_agent_store(db).ok()?;
    connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .ok()?;
    let transaction = connection.transaction().ok()?;
    let exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM session WHERE id = ?1)",
            [id],
            |row| row.get::<_, bool>(0),
        )
        .ok()?;
    if !exists {
        return None;
    }
    let content = transcript_records(&transaction, id).ok()?;
    transaction.commit().ok()?;
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

/// The `sess_…` conversation id a dead ZCode names in its exit screen's
/// resume hint ("To continue this session, run zcode --resume sess_…"), or
/// `None` when the tail shows no hint (a death before any conversation) or
/// the last hint's argument is not a valid id. Only the argument of the last
/// hint counts: an older hint or any other `sess_…` token on screen may name
/// a different conversation.
pub(crate) fn zcode_resume_session_id_from_screen_tail(tail: &str) -> Option<&str> {
    const HINT: &str = "zcode --resume ";
    let at = tail.rfind(HINT)?;
    tail[at + HINT.len()..]
        .split_whitespace()
        .next()
        .filter(|token| is_safe_zcode_session_id(token))
}

/*
CDXC:AgentScreenDetection 2026-10-06 WHY:
A ZCode session whose hooks never reported its conversation (hooks not
installed or not approved, or the hook write missed) stays unbound after the
agent dies, and Chat View keeps telling the user to install hooks for an agent
that can no longer report anything. The exit screen itself names
the conversation in its resume hint, and that hint is inside every classified
notice's screen tail, so the read path binds the conversation from the same
capture that classified the death — through the same identity pipeline a
hook-reported id takes. Returns whether the row changed.
*/
pub(crate) fn bind_zcode_conversation_from_exit_screen(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    screen_tail: &str,
) -> bool {
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let already_bound = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty());
    if already_bound {
        return false;
    }
    let Some(agent_session_id) = zcode_resume_session_id_from_screen_tail(screen_tail) else {
        return false;
    };
    let mut params = serde_json::Map::new();
    params.insert("agentName".to_string(), json!("zcode"));
    params.insert(
        "agentSessionId".to_string(),
        json!(agent_session_id.to_string()),
    );
    params.insert(
        "agentSessionPath".to_string(),
        json!(zcode_database_path(None).to_string_lossy().to_string()),
    );
    let lifecycle = crate::agents::LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    crate::agents::identity::apply_session_state_update(
        repository,
        &lifecycle,
        &params,
        crate::agents::identity::SessionIdentityUpdateSource::Passive,
    )
    .map(|(result, _)| result.get("changed").and_then(Value::as_bool) == Some(true))
    .unwrap_or(false)
}

/// One call for the read path: bind the conversation the exit screen names,
/// and when the row changed, publish the presentation delta that reopens the
/// sidebar's Chat View gate for this session.
pub(crate) fn bind_zcode_conversation_from_exit_screen_tail(
    state: &crate::server::AppState,
    screen_tail: &str,
    project_id: &str,
    session_id: &str,
) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return false;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if !bind_zcode_conversation_from_exit_screen(&repository, project_id, session_id, screen_tail) {
        return false;
    }
    let _ = crate::server::presentation_delta::schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        project_id,
        session_id,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_wal_updates_rewinds_and_empty_sessions_without_writing_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db.sqlite");
        let path = dir.path().join("mirror.jsonl");
        let connection = Connection::open(&db).unwrap();
        connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE session(id TEXT); CREATE TABLE message(id TEXT, session_id TEXT, data TEXT, time_created INTEGER, sequence INTEGER); CREATE TABLE part(id TEXT, message_id TEXT, session_id TEXT, data TEXT, time_created INTEGER, sequence INTEGER); INSERT INTO session VALUES('sess_test'); INSERT INTO message VALUES('m','sess_test','{\"role\":\"assistant\"}',1,0); INSERT INTO part VALUES('p','m','sess_test','{\"type\":\"text\",\"text\":\"first\"}',1,0);").unwrap();
        sync_mirror(&db, "sess_test", &path).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("first"));
        connection
            .execute(
                "UPDATE part SET data = ?1",
                [r#"{"type":"text","text":"other"}"#],
            )
            .unwrap();
        sync_mirror(&db, "sess_test", &path).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("other"));
        assert!(!updated.contains("first"));
        connection
            .execute_batch("DELETE FROM part; DELETE FROM message;")
            .unwrap();
        sync_mirror(&db, "sess_test", &path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert!(sync_mirror(&db, "sess_missing", &path).is_none());
        assert!(!is_safe_zcode_session_id("sess_../../escape"));
    }
}
