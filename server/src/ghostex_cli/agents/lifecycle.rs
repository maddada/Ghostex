use super::{
    arguments::Arguments,
    identity::{self, text},
};
use crate::ghostex_cli::{
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector, sessions,
};
use serde_json::{json, Value};

pub(super) fn types(args: &Arguments) -> CliResult<Value> {
    let hud = call_gxserver_rpc("/api/readSidebarHud", &json!({}), &args.flags)?;
    let agents = hud
        .get("agents")
        .and_then(Value::as_array)
        .ok_or_else(|| CliError::Other("The server did not return agent types.".into()))?;
    Ok(
        json!({"ok": true, "agents": agents.iter().map(|agent| json!({
        "agentId": agent["agentId"], "name": agent["name"], "command": agent["command"],
    })).collect::<Vec<_>>()}),
    )
}

/// CDXC:Cli 2026-09-17 DECISION:
/// User: agents help must explain how to choose and create agents and close other sessions. Reuse configured launcher types and the existing provider startup and close operations.
pub(super) fn create(args: &Arguments) -> CliResult<Value> {
    let body = match &args.body_file {
        Some(path) => {
            Some(crate::ghostex_cli::args::read_body_file(path, "task").map_err(CliError::Other)?)
        }
        None => args.task.clone(),
    };
    if body.as_ref().is_some_and(|body| body.trim().is_empty()) {
        return Err(CliError::Other("Task must not be empty.".into()));
    }
    let standing_role = args
        .role
        .as_deref()
        .map(super::role::validate_role)
        .transpose()?;
    let caller = if args.project_id.is_none() || body.is_some() {
        Some(identity::caller()?)
    } else {
        None
    };
    let flags = if args.project_id.is_none() {
        identity::inventory_flags(&args.flags, text(caller.as_ref().unwrap(), "globalRef"))?
    } else {
        args.flags.clone()
    };
    let project = args
        .project_id
        .as_deref()
        .unwrap_or_else(|| text(caller.as_ref().unwrap(), "projectId"));
    let message = body
        .as_ref()
        .map(|body| identity::message(caller.as_ref().unwrap(), body));
    if message
        .as_ref()
        .is_some_and(|text| text.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES)
    {
        return Err(CliError::Other(
            "Task including sender header exceeds the send limit.".into(),
        ));
    }
    let agent_id = &args.positional[0];
    super::visibility::require_launchable_agent(agent_id, args.allow_hidden, &flags)?;
    let mut payload = json!({"projectId": project, "agentId": agent_id});
    // `--path <folder>`: gxserver adds the folder as a project and starts the session there
    // (gbots/project.rs `add_create_project_path`).
    if let (None, Some(folder)) = (&args.project_id, &args.path) {
        payload = json!({"projectPath": folder, "agentId": agent_id});
    }
    // `--no-project`: the No project project, its chat in the home folder (no_project/project.rs).
    if args.project_id.is_none() && args.flags.truthy("noProject") {
        payload = json!({"noProject": true, "agentId": agent_id});
    }
    if let Some(title) = &args.title {
        payload["title"] = json!(title);
        // CDXC:SessionTitles 2026-11-26 FIX: GitHub #755: Native Antigravity names its conversation
        // itself and replaced a `--title` name within 3 s of the first message; a Native agent's
        // own name gives way only to one the user chose (agents/title_reconcile.rs), so the name
        // given here is recorded as theirs, as a rename is.
        payload["runtimeSettings"] = json!({"titleSource": "user"});
    }
    // gxserver checks them against the agent as it does for `create-agent --model/--effort`.
    if let Some(model) = &args.model {
        payload["agentModel"] = json!(model);
    }
    if let Some(effort) = &args.effort {
        payload["agentEffort"] = json!(effort);
    }
    if let Some(role) = &standing_role {
        payload["standingRole"] = json!(role);
    }
    if args.chrome {
        payload[crate::acp::claude_chrome::CLAUDE_CHROME_PARAM] = json!(true);
    }
    // `--lead`: the link rules are checked before anything is created.
    let lead = args
        .lead
        .as_deref()
        .map(|lead| super::leads::lead_for_new_session(lead, &args.flags))
        .transpose()?;
    let created = call_gxserver_rpc("/api/createAgentSession", &payload, &flags)?;
    let session = created.get("session").ok_or_else(|| {
        CliError::Other(
            "Create response has no session. Inspect agents list before retrying.".into(),
        )
    })?;
    let reference = text(session, "globalRef");
    if text(session, "sessionId").is_empty() || reference.is_empty() {
        return Err(CliError::Other(
            "Create response lacks a session reference. Inspect agents list before retrying."
                .into(),
        ));
    }
    // `--lead`: linked before its first start, so the link can make it a lean Codex worker
    // (agents/codex_lean.rs) and the turn its task starts is the lead's.
    let lead_link = lead.as_ref().map(|lead| {
        super::leads::link_new_session(session, lead, body.is_some(), &args.flags).map_err(
            |error| {
                let error = error.to_string();
                format!(
                    "Created {reference}, but linking it to the lead failed: {}. Do not create another session.",
                    error.trim_end_matches('.')
                )
            },
        )
    });
    // `--role`: stored atomically by `/api/createAgentSession` on current gxservers; fall back to
    // `/api/sessionStandingRole` only if an older remote gxserver ignored `standingRole`.
    let role_set = if let Some(saved) = created.get("standingRole").and_then(Value::as_str) {
        Some(Ok(json!({ "role": saved })))
    } else {
        standing_role.as_deref().map(|role| {
            super::role::set_session_role(session, role, &flags).map_err(|error| {
                let error = error.to_string();
                format!(
                    "Created {reference}, but setting its standing role failed: {}. Do not create another session.",
                    error.trim_end_matches('.')
                )
            })
        })
    };
    // Retain the created identity if startup fails so a retry does not create a duplicate.
    let provider = call_gxserver_rpc("/api/startSessionProvider", &json!({
        "globalRef": reference, "projectId": session["projectId"], "sessionId": session["sessionId"],
    }), &flags).map_err(|error| CliError::Other(format!("Created {reference}, but provider startup failed or is uncertain: {error}. Inspect this session before retrying; do not create another session.")))?;
    let live_session = provider
        .get("session")
        .filter(|value| value.is_object())
        .unwrap_or(session);
    let mut result = json!({"ok": true, "status": "created", "globalRef": reference, "session": identity::summary(live_session)});
    if let Some(role_set) = role_set {
        match role_set {
            Ok(saved) => result["role"] = saved["role"].clone(),
            Err(error) => result["roleError"] = json!(error),
        }
    }
    // CDXC:Coordinators 2026-09-30 WHY: a coordinator that starts a worker the `$ghostex-agents` way still gets it as a tracked thread; for any other caller the link is a no-op.
    // CDXC:Coordinators 2026-10-15 DECISION: User: "a new thread ... that i just chat to about this ... maybe could be a thread not actually under you but separate in the sidebar, better." `--standalone` skips the link, so the session is a plain sidebar row that reports to nobody.
    if let Some(lead_link) = lead_link {
        // `--lead`: a lead link instead of a coordinator thread (agents/lead_links in gxserver),
        // made before the task is queued so the turn the task starts is the lead's.
        match lead_link {
            Ok(linked) => {
                result["lead"] = linked["leadGlobalRef"].clone();
                result["leadTitle"] = linked["leadTitle"].clone();
            }
            Err(error) => result["leadLinkError"] = json!(error),
        }
    } else if args.standalone {
        result["coordinatorThread"] = json!(false);
    } else if let (true, Some(caller)) = (
        flags.contains("computer"),
        caller.clone().or_else(|| identity::caller().ok()),
    ) {
        // A session on another computer becomes a thread of a coordinator here, linked here.
        let mut local = flags.clone();
        local.0.remove("computer");
        let linked = call_gxserver_rpc(
            "/api/linkRemoteCoordinatorThread",
            &json!({
                "coordinatorProjectId": caller["projectId"], "coordinatorSessionId": caller["sessionId"],
                "serverId": reference.split(':').next().unwrap_or_default(),
                "projectId": session["projectId"], "sessionId": session["sessionId"],
                "task": body.as_deref().unwrap_or_default(), "title": args.title.as_deref().unwrap_or_default(),
                "onlyIfCoordinator": true,
            }),
            &local,
        );
        if linked.is_ok_and(|linked| linked["linked"] == json!(true)) {
            result["coordinatorThread"] = json!(true);
        }
    } else if let Some(caller) = caller.clone().or_else(|| identity::caller().ok()) {
        let linked = call_gxserver_rpc(
            "/api/linkCoordinatorThread",
            &json!({
                "coordinatorProjectId": caller["projectId"], "coordinatorSessionId": caller["sessionId"],
                "projectId": session["projectId"], "sessionId": session["sessionId"],
                "task": body.as_deref().unwrap_or_default(), "onlyIfCoordinator": true,
                "pendingMessage": body.as_deref().unwrap_or_default(),
                // This session was just made for the coordinator (work_cleanup_threads.rs).
                crate::coordinators::STARTED_BY_COORDINATOR_KEY: true,
            }),
            &flags,
        );
        if linked.is_ok_and(|linked| linked["linked"] == json!(true)) {
            result["coordinatorThread"] = json!(true);
        }
    }
    if let Some(message) = message {
        let receipt = call_gxserver_rpc("/api/queueSessionChatPrompt", &json!({
            "globalRef": reference, "projectId": session["projectId"], "sessionId": session["sessionId"],
            "text": message, "startupSend": true, "sendRequestId": uuid::Uuid::new_v4().to_string(),
        }), &flags).map_err(|error| CliError::Other(format!("Created {reference}, but task delivery failed or is uncertain: {error}. Inspect this session and its queue; do not create another session to retry.")))?;
        result["taskStatus"] = json!("queued");
        // The sender's chat card shows the task from here (see `message` in delivery.rs).
        result["task"] = json!(body);
        result["receipt"] = super::delivery::receipt(&receipt);
    }
    Ok(result)
}

pub(super) fn close(args: &Arguments) -> CliResult<Value> {
    let reference = &args.positional[0];
    let flags = identity::inventory_flags(&args.flags, reference)?;
    let rows = sessions::fetch_session_list(&flags, false)?;
    let recipient = selector::resolve_one_listed_session(reference, &rows, &flags)?;
    if !identity::is_agent(&recipient) {
        return Err(CliError::Other(
            "The target is not an agent session.".into(),
        ));
    }
    let mut params = json!({
        "globalRef": recipient["globalRef"], "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
    });
    if let Some(params) = params.as_object_mut() {
        crate::ghostex_cli::wait::insert_close_source(params, &args.flags, "cli.agentsClose");
    }
    let result = call_gxserver_rpc("/api/killSession", &params, &flags)?;
    Ok(
        json!({"ok": true, "status": "closed", "session": identity::summary(&recipient), "receipt": super::delivery::receipt(&result)}),
    )
}

/// The agent session `reference` names, from the session list on its server.
fn agent_recipient(args: &Arguments) -> CliResult<(Value, crate::ghostex_cli::args::Flags)> {
    let reference = &args.positional[0];
    let mut flags = identity::inventory_flags(&args.flags, reference)?;
    let rows = sessions::fetch_session_list(&flags, false)?;
    let recipient = selector::resolve_one_listed_session(reference, &rows, &flags)?;
    if !identity::is_agent(&recipient) {
        return Err(CliError::Other(
            "The target is not an agent session.".into(),
        ));
    }
    // gxserver waits for the restart before it answers.
    flags.insert_text("timeout", "120000");
    Ok((recipient, flags))
}

/// `ghostex agents switch-account`: `/api/agentAccounts` `switchSession`, the chat's own Switch
/// Account (server/src/accounts/cli.rs).
pub(super) fn switch_account(args: &Arguments) -> CliResult<Value> {
    let (recipient, flags) = agent_recipient(args)?;
    let result = call_gxserver_rpc(
        "/api/agentAccounts",
        &json!({
            "operation": "switchSession",
            "globalRef": recipient["globalRef"], "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
            "account": args.account, "force": args.force, "userNamed": args.user_named,
        }),
        &flags,
    )
    .map_err(|error| {
        let message = error.to_string();
        if message.contains("Stop the current turn") {
            CliError::Other(format!("{message} Interrupt it with ghostex interrupt-session-chat {0}, restart it with ghostex agents reload {0}, or pass --force to end the turn, switch, and send \".\" so it carries on (its current turn is lost).", text(&recipient, "globalRef")))
        } else {
            error
        }
    })?;
    let mut result = result;
    result["ok"] = json!(result["status"] != "failed");
    Ok(result)
}

/// `ghostex agents reload`: `/api/reloadSessionAgent` (server/src/session_chat_agent_restart.rs).
pub(super) fn reload(args: &Arguments) -> CliResult<Value> {
    let (recipient, flags) = agent_recipient(args)?;
    let mut result = call_gxserver_rpc(
        "/api/reloadSessionAgent",
        &json!({
            "globalRef": recipient["globalRef"], "projectId": recipient["projectId"], "sessionId": recipient["sessionId"],
        }),
        &flags,
    )?;
    result["ok"] = json!(true);
    Ok(result)
}
