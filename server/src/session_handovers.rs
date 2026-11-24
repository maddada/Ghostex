//! Session handover lineage and cross-session (including delta) handover delivery.
//!
//! Records every handover edge in `session_handovers` (`0085_session_handovers.sql`) so:
//! - users and coordinators can inspect which session handed off to which, across any mix of
//!   CLI and ACP agents;
//! - handing off again from the same source session into the same live target session sends only
//!   the turns added since the previous handover (`delta: true`).

use axum::http::StatusCode;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::presentation::presentation_activity;
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, read_session_text, routed_json, AppState,
    RoutedResponse,
};
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_queue::enqueue_session_chat_prompt;
use crate::session_chat_queue_runtime::{
    send_session_chat_message_internal, SessionChatMessageSource,
};
use crate::session_transcript_export::{
    count_session_transcript, export_session_transcript, SessionTranscriptExportRequest,
    SessionTranscriptExportSelection,
};
use crate::storage::open_gxserver_database;

#[derive(Clone, Debug)]
pub(crate) struct HandoverRecordInput<'a> {
    pub(crate) from_project_id: &'a str,
    pub(crate) from_session_id: &'a str,
    pub(crate) to_project_id: &'a str,
    pub(crate) to_session_id: &'a str,
    pub(crate) from_agent: &'a str,
    pub(crate) to_agent: &'a str,
    pub(crate) mode: &'a str,
    pub(crate) preset: &'a str,
    pub(crate) from_turn_count: usize,
    pub(crate) exported_turns: usize,
    pub(crate) export_path: Option<&'a str>,
}

pub(crate) fn record_handover(
    db: &Connection,
    input: &HandoverRecordInput<'_>,
) -> Result<String, DomainStateError> {
    let handover_id = format!("ho_{}", uuid::Uuid::new_v4().simple());
    let created_at = Utc::now().to_rfc3339();
    db.execute(
        "INSERT INTO session_handovers (
            handoverId, fromProjectId, fromSessionId, toProjectId, toSessionId,
            fromAgent, toAgent, mode, preset, fromTurnCount, exportedTurns, exportPath, createdAt
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            handover_id,
            input.from_project_id,
            input.from_session_id,
            input.to_project_id,
            input.to_session_id,
            input.from_agent,
            input.to_agent,
            input.mode,
            input.preset,
            input.from_turn_count as i64,
            input.exported_turns as i64,
            input.export_path,
            created_at,
        ],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not record session handover: {error}"),
    })?;
    Ok(handover_id)
}

pub(crate) fn last_handover_turn_count(
    db: &Connection,
    from_project_id: &str,
    from_session_id: &str,
    to_project_id: &str,
    to_session_id: &str,
) -> usize {
    db.query_row(
        "SELECT fromTurnCount FROM session_handovers
         WHERE fromProjectId = ?1 AND fromSessionId = ?2
           AND toProjectId = ?3 AND toSessionId = ?4
         ORDER BY createdAt DESC LIMIT 1",
        params![
            from_project_id,
            from_session_id,
            to_project_id,
            to_session_id
        ],
        |row| row.get::<_, i64>(0),
    )
    .ok()
    .and_then(|count| usize::try_from(count).ok())
    .unwrap_or(0)
}

pub(crate) fn list_handovers_for_session(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Vec<Value>, DomainStateError> {
    let mut stmt = db
        .prepare(
            "SELECT handoverId, fromProjectId, fromSessionId, toProjectId, toSessionId,
                    fromAgent, toAgent, mode, preset, fromTurnCount, exportedTurns, exportPath, createdAt
             FROM session_handovers
             WHERE (fromProjectId = ?1 AND fromSessionId = ?2)
                OR (toProjectId = ?1 AND toSessionId = ?2)
             ORDER BY createdAt DESC LIMIT 100",
        )
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not list session handovers: {error}"),
        })?;
    let rows = stmt
        .query_map(params![project_id, session_id], |row| {
            let from_project_id: String = row.get(1)?;
            let from_session_id: String = row.get(2)?;
            let direction = if from_project_id == project_id && from_session_id == session_id {
                "outgoing"
            } else {
                "incoming"
            };
            Ok(json!({
                "handoverId": row.get::<_, String>(0)?,
                "direction": direction,
                "fromProjectId": from_project_id,
                "fromSessionId": from_session_id,
                "toProjectId": row.get::<_, String>(3)?,
                "toSessionId": row.get::<_, String>(4)?,
                "fromAgent": row.get::<_, String>(5)?,
                "toAgent": row.get::<_, String>(6)?,
                "mode": row.get::<_, String>(7)?,
                "preset": row.get::<_, String>(8)?,
                "fromTurnCount": row.get::<_, i64>(9)?,
                "exportedTurns": row.get::<_, i64>(10)?,
                "exportPath": row.get::<_, Option<String>>(11)?,
                "createdAt": row.get::<_, String>(12)?,
            }))
        })
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not read session handovers: {error}"),
        })?;
    let mut out = Vec::new();
    for row in rows {
        if let Ok(value) = row {
            out.push(value);
        }
    }
    Ok(out)
}

pub(crate) async fn handle_list_session_handovers_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if project_id.is_empty() || session_id.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "listSessionHandovers requires projectId and sessionId.".to_string(),
            },
        );
    }
    let paths = state.paths.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        list_handovers_for_session(&db, &pid, &sid)
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not list session handovers: {error}"),
    })
    .and_then(|res| res);
    match result {
        Ok(handovers) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, json!({ "handovers": handovers })),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/// Hand off conversation context from one session into another existing live session (CLI or ACP),
/// supporting compact `prompts_and_answers` exports and automatic delta tracking so repeat
/// handovers only transfer newly completed turns.
pub(crate) async fn handle_handoff_to_existing_session_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let read_str = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let from_project_id = read_str("fromProjectId");
    let from_session_id = read_str("fromSessionId");
    let to_project_id = read_str("toProjectId");
    let to_session_id = read_str("toSessionId");
    if from_project_id.is_empty()
        || from_session_id.is_empty()
        || to_project_id.is_empty()
        || to_session_id.is_empty()
    {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "handoffToExistingSession requires fromProjectId, fromSessionId, toProjectId, and toSessionId.".to_string(),
            },
        );
    }
    if from_project_id == to_project_id && from_session_id == to_session_id {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "invalidParams",
                message: "Cannot hand off a session into itself.".to_string(),
            },
        );
    }

    let delta = params.get("delta").and_then(Value::as_bool).unwrap_or(true);
    let preset = read_str("preset");
    let preset_name = if preset.is_empty() {
        "prompts_and_answers".to_string()
    } else {
        preset
    };
    let include_commands = params
        .get("includeCommands")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_patches = params
        .get("includePatches")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_reasoning = params
        .get("includeReasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let final_answer_only = params
        .get("finalAnswerOnly")
        .and_then(Value::as_bool)
        .unwrap_or(preset_name.eq_ignore_ascii_case("prompts_and_answers"));
    let last_turns = params
        .get("lastTurns")
        .and_then(Value::as_u64)
        .map(|turns| turns as usize);
    let note = read_str("note");
    let dry_run = params
        .get("dryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let resolve_paths = state.paths.clone();
    let resolve_server_id = state.metadata.server_id.clone();
    let resolve_from_pid = from_project_id.clone();
    let resolve_from_sid = from_session_id.clone();
    let resolve_to_pid = to_project_id.clone();
    let resolve_to_sid = to_session_id.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&resolve_paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repo = DomainRepository::new(&db, resolve_server_id.as_str());
        let from_session = repo
            .get_session(&resolve_from_pid, &resolve_from_sid)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The source session no longer exists.".to_string(),
            })?;
        let to_session = repo
            .get_session(&resolve_to_pid, &resolve_to_sid)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The target session no longer exists.".to_string(),
            })?;
        let from_project_path = repo.get_project(&resolve_from_pid)?.and_then(|project| {
            project
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        });
        let prior_turns = if delta {
            last_handover_turn_count(
                &db,
                &resolve_from_pid,
                &resolve_from_sid,
                &resolve_to_pid,
                &resolve_to_sid,
            )
        } else {
            0
        };
        let now_iso = Utc::now().to_rfc3339();
        let to_is_working = presentation_activity(&to_session, &now_iso) == "working";
        Ok::<_, DomainStateError>((
            session_chat_agent_for_session(&from_session),
            session_chat_agent_for_session(&to_session),
            read_runtime_text(&from_session, "agentSessionId"),
            read_runtime_text(&from_session, "agentSessionPath"),
            read_session_text(&from_session, "title"),
            from_project_path,
            prior_turns,
            to_is_working,
        ))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not resolve handover sessions: {error}"),
    })
    .and_then(|res| res);
    let (
        from_agent,
        to_agent,
        from_agent_session_id,
        from_agent_session_path,
        from_title,
        from_project_path,
        prior_turns,
        to_is_working,
    ) = match resolved {
        Ok(resolved) => resolved,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };

    let mut selection = if preset_name.eq_ignore_ascii_case("prompts_and_answers") {
        SessionTranscriptExportSelection::prompts_and_answers()
    } else {
        SessionTranscriptExportSelection::from_include_toggles(
            include_commands,
            include_patches,
            include_reasoning,
        )
        .with_final_answer_only(final_answer_only)
    };
    selection = selection
        .with_last_turns(last_turns)
        .with_skip_turns(prior_turns);

    // First check turn count so a delta handover with 0 new turns skips cleanly without writing
    // an empty file or disturbing the target session.
    let exports_dir = state.paths.app_data_dir.join("exports");
    let data_dir = state.paths.app_data_dir.clone();
    let check_agent = from_agent.clone();
    let check_sid = from_agent_session_id.clone();
    let check_path = from_agent_session_path.clone();
    let check_session_id = from_session_id.clone();
    let check_title = from_title.clone();
    let check_exports = exports_dir.clone();
    let check_selection = selection.clone();
    let counts = match tokio::task::spawn_blocking(move || {
        count_session_transcript(&SessionTranscriptExportRequest {
            agent: check_agent.as_deref(),
            agent_session_id: check_sid.as_deref(),
            agent_session_path: check_path.as_deref(),
            session_id: &check_session_id,
            session_title: check_title.as_deref(),
            exports_dir: &check_exports,
            selection: Some(&check_selection),
            data_dir: None,
            project_path: None,
        })
    })
    .await
    {
        Ok(Ok(counts)) => counts,
        Ok(Err(error)) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: error.code(),
                    message: error.to_string(),
                },
            );
        }
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "The handover turn count did not finish.".to_string(),
                },
            );
        }
    };

    if delta && prior_turns > 0 && counts.turns <= prior_turns {
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(
                request_id,
                json!({
                    "skipped": true,
                    "reason": "noNewTurns",
                    "fromTurnCount": counts.turns,
                    "previousTurnCount": prior_turns,
                    "exportedTurns": 0,
                    "delta": true,
                }),
            ),
        );
    }

    if dry_run {
        let available = counts.turns.saturating_sub(selection.skip_turns);
        let kept_turns = selection
            .last_turns
            .map_or(available, |last| last.min(available));
        let estimated_bytes = if kept_turns == 0 {
            0
        } else if kept_turns == counts.turns {
            counts.whole_bytes
        } else {
            counts
                .last_turns_bytes
                .get(kept_turns - 1)
                .copied()
                .unwrap_or(counts.whole_bytes)
        };
        return routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(
                request_id,
                json!({
                    "dryRun": true,
                    "bytes": estimated_bytes,
                    "turns": counts.turns,
                    "exportedTurns": kept_turns,
                    "previousTurnCount": prior_turns,
                    "delta": delta && prior_turns > 0,
                    "delivery": "dry_run",
                    "preset": preset_name,
                }),
            ),
        );
    }

    let export_agent = from_agent.clone();
    let export_sid = from_agent_session_id.clone();
    let export_path = from_agent_session_path.clone();
    let export_session_id = from_session_id.clone();
    let export_title = from_title.clone();
    let export_project_path = from_project_path.clone();
    let export_selection = selection.clone();
    let outcome = match tokio::task::spawn_blocking(move || {
        export_session_transcript(&SessionTranscriptExportRequest {
            agent: export_agent.as_deref(),
            agent_session_id: export_sid.as_deref(),
            agent_session_path: export_path.as_deref(),
            session_id: &export_session_id,
            session_title: export_title.as_deref(),
            exports_dir: &exports_dir,
            selection: Some(&export_selection),
            data_dir: Some(&data_dir),
            project_path: export_project_path.as_deref(),
        })
    })
    .await
    {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: error.code(),
                    message: error.to_string(),
                },
            );
        }
        Err(_) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: "The handover transcript export did not finish.".to_string(),
                },
            );
        }
    };

    let export_path_str = outcome.path.to_string_lossy().to_string();
    let source_label = from_title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&from_session_id);
    let handover_kind = if delta && prior_turns > 0 {
        format!(
            "Delta handover ({} since turn {}) from session \"{source_label}\"",
            ghostex_gx_protocol::plural(outcome.kept_turns, "new turn", "new turns"),
            prior_turns
        )
    } else {
        format!(
            "Handover ({}) from session \"{source_label}\"",
            ghostex_gx_protocol::plural(outcome.kept_turns, "turn", "turns")
        )
    };
    let mut prompt = format!(
        "{handover_kind}.\nRead the handover transcript at `{export_path_str}` and continue from there."
    );
    if !note.is_empty() {
        prompt.push_str("\n\nAdditional note:\n");
        prompt.push_str(&note);
    }

    let delivery = if to_is_working {
        let queue_paths = state.paths.clone();
        let queue_to_pid = to_project_id.clone();
        let queue_to_sid = to_session_id.clone();
        let queue_prompt = prompt.clone();
        let queued = tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&queue_paths).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            })?;
            enqueue_session_chat_prompt(&db, &queue_to_pid, &queue_to_sid, &queue_prompt, false)
        })
        .await
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not queue handover prompt: {error}"),
        })
        .and_then(|res| res);
        if let Err(error) = queued {
            return domain_error_response(endpoint_path, request_id, error);
        }
        "queued"
    } else {
        if let Err(error) = send_session_chat_message_internal(
            state,
            &to_project_id,
            &to_session_id,
            &prompt,
            &[],
            SessionChatMessageSource::Composer,
        )
        .await
        {
            return domain_error_response(endpoint_path, request_id, error);
        }
        "sent"
    };

    let mode = if delta && prior_turns > 0 {
        "delta_existing_session"
    } else {
        "existing_session"
    };
    let record_paths = state.paths.clone();
    let record_from_pid = from_project_id.clone();
    let record_from_sid = from_session_id.clone();
    let record_to_pid = to_project_id.clone();
    let record_to_sid = to_session_id.clone();
    let record_from_agent = from_agent.unwrap_or_else(|| "unknown".to_string());
    let record_to_agent = to_agent.unwrap_or_else(|| "unknown".to_string());
    let record_preset = preset_name.clone();
    let record_export_path = export_path_str.clone();
    let outcome_turns = outcome.turns;
    let outcome_kept_turns = outcome.kept_turns;
    let handover_id = match tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&record_paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        record_handover(
            &db,
            &HandoverRecordInput {
                from_project_id: &record_from_pid,
                from_session_id: &record_from_sid,
                to_project_id: &record_to_pid,
                to_session_id: &record_to_sid,
                from_agent: &record_from_agent,
                to_agent: &record_to_agent,
                mode,
                preset: &record_preset,
                from_turn_count: outcome_turns,
                exported_turns: outcome_kept_turns,
                export_path: Some(&record_export_path),
            },
        )
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not record session handover: {error}"),
    })
    .and_then(|res| res)
    {
        Ok(id) => id,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };

    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "handoverId": handover_id,
                "exportPath": export_path_str,
                "bytes": outcome.bytes,
                "turns": outcome.turns,
                "exportedTurns": outcome.kept_turns,
                "previousTurnCount": prior_turns,
                "delta": delta && prior_turns > 0,
                "delivery": delivery,
                "preset": preset_name,
            }),
        ),
    )
}
