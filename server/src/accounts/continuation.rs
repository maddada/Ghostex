use super::endpoint;
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// CDXC:AgentProviders 2026-09-09 DECISION:
/// Account switches on prompted sessions send one literal "." once the agent input is ready, regardless of auto-continue defaults. Drafts keep their input unsent and receive no continuation, superseding the earlier every-switch rule for drafts only.
/// Stop or a manual send cancels the claim; an uncertain delivery is never replayed.
pub(crate) fn start(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    let project = session["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let id = session["sessionId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let session = repo
        .get_session(&project, &id)?
        .ok_or_else(|| DomainStateError::not_found("Session not found."))?;
    if crate::agents::session_is_draft(&session) {
        return Ok(());
    }
    let claim = uuid::Uuid::new_v4();
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.insert("accountRecoverySuppressed".into(), json!(false));
    runtime.insert(
        "accountRecovery".into(),
        json!({
            "status":"retrying", "trigger":"accountSwitch", "claim":claim.to_string(), "attempt":0,
            "reason":"Waiting for the agent input to continue on this account.",
            "updatedAt":chrono::Utc::now().to_rfc3339()
        }),
    );
    endpoint::update_session(repo, &session, runtime)?;
    let state = Arc::new(state.clone());
    tokio::spawn(async move {
        deliver(state, project, id, claim).await;
    });
    Ok(())
}

async fn deliver(state: Arc<AppState>, project: String, session_id: String, claim: uuid::Uuid) {
    let claim_text = claim.to_string();
    let detector = crate::session_chat_options::SessionChatOptionDetector::new(&state);
    let mut shutdown = state.shutdown_tx.subscribe();
    let result = loop {
        let row = {
            let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
                return;
            };
            let repo = DomainRepository::new(&db, &state.metadata.server_id);
            let Ok(Some(row)) = repo.get_session(&project, &session_id) else {
                return;
            };
            row
        };
        if row
            .pointer("/runtimeSettings/accountRecovery/claim")
            .and_then(Value::as_str)
            != Some(claim_text.as_str())
        {
            return;
        }
        if row["lifecycleState"] == "running" {
            let agent = crate::session_chat_composer::session_chat_composer_agent_id(&row);
            let detection = detector
                .detect(&project, &session_id, agent.as_deref(), true)
                .await;
            let blocked = detection.notice.as_ref().is_some_and(|notice| {
                notice.blocks_queued_delivery() && !super::recovery::retryable(notice)
            });
            if detection.captured
                && !blocked
                && detection.prompt.is_none()
                && detection.composer.state
                    == crate::session_chat_composer::SessionChatComposerState::Ready
            {
                let sent = crate::session_chat_queue_runtime::send_session_chat_message_internal(
                    &state,
                    &project,
                    &session_id,
                    ".",
                    &[],
                    crate::session_chat_queue_runtime::SessionChatMessageSource::AccountSwitch(
                        claim,
                    ),
                )
                .await;
                match sent {
                    Err(error)
                        if matches!(error.code, "composerNotReady" | "accountRecoveryNotReady") => {
                    }
                    result => break result.map(|_| ()),
                }
            }
        }
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
        }
    };
    let Ok(_gate) = state.accounts.mutations.lock() else {
        return;
    };
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(row)) = repo.get_session(&project, &session_id) else {
        return;
    };
    if row
        .pointer("/runtimeSettings/accountRecovery/claim")
        .and_then(Value::as_str)
        != Some(claim_text.as_str())
    {
        return;
    }
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime["accountRecovery"]["status"] = json!(if result.is_ok() {
        "resumed"
    } else {
        "needsAttention"
    });
    runtime["accountRecovery"]["reason"] = json!(match result {
        Ok(()) => "Continuation sent on the selected account.".to_string(),
        Err(error) => error.message,
    });
    runtime["accountRecovery"]["updatedAt"] = json!(chrono::Utc::now().to_rfc3339());
    if endpoint::update_session(&repo, &row, runtime).is_ok() {
        let _ = endpoint::publish(&state, &repo, &row);
    }
}
