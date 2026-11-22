//! CDXC:AgentProviders 2026-10-06 DECISION:
//! Sven (Empryo harness spec, user story 22): "start an Empryo session with a chosen model and effort, so that it begins on the right lane."
//!
//! CDXC:AgentProviders 2026-10-06 WHY:
//! The spec planned `--model <provider/model> --effort <level>` on the launch line, but Empryo 3.9.0-beta's terminal app ignores both (only `--headless` parses them; measured: `empryo --model subscriptions/gpt-6-sol --effort high` opened on the saved default). Typing the pick through `/models` would save it as Empryo's `defaultModel`, which breaks the session-only launch decision in launch_plan.rs. A tab's model lives in its session folder instead, so the create seeds a new session whose one tab names the model and launches `empryo --session <id>` (verified on a seeded folder: Empryo opened on that model and the config kept its default). The effort is per tab but keyed by Empryo's model family, which Ghostex cannot name, so it rides the session row and the first hook Empryo sends once it is up hands it to the durable model selection queue, which types `/effort <level>` and holds chat messages behind it until Empryo has it.
//! SEE-ALSO: server/src/agents/launch_plan.rs `apply_requested_agent_model`, server/src/agents/fork_empryo.rs (the same folder layout), server/src/session_chat_empryo_picker.rs, server/src/session_chat_model_selection.rs.

use std::path::Path;

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

/// The runtime setting that carries an Empryo launch effort until Empryo is up.
const EMPRYO_LAUNCH_SELECTION_KEY: &str = "empryoLaunchSelection";

/// The model an Empryo create names, refused when it is not `provider/model` or when an effort
/// comes without one.
pub(crate) fn empryo_launch_model(model: Option<&str>) -> Result<&str, DomainStateError> {
    model
        .filter(|model| model.contains('/') && !model.starts_with('/') && !model.ends_with('/'))
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "An Empryo launch model is provider/model, such as subscriptions/gpt-6-luna, and an effort needs one.",
            )
        })
}

/// The model Empryo starts a new tab in `cwd` on: the folder's `.empryo/config.json`
/// `defaultModel`, else the user's. `None` when neither names one.
fn empryo_default_model(cwd: &Path) -> Option<String> {
    let hook_paths =
        crate::agent_hooks::config::HookPaths::from_paths(&crate::paths::get_gxserver_paths(None));
    [
        cwd.join(".empryo"),
        crate::agent_hooks::config::empryo_home(&hook_paths),
    ]
    .iter()
    .find_map(|dir| crate::session_chat_pi_models::empryo_config_default_model(dir))
}

/// Writes `<cwd>/.empryo/sessions/<new id>/` with one tab on `model` (without one, on the folder's
/// default model, [`empryo_default_model`]), the minimum `--session` resumes (a `meta.json` index and
/// the `session.jsonl` log it replays), records the id and log on the session row and returns the
/// id. The log is recorded up front because a window that joins another session's engine never
/// sends hooks of its own (session_chat_empryo_tabs.rs).
pub(crate) fn seed_empryo_launch_session(
    cwd: &Path,
    model: Option<&str>,
    runtime_settings: &mut Map<String, Value>,
) -> Result<String, DomainStateError> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let tab_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let cwd_text = cwd.to_string_lossy();
    let mut tab = json!({ "label": "TAB-1", "forgeMode": "default" });
    // A window that joins another session's engine opens its tab on the tab's own model, and a
    // tab with none drops every prompt (seen live on 3.9.1-beta), so the default is written in.
    if let Some(model) = model
        .map(str::to_string)
        .or_else(|| empryo_default_model(cwd))
    {
        tab["activeModel"] = json!(model);
    }
    let mut meta_tab = tab.clone();
    meta_tab["id"] = json!(tab_id);
    meta_tab["sessionId"] = json!(session_id);
    meta_tab["planMode"] = json!(false);
    let meta = json!({
        "id": session_id,
        "title": "",
        "cwd": cwd_text,
        "startedAt": now,
        "updatedAt": now,
        "activeTabId": tab_id,
        "forgeMode": "default",
        "tabs": [meta_tab],
    });
    let log = format!(
        "{}\n{}\n",
        json!({ "k": "meta", "v": 1, "id": session_id, "cwd": cwd_text, "host": "tui", "startedAt": now, "seq": 1, "ts": now }),
        json!({ "k": "tab", "tabId": tab_id, "patch": tab, "seq": 2, "ts": now }),
    );
    let sessions = cwd.join(".empryo").join("sessions");
    let folder = sessions.join(&session_id);
    let failed = |error: std::io::Error| {
        DomainStateError::bad_request(format!(
            "Could not prepare the Empryo session for its launch model: {error}"
        ))
    };
    std::fs::create_dir_all(&sessions).map_err(failed)?;
    crate::agents::fork_empryo::create_private_dir(&folder).map_err(failed)?;
    let written = crate::agents::fork_empryo::write_private_file(
        &folder.join("session.jsonl"),
        log.as_bytes(),
    )
    .and_then(|()| {
        crate::agents::fork_empryo::write_private_file(
            &folder.join("meta.json"),
            serde_json::to_string_pretty(&meta)
                .unwrap_or_default()
                .as_bytes(),
        )
    });
    if let Err(error) = written {
        // `create_private_dir` refuses an existing folder, so this one holds only this seed.
        let _ = std::fs::remove_dir_all(&folder);
        return Err(failed(error));
    }
    runtime_settings.insert("agentSessionId".to_string(), json!(session_id));
    runtime_settings.insert(
        "agentSessionPath".to_string(),
        json!(folder.join("session.jsonl").to_string_lossy()),
    );
    Ok(session_id)
}

pub(crate) fn record_empryo_launch_effort(
    runtime_settings: &mut Map<String, Value>,
    model: &str,
    effort: &str,
) {
    runtime_settings.insert(
        EMPRYO_LAUNCH_SELECTION_KEY.to_string(),
        json!({ "model": model, "effort": effort }),
    );
}

/// Moves the Empryo launch effort of the session an agent hook reported into the model selection
/// queue; a no-op for every other hook, so the first one Empryo sends once it is up delivers it.
pub(crate) fn queue_empryo_launch_selection_from_hook(
    state: &AppState,
    db: &rusqlite::Connection,
    hook_result: &Value,
) {
    let Some(session) = hook_result.get("session").filter(|session| {
        session
            .get("runtimeSettings")
            .and_then(|runtime| runtime.get(EMPRYO_LAUNCH_SELECTION_KEY))
            .is_some()
    }) else {
        return;
    };
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text(session, "projectId"), text(session, "sessionId"));
    let Some(selection) = take_empryo_launch_selection(state, db, &project_id, &session_id) else {
        return;
    };
    let params = json!({
        "projectId": project_id,
        "sessionId": session_id,
        "model": text(&selection, "model"),
        "effort": text(&selection, "effort"),
    });
    if let Err(error) =
        crate::session_chat_model_selection::enqueue(state, params.as_object().unwrap())
    {
        let _ = state.logger.log(crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Warn,
            event: "empryoLaunchSelectionNotQueued".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(error.message),
            details: Some(params),
        });
    }
}

/// Removes and returns the pick, under the database's write lock so a hook updating the same row
/// at the same moment keeps its change and a second hook finds nothing left to queue.
fn take_empryo_launch_selection(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Option<Value> {
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate).ok()?;
    let repository = DomainRepository::new(&transaction, &state.metadata.server_id);
    let session = repository.get_session(project_id, session_id).ok()??;
    let mut runtime = session.get("runtimeSettings")?.as_object()?.clone();
    let selection = runtime.remove(EMPRYO_LAUNCH_SELECTION_KEY)?;
    repository
        .update_session(
            json!({
                "projectId": project_id,
                "sessionId": session_id,
                "runtimeSettings": runtime,
            })
            .as_object()?,
        )
        .ok()?;
    transaction.commit().ok()?;
    Some(selection)
}
