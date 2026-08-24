use serde_json::{Map, Value};

use crate::ghostex_cli::args::{parse_args, ParsedArgs};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::selector;

/*
CDXC:BoardStartWork 2026-08-07:
`ghostex board start-work <bead-id>` is the CLI face of gxserver's
`/api/startBoardWork`: a thin wrapper like `run-action` and `create-agent`,
so external orchestrators dispatch a Project Board card through the daemon
instead of launching a worker session themselves. The daemon owns bead
resolution, the idempotent reuse guard, the canonical work prompt, and the
conversation link; the CLI only ships the parameters and prints the JSON
result (`{ projectId, sessionId, created }`).
*/
/// Dispatch a Ghostex Project Board CLI subcommand.
pub fn board_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("start-work") => start_work_command(&args[1..]),
        Some("associate") => associate_command(&args[1..]),
        Some("install-skill") => {
            crate::ghostex_cli::skills::install_manage_beads_skill_command(&args[1..])
        }
        None | Some("help") | Some("-h") | Some("--help") => {
            println!("{}", crate::ghostex_cli::usage::board_usage());
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown board command: {other}\n\n{}",
            crate::ghostex_cli::usage::board_usage()
        ))),
    }
}

fn start_work_command(args: &[String]) -> CliResult<()> {
    if matches!(
        args.first().map(String::as_str),
        Some("help") | Some("-h") | Some("--help")
    ) {
        println!("{}", crate::ghostex_cli::usage::board_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let bead_id = parsed
        .flags
        .text("beadId")
        .or_else(|| parsed.rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    if bead_id.is_empty() {
        return Err(CliError::Other(
            "board start-work requires a bead id.".to_string(),
        ));
    }
    let mut payload = Map::new();
    payload.insert("beadId".to_string(), Value::String(bead_id));
    if let Some(agent) = parsed
        .flags
        .text("agent")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert("agent".to_string(), Value::String(agent));
    }
    if let Some(project_id) = parsed
        .flags
        .text("projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert("projectId".to_string(), Value::String(project_id));
    }
    let result = call_gxserver_rpc(
        "/api/startBoardWork",
        &Value::Object(payload),
        &parsed.flags,
    )?;
    print_json(&result);
    if is_failed_cli_result(&result) {
        crate::ghostex_cli::set_exit_code(1);
    }
    Ok(())
}

/*
CDXC:BoardAssociateSession 2026-08-24:
`ghostex board associate <bead-id>` is how an agent that was prompted by hand —
"tackle bead 12345" — puts itself on the card instead of leaving it looking
unworked. With no --session-id it links the session it is running in, read from
the Ghostex session environment every pane exports, so the agent needs to know
nothing about its own routing ids. gxserver owns bead resolution and the link
write; the CLI only names the session.
*/
fn associate_command(args: &[String]) -> CliResult<()> {
    if matches!(
        args.first().map(String::as_str),
        Some("help") | Some("-h") | Some("--help")
    ) {
        println!("{}", crate::ghostex_cli::usage::board_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let bead_id = parsed
        .flags
        .text("beadId")
        .or_else(|| parsed.rest.first().cloned())
        .unwrap_or_default()
        .trim()
        .to_string();
    if bead_id.is_empty() {
        return Err(CliError::Other(
            "board associate requires a bead id.".to_string(),
        ));
    }
    let (session_project_id, session_id) = resolve_associate_session(&parsed)?;
    let mut payload = Map::new();
    payload.insert("beadId".to_string(), Value::String(bead_id));
    payload.insert("sessionId".to_string(), Value::String(session_id));
    payload.insert(
        "sessionProjectId".to_string(),
        Value::String(session_project_id),
    );
    if let Some(project_id) = parsed
        .flags
        .text("projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert("projectId".to_string(), Value::String(project_id));
    }
    let result = call_gxserver_rpc(
        "/api/associateBoardSession",
        &Value::Object(payload),
        &parsed.flags,
    )?;
    print_json(&result);
    if is_failed_cli_result(&result) {
        crate::ghostex_cli::set_exit_code(1);
    }
    Ok(())
}

/// The session to link: an explicit selector, else the calling session.
fn resolve_associate_session(parsed: &ParsedArgs) -> CliResult<(String, String)> {
    let selector_text = parsed
        .flags
        .text("sessionId")
        .or_else(|| parsed.flags.text("session"))
        .or_else(|| parsed.flags.text("selector"))
        .unwrap_or_default()
        .trim()
        .to_string();
    if selector_text.is_empty() {
        return session_reference_from_environment().ok_or_else(|| {
            CliError::Other(
                "board associate must run inside a Ghostex session. Pass --session-id <alias|id|title> to link another session."
                    .to_string(),
            )
        });
    }
    /*
    A shared board is worked from sibling projects, so --project-id names the
    board the bead lives on and must not narrow the session lookup to it.
    */
    let mut flags = parsed.flags.clone();
    flags.0.remove("projectId");
    let session = selector::resolve_cli_session_selector(&selector_text, &flags)?;
    session_reference(&session).ok_or_else(|| {
        CliError::Other(format!(
            "Session \"{selector_text}\" does not report a project id."
        ))
    })
}

/// `(projectId, sessionId)` of a listed session row.
fn session_reference(session: &Value) -> Option<(String, String)> {
    let session_id = trimmed_text(session.get("sessionId"))?;
    let project_id = trimmed_text(session.get("projectId")).or_else(|| {
        let global_ref = trimmed_text(session.get("globalRef"))?;
        let (project_id, _) = crate::agent_hooks::probing::parse_global_session_ref(&global_ref);
        project_id
    })?;
    Some((project_id, session_id))
}

/*
Every Ghostex pane exports its own routing ids: `GHOSTEX_GLOBAL_SESSION_REF`
(`S90:P3lv0:G4sp7`) and `GHOSTEX_NATIVE_SESSION_ID` (`P3lv0:G4sp7`). Reading them
here is what makes "link the session I am in" need no arguments at all.
*/
fn session_reference_from_environment() -> Option<(String, String)> {
    if let Some(global_ref) = environment_text("GHOSTEX_GLOBAL_SESSION_REF") {
        if let (Some(project_id), Some(session_id)) =
            crate::agent_hooks::probing::parse_global_session_ref(&global_ref)
        {
            return Some((project_id, session_id));
        }
    }
    let native_session_id = environment_text("GHOSTEX_NATIVE_SESSION_ID")?;
    let (project_id, session_id) = native_session_id.split_once(':')?;
    let project_id = project_id.trim();
    let session_id = session_id.trim();
    if project_id.is_empty() || session_id.is_empty() {
        return None;
    }
    Some((project_id.to_string(), session_id.to_string()))
}

fn environment_text(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn trimmed_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}
