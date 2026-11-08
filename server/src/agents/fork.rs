use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;
use crate::zmx::dispatch_zmx_lifecycle_endpoint;
use crate::zmx::ZmxServerContext;

#[cfg(test)]
#[path = "fork_tests.rs"]
mod tests;

pub(crate) fn fork_session(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    db: &Connection,
    context: &ZmxServerContext,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    fork_session_as(repository, lifecycle, db, context, None)
}

/// A fork, or with `side_chat_question` a side chat of the session (server/src/side_chats/): the
/// same fork, hidden from the session lists, started in ask-before-edits and named after its
/// question.
pub(crate) fn fork_session_as(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    db: &Connection,
    context: &ZmxServerContext,
    side_chat_question: Option<&str>,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let _identity_source =
        crate::session_identity_log::identity_write_source(if side_chat_question.is_some() {
            "sideChat"
        } else {
            "fork"
        });
    let project = require_project(repository, &lifecycle.project_id)?;
    let source_session = require_session(repository, lifecycle)?;
    /*
    CDXC:Drafts 2026-08-28:
    A draft has nothing to fork. Its CLI publishes an `agentSessionId` at
    startup, before any prompt, so `build_agent_fork_plan` below happily finds a
    restorable reference and passes the identity check — and the fork it builds
    would run `--resume <id> --fork-session` against a conversation that was
    never recorded, leaving the user with a junk non-draft session whose agent
    errors on launch. Refuse first, ahead of the plan build and every side
    effect after it. The chat menus already hide Fork for drafts; the native
    terminal action bar and the web terminal surface bar do not, so the daemon
    is the only place this can be enforced for all of them.
    */
    if session_is_draft(&source_session) {
        return Err(DomainStateError::bad_request(
            "Fork is only available after a session's first prompt. Send a prompt to this draft, then fork it.",
        )
        .into());
    }
    crate::agentbox::refuse_for_agentbox_session(&source_session, "Fork")?;
    if side_chat_question.is_some() && crate::side_chats::parent_of(&source_session).is_some() {
        return Err(DomainStateError::bad_request(
            "A side chat can't start another side chat. Start it from the main session.",
        )
        .into());
    }
    let settings = read_agent_settings(db)?;
    let mut plan = build_agent_fork_plan(&project, &source_session, &settings);
    if side_chat_question.is_some() {
        side_chat_plan(&mut plan);
    }
    if plan
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        || plan
            .get("primaryCommand")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(DomainStateError::bad_request(
            "Fork is only available for Codex, Claude, Pi, Empryo, Copilot, and Grok Build sessions with a restorable identity.",
        )
        .into());
    }
    let mut fork_params = create_agent_fork_session_params(&project, &source_session, &plan);
    if let Some(question) = side_chat_question {
        crate::side_chats::mark_fork_params(&mut fork_params, &lifecycle.session_id, question);
    }
    let empryo_fork = plan
        .get("empryoForkSessionId")
        .and_then(Value::as_str)
        .map(|fork_id| {
            super::fork_empryo::fork_empryo_session_folder(&project, &source_session, fork_id)
                .map(|folder| (fork_id.to_string(), folder))
        })
        .transpose()?;
    // CDXC:SessionFork 2026-10-07 WHY: the chat finds an Empryo log by the row's `agentSessionPath`, which otherwise arrives only with Empryo's first hook, and an Empryo hook reports nothing on native Windows (seen live 2026-10-07: a fork's chat stayed empty while its terminal showed the copied conversation). The fork wrote that folder itself, so its row records the id and log at once, as a create's seeded session does.
    if let (Some((fork_id, folder)), Some(runtime)) = (
        empryo_fork.as_ref(),
        fork_params
            .get_mut("runtimeSettings")
            .and_then(Value::as_object_mut),
    ) {
        runtime.insert("agentSessionId".to_string(), json!(fork_id));
        runtime.insert(
            "agentSessionPath".to_string(),
            json!(folder.session_log().to_string_lossy()),
        );
    }
    // Copilot's fork resumes a copy of the session folder (`fork_copilot.rs`), recorded on the row
    // at once for the same reason as Empryo's above.
    let copilot_fork = plan
        .get("copilotForkSessionId")
        .and_then(Value::as_str)
        .map(|fork_id| {
            super::fork_copilot::fork_copilot_session_folder(&source_session, fork_id)
                .map(|folder| (fork_id.to_string(), folder))
        })
        .transpose()?;
    if let (Some((fork_id, folder)), Some(runtime)) = (
        copilot_fork.as_ref(),
        fork_params
            .get_mut("runtimeSettings")
            .and_then(Value::as_object_mut),
    ) {
        runtime.insert("agentSessionId".to_string(), json!(fork_id));
        runtime.insert(
            "agentSessionPath".to_string(),
            json!(folder.session_log().to_string_lossy()),
        );
    }
    let fork_params = fork_params
        .as_object()
        .ok_or_else(|| DomainStateError::corrupt_state("Fork params must be an object."))?;
    let empryo_fork = empryo_fork.map(|(_, folder)| folder);
    let copilot_fork = copilot_fork.map(|(_, folder)| folder);
    let created_session = match repository.create_session(fork_params, false) {
        Ok(session) => session,
        Err(error) => {
            if let Some(folder) = empryo_fork {
                folder.discard();
            }
            if let Some(folder) = copilot_fork {
                folder.discard();
            }
            return Err(error.into());
        }
    };
    let project_id = read_text_value(&created_session, "projectId")
        .ok_or_else(|| DomainStateError::corrupt_state("Forked session is missing projectId."))?;
    let session_id = read_text_value(&created_session, "sessionId")
        .ok_or_else(|| DomainStateError::corrupt_state("Forked session is missing sessionId."))?;
    let start_params = json!({ "projectId": project_id, "sessionId": session_id });
    let provider = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        start_params
            .as_object()
            .ok_or_else(|| DomainStateError::corrupt_state("Fork provider params missing."))?,
        context,
        &settings,
    )?;
    let session = provider
        .result
        .get("session")
        .cloned()
        .unwrap_or(created_session);
    Ok(AgentEndpointOutput {
        presentation_session: read_session_target(&session),
        result: json!({
            "fork": {
                "plan": plan,
                "provider": provider.result,
                "session": session,
                "sourceSession": source_session,
            }
        }),
    })
}

/// The fork's command in ask-before-edits (`side_chats::ask_before_edits_command`), and the
/// startup text that types it.
fn side_chat_plan(plan: &mut Value) {
    let agent_id = plan
        .get("agentId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let Some(command) = plan
        .get("primaryCommand")
        .and_then(Value::as_str)
        .map(|command| crate::side_chats::ask_before_edits_command(command, &agent_id))
    else {
        return;
    };
    plan["primaryCommand"] = json!(command);
    plan["displayCommand"] = json!(command);
    plan["startupText"] = json!(as_atuin_ignored_shell_input(&command));
}

pub(crate) fn build_agent_fork_plan(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Value {
    let resume_plan = build_agent_resume_plan(project, session, settings);
    let agent_id = resume_plan.get("agentId").and_then(Value::as_str);
    let runtime_command = resume_plan.get("runtimeCommand").and_then(Value::as_str);
    let exact_reference = object_field(session, "runtimeSettings")
        .get("agentSessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let trusted_title = trusted_resume_title(session);
    let mut empryo_fork_id = None;
    let mut copilot_fork_id = None;
    let primary_command = match (agent_id, runtime_command) {
        (Some("codex"), Some(command)) => exact_reference
            .as_deref()
            .and_then(normalize_codex_session_id)
            .map(|reference| {
                build_codex_fork_invocation(command, &quote_shell_double_arg(&reference))
            }),
        (Some("claude"), Some(command)) => exact_reference
            .clone()
            .or_else(|| trusted_title.clone())
            .map(|reference| {
                build_claude_fork_invocation(command, &quote_shell_double_arg(&reference))
            }),
        // Grok names the fork's own conversation at launch (`--session-id`, valid with a fork only),
        // and its hooks report that id like any other start.
        (Some("grok"), Some(command)) => exact_reference.map(|reference| {
            format!(
                "{command} --resume {} --fork-session --session-id {}",
                quote_shell_double_arg(&reference),
                uuid::Uuid::new_v4()
            )
        }),
        (Some("pi"), Some(command)) => exact_reference
            .or(trusted_title)
            .map(|reference| format!("{command} --fork {}", quote_shell_double_arg(&reference))),
        // Empryo has no fork flag: `fork_session` copies the conversation to this id first
        // (`fork_empryo.rs`), and the fork resumes the copy.
        (Some("empryo"), Some(command)) => exact_reference.map(|_| {
            let fork_id = uuid::Uuid::new_v4().to_string();
            let invocation = format!("{command} --session {}", quote_shell_double_arg(&fork_id));
            empryo_fork_id = Some(fork_id);
            invocation
        }),
        (Some("copilot"), Some(command)) => exact_reference.map(|_| {
            let fork_id = uuid::Uuid::new_v4().to_string();
            let invocation = format!("{command} --resume {}", quote_shell_double_arg(&fork_id));
            copilot_fork_id = Some(fork_id);
            invocation
        }),
        _ => None,
    };
    let mut plan = Map::new();
    insert_optional_string(&mut plan, "agentId", agent_id.map(str::to_string));
    insert_optional_string(
        &mut plan,
        "baseCommand",
        resume_plan
            .get("baseCommand")
            .and_then(Value::as_str)
            .map(str::to_string),
    );
    insert_optional_string(&mut plan, "displayCommand", primary_command.clone());
    insert_optional_string(&mut plan, "empryoForkSessionId", empryo_fork_id);
    insert_optional_string(&mut plan, "copilotForkSessionId", copilot_fork_id);
    insert_optional_string(&mut plan, "primaryCommand", primary_command.clone());
    insert_optional_string(
        &mut plan,
        "runtimeCommand",
        runtime_command.map(str::to_string),
    );
    if let Some(command) = primary_command {
        plan.insert(
            "startupText".to_string(),
            Value::String(as_atuin_ignored_shell_input(&command)),
        );
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("queueAfterTerminalReady".to_string()),
        );
    } else {
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("none".to_string()),
        );
    }
    Value::Object(plan)
}

/// CDXC:SessionFork 2026-09-11 DECISION:
/// User: a newly forked session must show `Fork:` in its sidebar name.
/// Keep this provisional name through launch identity updates and inherited Codex metadata until the fork receives its own name.
pub(crate) fn provisional_fork_title(session: &Value) -> Option<String> {
    let runtime = object_field(session, "runtimeSettings");
    if runtime
        .get("forkFirstPromptAutoTitlePending")
        .and_then(Value::as_bool)
        != Some(true)
        || runtime.get("titleSource").and_then(Value::as_str) != Some("placeholder")
    {
        return None;
    }
    read_text_value(session, "title").filter(|title| title.starts_with("Fork: "))
}

/// `Fork: <name>`, the name a fork starts with (CDXC:SessionFork 2026-09-11 above).
pub(crate) fn fork_title(name: &str) -> String {
    format!("Fork: {name}")
}

pub(crate) fn create_agent_fork_session_params(
    project: &Value,
    source_session: &Value,
    plan: &Value,
) -> Value {
    let agent_id = plan
        .get("agentId")
        .and_then(Value::as_str)
        .or_else(|| source_session.get("agentId").and_then(Value::as_str))
        .unwrap_or("codex");
    let source_session_id = source_session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let startup_text = plan
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty());
    let source_title = source_session
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Terminal Session");
    let title = fork_title(source_title);
    let cwd = read_text_value(source_session, "cwd").or_else(|| read_text_value(project, "path"));
    let mut launch_plan = Map::new();
    insert_optional_string(
        &mut launch_plan,
        "agentCommand",
        plan.get("baseCommand")
            .and_then(Value::as_str)
            .map(str::to_string),
    );
    launch_plan.insert(
        "command".to_string(),
        Value::String(
            plan.get("primaryCommand")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    );
    launch_plan.insert(
        "startupText".to_string(),
        Value::String(startup_text.clone().unwrap_or_default()),
    );
    launch_plan.insert(
        "startupTextDisposition".to_string(),
        plan.get("startupTextDisposition")
            .cloned()
            .unwrap_or_else(|| json!("none")),
    );
    let mut params = Map::new();
    params.insert("agentId".to_string(), json!(agent_id));
    insert_optional_string(&mut params, "cwd", cwd);
    params.insert("kind".to_string(), json!("agent"));
    params.insert(
        "launchSettings".to_string(),
        json!({
            "agentLaunchPlan": launch_plan,
            "forkedFromSessionId": source_session_id,
            "runtimeRelevant": { "queueProviderStartupText": startup_text.is_some() },
        }),
    );
    params.insert("lifecycleState".to_string(), json!("running"));
    params.insert(
        "providerState".to_string(),
        json!({ "lifecycleState": "missing", "provider": "zmx" }),
    );
    params.insert(
        "projectId".to_string(),
        project.get("projectId").cloned().unwrap_or(Value::Null),
    );
    params.insert(
        "restoredFromSessionId".to_string(),
        json!(source_session_id),
    );
    params.insert(
        "runtimeSettings".to_string(),
        json!({
            "agentActivity": default_activity(Some(agent_id), startup_text.is_some().then_some("working")),
            "agentCommand": plan.get("baseCommand").and_then(Value::as_str),
            "launchAgentId": agent_id,
            "agentName": agent_id,
            "autoTitleFromFirstPrompt": false,
            "forkFirstPromptAutoTitlePending": true,
            "forkedFromSessionId": source_session_id,
            // Empryo's and Copilot's copies already carry the fork's name (`fork_empryo.rs`,
            // `fork_copilot.rs`).
            "gxserverForkInitialRenameStatus": if matches!(agent_id, "empryo" | "copilot") { "applied" } else { "pending" },
            "startupText": startup_text,
            "titleSource": "placeholder",
        }),
    );
    // CDXC:AgentProviders 2026-11-05 WHY: a fork runs its source's account command (`cswap run 2 … -- <args>`), so switching its account starts from the agent's own command the source kept, not from that wrapper (#415). Absent rather than null, since account assignment falls back to `agentCommand` only when the key is missing.
    if let (Some(base), Some(runtime)) = (
        source_session
            .pointer("/runtimeSettings/accountBaseCommand")
            .and_then(Value::as_str),
        params
            .get_mut("runtimeSettings")
            .and_then(Value::as_object_mut),
    ) {
        runtime.insert("accountBaseCommand".to_string(), json!(base));
    }
    if let Some(surface) = source_session.get("surface").cloned() {
        params.insert("surface".to_string(), surface);
    }
    params.insert("title".to_string(), Value::String(title));
    Value::Object(params)
}

pub(crate) fn request_session_rename(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    home_dir: &Path,
) -> Result<Value, DomainStateError> {
    let _identity_source =
        crate::session_identity_log::identity_write_source("requestSessionRename");
    let session = require_session(repository, lifecycle)?;
    let requested_title = read_text(params, "title");
    let Some(title) = requested_title else {
        return Ok(json!({
            "changed": false,
            "pendingAgentMetadata": false,
            "projection": project_session_title_projection(&session),
            "reason": "empty-title",
            "session": session,
            "shouldSendAgentRenameCommand": false,
        }));
    };
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(&session, "agentId"),
        agent_name: read_text(params, "agentName"),
        agent_session_id: read_text(params, "agentSessionId"),
        agent_session_path: read_text(params, "agentSessionPath"),
        runtime_settings: object_field(&session, "runtimeSettings"),
        startup_text: None,
    });
    // CDXC:GBots 2026-11-08 WHY: a GBots conversation's terminal is Ghostex's own chat client, not
    // an agent with a `/rename` command: a typed `/rename <title>` went to the bot as a message and
    // the runtime titled the conversation after it. Its title is Ghostex's to set, like a terminal's.
    if is_agent_associated(&session, &identity) && !crate::gbots::is_conversation_session(&session)
    {
        if identity
            .agent_id
            .as_deref()
            .is_some_and(|agent_id| agent_id.eq_ignore_ascii_case("zcode"))
        {
            return zcode_session_rename(
                repository,
                lifecycle,
                &session,
                identity.agent_session_id.as_deref(),
                params,
                &title,
                home_dir,
            );
        }
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        if let Some(agent_id) = identity.agent_id.clone() {
            runtime_settings.insert("agentName".to_string(), json!(agent_id));
        }
        insert_optional_string(
            &mut runtime_settings,
            "agentSessionId",
            identity.agent_session_id,
        );
        insert_optional_string(
            &mut runtime_settings,
            "agentSessionPath",
            identity.agent_session_path,
        );
        runtime_settings.insert(
            "pendingAgentTitleRequestRequestedAt".to_string(),
            json!(now_iso()),
        );
        runtime_settings.insert(
            "pendingAgentTitleRequestStatus".to_string(),
            json!("pending"),
        );
        runtime_settings.insert("pendingAgentTitleRequestTitle".to_string(), json!(title));
        runtime_settings.remove("pendingAgentTitleRequestFailedAt");
        runtime_settings.insert(
            "pendingAgentTitleRequestTitleSource".to_string(),
            json!(read_text(params, "titleSource").unwrap_or_else(|| "user".to_string())),
        );
        let mut update = lifecycle_update(lifecycle);
        if session.get("kind").and_then(Value::as_str) == Some("terminal") {
            update.insert("kind".to_string(), json!("agent"));
        }
        if session.get("agentId").and_then(Value::as_str).is_none() {
            insert_optional_string(&mut update, "agentId", identity.agent_id);
        }
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let updated = repository.update_session(&update)?;
        let pending_changed = updated.get("updatedAt") != session.get("updatedAt");
        /*
        CDXC:Drafts 2026-08-28:
        Renaming is a supported action on a draft, and it must not promote one.
        This branch ends with `/rename <title>` being typed into the agent's
        input line — by the local GPUI's own surface, or by the remote submission
        in `agent_http.rs` — and the terminal churn that produces is
        indistinguishable from a user prompt to the title-derived half of draft
        promotion. Ghostex typed those bytes, not the user, so re-arm layer 1 of
        CDXC:AgentScreenDetection here, at the ONE point both submission
        paths pass through. Same carve-out the composer's option commands get,
        and a no-op on every session that is not a draft.
        */
        let updated = arm_draft_launch_activity_suppression(repository, &updated)?;
        /*
        CDXC:SessionTitles 2026-06-21-15:35:
        Rust requestSessionRename must mirror TypeScript gxserver for Agent CLI renames: the renderer command can ask the CLI to rename, but the app sidebar title must be reconciled from the agent's own structured session metadata before the RPC returns so clients receive the canonical session projection.
        */
        let reconciled =
            reconcile_agent_metadata_title(repository, lifecycle, home_dir, "pending")?;
        let _metadata_title_found = reconciled.metadata_title_found;
        let reconciled_changed = reconciled.changed;
        let reason = if reconciled_changed {
            reconciled.reason
        } else {
            "agent-rename-request-pending-metadata".to_string()
        };
        let final_session = reconciled.session.unwrap_or(updated);
        return Ok(json!({
            "changed": pending_changed || reconciled_changed,
            "pendingAgentMetadata": true,
            "projection": project_session_title_projection(&final_session),
            "reason": reason,
            "session": final_session,
            "shouldSendAgentRenameCommand": true,
        }));
    }
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(
        "titleSource".to_string(),
        json!(read_text(params, "titleSource").unwrap_or_else(|| "user".to_string())),
    );
    let mut update = lifecycle_update(lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update.insert("title".to_string(), Value::String(title));
    let updated = repository.update_session(&update)?;
    Ok(json!({
        "changed": true,
        "pendingAgentMetadata": false,
        "projection": project_session_title_projection(&updated),
        "reason": "non-agent-title-applied",
        "session": updated,
        "shouldSendAgentRenameCommand": false,
    }))
}
