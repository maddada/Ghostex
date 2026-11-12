use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::{actions, sessions};

use super::*;

/// Where a CLI close came from, for gxserver's one line per close (session_close_log.rs):
/// `--close-source` when a client names its door (the phone sends `phone.<door>`), else
/// `default`, and the session that ran the command (`GHOSTEX_GLOBAL_SESSION_REF`) when an agent
/// or a terminal in Ghostex ran it.
pub(crate) fn insert_close_source(payload: &mut Map<String, Value>, flags: &Flags, default: &str) {
    let source = flags
        .text("closeSource")
        .map(|source| source.trim().to_string())
        .filter(|source| !source.is_empty())
        .unwrap_or_else(|| default.to_string());
    payload.insert(
        crate::session_close_log::CLOSE_SOURCE_PARAM.to_string(),
        Value::String(source),
    );
    if let Some(caller) = std::env::var("GHOSTEX_GLOBAL_SESSION_REF")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        payload.insert(
            crate::session_close_log::CLOSED_BY_PARAM.to_string(),
            Value::String(caller),
        );
    }
}

pub fn session_action_command(
    action: &str,
    past_tense: &str,
    extra_payload: &Value,
    args: &[String],
) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let selector_text = match flags.text("sessionId") {
        Some(value) => value,
        None => parsed.rest.join(" ").trim().to_string(),
    };
    let list = sessions::fetch_session_list(&flags, false)?;
    let selected: Vec<Value> = if selector_text.to_lowercase() == "all" {
        list.clone()
    } else {
        vec![resolve_one_listed_session(&selector_text, &list, &flags)?]
    };
    if selected.is_empty() {
        return Err(CliError::Other(
            "No running terminal sessions matched.".to_string(),
        ));
    }
    let mut affected: Vec<(bool, Value)> = Vec::new();
    for session in &selected {
        let mut payload = extra_payload.as_object().cloned().unwrap_or_default();
        insert_session_field(&mut payload, "projectId", session);
        insert_session_field(&mut payload, "sessionId", session);
        if action == "closeSession" {
            insert_close_source(&mut payload, &flags, "cli.kill");
        }
        let action_result =
            actions::send_gxserver_cli_action(action, &Value::Object(payload), &flags)?;
        if is_failed_cli_result(&action_result) {
            if flags.truthy("json") {
                print_json(&action_result);
                crate::ghostex_cli::set_exit_code(1);
                return Ok(());
            }
            return Err(CliError::Other(result_error_message(
                &action_result,
                || format!("Could not {action} {}.", js_display(session.get("title"))),
            )));
        }
        affected.push((
            action_result.get("ok") != Some(&Value::Bool(false)),
            session.clone(),
        ));
    }
    if flags.truthy("json") {
        let sessions_json: Vec<Value> = affected
            .iter()
            .map(|(ok, session)| json!({ "ok": ok, "session": session }))
            .collect();
        print_json(&json!({
            "ok": affected.iter().all(|(ok, _)| *ok),
            "sessions": sessions_json,
        }));
        return Ok(());
    }
    for (_, session) in &affected {
        println!(
            "{past_tense} {}: {}",
            js_display(session.get("alias")),
            js_display(session.get("title"))
        );
    }
    Ok(())
}

pub fn fork_session_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let selector_text = match flags.text("sessionId") {
        Some(value) => value,
        None => parsed.rest.join(" ").trim().to_string(),
    };
    let list = sessions::fetch_session_list(&flags, false)?;
    let session = resolve_one_listed_session(&selector_text, &list, &flags)?;
    // CLI/mobile Fork calls gxserver directly; the daemon owns provider-specific
    // fork command construction and returns the created session.
    let mut payload = Map::new();
    insert_session_field(&mut payload, "projectId", &session);
    insert_session_field(&mut payload, "sessionId", &session);
    // `--message-id <id>`: a Claude Code fork whose conversation ends at that message.
    if let Some(message_id) = flags.text("messageId").filter(|id| !id.trim().is_empty()) {
        payload.insert("messageId".to_string(), Value::String(message_id));
    }
    let action_result =
        actions::send_gxserver_cli_action("forkSession", &Value::Object(payload), &flags)?;
    if is_failed_cli_result(&action_result) {
        if flags.truthy("json") {
            print_json(&action_result);
            crate::ghostex_cli::set_exit_code(1);
            return Ok(());
        }
        return Err(CliError::Other(result_error_message(
            &action_result,
            || format!("Could not fork {}.", js_display(session.get("title"))),
        )));
    }
    if flags.truthy("json") {
        print_json(&action_result);
        return Ok(());
    }
    let forked_session = action_result
        .get("fork")
        .and_then(|fork| fork.get("session"));
    let suffix = forked_session
        .and_then(|forked| forked.get("sessionId"))
        .filter(|session_id| js_truthy(Some(session_id)))
        .map(|session_id| format!(" -> {}", js_string(session_id)))
        .unwrap_or_default();
    println!(
        "forked {}: {}{suffix}",
        js_display(session.get("alias")),
        js_display(session.get("title"))
    );
    Ok(())
}

pub fn focus_smart_session_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let list = sessions::fetch_session_list(&flags, false)?;
    /*
    CDXC:PromptSearch 2026-08-07-09:18:
    Zehn owns agent-history selection while Ghostex owns live workspace identity. Let `ghostex focus` resolve an exact agent conversation id to its live Ghostex session, and give `--if-running` a distinct exit code so Zehn resumes only when no live owner exists—not when focus delivery itself fails.
    */
    let session = if let Some(agent_session_id) = flags.text("agentSessionId") {
        match resolve_live_agent_session_owner(
            &agent_session_id,
            flags.text("agent").as_deref(),
            &list,
        )? {
            Some(session) => session,
            None if flags.truthy("ifRunning") => {
                if flags.truthy("json") {
                    print_json(&json!({
                        "agentSessionId": agent_session_id,
                        "focused": false,
                        "ok": true,
                        "reason": "notRunning",
                    }));
                }
                crate::ghostex_cli::set_exit_code(3);
                return Ok(());
            }
            None => {
                return Err(CliError::Other(format!(
                    "No live Ghostex session owns agent session \"{agent_session_id}\"."
                )));
            }
        }
    } else {
        let selector_text = match flags.text("sessionId") {
            Some(value) => value,
            None => parsed.rest.join(" ").trim().to_string(),
        };
        resolve_one_listed_session(&selector_text, &list, &flags)?
    };
    let mut payload = Map::new();
    insert_session_field(&mut payload, "projectId", &session);
    insert_session_field(&mut payload, "sessionId", &session);
    let action_result =
        actions::send_gxserver_cli_action("focusSession", &Value::Object(payload), &flags)?;
    // Android treats the SSH process exit status as the remote action contract.
    if is_failed_cli_result(&action_result) {
        if flags.truthy("json") {
            print_json(&action_result);
            crate::ghostex_cli::set_exit_code(1);
            return Ok(());
        }
        return Err(CliError::Other(result_error_message(
            &action_result,
            || format!("Could not focus {}.", js_display(session.get("title"))),
        )));
    }
    if flags.truthy("json") {
        print_json(&action_result);
        return Ok(());
    }
    println!(
        "focused {}: {}",
        js_display(session.get("alias")),
        js_display(session.get("title"))
    );
    Ok(())
}

pub(super) fn resolve_live_agent_session_owner(
    agent_session_id: &str,
    agent: Option<&str>,
    sessions_list: &[Value],
) -> CliResult<Option<Value>> {
    let normalized_id = agent_session_id.trim();
    if normalized_id.is_empty() {
        return Err(CliError::Other(
            "--agent-session-id requires a non-empty value.".to_string(),
        ));
    }
    let normalized_agent = agent.map(|value| value.trim().to_lowercase());
    let matches: Vec<&Value> = sessions_list
        .iter()
        .filter(|session| {
            session.get("agentSessionId").and_then(Value::as_str) == Some(normalized_id)
        })
        // CDXC:PromptSearch 2026-09-21 WHY:
        // A sleeping session is still a sidebar row and focusing it wakes it, so it owns its conversation the same way a running one does; resuming next to it made a duplicate session.
        .filter(|session| {
            ["isLive", "isSleeping"]
                .iter()
                .any(|key| session.get(*key).and_then(Value::as_bool) == Some(true))
        })
        .filter(|session| {
            let Some(expected_agent) = normalized_agent.as_deref() else {
                return true;
            };
            listed_session_runs_agent(session, expected_agent)
        })
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [session] => Ok(Some((*session).clone())),
        _ => Err(CliError::Other(format!(
            "Multiple live Ghostex sessions own agent session \"{normalized_id}\":\n{}",
            format_session_matches(
                &matches
                    .iter()
                    .map(|session| (*session).clone())
                    .collect::<Vec<_>>()
            )
        ))),
    }
}

/// CDXC:PromptSearch 2026-09-19 WHY:
/// A session launched from a custom agent configuration is listed with that configuration's `custom-…` id, so the family it runs is read from the projected agent icon the way resume planning does. Comparing ids alone made `gx f` report no live owner for a running custom Claude session and start a second `claude --resume` writer.
/// SEE-ALSO: `session_owns_agent_conversation` in server/src/agent_prompt_search.rs, the same rule for the Find surface.
fn listed_session_runs_agent(session: &Value, expected_agent: &str) -> bool {
    let matches_id = ["agentId", "agent"]
        .iter()
        .filter_map(|key| session.get(*key).and_then(Value::as_str))
        .any(|value| value.trim().eq_ignore_ascii_case(expected_agent));
    if matches_id {
        return true;
    }
    let is_custom_configuration =
        session
            .get("agentId")
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value
                    .trim()
                    .to_ascii_lowercase()
                    .starts_with(crate::custom_session_tags::CUSTOM_SESSION_TAG_ID_PREFIX)
            });
    if !is_custom_configuration {
        return false;
    }
    session
        .get("agentIcon")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|icon| !icon.is_empty())
        .is_some_and(|icon| {
            crate::agents::default_agent_icon_to_id(icon)
                .unwrap_or(icon)
                .eq_ignore_ascii_case(expected_agent)
        })
}
