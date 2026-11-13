use serde_json::{json, Value};

use super::command::{
    flag_text, launch_settings_for, read_view, resolve_session, resolve_thread_session,
    server_flags, target_coordinator, text_or_file,
};
use super::delivery::{confirm_brief_started, BriefOutcome};
use crate::coordinators::{agent_message, thread_brief, BriefContext, MessageSender};
use crate::ghostex_cli::{
    agents,
    args::ParsedArgs,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

fn sender(row: &Value) -> MessageSender {
    let summary = agents::summary(row);
    MessageSender {
        agent_name: agents::text(&summary, "agentName").to_string(),
        title: agents::text(&summary, "title").to_string(),
        session_id: agents::text(&summary, "sessionId").to_string(),
        agent_id: agents::text(&summary, "agentId").to_string(),
        agent_session_id: agents::text(&summary, "agentSessionId").to_string(),
        global_ref: agents::text(&summary, "globalRef").to_string(),
    }
}

/// A session ref of another computer: its server, project and session ids, plus the flags that
/// reach that computer when it is connected with Agent access on (`None` while it is away).
struct RemoteRef {
    server_id: String,
    project_id: String,
    session_id: String,
    flags: Option<crate::ghostex_cli::args::Flags>,
}

/// `reference` is a session of another computer that this computer either links already (so a
/// thread whose computer is away can still be resolved or unlinked here) or can reach now.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// Deciding by "its computer is connected" left a thread on a computer that went away (or turned off Agent access) impossible to resolve or unlink: the command fell into the local path and failed. The coordinator's own computer holds the link, so its row is enough; only closing or waking the session there needs the computer.
fn remote_ref(reference: &str, linked_only: bool) -> Option<RemoteRef> {
    if !crate::ghostex_cli::rpc::is_gxserver_global_session_ref(reference) {
        return None;
    }
    let mut parts = reference.splitn(3, ':');
    let (server_id, project_id, session_id) = (parts.next()?, parts.next()?, parts.next()?);
    if crate::ghostex_cli::rpc::local_gxserver_server_id().as_deref() == Some(server_id) {
        return None;
    }
    let flags = crate::ghostex_cli::rpc::resolve_gxserver_server_target(
        &crate::ghostex_cli::args::Flags::default(),
        &json!({ "globalRef": reference }),
    )
    .ok()
    .filter(|target| target.kind == "computer")
    .map(|_| {
        let mut flags = crate::ghostex_cli::args::Flags::default();
        flags.insert_text("computer", server_id);
        flags
    });
    let linked = call_gxserver_rpc(
        "/api/listRemoteCoordinatorThreads",
        &json!({}),
        &crate::ghostex_cli::args::Flags::default(),
    )
    .ok()
    .and_then(|listed| listed.get("threads").and_then(Value::as_array).cloned())
    .is_some_and(|threads| {
        threads
            .iter()
            .any(|thread| thread.get("globalRef").and_then(Value::as_str) == Some(reference))
    });
    (linked || (!linked_only && flags.is_some())).then(|| RemoteRef {
        server_id: server_id.to_string(),
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        flags,
    })
}

fn update_remote_thread(
    server_id: &str,
    project_id: &str,
    session_id: &str,
    action: &str,
) -> CliResult<Value> {
    call_gxserver_rpc(
        "/api/updateRemoteCoordinatorThread",
        &json!({ "serverId": server_id, "projectId": project_id, "sessionId": session_id, "action": action }),
        &crate::ghostex_cli::args::Flags::default(),
    )
}

/// The flags that reach the computer `--computer` names, and its server id; this computer's own
/// flags and `None` without it (or when it names this computer).
fn thread_computer_flags(
    parsed: &ParsedArgs,
    local_flags: &crate::ghostex_cli::args::Flags,
) -> CliResult<(crate::ghostex_cli::args::Flags, Option<String>)> {
    let Some(computer) = flag_text(&parsed.flags, "computer") else {
        return Ok((local_flags.clone(), None));
    };
    let mut flags = local_flags.clone();
    flags.0.remove("server");
    flags.insert_text("computer", &computer);
    let target = crate::ghostex_cli::rpc::resolve_gxserver_server_target(&flags, &json!({}))?;
    if target.kind != "computer" {
        return Ok((local_flags.clone(), None));
    }
    Ok((flags, target.server_id))
}

/// The other computer's project for a thread started there without `--project-id`: the one with
/// the coordinator's project name, else an error that lists that computer's projects.
fn remote_project_like(
    coordinator_row: &Value,
    flags: &crate::ghostex_cli::args::Flags,
) -> CliResult<String> {
    let wanted = agents::text(coordinator_row, "projectName").to_lowercase();
    let projects = call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let projects: Vec<(String, String)> = projects
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let id = project
                .get("projectId")
                .and_then(Value::as_str)?
                .to_string();
            let name = project
                .get("name")
                .or_else(|| project.get("title"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some((id, name))
        })
        .collect();
    if let Some((id, _)) = projects
        .iter()
        .find(|(_, name)| !wanted.is_empty() && name.to_lowercase() == wanted)
    {
        return Ok(id.clone());
    }
    Err(CliError::Other(format!(
        "That computer has no project called \"{}\". Pass --project-id with one of its projects: {}.",
        agents::text(coordinator_row, "projectName"),
        projects
            .iter()
            .map(|(id, name)| format!("{name} ({id})"))
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// CDXC:Coordinators 2026-09-30 WHY:
/// The task reaches the thread as a message from the coordinator (so its chat shows who sent it and `Reply to` names the coordinator), wrapped with the goal, standing instructions and reporting rules (memory notes are the coordinator's own since 2026-11-07, brief.rs `thread_brief`). The thread is linked before its agent starts, so the supervisor sees its first turn from the beginning.
pub(super) fn start_thread(parsed: &ParsedArgs) -> CliResult<()> {
    let title = flag_text(&parsed.flags, "title")
        .ok_or_else(|| CliError::Other("start-thread needs --title \"<3 to 6 words>\".".into()))?;
    let task = text_or_file(parsed, "task")?
        .filter(|task| !task.trim().is_empty())
        .ok_or_else(|| {
            CliError::Other("start-thread needs --task \"<brief>\" or --body-file <path>.".into())
        })?;
    let (coordinator_row, flags) = target_coordinator(parsed)?;
    let view = read_view(&coordinator_row, &flags)?;
    let coordinator = &view["coordinator"];
    let brief = thread_brief(
        &BriefContext {
            goal: agents::text(coordinator, "goal"),
            instructions: agents::text(coordinator, "instructions"),
            worktree: parsed.flags.truthy("worktree"),
        },
        &task,
    );
    let mut sender_row = coordinator_row.clone();
    agents::resolve_names(std::slice::from_mut(&mut sender_row), &flags);
    let message = agent_message(&sender(&sender_row), &brief);
    if message.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(CliError::Other(
            "The task with its standing instructions is too long to send. Put the details in a file and point the thread at it.".into(),
        ));
    }
    // `--computer` runs the thread on a connected computer; the coordinator stays here.
    let local_flags = flags.clone();
    let (flags, remote_server_id) = thread_computer_flags(parsed, &local_flags)?;
    // `--path <folder>`: gxserver adds the folder as a project (on the `--computer` one too) and
    // starts the thread there (gbots/project.rs `add_create_project_path`).
    let folder = flag_text(&parsed.flags, "path");
    let project_id = match (flag_text(&parsed.flags, "projectId"), &remote_server_id) {
        (Some(project_id), _) => project_id,
        (None, _) if folder.is_some() => String::new(),
        (None, None) => agents::text(coordinator, "projectId").to_string(),
        (None, Some(_)) => remote_project_like(&coordinator_row, &flags)?,
    };
    let agent_id = flag_text(&parsed.flags, "agent")
        .unwrap_or_else(|| agents::text(coordinator, "agentId").to_string());
    let agent_row =
        agents::require_launchable_agent(&agent_id, parsed.flags.truthy("allowHidden"), &flags)?;
    let agent_rows = [agent_row];
    let mut create = json!({
        "projectId": project_id,
        "agentId": agent_id,
        "launchSettings": launch_settings_for(&agent_rows, &agent_id),
        "title": title,
        "markedHandovers": true,
    });
    if project_id.is_empty() {
        if let Some(folder) = &folder {
            create["projectPath"] = json!(folder);
        }
    }
    if let Some(model) = flag_text(&parsed.flags, "model") {
        create["agentModel"] = json!(model);
    }
    if let Some(effort) = flag_text(&parsed.flags, "effort") {
        create["agentEffort"] = json!(effort);
    }
    // gxserver resolves and checks it the way switch-account does (accounts/launch.rs `requested_account`).
    if let Some(account) = flag_text(&parsed.flags, "account") {
        create["account"] = json!(account);
    }
    // `--read-only`: gxserver starts the agent in its read-only mode, or refuses an agent that has
    // none (CDXC:Coordinators 2026-11-13 in server/src/coordinators/read_only.rs).
    let read_only = parsed.flags.truthy("readOnly");
    if read_only {
        create[crate::coordinators::READ_ONLY_THREAD_PARAM] = json!(true);
    }
    let worktree = parsed.flags.truthy("worktree");
    let (session_id, branch, worktree_path, created_project) = if worktree {
        if let Some(base) = flag_text(&parsed.flags, "baseBranch") {
            create["baseBranch"] = json!(base);
        }
        // A worktree thread's agent starts inside this create, before the link below is written,
        // so its coordinator markers name the coordinator here (coordinators/launch_env.rs). Not
        // on another computer: that one keeps no record of the link (issue #463).
        let coordinator_ref = agents::text(&coordinator_row, "globalRef");
        if remote_server_id.is_none() && !coordinator_ref.is_empty() {
            create[crate::coordinators::COORDINATOR_THREAD_OF_PARAM] = json!(coordinator_ref);
        }
        // There it is kept on the session instead, so the worktree it gets is trusted like a local
        // thread's (CDXC:Coordinators 2026-11-08 at `coordinator_thread_folder_trusted`) and its
        // agent starts with the thread markers (2026-11-09, GitHub #523).
        if remote_server_id.is_some() && !coordinator_ref.is_empty() {
            create[crate::coordinators::REMOTE_COORDINATOR_THREAD_OF_PARAM] =
                json!(coordinator_ref);
        }
        let created = call_gxserver_rpc("/api/createWorktreeSession", &create, &flags)?;
        if read_only && created["readOnly"] != true {
            undo_unrestricted_thread(&created, &flags)?;
        }
        (
            agents::text(&created, "sessionId").to_string(),
            Some(agents::text(&created, "branch").to_string()),
            Some(agents::text(&created, "worktreePath").to_string()),
            agents::text(&created, "projectId").to_string(),
        )
    } else {
        // A plain thread on another computer names its coordinator the same way, in the session's
        // runtime settings, which `createAgentSession` stores as given (GitHub #523).
        let coordinator_ref = agents::text(&coordinator_row, "globalRef");
        if remote_server_id.is_some() && !coordinator_ref.is_empty() {
            create["runtimeSettings"] = json!({
                crate::coordinators::REMOTE_COORDINATOR_THREAD_KEY: coordinator_ref,
            });
        }
        let created = call_gxserver_rpc("/api/createAgentSession", &create, &flags)?;
        if read_only
            && created["session"]["runtimeSettings"][crate::coordinators::READ_ONLY_THREAD_KEY]
                != true
        {
            undo_unrestricted_thread(&created["session"], &flags)?;
        }
        (
            agents::text(&created["session"], "sessionId").to_string(),
            None,
            None,
            agents::text(&created["session"], "projectId").to_string(),
        )
    };
    // The project gxserver added for `--path`.
    let project_id = if project_id.is_empty() {
        created_project
    } else {
        project_id
    };
    if session_id.is_empty() {
        return Err(CliError::Other(
            "The create response has no session id. Run ghostex coordinator status before retrying.".into(),
        ));
    }
    // The brief's send id: its delivery watch keeps it, and the confirmation below names it.
    let brief_send_id = uuid::Uuid::new_v4().to_string();
    let linked = match &remote_server_id {
        Some(server_id) => call_gxserver_rpc(
            "/api/linkRemoteCoordinatorThread",
            &json!({
                "coordinatorProjectId": coordinator["projectId"],
                "coordinatorSessionId": coordinator["sessionId"],
                "serverId": server_id,
                "projectId": project_id,
                "sessionId": session_id,
                "task": task.trim(),
                "title": title,
            }),
            &local_flags,
        )?,
        None => call_gxserver_rpc(
            "/api/linkCoordinatorThread",
            &json!({
                "coordinatorProjectId": coordinator["projectId"],
                "coordinatorSessionId": coordinator["sessionId"],
                "projectId": project_id,
                "sessionId": session_id,
                "task": task.trim(),
                "pendingMessage": task.trim(),
                "sendRequestId": brief_send_id,
            }),
            &flags,
        )?,
    };
    let reference = agents::text(&linked, "globalRef").to_string();
    if !worktree {
        call_gxserver_rpc(
            "/api/startSessionProvider",
            &json!({"globalRef": reference, "projectId": project_id, "sessionId": session_id}),
            &flags,
        )
        .map_err(|error| {
            CliError::Other(format!(
                "Started thread {reference}, but its agent did not start: {error}. Check ghostex coordinator status; do not start another."
            ))
        })?;
    }
    let queued = call_gxserver_rpc(
        "/api/queueSessionChatPrompt",
        &json!({
            "globalRef": reference, "projectId": project_id, "sessionId": session_id,
            "text": message, "startupSend": true, "sendRequestId": brief_send_id,
        }),
        &flags,
    )
    .map_err(|error| {
        CliError::Other(format!(
            "Started thread {reference}, but its task was not delivered: {error}. Send it with ghostex agents send {reference} --body-file <file>; do not start another."
        ))
    })?;
    let thread =
        json!({ "globalRef": reference, "projectId": project_id, "sessionId": session_id });
    let outcome = confirm_brief_started(
        &thread,
        &task,
        queued.pointer("/prompt/id").and_then(Value::as_str),
        &flags,
    )?;
    if matches!(outcome, BriefOutcome::Started) && remote_server_id.is_none() {
        agents::confirm_coordinator_delivery(coordinator, &thread, &brief_send_id, &flags);
    }
    let (status, note) = match &outcome {
        BriefOutcome::Started => ("started", "Its transcript shows the brief: it is working on it.".to_string()),
        BriefOutcome::Pending(reason) => ("pending", format!("Its brief has not started yet: {reason}. Ghostex keeps watching and reports to you if it never arrives; do not start another thread or resend the brief.")),
    };
    let mut result = json!({
        "ok": true,
        "status": status,
        "note": note,
        "thread": { "globalRef": reference, "projectId": project_id, "sessionId": session_id, "title": title },
    });
    if read_only {
        result["thread"]["readOnly"] = json!(true);
    }
    if let Some(branch) = branch.filter(|branch| !branch.is_empty()) {
        result["thread"]["branch"] = json!(branch);
    }
    if let Some(path) = worktree_path.filter(|path| !path.is_empty()) {
        result["thread"]["worktreePath"] = json!(path);
    }
    if let Some(computer) =
        flag_text(&parsed.flags, "computer").filter(|_| remote_server_id.is_some())
    {
        result["thread"]["computer"] = json!(computer);
    }
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        let where_ = result["thread"]["branch"]
            .as_str()
            .map(|branch| format!(" on branch {branch}"))
            .unwrap_or_default();
        let kind = if read_only {
            "read-only thread"
        } else {
            "thread"
        };
        match status {
            "started" => println!("Started {kind} \"{title}\" ({reference}){where_}. {note}"),
            _ => println!("Created {kind} \"{title}\" ({reference}){where_}, pending: {note}"),
        }
        println!("Its report comes back to you when it finishes a turn; end your turn now instead of waiting.");
    }
    Ok(())
}

/// A `--read-only` thread whose gxserver did not say it started read-only (another computer on a
/// Ghostex from before read-only threads ignores the request): its session is ended and removed
/// before it gets the brief, and start-thread fails instead of handing work to an agent with full
/// rights.
fn undo_unrestricted_thread(
    created: &Value,
    flags: &crate::ghostex_cli::args::Flags,
) -> CliResult<()> {
    let session = json!({
        "projectId": agents::text(created, "projectId"),
        "sessionId": agents::text(created, "sessionId"),
    });
    let _ = call_gxserver_rpc("/api/killSession", &session, flags);
    let _ = call_gxserver_rpc("/api/removeSession", &session, flags);
    Err(CliError::Other(
        "Not started: that computer's Ghostex is older than read-only threads, so its agent would have had full rights. Its session was removed and no brief was sent. Update Ghostex there, or start the thread without --read-only.".into(),
    ))
}

pub(super) fn link(parsed: &ParsedArgs) -> CliResult<()> {
    let reference = parsed.rest.first().cloned().ok_or_else(|| {
        CliError::Other("Usage: ghostex coordinator link <session-ref> [--task <text>]".into())
    })?;
    let (coordinator, coordinator_flags) = target_coordinator(parsed)?;
    if let Some(remote) = remote_ref(&reference, false) {
        let result = call_gxserver_rpc(
            "/api/linkRemoteCoordinatorThread",
            &json!({
                "coordinatorProjectId": coordinator["projectId"],
                "coordinatorSessionId": coordinator["sessionId"],
                "serverId": remote.server_id, "projectId": remote.project_id, "sessionId": remote.session_id,
                "task": flag_text(&parsed.flags, "task").unwrap_or_default(),
            }),
            &coordinator_flags,
        )?;
        if parsed.flags.truthy("json") {
            print_json(&result);
        } else {
            println!(
                "Linked {reference} (on another computer) as a thread of {}.",
                agents::text(&coordinator, "title")
            );
        }
        return Ok(());
    }
    let (thread, flags) = resolve_session(&reference, &server_flags(&parsed.flags))?;
    let result = call_gxserver_rpc(
        "/api/linkCoordinatorThread",
        &json!({
            "coordinatorProjectId": coordinator["projectId"],
            "coordinatorSessionId": coordinator["sessionId"],
            "projectId": thread["projectId"],
            "sessionId": thread["sessionId"],
            "task": flag_text(&parsed.flags, "task").unwrap_or_default(),
        }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        println!(
            "Linked {} as a thread of {}.",
            agents::text(&thread, "globalRef"),
            agents::text(&coordinator, "title")
        );
    }
    Ok(())
}

/// `unlink` takes one of the orchestrator's threads out of its tree: gxserver deletes the thread
/// record and the session becomes an ordinary one in its project, left running as it is
/// (server/src/coordinators/endpoint.rs).
///
/// CDXC:Coordinators 2026-10-15 DECISION:
/// User: "a new thread ... that i just chat to about this ... maybe could be a thread not actually under you but separate in the sidebar, better." An orchestrator can start a session that is not its thread (`ghostex agents create --standalone`) and detach one that is (`unlink`), so a session the user chats to directly neither nests under the orchestrator nor sends it turn-end reports.
pub(super) fn unlink(parsed: &ParsedArgs) -> CliResult<()> {
    let [reference] = parsed.rest.as_slice() else {
        return Err(CliError::Other(
            "Usage: ghostex coordinator unlink <thread-ref> [--coordinator <ref>] [--json]".into(),
        ));
    };
    let (coordinator, _) = target_coordinator(parsed)?;
    if let Some(remote) = remote_ref(reference, true) {
        let result = update_remote_thread(
            &remote.server_id,
            &remote.project_id,
            &remote.session_id,
            "unlink",
        )?;
        if parsed.flags.truthy("json") {
            print_json(&result);
        } else {
            println!("{reference} is no longer a thread of {}; its session keeps running on its computer.", agents::text(&coordinator, "title"));
        }
        return Ok(());
    }
    let (thread, flags) = resolve_thread_session(reference, &server_flags(&parsed.flags))?;
    let global_ref = agents::text(&thread, "globalRef").to_string();
    let result = call_gxserver_rpc(
        "/api/unlinkCoordinatorThread",
        &json!({
            "coordinatorProjectId": coordinator["projectId"],
            "coordinatorSessionId": coordinator["sessionId"],
            "projectId": thread["projectId"],
            "sessionId": thread["sessionId"],
        }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else {
        println!(
            "Unlinked {global_ref}: it is a standalone session now, no longer a thread of {}. Its session was left as it is.",
            agents::text(&coordinator, "title")
        );
    }
    Ok(())
}

/// `resolve` marks a thread done and closes its session (`--keep-open` parks it instead);
/// `reopen` resumes a closed thread's session, then puts it back in its coordinator's tree.
pub(super) fn set_resolved(parsed: &ParsedArgs, resolved: bool) -> CliResult<()> {
    let verb = if resolved { "resolve" } else { "reopen" };
    let reference = parsed.rest.first().cloned().ok_or_else(|| {
        CliError::Other(format!("Usage: ghostex coordinator {verb} <thread-ref>"))
    })?;
    if let Some(remote) = remote_ref(&reference, true) {
        let close_session = resolved && !parsed.flags.truthy("keepOpen");
        let result = update_remote_thread(
            &remote.server_id,
            &remote.project_id,
            &remote.session_id,
            if resolved { "resolve" } else { "reopen" },
        )?;
        let session = json!({ "projectId": remote.project_id, "sessionId": remote.session_id });
        // Its own computer closes or wakes the session, as for a thread here; while it is away
        // only the link here changes.
        let session_result = match (&remote.flags, close_session, resolved) {
            (None, true, _) | (None, _, false) => Err(CliError::Other("away".into())),
            (Some(flags), true, _) => {
                call_gxserver_rpc("/api/killSession", &session, flags).map(|_| ())
            }
            (Some(flags), false, false) => {
                call_gxserver_rpc("/api/wakeSession", &session, flags).map(|_| ())
            }
            _ => Ok(()),
        };
        let away = remote.flags.is_none();
        if parsed.flags.truthy("json") {
            let mut result = result;
            result["computerConnected"] = json!(!away);
            print_json(&result);
        } else {
            match (resolved, close_session, away, &session_result) {
                (true, true, true, _) | (false, _, true, _) => println!(
                    "{} {reference} here. Its computer isn't connected (or its Agent access is off), so its session there was left as it is.",
                    if resolved { "Marked done" } else { "Reopened" }
                ),
                (true, true, false, Ok(())) => {
                    println!("Marked {reference} done and closed its session on its computer.")
                }
                (true, true, false, Err(error)) => println!(
                    "Marked {reference} done; its session on its computer did not close: {error}"
                ),
                (true, false, _, _) => {
                    println!("Marked {reference} done; its session stays open on its computer.")
                }
                (false, _, false, _) => println!("Reopened {reference}."),
            }
        }
        return Ok(());
    }
    let (thread, flags) = resolve_thread_session(&reference, &server_flags(&parsed.flags))?;
    let global_ref = agents::text(&thread, "globalRef").to_string();
    let close_session = resolved && !parsed.flags.truthy("keepOpen");
    // gxserver wakes a thread it reopens and closes one it marks done
    // (server/src/server/coordinator_resolve.rs), the same for every client.
    let result = call_gxserver_rpc(
        "/api/resolveCoordinatorThread",
        &json!({
            "projectId": thread["projectId"],
            "sessionId": thread["sessionId"],
            "resolved": resolved,
            "keepOpen": !close_session,
        }),
        &flags,
    )
    .map_err(|error| match (resolved, &error) {
        (true, CliError::Rpc { response, .. })
            if response.get("error").and_then(Value::as_str) == Some("sessionNotClosed") =>
        {
            CliError::Other(format!(
                "{error} Close it with ghostex agents close {global_ref}."
            ))
        }
        (true, _) => error,
        (false, _) => CliError::Other(format!("Could not reopen {global_ref}: {error}")),
    })?;
    let resumed = result["sessionResumed"].as_bool() == Some(true);
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else if close_session {
        println!(
            "Marked {global_ref} done and closed its session. `ghostex coordinator reopen {global_ref}` or a message resumes the same conversation."
        );
    } else if resolved {
        println!("Marked {global_ref} done; its session stays open (parked).");
    } else if result["sessionResuming"].as_bool() == Some(true) {
        println!("Reopened {global_ref}; its session is still starting and resumes on its own.");
    } else if resumed {
        println!("Reopened {global_ref} and resumed its session.");
    } else {
        println!("Reopened {global_ref}.");
    }
    Ok(())
}

/// `rename` gives one of the orchestrator's threads a new title through gxserver, which refuses
/// a thread the user renamed themselves (server/src/coordinators/rename.rs).
pub(super) fn rename(parsed: &ParsedArgs) -> CliResult<()> {
    let [reference, title] = parsed.rest.as_slice() else {
        return Err(CliError::Other(
            "Usage: ghostex coordinator rename <thread-ref> \"<title>\" [--coordinator <ref>] [--json]"
                .into(),
        ));
    };
    let (coordinator, _) = target_coordinator(parsed)?;
    let (thread, flags) = resolve_thread_session(reference, &server_flags(&parsed.flags))?;
    let global_ref = agents::text(&thread, "globalRef").to_string();
    let result = call_gxserver_rpc(
        "/api/renameCoordinatorThread",
        &json!({
            "coordinatorProjectId": coordinator["projectId"],
            "coordinatorSessionId": coordinator["sessionId"],
            "projectId": thread["projectId"],
            "sessionId": thread["sessionId"],
            "title": title,
        }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else if result["changed"] == json!(true) {
        println!(
            "Renamed {global_ref} from \"{}\" to \"{}\".",
            agents::text(&result, "previousTitle"),
            agents::text(&result, "title")
        );
    } else {
        println!("{global_ref} is already called \"{title}\".");
    }
    Ok(())
}

/// `rename-self` is a thread renaming its own session when its orchestrator starts it on a task
/// its title does not describe; gxserver refuses it when the user renamed the thread
/// (server/src/coordinators/rename.rs).
pub(super) fn rename_self(parsed: &ParsedArgs) -> CliResult<()> {
    let [title] = parsed.rest.as_slice() else {
        return Err(CliError::Other(
            "Usage: ghostex coordinator rename-self \"<title>\" [--json]".into(),
        ));
    };
    let caller = agents::caller()?;
    let global_ref = agents::text(&caller, "globalRef").to_string();
    let flags = agents::inventory_flags(&server_flags(&parsed.flags), &global_ref)?;
    let result = call_gxserver_rpc(
        "/api/renameCoordinatorThread",
        &json!({
            "projectId": caller["projectId"],
            "sessionId": caller["sessionId"],
            "selfRename": true,
            "title": title,
        }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&result);
    } else if result["changed"] == json!(true) {
        println!(
            "Renamed this thread from \"{}\" to \"{}\".",
            agents::text(&result, "previousTitle"),
            agents::text(&result, "title")
        );
    } else {
        println!("This thread is already called \"{title}\".");
    }
    Ok(())
}
