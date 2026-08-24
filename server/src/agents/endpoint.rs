use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::{
    domain::{read_project_id, DomainRepository, DomainStateError},
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
CDXC:GxserverRustPort 2026-06-16-10:00:
Phase 6 moves agent policy, launch/resume planning, passive title/status ingestion, and fork planning into Rust while keeping the TypeScript RPC shape. These handlers mutate only durable session metadata and never log raw titles, prompts, hook payloads, or command output.

CDXC:GxserverAgentSettings 2026-06-19-13:59:
Agent settings parity uses the TypeScript metadata key `agents.settings.v1` and stores Default Prompt Agent beside global Accept All. Normalize the prompt-agent id at the daemon boundary by trimming whitespace, falling back to `codex`, and capping it to 120 chars without validating against a client-local agent registry.

CDXC:GxserverAgentSettings 2026-06-22-07:33:
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
    let output = match endpoint_path {
        "/api/readAgentSettings" => AgentEndpointOutput {
            presentation_session: None,
            result: read_agent_settings_with_metadata(db)?,
        },
        "/api/updateAgentSettings" => AgentEndpointOutput {
            presentation_session: None,
            result: json!({ "settings": update_agent_settings(db, params)? }),
        },
        "/api/readAgentLaunchPlan" => {
            let project_id = read_project_id(params)?;
            let project = require_project(repository, &project_id)?;
            let agent_id = read_required_text(params.get("agentId"), "agentId")?;
            let settings = read_agent_settings(db)?;
            AgentEndpointOutput {
                presentation_session: None,
                result: json!({
                    "plan": build_project_agent_launch_plan(
                        &project,
                        &agent_id,
                        read_text(params, "agentSessionId"),
                        &settings,
                    )
                }),
            }
        }
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
        "/api/ingestSessionStateEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let result = ingest_session_state_event(repository, &lifecycle, params, home_dir)?;
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
    Ok(output)
}
