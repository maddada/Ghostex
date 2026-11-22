//! `ghostex clear-command <session>`: starts a session's conversation over with its agent's own
//! clear command (`/clear`, in Claude Code and Codex alike) and answers with the new conversation
//! id. gxserver decides who may and refuses a busy session (server/src/server/session_clear_http.rs,
//! CDXC:Security 2026-11-20); this side only finds the session, refuses to run as your terminal
//! from an agent that dropped its session variables, and waits for the new conversation.

use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{selector, usage};

const ENDPOINT: &str = "/api/clearSessionConversation";
/// How long to wait for the new conversation, and for a sleeping session to wake and take the clear.
const WAIT_RUNNING: Duration = Duration::from_secs(30);
const WAIT_SLEEPING: Duration = Duration::from_secs(120);
/// How long to look for the new conversation of an agent that shows it late.
const WAIT_LATE: Duration = Duration::from_secs(10);
/// The exit code for a clear that was typed but whose new conversation the agent only shows with
/// its next message (`shows_new_conversation_late`); 1 stays "not cleared".
const EXIT_SENT_UNCONFIRMED: i32 = 2;

/// Whether the session's agent only reports its new conversation id once it gets its next
/// message, so a wait that runs out says nothing about the clear.
///
/// CDXC:Cli 2026-11-21 FIX:
/// GitHub #700, item 1: Codex writes its new conversation only when the next message reaches it,
/// and a Claude session clear-command had to wake does not report the new one until then either
/// (the reporter's retest of 11.12.0), so both came back "not cleared", exit 1, although the clear
/// worked. They answer "sent, not confirmed yet" with exit 2 now; a running Claude session that
/// does not move on in time is still exit 1, as are the refusals.
fn shows_new_conversation_late(agent: &str, woke: bool) -> bool {
    agent == "codex" || woke
}

fn timeout(flags: &Flags) -> Option<Duration> {
    flags
        .number("timeout")
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map(Duration::from_secs_f64)
}

pub fn clear_command(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{}", usage::clear_command_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    // `--timeout` is this command's wait in seconds; the gxserver calls read a `timeout` flag as
    // their own limit in milliseconds, so `--timeout 60` gave the clear request 60 ms. They get
    // the flags without it.
    let wait_flags = &parsed.flags;
    let mut rpc_flags = parsed.flags.clone();
    rpc_flags.0.remove("timeout");
    let flags = &rpc_flags;
    let selector_text = selector::session_selector_from_args(&parsed.rest, flags)
        .ok_or_else(|| CliError::Other(usage::clear_command_usage()))?;
    // CDXC:Security 2026-11-20 WHY: without its session's `gxs_` token the CLI uses the token
    // file, which gxserver takes as you. Under a Ghostex session pane that is an agent that dropped
    // its token (Fable review finding 1: keeping the session variables and unsetting only
    // `GHOSTEX_SESSION_TOKEN` skipped the 11.11.1 variable check), so it is refused whatever the
    // variables say. Codex and Empryo panes get their token back from the process tree before this
    // runs (agent_hooks/ancestor_session.rs).
    let holds_session_token =
        std::env::var(crate::security::SESSION_TOKEN_ENV).is_ok_and(|token| {
            token
                .trim()
                .starts_with(crate::security::SESSION_TOKEN_PREFIX)
        });
    if !holds_session_token && crate::agent_hooks::caller_ancestry().in_session_pane {
        return Err(CliError::Other("This runs inside a Ghostex session but without its session token (GHOSTEX_SESSION_TOKEN), so the clear would run as yours. Run it from the agent's own shell, or from your own terminal outside Ghostex.".into()));
    }
    let session = selector::resolve_cli_session_selector(&selector_text, flags)?;
    let params = json!({ "projectId": session["projectId"], "sessionId": session["sessionId"] });
    let mut answer = call_gxserver_rpc(ENDPOINT, &params, flags)?;
    let woke = answer["delivery"] == "waking";
    if woke {
        // gxserver woke a sleeping session and typed nothing: ask again once it is up, so the
        // clear is typed into the running agent and the conversation change seen is its own.
        let deadline = Instant::now() + timeout(wait_flags).unwrap_or(WAIT_SLEEPING);
        loop {
            if Instant::now() >= deadline {
                return Err(CliError::Other("The session was asleep and did not wake in time, so nothing was typed. Try again in a moment.".into()));
            }
            sleep(Duration::from_secs(2));
            match call_gxserver_rpc(ENDPOINT, &params, flags) {
                Ok(next) if next["delivery"] == "waking" => {}
                Ok(next) => {
                    answer = next;
                    break;
                }
                // Only "its input box is not ready yet" is asked again (Fable review finding 6): a
                // woken session that is working or holds messages is refused, not cleared later.
                Err(CliError::Rpc { response, .. }) if response["error"] == "inputBoxNotReady" => {}
                Err(error) => return Err(error),
            }
        }
    }
    let previous = answer["previousAgentSessionId"]
        .as_str()
        .map(str::to_string);
    let agent = answer["agent"].as_str().unwrap_or_default().to_string();
    let late = shows_new_conversation_late(&agent, woke);
    let wait = if flags.truthy("noWait") {
        None
    } else {
        // It may still show up sooner (a later Codex), but a long wait for it is mostly lost time.
        let limit = timeout(wait_flags).unwrap_or(if late { WAIT_LATE } else { WAIT_RUNNING });
        Some(wait_for_new_conversation(
            &session,
            previous.as_deref(),
            limit,
            flags,
        ))
    };
    let reference = session["globalRef"]
        .as_str()
        .or_else(|| session["sessionId"].as_str())
        .unwrap_or_default()
        .to_string();
    let command = answer["command"].as_str().unwrap_or("/clear");
    let agent = agent.as_str();
    let mut result = json!({
        "ok": true,
        "session": reference,
        "command": command,
        "delivery": answer["delivery"],
        "previousAgentSessionId": previous,
    });
    match &wait {
        None => {
            result["cleared"] = Value::Null;
            result["status"] = json!("sent");
        }
        Some(Some(id)) => {
            result["cleared"] = json!(true);
            result["status"] = json!("cleared");
            result["agentSessionId"] = json!(id);
        }
        Some(None) if late => {
            result["cleared"] = Value::Null;
            result["status"] = json!("sentNotConfirmed");
            result["note"] = json!(late_note(agent));
        }
        Some(None) => {
            result["ok"] = json!(false);
            result["cleared"] = json!(false);
            result["status"] = json!("notCleared");
        }
    }
    if flags.truthy("json") {
        print_json(&result);
    } else {
        match &wait {
            None => println!("Sent {command} to {reference}; not waiting for the new conversation."),
            Some(Some(id)) => println!("Cleared {reference} with {command}. New conversation: {id}"),
            Some(None) if late => println!(
                "Sent {command} to {reference}; not confirmed yet. {}",
                late_note(agent)
            ),
            Some(None) => eprintln!(
                "Sent {command} to {reference}, but its conversation did not change in time{}. Check the session before sending it new work.",
                previous
                    .as_deref()
                    .map(|id| format!(" (still {id})"))
                    .unwrap_or_default()
            ),
        }
    }
    match wait {
        Some(None) if late => crate::ghostex_cli::set_exit_code(EXIT_SENT_UNCONFIRMED),
        Some(None) => crate::ghostex_cli::set_exit_code(1),
        _ => {}
    }
    Ok(())
}

fn late_note(agent: &str) -> &'static str {
    if agent == "codex" {
        "Codex shows its new conversation only once it gets its next message."
    } else {
        "A session that had to wake shows its new conversation only once it gets its next message."
    }
}

/// The session's new conversation id once it differs from `previous`, or `None` after `limit`.
fn wait_for_new_conversation(
    session: &Value,
    previous: Option<&str>,
    limit: Duration,
    flags: &Flags,
) -> Option<String> {
    let reference = session["globalRef"]
        .as_str()
        .or_else(|| session["sessionId"].as_str())?
        .to_string();
    let started = Instant::now();
    loop {
        sleep(Duration::from_millis(1_000));
        let current = selector::find_one_by_id(&reference, flags)
            .and_then(|row| row["agentSessionId"].as_str().map(str::to_string))
            .filter(|id| !id.trim().is_empty());
        if let Some(id) = current.filter(|id| Some(id.as_str()) != previous) {
            return Some(id);
        }
        if started.elapsed() >= limit {
            return None;
        }
    }
}
