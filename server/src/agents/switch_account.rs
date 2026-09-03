/*
CDXC:AgentProviders 2026-09-03:
"Switch Account" moves a PROMPTED session onto another agent configuration of
the same CLI family, so the same conversation can be resumed under a different
account (two Claude configurations, each logged into its own subscription, are
the motivating case: when one account's usage runs out, the session continues
on the other). It is Full Reload with a different launch identity: this module
rewrites which agent the row belongs to, and the client then cycles the
provider exactly the way Full Reload does, so the wake resumes the SAME
provider conversation through the ordinary resume planner, which now reads the
new agent's command.

The rule for "compatible" is the family the session resumes with
(`resume_agent_family_id`): a session on any Claude configuration can move to
the built-in Claude row or to any other custom agent whose icon declares Claude,
and to nothing else. The daemon that owns the project is the single owner of
that rule. It publishes the rows on every presentation session
(`switchableAgents`) and on the chat read state, so the sidebar context menu,
the terminal action bar, and the chat composer all render the same list without
any client re-deriving it from project configuration.

Drafts are excluded on purpose: a draft has no conversation to carry over, and
its composer already offers "Switch Agent CLI", which may cross families.
*/

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};

/// One row of the Switch Account submenu, in the shape `availableAgents` rows
/// already use so the shared chat contract needs no second agent type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchableSessionAgent {
    pub(crate) agent_id: String,
    pub(crate) base_agent_id: String,
    pub(crate) icon: String,
    pub(crate) name: String,
}

impl SwitchableSessionAgent {
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "agentId": self.agent_id,
            "baseAgentId": self.base_agent_id,
            "icon": self.icon,
            "name": self.name,
        })
    }
}

/// The CLI family a session's launch agent resumes with: the id itself for a
/// built-in agent, and the family its icon declares for a `custom-…` id.
pub(crate) fn session_agent_family_id(project: &Value, session: &Value) -> Option<String> {
    let configured_agent_id = read_text_value(session, "agentId")?;
    let launch_settings = object_field(session, "launchSettings");
    let agent_config =
        resolve_project_agent_config(project, &configured_agent_id, Some(&launch_settings));
    resume_agent_family_id(Some(configured_agent_id), &agent_config, &launch_settings)
        .map(|family| family.trim().to_ascii_lowercase())
}

/// The family a stored project agent configuration launches, by the same
/// icon-to-id contract resume planning uses for the session itself.
fn configured_agent_family_id(configured: &Map<String, Value>) -> Option<String> {
    let agent_id = read_text_from_map(configured, "agentId")?;
    let normalized = agent_id.trim().to_ascii_lowercase();
    if !normalized.starts_with("custom-") {
        return Some(normalized);
    }
    let icon = read_text_from_map(configured, "icon")?;
    default_agent_icon_to_id(&icon)
        .map(str::to_string)
        .or_else(|| normalize_agent_id(Some(&icon)))
        .map(|family| family.trim().to_ascii_lowercase())
}

fn configured_is_hidden(configured: &Map<String, Value>) -> bool {
    configured.get("hidden").and_then(Value::as_bool) == Some(true)
}

/// Every agent configuration this session could be moved to: the built-in
/// family row (named and iconed by the project's override for that id when it
/// has one) plus every custom agent of the same family, minus hidden rows and
/// minus the configuration the session is on right now. Empty for drafts, for
/// plain terminals, and for families the resume planner cannot restore.
pub(crate) fn switchable_session_agents(
    project: &Value,
    session: &Value,
) -> Vec<SwitchableSessionAgent> {
    if session_is_draft(session) {
        return Vec::new();
    }
    let Some(current_agent_id) = read_text_value(session, "agentId") else {
        return Vec::new();
    };
    let Some(family) = session_agent_family_id(project, session) else {
        return Vec::new();
    };
    if restorable_agent_id(Some(&family)).is_none() {
        return Vec::new();
    }
    let custom_agents = project
        .get("customAgents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    let family_override = custom_agents.iter().find(|configured| {
        read_text_from_map(configured, "agentId")
            .is_some_and(|agent_id| agent_id.trim().eq_ignore_ascii_case(&family))
    });
    let family_hidden = family_override.is_some_and(|configured| configured_is_hidden(configured));
    let family_launchable = family_override
        .and_then(|configured| read_text_from_map(configured, "command"))
        .is_some()
        || default_agent_command(&family).is_some();
    if !family_hidden && family_launchable {
        let (default_name, default_icon) =
            crate::sidebar_hud::default_sidebar_agent_identity(&family)
                .unwrap_or((family.as_str(), family.as_str()));
        rows.push(SwitchableSessionAgent {
            agent_id: family.clone(),
            base_agent_id: family.clone(),
            icon: family_override
                .and_then(|configured| read_text_from_map(configured, "icon"))
                .unwrap_or_else(|| default_icon.to_string()),
            name: family_override
                .and_then(|configured| read_text_from_map(configured, "name"))
                .unwrap_or_else(|| default_name.to_string()),
        });
    }
    for configured in &custom_agents {
        let Some(agent_id) = read_text_from_map(configured, "agentId") else {
            continue;
        };
        if !agent_id.trim().to_ascii_lowercase().starts_with("custom-") {
            continue;
        }
        if configured_is_hidden(configured) {
            continue;
        }
        if configured_agent_family_id(configured).as_deref() != Some(family.as_str()) {
            continue;
        }
        if read_text_from_map(configured, "command").is_none() {
            continue;
        }
        rows.push(SwitchableSessionAgent {
            base_agent_id: family.clone(),
            icon: read_text_from_map(configured, "icon").unwrap_or_else(|| family.clone()),
            name: read_text_from_map(configured, "name").unwrap_or_else(|| agent_id.clone()),
            agent_id,
        });
    }
    rows.retain(|row| !row.agent_id.eq_ignore_ascii_case(&current_agent_id));
    rows
}

pub(crate) fn switchable_session_agents_value(project: &Value, session: &Value) -> Option<Value> {
    let rows = switchable_session_agents(project, session);
    (!rows.is_empty()).then(|| {
        Value::Array(
            rows.iter()
                .map(SwitchableSessionAgent::to_value)
                .collect::<Vec<_>>(),
        )
    })
}

/*
`/api/switchSessionAgent`. Rewrites the row's launch identity and nothing else.
The provider is deliberately NOT cycled here: the client runs its own Full
Reload afterwards (sleep, then wake), which is the one path that already knows
how to tear down the mounted terminal and re-attach to the restored daemon on
every surface. The wake then builds the resume command from the row as
rewritten below, so it carries the new agent's binary and flags with the same
provider conversation id.

Everything the OLD agent's launch left behind is dropped so the resume planner
resolves the new configuration from the project instead of replaying the
stored command: `runtimeSettings.agentCommand` and `launchSettings.agentCommand`
would otherwise win over the configured command, `launchSettings.icon` names
the old icon on every sidebar row, and `agentLaunchPlan` is the old first
launch. The conversation identity (`agentSessionId`, `agentSessionPath`,
`firstUserMessage`) stays, because it is the whole point of the switch.
*/
pub(crate) fn switch_session_agent(
    repository: &DomainRepository<'_>,
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let lifecycle = read_lifecycle(params)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let session = require_session(repository, &lifecycle)?;
    let agent_id = read_required_text(params.get("agentId"), "agentId")?;
    let previous_agent_id = read_text_value(&session, "agentId");
    if previous_agent_id
        .as_deref()
        .is_some_and(|previous| previous.eq_ignore_ascii_case(&agent_id))
    {
        return Ok(AgentEndpointOutput {
            presentation_session: None,
            result: json!({ "agentId": agent_id, "session": session }),
        });
    }
    if session_is_draft(&session) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This session hasn't been prompted yet. Use Switch Agent CLI in the chat composer instead."
                .to_string(),
        }
        .into());
    }
    let target = switchable_session_agents(&project, &session)
        .into_iter()
        .find(|row| row.agent_id.eq_ignore_ascii_case(&agent_id))
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "{agent_id} is not an account this session can switch to. Only agents that run the same CLI as this session are compatible."
            ))
        })?;
    let settings = read_agent_settings(db)?;

    let mut runtime_settings = object_field(&session, "runtimeSettings");
    for key in ["agentCommand", "launchAgentId", "agentName"] {
        runtime_settings.remove(key);
    }
    let mut launch_settings = object_field(&session, "launchSettings");
    for key in [
        "acceptAllMode",
        "agentCommand",
        "agentLaunchPlan",
        "agentResumePlan",
        "icon",
    ] {
        launch_settings.remove(key);
    }
    let agent_config = resolve_project_agent_config(&project, &target.agent_id, None);
    if let Some(icon) = read_text_from_map(&agent_config, "icon") {
        launch_settings.insert("icon".to_string(), Value::String(icon));
    } else {
        launch_settings.insert("icon".to_string(), Value::String(target.icon.clone()));
    }
    if let Some(accept_all_mode) = read_text_from_map(&agent_config, "acceptAllMode") {
        launch_settings.insert("acceptAllMode".to_string(), Value::String(accept_all_mode));
    }
    runtime_settings.insert(
        "launchAgentId".to_string(),
        Value::String(target.agent_id.clone()),
    );

    let mut update = lifecycle_update(&lifecycle);
    update.insert("agentId".to_string(), json!(target.agent_id.clone()));
    update.insert("kind".to_string(), json!("agent"));
    update.insert("launchSettings".to_string(), Value::Object(launch_settings));
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    let updated = repository.update_session(&update)?;
    let plan = build_agent_resume_plan(&project, &updated, &settings);
    Ok(AgentEndpointOutput {
        presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
        result: json!({
            "agentId": target.agent_id,
            "plan": plan,
            "session": updated,
        }),
    })
}
