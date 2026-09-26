//! The gxserver parameters of every create the sidebar, the New Thread picker, Quick Access, the
//! Help menu and the OS integrations make, built exactly as `session-create.ts` built them.
//!
//! One function per call shape, so a create and the parity gate that compared it (while the
//! TypeScript still ran) could not disagree about a key. Keys a TypeScript spread left out when their value was absent are left out here too: the
//! daemon treats an absent key and an empty one differently for several of them.
//!
//! Ported from `createSession`, `createAgentSession`, `createAgentSessionRecordForProject`,
//! `createOsIntegrationTerminal` and `startAgentSessionProviderAndSendPrompt` in the deleted
//! `gxserver-runtime/session-create.ts` (see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/create/ (the host).

use serde_json::{json, Map, Value};

use crate::selectors::DEFAULT_TERMINAL_SESSION_TITLE;

use super::agents::SidebarAgent;

/// `/api/createSession` for a terminal: `lifecycleState: running` at create.
///
/// CDXC:StateSync 2026-07-07:
/// gxserver defaults an omitted lifecycleState to "unknown", which the presentation layer treats
/// as inactive, so the created terminal never gets a sidebar row even though the workspace pane
/// opens. Declare the session running at create time like the remote path and the macOS client do.
pub fn terminal_create_params(project_id: Option<&str>, title: Option<&str>) -> Value {
    let mut params = Map::new();
    if let Some(project_id) = project_id {
        params.insert("projectId".to_string(), json!(project_id));
    }
    params.insert("kind".to_string(), json!("terminal"));
    params.insert("lifecycleState".to_string(), json!("running"));
    params.insert("surface".to_string(), json!("workspace"));
    params.insert(
        "title".to_string(),
        json!(title.unwrap_or(DEFAULT_TERMINAL_SESSION_TITLE)),
    );
    Value::Object(params)
}

/// `/api/createAgentSession` of a sidebar launch on THIS computer: a draft, with the agent's own
/// command and icon.
///
/// CDXC:Drafts 2026-08-28:
/// A sidebar agent launch has no prompt, so the row is created as a draft. The agent CLI is NOT
/// started here: the focus that follows hands the session to the Rust attach path, whose
/// `should_start_local_zmx_provider_before_gpui_attach` check starts the missing provider, so
/// trust/login/update screens surface while the user types. gxserver drops `draftStatus` when the
/// first prompt lands.
pub fn local_agent_launch_params(
    agent: &SidebarAgent,
    project_id: &str,
    title_runtime_settings: Map<String, Value>,
    account_id: Option<&str>,
    title: &str,
) -> Value {
    let mut runtime_settings = title_runtime_settings;
    if let Some(account_id) = account_id.filter(|id| !id.is_empty()) {
        runtime_settings.insert("accountId".to_string(), json!(account_id));
    }
    json!({
        "agentId": agent.agent_id,
        "draft": true,
        "launchSettings": launch_settings(agent),
        "projectId": project_id,
        "runtimeSettings": runtime_settings,
        "surface": "workspace",
        "title": title,
    })
}

/// `/api/createAgentSession` of a sidebar launch on a REMOTE machine: the machine resolves the
/// command itself, so only the agent id travels, with the guard that refuses a launch the machine
/// has no command for.
///
/// CDXC:RemoteMachines 2026-06-24-17:19:
/// Remote agent launches must let the owning remote gxserver resolve default and project-custom
/// agent commands from remote project metadata. GPUI sends only the selected agent id, project id,
/// surface, and a require-command guard through Rust's authenticated tunnel, never a
/// renderer-provided command string.
pub fn remote_agent_launch_params(
    agent_id: &str,
    project_id: &str,
    title_runtime_settings: Map<String, Value>,
    account_id: Option<&str>,
    title: &str,
) -> Value {
    let mut runtime_settings = title_runtime_settings;
    if let Some(account_id) = account_id.filter(|id| !id.is_empty()) {
        runtime_settings.insert("accountId".to_string(), json!(account_id));
    }
    json!({
        "agentId": agent_id,
        "draft": true,
        "projectId": project_id,
        "requireLaunchCommand": true,
        "runtimeSettings": runtime_settings,
        "surface": "workspace",
        "title": title,
    })
}

/// The options of `createAgentSessionRecordForProject`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRecordOptions {
    pub agent_effort: Option<String>,
    pub agent_model: Option<String>,
    pub draft: bool,
    pub title: Option<String>,
}

/// `/api/createAgentSession` of `createAgentSessionRecordForProject` (the Help chat, the board, the
/// Git prompts): the agent's command, a draft whenever the launch has no prompt yet OR carries one
/// the startup queue has not delivered.
///
/// CDXC:Drafts 2026-09-23 WHY:
/// An initial prompt is still unsent until the durable startup queue delivers it. Keep the new
/// session chat-eligible as a draft while its input box starts, then let the shared delivery path
/// promote it; this supersedes treating a supplied prompt as already sent.
pub fn agent_record_params(
    agent: &SidebarAgent,
    project_id: &str,
    prompt: &str,
    title_runtime_settings: Map<String, Value>,
    options: &AgentRecordOptions,
    default_title: &str,
) -> Value {
    let mut params = Map::new();
    if let Some(effort) = options.agent_effort.as_deref().filter(|v| !v.is_empty()) {
        params.insert("agentEffort".to_string(), json!(effort));
    }
    params.insert("agentId".to_string(), json!(agent.agent_id));
    if let Some(model) = options.agent_model.as_deref().filter(|v| !v.is_empty()) {
        params.insert("agentModel".to_string(), json!(model));
    }
    if options.draft || !crate::sidebar_view::text::js_trim(prompt).is_empty() {
        params.insert("draft".to_string(), json!(true));
    }
    params.insert("launchSettings".to_string(), launch_settings(agent));
    params.insert("projectId".to_string(), json!(project_id));
    params.insert(
        "runtimeSettings".to_string(),
        Value::Object(title_runtime_settings),
    );
    params.insert("surface".to_string(), json!("workspace"));
    params.insert(
        "title".to_string(),
        json!(options.title.as_deref().unwrap_or(default_title)),
    );
    Value::Object(params)
}

/// `/api/createAgentSession` of `ghostex://terminal` with a command: the command IS the launch.
pub fn os_integration_command_params(command: &str, project_id: &str, title: &str) -> Value {
    json!({
        "agentId": "os-integration-terminal",
        "launchSettings": { "agentCommand": command },
        "projectId": project_id,
        "surface": "workspace",
        "title": title,
    })
}

/// `{ agentCommand, icon }`, the icon left out when the agent has none (`JSON.stringify` drops an
/// `undefined`).
fn launch_settings(agent: &SidebarAgent) -> Value {
    let mut settings = Map::new();
    if let Some(command) = &agent.command {
        settings.insert("agentCommand".to_string(), json!(command));
    }
    if let Some(icon) = &agent.icon {
        settings.insert("icon".to_string(), json!(icon));
    }
    Value::Object(settings)
}

/// `/api/startSessionProvider`.
pub fn start_provider_params(project_id: &str, session_id: &str) -> Value {
    json!({ "projectId": project_id, "sessionId": session_id })
}

/// `/api/queueSessionChatPrompt` of a startup prompt.
pub fn queue_startup_prompt_params(project_id: &str, session_id: &str, text: &str) -> Value {
    json!({ "projectId": project_id, "sessionId": session_id, "startupSend": true, "text": text })
}

/// The receipt check of `startAgentSessionProviderAndSendPrompt`: `Err` with the sentence the
/// TypeScript threw.
pub fn check_startup_prompt_receipt(receipt: &Value) -> Result<(), &'static str> {
    let Some(queued) = receipt
        .as_object()
        .and_then(|receipt| receipt.get("prompt"))
    else {
        return Err(
            "gxserver did not confirm the startup prompt was queued. Inspect this session before retrying.",
        );
    };
    let id_ok = queued
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !crate::sidebar_view::text::js_trim(id).is_empty());
    if !queued.is_object() || !id_ok {
        return Err(
            "gxserver did not return a startup prompt receipt. Inspect this session before retrying.",
        );
    }
    Ok(())
}

/// The created session a create answered with: `(projectId, sessionId)`, the project falling back
/// to the one the create named (`normalizeNonEmptyString(response.session?.projectId) ?? projectId`).
pub fn created_session(
    response: &Value,
    requested_project_id: Option<&str>,
) -> Option<(Option<String>, String)> {
    let session = response.get("session")?;
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !crate::sidebar_view::text::js_trim(value).is_empty())
            .map(str::to_string)
    };
    let session_id = text("sessionId")?;
    let project_id = text("projectId").or_else(|| requested_project_id.map(str::to_string));
    Some((project_id, session_id))
}
