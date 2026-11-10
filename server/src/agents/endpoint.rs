use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::{
    domain::{DomainRepository, DomainStateError},
    zmx::{ZmxEndpointError, ZmxServerContext},
};

use super::*;

#[derive(Debug)]
pub enum AgentEndpointError {
    DependencyUnavailable(String),
    Domain(DomainStateError),
}

impl From<DomainStateError> for AgentEndpointError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ZmxEndpointError> for AgentEndpointError {
    fn from(error: ZmxEndpointError) -> Self {
        match error {
            ZmxEndpointError::DependencyUnavailable(message) => {
                Self::DependencyUnavailable(message)
            }
            ZmxEndpointError::Domain(error) => Self::Domain(error),
        }
    }
}

pub struct AgentEndpointOutput {
    pub presentation_session: Option<(String, String)>,
    pub result: Value,
}

/*
CDXC:RepoStructure 2026-06-16-10:00:
Phase 6 moves agent policy, launch/resume planning, passive title/status ingestion, and fork planning into Rust while keeping the TypeScript RPC shape. These handlers mutate only durable session metadata and never log raw titles, prompts, hook payloads, or command output.

CDXC:AgentProviders 2026-06-19-13:59:
Agent settings parity uses the TypeScript metadata key `agents.settings.v1` and stores Default Prompt Agent beside global Accept All. Normalize the prompt-agent id at the daemon boundary by trimming whitespace, falling back to `codex`, and capping it to 120 chars without validating against a client-local agent registry.

CDXC:AgentProviders 2026-06-22-07:33:
Agent settings persistence must match TypeScript gxserver exactly: read and write only `agents.settings.v1`. Legacy or sidebar-local keys are not daemon settings and must not make `/api/readAgentSettings` report persisted values.
*/
pub fn dispatch_agent_endpoint(
    repository: &DomainRepository<'_>,
    db: &Connection,
    home_dir: &Path,
    endpoint_path: &str,
    params: &Map<String, Value>,
    zmx_context: Option<&ZmxServerContext>,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let _identity_source =
        crate::session_identity_log::identity_write_source(format!("endpoint:{endpoint_path}"));
    // CDXC:SessionTitles 2026-09-10 WHY:
    // Hooks and terminal observations replace runtime settings as a whole. Their reads must share a write transaction with their updates, or they can resurrect a title worker's completed running flag. Provider lifecycle operations keep their separate process-owning path.
    let updates_session_metadata = matches!(
        endpoint_path,
        "/api/requestSessionRename"
            | "/api/cancelFirstPromptAutoTitle"
            | "/api/ingestTerminalTitleEvent"
            | "/api/updateAgentActivity"
            | SETTLE_GONE_AGENT_ACTIVITY_ENDPOINT
            | "/api/ingestAgentHookEvent"
    );
    let transaction = (updates_session_metadata && db.is_autocommit())
        .then(|| rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate))
        .transpose()
        .map_err(sql_error)?;
    let output = match endpoint_path {
        "/api/readAgentSettings" => AgentEndpointOutput {
            presentation_session: None,
            result: read_agent_settings_with_metadata(db)?,
        },
        "/api/updateAgentSettings" => AgentEndpointOutput {
            presentation_session: None,
            result: json!({ "settings": update_agent_settings(db, params)? }),
        },
        "/api/readAgentResumePlan" => {
            let lifecycle = read_lifecycle(params)?;
            let project = require_project(repository, &lifecycle.project_id)?;
            let session = require_session(repository, &lifecycle)?;
            let settings = read_agent_settings(db)?;
            AgentEndpointOutput {
                presentation_session: None,
                result: json!({
                    "plan": build_agent_resume_plan(&project, &session, &settings),
                    "session": session,
                }),
            }
        }
        "/api/forkSession" => {
            let context = zmx_context.ok_or_else(|| {
                AgentEndpointError::DependencyUnavailable(
                    "Cannot fork session without gxserver zmx context.".to_string(),
                )
            })?;
            let lifecycle = read_lifecycle(params)?;
            fork_session(repository, &lifecycle, db, context)?
        }
        // A side chat (server/src/side_chats/): the fork above, marked, with its question sent by
        // server/src/server/side_chat_http.rs once it has started.
        "/api/startSideChat" => {
            let context = zmx_context.ok_or_else(|| {
                AgentEndpointError::DependencyUnavailable(
                    "Cannot start a side chat without gxserver zmx context.".to_string(),
                )
            })?;
            let lifecycle = read_lifecycle(params)?;
            let question = read_text(params, "question").ok_or_else(|| {
                DomainStateError::bad_request("A side chat starts with a question.")
            })?;
            fork_session_as(repository, &lifecycle, db, context, Some(&question))?
        }
        /*
        CDXC:Drafts 2026-08-28:
        Drafts-only agent switching. Grouped with fork above because it has the
        same shape: an agent-policy decision that has to reach into zmx to kill
        and restart a provider, so it needs the same zmx context.
        */
        "/api/switchDraftAgent" => {
            let context = zmx_context.ok_or_else(|| {
                AgentEndpointError::DependencyUnavailable(
                    "Cannot switch a draft's agent without gxserver zmx context.".to_string(),
                )
            })?;
            switch_draft_agent(repository, db, params, context)?
        }
        // Where a draft runs (this computer or an agentbox box): the chat composer's Run on row.
        "/api/draftRunLocation" => {
            let context = zmx_context.ok_or_else(|| {
                AgentEndpointError::DependencyUnavailable(
                    "Cannot move a draft without gxserver zmx context.".to_string(),
                )
            })?;
            draft_run_location(repository, db, home_dir, params, context)?
        }
        /*
        CDXC:AgentProviders 2026-09-03:
        Prompted-session account switching. Only a row rewrite: the client's
        Full Reload cycles the provider afterwards, so no zmx context is needed.
        */
        "/api/switchSessionAgent" => switch_session_agent(repository, db, params)?,
        "/api/requestSessionRename" => {
            let lifecycle = read_lifecycle(params)?;
            let result = request_session_rename(repository, &lifecycle, params, home_dir)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/cancelFirstPromptAutoTitle" => {
            let lifecycle = read_lifecycle(params)?;
            let result = cancel_first_prompt_auto_title(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/ingestTerminalTitleEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let output =
                ingest_terminal_title_event_with_home(repository, &lifecycle, params, home_dir)?;
            AgentEndpointOutput {
                presentation_session: output
                    .schedule_presentation_delta
                    .then_some((lifecycle.project_id, lifecycle.session_id)),
                result: output.result,
            }
        }
        "/api/updateAgentActivity" => {
            let lifecycle = read_lifecycle(params)?;
            let result = update_agent_activity_endpoint(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        SETTLE_GONE_AGENT_ACTIVITY_ENDPOINT => {
            let lifecycle = read_lifecycle(params)?;
            let result = settle_gone_agent_activity(repository, &lifecycle, params)?;
            let changed = result.get("changed").and_then(Value::as_bool) == Some(true);
            AgentEndpointOutput {
                presentation_session: changed
                    .then_some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/ingestAgentHookEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let result = ingest_agent_hook_event(repository, &lifecycle, params, home_dir)?;
            let rejected = matches!(
                result.get("reason").and_then(Value::as_str),
                Some("agent-hook-agent-mismatch" | "passive-session-identity-conflict")
            );
            AgentEndpointOutput {
                presentation_session: (!rejected)
                    .then_some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        _ => {
            return Err(DomainStateError::not_found(format!(
                "{endpoint_path} is not a gxserver agent endpoint."
            ))
            .into())
        }
    };
    if let Some(transaction) = transaction {
        transaction.commit().map_err(sql_error)?;
    }
    Ok(output)
}
