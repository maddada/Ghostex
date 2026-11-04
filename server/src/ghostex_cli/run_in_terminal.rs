//! `ghostex run-in-terminal`: a chat's Run in terminal for the phone (and anyone at a shell).
//!
//! CDXC:SessionChat 2026-10-24 WHY: the phone reaches this computer only through the CLI, so its Run in terminal is this verb. Like the desktop (User: "please make clicking on the floating terminal button that we added add that command to a terminal in the commands panel not a new terminal session in the sidebar"), it types into the project's Commands panel terminal when one is running and idle, which the phone then opens as its tab. Without one it makes a new plain terminal in the chat session's own folder rather than a Commands panel session, which the desktop's panel, whose layout is its own, would never show. It never types into the agent's terminal, where the agent would read the command as a prompt. The output names the terminal it used.
//! SEE-ALSO: `command_terminal_create_params` in packages/gx-core/src/session_create/params.rs builds the same create for the desktop; packages/gx-chat-core/src/transcript/run_command.rs decides what runs.

use serde_json::{json, Value};

use crate::ghostex_cli::actions::resolve_cli_cwd;
use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/// A terminal's title: the command's first line, cut to the length a sidebar row shows.
fn terminal_title(command: &str) -> String {
    let title: String = command
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(60)
        .collect();
    if title.is_empty() {
        "Terminal".to_string()
    } else {
        title
    }
}

/// The `/api/createSession` params of the terminal: one that runs `command` once its shell is up,
/// or (`paste`) a plain one the command is typed into afterwards.
fn run_in_terminal_payload(
    project_id: &str,
    cwd: Option<&str>,
    command: &str,
    paste: bool,
) -> Value {
    let mut params = json!({
        "kind": "terminal",
        "lifecycleState": "running",
        "projectId": project_id,
        "surface": "workspace",
        "title": terminal_title(command),
    });
    if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
        params["cwd"] = json!(cwd);
    }
    if !paste {
        params["launchSettings"] = json!({
            "startupText": command,
            "runtimeRelevant": { "queueProviderStartupText": true },
        });
    }
    params
}

/// The folder the chat session works in, from the presentation snapshot; `None` lets gxserver use
/// the project's folder.
fn session_cwd(
    project_id: &str,
    session_id: &str,
    flags: &crate::ghostex_cli::args::Flags,
) -> Option<String> {
    let snapshot = call_gxserver_rpc("/api/readPresentationSnapshot", &json!({}), flags).ok()?;
    snapshot["snapshot"]["sessions"]
        .as_array()?
        .iter()
        .find(|session| {
            session["projectId"].as_str() == Some(project_id)
                && session["sessionId"].as_str() == Some(session_id)
        })
        .and_then(|session| session["cwd"].as_str())
        .map(str::to_string)
}

/// Waits (up to 5 seconds) until the new terminal's screen shows something, which for a plain shell
/// is its prompt.
fn wait_for_prompt(project_id: &str, session_id: &str, flags: &crate::ghostex_cli::args::Flags) {
    let ids = json!({ "projectId": project_id, "sessionId": session_id });
    for _ in 0..33 {
        let shown = call_gxserver_rpc("/api/readSessionText", &ids, flags)
            .ok()
            .and_then(|read| read["text"].as_str().map(|text| !text.trim().is_empty()));
        if shown == Some(true) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}

/// The project's Commands panel terminal a command goes to: a running plain terminal of the panel
/// (not an Action's) that is not busy, the most recently used first. Panel terminals are left out of
/// the presentation snapshot, so they are read from the session list.
fn idle_command_panel_terminal(
    project_id: &str,
    flags: &crate::ghostex_cli::args::Flags,
) -> Option<String> {
    let listed = call_gxserver_rpc(
        "/api/listSessions",
        &json!({ "includeStopped": false }),
        flags,
    )
    .ok()?;
    let text = |session: &Value, key: &str| session[key].as_str().unwrap_or_default().to_string();
    listed["sessions"]
        .as_array()?
        .iter()
        .filter(|session| {
            text(session, "projectId") == project_id
                && text(session, "surface") == "commands"
                && text(session, "kind") == "terminal"
                && text(session, "commandId").is_empty()
                && text(session, "lifecycleState") == "running"
                && session["runtimeSettings"]["agentActivity"]["activity"].as_str()
                    != Some("working")
        })
        .max_by_key(|session| text(session, "updatedAt"))
        .map(|session| text(session, "sessionId"))
        .filter(|session_id| !session_id.is_empty())
}

pub fn run_in_terminal_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let project_id = flags
        .text("projectId")
        .unwrap_or_default()
        .trim()
        .to_string();
    let command = flags
        .text("command")
        .unwrap_or_else(|| parsed.rest.join(" "))
        .trim()
        .to_string();
    if project_id.is_empty() || command.is_empty() {
        return Err(CliError::Other(
            "run-in-terminal requires --project-id and a command (--command text, or -- command...)."
                .to_string(),
        ));
    }
    let paste = flags.truthy("paste");
    // Paste only types the command and leaves Enter to the user, so a line break or other control
    // character in it (which a terminal would act on) is refused, as the chat does.
    if paste && command.chars().any(char::is_control) {
        return Err(CliError::Other(
            "run-in-terminal --paste types one line; the command holds a line break or other control character."
                .to_string(),
        ));
    }
    if let Some(session_id) = idle_command_panel_terminal(&project_id, flags) {
        let ids = json!({ "projectId": project_id, "sessionId": session_id });
        let mut typed = ids.clone();
        // Ctrl-U first: the prompt may hold half-typed text (a command pasted earlier and never
        // run), which the command would otherwise be appended to.
        typed["text"] = json!(format!("\u{15}{command}"));
        call_gxserver_rpc("/api/sendSessionText", &typed, flags)?;
        if !paste {
            call_gxserver_rpc("/api/sendSessionEnter", &ids, flags)?;
        }
        if flags.truthy("json") {
            print_json(&json!({
                "ok": true,
                "projectId": project_id,
                "sessionId": session_id,
                "commandsPanel": true,
            }));
        } else {
            println!("Typed into the Commands panel terminal {session_id}: {command}");
        }
        return Ok(());
    }
    let cwd = match flags.text("cwd").filter(|cwd| !cwd.trim().is_empty()) {
        Some(cwd) => Some(resolve_cli_cwd(
            &cwd,
            flags,
            &json!({ "projectId": project_id }),
            "run-in-terminal",
        )?),
        None => flags
            .text("sessionId")
            .and_then(|session_id| session_cwd(&project_id, session_id.trim(), flags)),
    };
    let created = call_gxserver_rpc(
        "/api/createSession",
        &run_in_terminal_payload(&project_id, cwd.as_deref(), &command, paste),
        flags,
    )?;
    let session = created.get("session").cloned().unwrap_or(Value::Null);
    let session_id = session["sessionId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if is_failed_cli_result(&created) || session_id.is_empty() {
        return Err(CliError::Other(format!(
            "Could not create the terminal: {}",
            created
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("gxserver returned no session")
        )));
    }
    // Started here rather than on first open, so the command runs even before a client attaches.
    let provider = call_gxserver_rpc(
        "/api/startSessionProvider",
        &json!({ "projectId": project_id, "sessionId": session_id }),
        flags,
    )?;
    let session = match provider.get("session") {
        Some(value) if !value.is_null() => value.clone(),
        _ => session,
    };
    // Paste types the command at the new shell's prompt and leaves Enter to the user. It waits for
    // the prompt first: text typed before the shell reads its line is echoed once above it too.
    if paste {
        wait_for_prompt(&project_id, &session_id, flags);
        call_gxserver_rpc(
            "/api/sendSessionText",
            &json!({ "projectId": project_id, "sessionId": session_id, "text": command }),
            flags,
        )?;
    }
    if flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "projectId": project_id,
            "sessionId": session_id,
            "cwd": cwd,
            "session": session,
        }));
    } else {
        println!("Started {session_id} running: {command}");
    }
    Ok(())
}
