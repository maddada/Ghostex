//! CDXC:SessionStatus 2026-09-12 DECISION:
//! User: always show attention for unanswered asynchronous questions, including while the agent keeps working.
//! Observe every running Codex session independently of chat subscriptions. Pending questions are separate from agentActivity so they cannot interrupt work or trigger completion handling.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::OptionalExtension;
use serde_json::{json, Value};

use crate::domain::DomainRepository;
use crate::server::{read_runtime_text, AppState};
use crate::session_chat::*;
use crate::storage::open_gxserver_database;

#[derive(Default)]
struct QuestionCursor {
    path: PathBuf,
    version: Option<TranscriptFileVersion>,
    started_at: Option<i64>,
    boundary: String,
    incremental: SessionChatIncrementalState,
    seen: HashSet<String>,
    pending: Vec<(String, String)>,
}

pub(crate) fn answer_prefix(title: &str) -> String {
    let mut end = title.len().min(512);
    while !title.is_char_boundary(end) {
        end -= 1;
    }
    format!("> {}\n\n", title[..end].replace(['\r', '\n'], " "))
}

fn observe_messages(
    seen: &mut HashSet<String>,
    pending: &mut Vec<(String, String)>,
    messages: Vec<SessionChatMessage>,
    started_at: Option<i64>,
) {
    for message in messages {
        if message.role == SessionChatRole::Assistant {
            if message
                .timestamp
                .zip(started_at)
                .is_some_and(|(at, start)| at < start)
            {
                continue;
            }
            for (index, question) in message
                .async_questions
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                let key = format!("{}:{index}", message.id);
                if seen.insert(key.clone()) {
                    pending.push((key, answer_prefix(&question.title)));
                }
            }
        } else if message.role == SessionChatRole::User {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    SessionChatBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut matching = pending
                .iter()
                .enumerate()
                .filter(|(_, (_, prefix))| text.starts_with(prefix));
            // CDXC:SessionChat 2026-10-04 WHY:
            // A framed reply loses its canonical question ID in the decoder. Equal prefixes cannot identify a card; the verified answer endpoint retires its exact ID separately. Keep dismissed entries in this ambiguity check so replay cannot retire a sibling.
            if let Some((index, _)) = matching.next() {
                if matching.next().is_none() {
                    pending.remove(index);
                }
            }
        }
    }
}

pub(crate) fn spawn_async_question_status_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut cursors = HashMap::new();
        loop {
            let state = state.clone();
            cursors = tokio::task::spawn_blocking(move || {
                refresh(&state, &mut cursors);
                cursors
            })
            .await
            .unwrap_or_default();
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        }
    })
}

fn refresh(state: &AppState, cursors: &mut HashMap<String, QuestionCursor>) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(sessions) = repository.running_sessions() else {
        return;
    };
    let mut active = HashSet::new();
    for session in sessions.iter() {
        if resolve_session_chat_transcript_agent(
            crate::session_chat_follower::session_chat_agent_for_session(&session).as_deref(),
        ) != Some(SessionChatTranscriptAgent::Codex)
        {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        let key = crate::server::session_observer_key(project_id, session_id);
        active.insert(key.clone());
        let Some(path) = resolve_session_chat_transcript_path(
            SessionChatTranscriptAgent::Codex,
            read_runtime_text(&session, "agentSessionId").as_deref(),
            read_runtime_text(&session, "agentSessionPath").as_deref(),
        ) else {
            continue;
        };
        let Ok(version) = read_transcript_file_version(&path) else {
            continue;
        };
        let started_at = record_process_run_start(state, &session, project_id, session_id);
        let cursor = cursors.entry(key).or_default();
        if cursor.path != path
            || cursor.started_at != started_at
            || cursor.version.as_ref().is_some_and(|old| {
                old.identity != version.identity
                    || version.size < cursor.incremental.offset
                    || (old != &version
                        && boundary_fingerprint(&path, cursor.incremental.offset)
                            .ok()
                            .as_deref()
                            != Some(&cursor.boundary))
            })
        {
            *cursor = QuestionCursor {
                path: path.clone(),
                started_at,
                ..Default::default()
            };
        }
        if cursor.version.as_ref() != Some(&version) {
            // Stream the initial history once, then only appended bytes. A bounded tail could lose a question during a long-running turn.
            let mut on_batch = |messages| {
                observe_messages(&mut cursor.seen, &mut cursor.pending, messages, started_at)
            };
            let Ok(messages) = read_incremental_transcript_messages(
                &path,
                &mut cursor.incremental,
                decode_codex_transcript_line,
                Some(&mut on_batch),
                None,
                None,
                None,
            ) else {
                continue;
            };
            on_batch(messages);
            cursor.boundary =
                boundary_fingerprint(&path, cursor.incremental.offset).unwrap_or_default();
            cursor.version = Some(version);
        }
        let dismissed = retired_question_ids(&session);
        let candidates: Vec<_> = cursor
            .pending
            .iter()
            .filter(|(id, _)| !dismissed.contains(id))
            .collect();
        if !candidates.is_empty() {
            if let Ok(capture) =
                crate::zmx::read_zmx_session_history_capture(&repository, project_id, session_id)
            {
                if !capture.truncated {
                    for preview in
                        crate::session_chat_notice::codex_queued_input_previews(&capture.text)
                    {
                        let matching: Vec<_> = cursor
                            .pending
                            .iter()
                            .filter(|(_, prefix)| {
                                let prefix =
                                    prefix.split_whitespace().collect::<Vec<_>>().join(" ");
                                preview
                                    .strip_prefix(&prefix)
                                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
                                    || (preview.chars().count() >= 12
                                        && prefix.starts_with(&preview))
                            })
                            .collect();
                        // A clipped preview must identify exactly one question before retiring it.
                        if matching.len() == 1 && !dismissed.contains(&matching[0].0) {
                            let _ = dismiss(state, project_id, session_id, &matching[0].0);
                        }
                    }
                }
            }
        }
        let ids: Vec<&str> = cursor.pending.iter().map(|(id, _)| id.as_str()).collect();
        if session
            .pointer("/runtimeSettings/sessionChatAsyncQuestionIds")
            .cloned()
            .unwrap_or(json!([]))
            != json!(ids)
        {
            let _ = publish_pending(state, project_id, session_id, &ids);
        }
    }
    cursors.retain(|key, _| active.contains(key));
}

fn broadcast(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let _sequence = state
        .presentation_event_sequence
        .lock()
        .map_err(|_| anyhow::anyhow!("Presentation lock unavailable"))?;
    let repository = DomainRepository::new(db, &state.metadata.server_id);
    let delta = crate::presentation::build_presentation_session_delta(
        db,
        &repository,
        project_id,
        session_id,
    )?;
    let revision = crate::presentation::increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "delta": delta, "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
        "revision": revision, "serverId": state.metadata.server_id, "type": "presentationDelta",
    }));
    drop(_sequence);
    if let Some(session) = repository.get_session(project_id, session_id)? {
        crate::session_chat_interactive::emit_session_chat_prompt_state_frame(state, &session);
    }
    Ok(())
}

/// CDXC:SessionChat 2026-09-17 WHY:
/// Codex's unanswered async question UI belongs to its current process run. Replaying questions from before startup/resume resurrected a card that was no longer present in the CLI.
/// Turn completion and compaction do not start a new run, so questions still pending there remain answerable.
pub(crate) fn async_questions_since(session: &Value) -> Option<i64> {
    parse_started_at(read_runtime_text(session, "sessionChatCodexStartedAt"))
}

/// CDXC:SessionChat 2026-09-24 WHY:
/// The SessionStart hook is not a reliable run boundary: most running Codex sessions had never recorded one, and a thread resumed in another session kept none, so its old questions came back as cards and Skip failed with "Codex is not showing its pending questions". The live Codex process start is the run boundary; a later hook time (a /clear in the same process) still wins.
fn record_process_run_start(
    state: &AppState,
    session: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<i64> {
    let stored = async_questions_since(session);
    let Ok((_, started)) = crate::session_chat_fleet_process::current_process(session, "codex")
    else {
        return stored;
    };
    if stored.is_some_and(|at| at >= started) {
        return stored;
    }
    let at = chrono::DateTime::from_timestamp_millis(started)?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let changed = db.execute(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatCodexStartedAt', ?3)
            WHERE projectId = ?1 AND sessionId = ?2"#,
            rusqlite::params![project_id, session_id, at],
        );
        if changed.is_ok_and(|changed| changed > 0) {
            let _ = broadcast(state, &db, project_id, session_id);
        }
    }
    Some(started)
}

pub(crate) fn read_async_questions_since(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Option<i64> {
    let value = db.query_row(
        "SELECT json_extract(runtimeSettingsJson, '$.sessionChatCodexStartedAt') FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id, session_id], |row| row.get::<_, Option<String>>(0),
    ).optional().ok().flatten().flatten();
    parse_started_at(value)
}

fn parse_started_at(value: Option<String>) -> Option<i64> {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
        .map(|value| value.timestamp_millis())
}

fn publish_pending(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    ids: &[&str],
) -> anyhow::Result<()> {
    let db = open_gxserver_database(&state.paths)?;
    // Dismissals live in a separate member and are filtered during projection, so a concurrent Skip cannot be undone by this pass.
    let changed = db.execute(
        r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.sessionChatAsyncQuestionIds', json(?3))
        WHERE projectId = ?1 AND sessionId = ?2
        AND coalesce(json_extract(runtimeSettingsJson, '$.sessionChatAsyncQuestionIds'), '[]') != ?3"#,
        rusqlite::params![project_id, session_id, serde_json::to_string(ids)?],
    )?;
    if changed > 0 {
        broadcast(state, &db, project_id, session_id)?;
    }
    Ok(())
}

pub(crate) fn dismiss(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    question_id: &str,
) -> anyhow::Result<()> {
    let db = open_gxserver_database(&state.paths)?;
    let changed = db.execute(
        r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson,
            '$.sessionChatAsyncQuestionsDismissed', json_insert(coalesce(json_extract(runtimeSettingsJson, '$.sessionChatAsyncQuestionsDismissed'), '[]'), '$[#]', ?3))
        WHERE projectId = ?1 AND sessionId = ?2
        AND NOT EXISTS (SELECT 1 FROM json_each(json_extract(runtimeSettingsJson, '$.sessionChatAsyncQuestionsDismissed')) WHERE value = ?3)"#,
        rusqlite::params![project_id, session_id, question_id],
    )?;
    if changed > 0 {
        broadcast(state, &db, project_id, session_id)?;
    }
    Ok(())
}

pub(crate) fn retired_question_ids(session: &Value) -> Vec<String> {
    session
        .pointer("/runtimeSettings/sessionChatAsyncQuestionsDismissed")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn read_retired_question_ids(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    let value = db.query_row(
        "SELECT json_extract(runtimeSettingsJson, '$.sessionChatAsyncQuestionsDismissed') FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id, session_id], |row| row.get::<_, Option<String>>(0),
    ).optional().ok().flatten().flatten();
    value
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn pending_question_count(session: &Value) -> usize {
    let dismissed = session
        .pointer("/runtimeSettings/sessionChatAsyncQuestionsDismissed")
        .and_then(Value::as_array);
    session
        .pointer("/runtimeSettings/sessionChatAsyncQuestionIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter(|id| !dismissed.is_some_and(|dismissed| dismissed.contains(id)))
                .count()
        })
        .unwrap_or_default()
}
