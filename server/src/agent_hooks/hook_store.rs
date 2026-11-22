use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use super::event_mapping::env_string;
use super::install::{parent_process_id, read_json_object};
use super::probing::{parse_global_session_ref, read_file_text, temp_path_for};

/// CDXC:AgentHooks 2026-09-19 WHY:
/// Every hook event used to rewrite the whole `<agent>-hook-sessions.json` (hundreds of KB, several times per second while agents run) just to bump one `updatedAt`. Readers only need the identity fields and, per surface, which entry is newest, so an event that changes neither writes nothing. Writes that do happen take an exclusive lock because hook processes run concurrently and an unlocked read-modify-write dropped the other process's entry.
pub(super) fn write_hook_store(
    hook_state_dir: &Path,
    agent_key: &str,
    session_id: &str,
    transcript_path: Option<&str>,
    payload: &Value,
) {
    let (global_project_id, global_session_id) = parse_global_session_ref(
        env::var("GHOSTEX_GLOBAL_SESSION_REF")
            .unwrap_or_default()
            .as_str(),
    );
    let workspace_id = env_string("GHOSTEX_WORKSPACE_ID")
        .or_else(|| env_string("VSMUX_WORKSPACE_ID"))
        .or_else(|| env_string("ghostex_WORKSPACE_ID"))
        .or(global_project_id);
    let surface_id = env_string("GHOSTEX_SESSION_ID")
        .or_else(|| env_string("VSMUX_SESSION_ID"))
        .or_else(|| env_string("ghostex_SESSION_ID"))
        .or(global_session_id);
    let (Some(workspace_id), Some(surface_id)) = (workspace_id, surface_id) else {
        return;
    };
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| env_string("GHOSTEX_WORKSPACE_ROOT"))
        .or_else(|| env_string("VSMUX_WORKSPACE_ROOT"))
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .to_string_lossy()
                .to_string()
        });
    let mut entry = Map::new();
    entry.insert("sessionId".to_string(), json!(session_id));
    entry.insert("workspaceId".to_string(), json!(workspace_id));
    entry.insert("surfaceId".to_string(), json!(surface_id));
    entry.insert("cwd".to_string(), json!(cwd));
    entry.insert("transcriptPath".to_string(), json!(transcript_path));
    entry.insert("isRestorable".to_string(), json!(true));
    // The hook's parent is the short-lived wrapper shell, so its pid differs on every event: record it, but never compare it.
    store_entry(
        hook_state_dir,
        agent_key,
        session_id,
        entry,
        parent_process_id(),
    );
}

/// CDXC:AgentHooks 2026-11-02 DECISION:
/// User, asked how to close the gaps that keep Claude's hooks installed: "Everything, including Windows". Resume repair (`agents/resume_plan/claude_identity.rs`) reads every conversation a pane showed from `claude-hook-sessions.json`, which only Claude's hooks wrote. While Track Claude without its hooks is on, the hook-free observer writes the same entry (conversation, project, pane, cwd, transcript path, time) for every conversation it sees a pane show, so the repair works with no hooks.
pub(crate) fn write_claude_surface_record(
    hook_state_dir: &Path,
    conversation: &str,
    workspace_id: &str,
    surface_id: &str,
    cwd: Option<&str>,
    transcript_path: Option<&str>,
    pid: u32,
) {
    let mut entry = Map::new();
    entry.insert("sessionId".to_string(), json!(conversation));
    entry.insert("workspaceId".to_string(), json!(workspace_id));
    entry.insert("surfaceId".to_string(), json!(surface_id));
    entry.insert("cwd".to_string(), json!(cwd.unwrap_or_default()));
    entry.insert("transcriptPath".to_string(), json!(transcript_path));
    entry.insert("isRestorable".to_string(), json!(true));
    store_entry(hook_state_dir, "claude", conversation, entry, pid);
}

fn store_entry(
    hook_state_dir: &Path,
    agent_key: &str,
    session_id: &str,
    mut entry: Map<String, Value>,
    pid: u32,
) {
    let store_path = hook_state_dir.join(format!("{agent_key}-hook-sessions.json"));
    if entry_is_current(&read_store(&store_path), session_id, &entry) {
        return;
    }

    if let Some(parent) = store_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let lock_path = store_path.with_extension("json.lock");
    let Ok(lock_file) = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
    else {
        return;
    };
    // Session metadata is best effort; a contended writer must not stall the agent.
    let lock_deadline = Instant::now() + Duration::from_millis(200);
    loop {
        match lock_file.try_lock() {
            Ok(()) => break,
            Err(fs::TryLockError::WouldBlock) if Instant::now() < lock_deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return,
        }
    }
    // Re-read under the lock: another hook process may have written since the unlocked check.
    let mut data = read_store(&store_path);
    if entry_is_current(&data, session_id, &entry) {
        return;
    }
    entry.insert("pid".to_string(), json!(pid));
    entry.insert("updatedAt".to_string(), json!(now_seconds()));
    let object = data.as_object_mut().expect("object");
    let sessions = object
        .entry("sessions".to_string())
        .or_insert_with(|| json!({}));
    if !sessions.is_object() {
        *sessions = json!({});
    }
    sessions
        .as_object_mut()
        .expect("sessions object")
        .insert(session_id.to_string(), Value::Object(entry));
    object.insert("version".to_string(), json!(1));
    let temp_path = temp_path_for(&store_path);
    if let Ok(text) = serde_json::to_string(&data) {
        let _ = fs::write(&temp_path, format!("{text}\n"));
        let _ = fs::rename(&temp_path, &store_path);
    }
}

fn read_store(store_path: &Path) -> Value {
    let data = read_json_object(&read_file_text(store_path));
    if data.is_object() {
        data
    } else {
        json!({})
    }
}

/// True when the stored entry already carries these identity fields and no other entry on the same surface is newer, which is everything a reader can observe.
fn entry_is_current(data: &Value, session_id: &str, entry: &Map<String, Value>) -> bool {
    let Some(sessions) = data.get("sessions").and_then(Value::as_object) else {
        return false;
    };
    let Some(stored) = sessions.get(session_id).and_then(Value::as_object) else {
        return false;
    };
    if entry
        .iter()
        .any(|(key, value)| stored.get(key) != Some(value))
    {
        return false;
    }
    let updated_at = |session: &Map<String, Value>| {
        session
            .get("updatedAt")
            .and_then(Value::as_f64)
            .unwrap_or_default()
    };
    let stored_updated_at = updated_at(stored);
    let surface_id = entry.get("surfaceId");
    !sessions.iter().any(|(other_id, other)| {
        other_id != session_id
            && other.as_object().is_some_and(|other| {
                other.get("surfaceId") == surface_id && updated_at(other) >= stored_updated_at
            })
    })
}

fn now_seconds() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1000.0
}
