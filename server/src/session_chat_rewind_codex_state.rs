use super::*;

const PENDING_KEY: &str = "sessionChatPendingCodexRewind";

/// CDXC:SessionChat 2026-09-11 DECISION:
/// User approved retrying synchronization after Codex rewinds. Persist the submitted target before Enter so retrying the request, including after a daemon restart, cannot drive the picker a second time.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PendingCodexRewind {
    pub previous_agent_session_id: String,
    pub previous_transcript_path: PathBuf,
    pub target_message_id: String,
    pub started_at: i64,
    pub zmx_name: String,
}

pub(super) fn pending_rewind(
    session: &Value,
) -> Result<Option<PendingCodexRewind>, DomainStateError> {
    session
        .pointer(&format!("/runtimeSettings/{PENDING_KEY}"))
        .map(|pending| {
            serde_json::from_value(pending.clone()).map_err(|error| {
                agent_busy(format!(
                    "The pending Codex rewind could not be read: {error}"
                ))
            })
        })
        .transpose()
}

pub(super) fn write_pending_rewind(
    driver: &RewindDriver<'_>,
    codex: &CodexRewindPlan,
    started_at: Option<i64>,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&codex.paths)
        .map_err(|error| session_not_running(error.to_string()))?;
    let transaction =
        rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)
            .map_err(crate::domain::sql_error)?;
    let repository = DomainRepository::new(&transaction, &codex.server_id);
    let session = repository
        .get_session(driver.project_id, driver.session_id)?
        .ok_or_else(|| session_not_running("The session no longer exists."))?;
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(started_at) = started_at {
        if read_runtime_text(&session, "agentSessionId").as_deref() != Some(&codex.agent_session_id)
            || session.get("zmxName").and_then(Value::as_str) != Some(driver.zmx_name)
        {
            return Err(agent_busy(
                "The conversation changed before rewind confirmation.",
            ));
        }
        if pending_rewind(&session)?.is_some() {
            return Err(agent_busy(
                "A previous Codex rewind still needs synchronization.",
            ));
        }
        runtime.insert(
            PENDING_KEY.into(),
            json!(PendingCodexRewind {
                previous_agent_session_id: codex.agent_session_id.clone(),
                previous_transcript_path: codex.transcript_path.clone(),
                target_message_id: codex.message_id.clone(),
                started_at,
                zmx_name: driver.zmx_name.to_string(),
            }),
        );
    } else {
        runtime.remove(PENDING_KEY);
    }
    repository.update_session(json!({
        "projectId": driver.project_id, "sessionId": driver.session_id, "runtimeSettings": runtime,
    }).as_object().unwrap())?;
    transaction.commit().map_err(crate::domain::sql_error)?;
    Ok(())
}
