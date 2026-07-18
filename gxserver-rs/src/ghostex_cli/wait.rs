use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, parse_boolean, FlagValue, Flags};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{ghostex_home, project_id_from_global_ref, CliError, CliResult};
use crate::ghostex_cli::{actions, selector, sessions};

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI session lifecycle + polling commands
(scripts/ghostex-cli.mjs lines 5570-5879): kill/sleep/wake (sessionActionCommand),
fork-session, focus, read-text, wait-for-text, and send-message. wait-for-text is
the agent-orchestration sentinel loop and must keep the exact tail-window
--lines semantics, per-line regex matching (so ^ anchors to a line start), the
session-liveness fallback when reads fail, the bounded timeout, and exit-code
behavior. The selector resolution helpers (resolveListedSessions & friends) are
private ports here because send-message needs the raw match list, and the
lifecycle commands resolve against the same single fetched inventory like the
Node CLI does.

JS `new RegExp(pattern)` is replaced by the private `js_regex` engine below —
see its module comment for the exact supported subset.
*/

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
    let selector_text = match flags.text("sessionId") {
        Some(value) => value,
        None => parsed.rest.join(" ").trim().to_string(),
    };
    let list = sessions::fetch_session_list(&flags, false)?;
    let session = resolve_one_listed_session(&selector_text, &list, &flags)?;
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

pub fn read_session_text_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let selector_text =
        selector::session_selector_from_args(&parsed.rest, &flags).unwrap_or_default();
    let mut payload = Map::new();
    let visible = matches!(flags.0.get("visible"), Some(FlagValue::Bool(true)))
        || flags.string_value("source") == Some("visible");
    payload.insert(
        "source".to_string(),
        Value::String(if visible { "visible" } else { "screen" }.to_string()),
    );
    if flags.contains("timeoutMs") {
        payload.insert(
            "timeoutMs".to_string(),
            json_finite_number(flags.number("timeoutMs")),
        );
    }
    if !selector_text.is_empty() {
        let session = selector::resolve_cli_session_selector(&selector_text, &flags)?;
        insert_session_field(&mut payload, "projectId", &session);
        insert_session_field(&mut payload, "sessionId", &session);
    }
    let result =
        actions::send_gxserver_cli_action("readSessionText", &Value::Object(payload), &flags)?;
    if is_failed_cli_result(&result) {
        if flags.truthy("json") {
            print_json(&result);
            crate::ghostex_cli::set_exit_code(1);
            return Ok(());
        }
        return Err(CliError::Other(result_error_message(&result, || {
            "Could not read terminal text.".to_string()
        })));
    }
    let text = js_string_or_empty(result.get("text"));
    // flags.lines === undefined and Number(non-numeric) = NaN both leave the
    // text unlimited in limitTextLines; flags.number covers both as None.
    let lines = if flags.contains("lines") {
        flags.number("lines")
    } else {
        None
    };
    let limited = limit_text_lines(&text, lines);
    if flags.truthy("json") {
        let mut json_result = result.clone();
        if let Some(object) = json_result.as_object_mut() {
            object.insert("text".to_string(), Value::String(limited));
        }
        print_json(&json_result);
        return Ok(());
    }
    print!("{limited}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
struct WaitForTextParams {
    interval_seconds: f64,
    lines: f64,
    pattern: String,
    selector: Option<String>,
    timeout_seconds: f64,
}

fn clamp_number(value: Option<f64>, fallback: f64, min: f64, max: f64) -> f64 {
    match value {
        Some(parsed) if parsed.is_finite() => parsed.clamp(min, max),
        _ => fallback,
    }
}

fn parse_wait_for_text(rest: &[String], flags: &Flags) -> WaitForTextParams {
    WaitForTextParams {
        interval_seconds: clamp_number(flags.number("intervalSeconds"), 20.0, 2.0, 300.0),
        lines: clamp_number(flags.number("lines"), 200.0, 10.0, 2000.0),
        pattern: flags
            .text("pattern")
            .unwrap_or_else(|| rest.iter().skip(1).cloned().collect::<Vec<_>>().join(" "))
            .trim()
            .to_string(),
        selector: if flags.contains("sessionId")
            || flags.contains("title")
            || flags.contains("index")
        {
            None
        } else {
            Some(rest.first().cloned().unwrap_or_default().trim().to_string())
        },
        timeout_seconds: clamp_number(flags.number("timeoutSeconds"), 1800.0, 5.0, 21600.0),
    }
}

fn find_wait_for_text_match<'a>(text: &'a str, regex: &js_regex::Regex) -> Option<&'a str> {
    let lines: Vec<&str> = text.split('\n').collect();
    for line in lines.iter().rev() {
        if regex.test(line) {
            return Some(line);
        }
    }
    None
}

pub fn wait_for_text_command(args: &[String]) -> CliResult<()> {
    /*
    Agent orchestrators poll worker panes for sentinel lines like
    "PHASE 1 COMPLETE". This command owns that loop: per-line regex matching
    over recent scrollback (so ^ anchors to a line start), a session-liveness
    check on every poll, and a bounded timeout, exiting 0 only when a line
    truly matches.
    */
    let parsed_args = parse_args(args);
    let flags = parsed_args.flags;
    let parsed = parse_wait_for_text(&parsed_args.rest, &flags);
    let selector_text = match &parsed.selector {
        Some(value) => value.clone(),
        None => selector::session_selector_from_args(&[], &flags).unwrap_or_default(),
    };
    if selector_text.is_empty() || parsed.pattern.is_empty() {
        return Err(CliError::Other(
            "wait-for-text requires a session selector and a pattern, e.g. `ghostex wait-for-text <sessionId> \"^\\s*PHASE 1 (COMPLETE|BLOCKED)\"`.".to_string(),
        ));
    }
    let regex = js_regex::Regex::new(&parsed.pattern).map_err(|message| {
        CliError::Other(format!(
            "wait-for-text pattern is not a valid regular expression: {message}"
        ))
    })?;
    let session = selector::resolve_cli_session_selector(&selector_text, &flags)?;
    let started_at = Instant::now();
    let mut polls: u64 = 0;
    loop {
        polls += 1;
        let mut payload = Map::new();
        insert_session_field(&mut payload, "projectId", &session);
        insert_session_field(&mut payload, "sessionId", &session);
        payload.insert("source".to_string(), Value::String("screen".to_string()));
        let read_result =
            actions::send_gxserver_cli_action("readSessionText", &Value::Object(payload), &flags)?;
        if !is_failed_cli_result(&read_result) {
            let text = limit_text_lines(
                &js_string_or_empty(read_result.get("text")),
                Some(parsed.lines),
            );
            if let Some(line) = find_wait_for_text_match(&text, &regex) {
                finish_wait_for_text(
                    &flags,
                    started_at,
                    polls,
                    json!({ "line": line, "matched": true }),
                    true,
                );
                return Ok(());
            }
        } else {
            let list = sessions::fetch_session_list(&flags, false)?;
            let listed = list.iter().find(|candidate| {
                candidate.get("sessionId") == session.get("sessionId")
                    && candidate.get("projectId") == session.get("projectId")
            });
            match listed {
                None => {
                    finish_wait_for_text(
                        &flags,
                        started_at,
                        polls,
                        json!({ "matched": false, "reason": "session no longer exists" }),
                        false,
                    );
                    return Ok(());
                }
                Some(listed) => {
                    if listed.get("isLive") == Some(&Value::Bool(false))
                        && listed.get("isSleeping") != Some(&Value::Bool(true))
                    {
                        finish_wait_for_text(
                            &flags,
                            started_at,
                            polls,
                            json!({ "matched": false, "reason": "session is not live" }),
                            false,
                        );
                        return Ok(());
                    }
                }
            }
        }
        if started_at.elapsed().as_millis() as f64 >= parsed.timeout_seconds * 1000.0 {
            finish_wait_for_text(
                &flags,
                started_at,
                polls,
                json!({
                    "matched": false,
                    "reason": format!(
                        "timed out after {}s without a match",
                        js_f64_string(parsed.timeout_seconds)
                    ),
                }),
                false,
            );
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(
            (parsed.interval_seconds * 1000.0) as u64,
        ));
    }
}

fn finish_wait_for_text(flags: &Flags, started_at: Instant, polls: u64, extras: Value, ok: bool) {
    let elapsed_seconds = ((started_at.elapsed().as_millis() as f64) / 1000.0).round() as i64;
    let mut result = Map::new();
    result.insert("elapsedSeconds".to_string(), json!(elapsed_seconds));
    result.insert("ok".to_string(), Value::Bool(ok));
    result.insert("polls".to_string(), json!(polls));
    if let Some(object) = extras.as_object() {
        for (key, value) in object {
            result.insert(key.clone(), value.clone());
        }
    }
    let result = Value::Object(result);
    if flags.truthy("json") {
        print_json(&result);
    } else if ok {
        println!("{}", js_string_or_empty(result.get("line")));
    } else {
        eprintln!("wait-for-text: {}", js_display(result.get("reason")));
    }
    if !ok {
        crate::ghostex_cli::set_exit_code(1);
    }
}

pub fn send_message_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let rest = parsed.rest;
    let explicit_selector = selector::session_selector_from_args(&[], &flags).unwrap_or_default();
    let mut selector_text = explicit_selector;
    let mut agent_id: Option<String> = flags.string_value("agent").map(str::to_string);
    let mut text_start_index = 0usize;

    let agent_truthy = agent_id
        .as_deref()
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    if selector_text.is_empty()
        && !agent_truthy
        && rest.first().map(|arg| !arg.is_empty()).unwrap_or(false)
    {
        let first_arg = rest[0].clone();
        let list = sessions::fetch_session_list(&flags, false)?;
        let matches = resolve_listed_sessions(&first_arg, &list, &flags)?;
        if matches.len() > 1 {
            return Err(CliError::Other(format!(
                "Multiple sessions matched \"{first_arg}\":\n{}",
                format_session_matches(&matches)
            )));
        }
        if matches.len() == 1 {
            selector_text = first_arg;
            text_start_index = 1;
        } else {
            agent_id = Some(first_arg);
            text_start_index = 1;
        }
    }

    let text = flags.text("text").unwrap_or_else(|| {
        rest.iter()
            .skip(text_start_index)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    });
    let mut payload = Map::new();
    if let Some(group_id) = flags.0.get("groupId") {
        payload.insert("groupId".to_string(), group_id.as_json());
    }
    if flags.contains("sendDelayMs") {
        payload.insert(
            "sendDelayMs".to_string(),
            json_finite_number(flags.number("sendDelayMs")),
        );
    }
    payload.insert(
        "submit".to_string(),
        Value::Bool(match flags.0.get("submit") {
            None => true,
            Some(value) => parse_boolean(value),
        }),
    );
    payload.insert("text".to_string(), Value::String(text));
    if !selector_text.is_empty() {
        let session = selector::resolve_cli_session_selector(&selector_text, &flags)?;
        insert_session_field(&mut payload, "projectId", &session);
        insert_session_field(&mut payload, "sessionId", &session);
    } else if let Some(agent_id) = agent_id {
        payload.insert("agentId".to_string(), Value::String(agent_id));
    }
    let result = actions::send_gxserver_cli_action("sendMessage", &Value::Object(payload), &flags)?;
    if is_failed_cli_result(&result) {
        print_json(&result);
        crate::ghostex_cli::set_exit_code(1);
        return Ok(());
    }
    print_json(&result);
    Ok(())
}

/// JS `payload.key = session.key`: present keys copy through (null included),
/// missing keys stay omitted like JSON.stringify dropping undefined.
fn insert_session_field(payload: &mut Map<String, Value>, key: &str, session: &Value) {
    if let Some(value) = session.get(key) {
        payload.insert(key.to_string(), value.clone());
    }
}

/// `result.error ?? fallback` with JS string coercion of the error value.
fn result_error_message(result: &Value, fallback: impl FnOnce() -> String) -> String {
    match result.get("error") {
        None | Some(Value::Null) => fallback(),
        Some(error) => js_string(error),
    }
}

fn limit_text_lines(text: &str, lines: Option<f64>) -> String {
    let Some(lines) = lines else {
        return text.to_string();
    };
    if !lines.is_finite() || lines <= 0.0 {
        return text.to_string();
    }
    let count = lines.trunc() as usize;
    let split: Vec<&str> = text
        .split('\n')
        .map(|part| part.strip_suffix('\r').unwrap_or(part))
        .collect();
    let start = split.len().saturating_sub(count);
    split[start..].join("\n")
}

fn json_finite_number(value: Option<f64>) -> Value {
    match value {
        Some(number) if number.is_finite() => {
            if number.fract() == 0.0 && number.abs() < 9.007_199_254_740_992e15 {
                Value::from(number as i64)
            } else {
                serde_json::Number::from_f64(number)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
        }
        _ => Value::Null,
    }
}

fn resolve_one_listed_session(
    selector_text: &str,
    sessions_list: &[Value],
    flags: &Flags,
) -> CliResult<Value> {
    let matches = resolve_listed_sessions(selector_text, sessions_list, flags)?;
    if matches.len() == 1 {
        return Ok(matches.into_iter().next().expect("one match"));
    }
    if matches.is_empty() {
        return Err(CliError::Other(format!(
            "No matching session found for \"{selector_text}\". Run \"ghostex sessions\" or \"gx sessions\" to list sessions."
        )));
    }
    Err(CliError::Other(format!(
        "Multiple sessions matched \"{selector_text}\":\n{}",
        format_session_matches(&matches)
    )))
}

fn resolve_listed_sessions(
    selector_text: &str,
    sessions_list: &[Value],
    flags: &Flags,
) -> CliResult<Vec<Value>> {
    let normalized_selector = selector_text.trim();
    if normalized_selector.is_empty() {
        return Err(CliError::Other(
            "Provide a session alias, id, provider session name, title, or project:title selector."
                .to_string(),
        ));
    }
    // Bare G session ids can repeat across projects; honor --project-id.
    let scoped_sessions = project_scoped_sessions(sessions_list, flags);
    if normalized_selector.chars().all(|c| c.is_ascii_digit()) {
        let alias = normalized_selector.parse::<f64>().unwrap_or(f64::NAN);
        let cache = read_session_alias_cache();
        let cached_session_id = cache
            .as_ref()
            .and_then(|cache| cache.get("sessions"))
            .and_then(Value::as_array)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.get("alias").and_then(Value::as_f64) == Some(alias))
            })
            .and_then(|entry| entry.get("sessionId"))
            .cloned();
        if let Some(cached_session_id) = cached_session_id {
            if js_truthy(Some(&cached_session_id)) {
                if let Some(live_session) = scoped_sessions
                    .iter()
                    .find(|session| session.get("sessionId") == Some(&cached_session_id))
                {
                    return Ok(vec![(*live_session).clone()]);
                }
            }
        }
        let live_alias_match = scoped_sessions
            .iter()
            .find(|session| session.get("alias").and_then(Value::as_f64) == Some(alias));
        return Ok(live_alias_match
            .map(|session| vec![(*session).clone()])
            .unwrap_or_default());
    }
    let exact_id_matches: Vec<Value> = scoped_sessions
        .iter()
        .filter(|session| {
            session.get("sessionId").and_then(Value::as_str) == Some(normalized_selector)
        })
        .map(|session| (*session).clone())
        .collect();
    if !exact_id_matches.is_empty() {
        return Ok(exact_id_matches);
    }
    if let Some(exact_global_ref) = scoped_sessions.iter().find(|session| {
        session.get("globalRef").and_then(Value::as_str) == Some(normalized_selector)
    }) {
        return Ok(vec![(*exact_global_ref).clone()]);
    }
    // Terminals export GHOSTEX_SESSION_ID as the provider persistence name;
    // resolve that before falling back to title matching.
    let provider_matches = rank_provider_session_matches(&scoped_sessions, normalized_selector);
    if !provider_matches.is_empty() {
        return Ok(provider_matches);
    }
    if let Some(project_separator_index) = normalized_selector.find(':').filter(|index| *index > 0)
    {
        let project_selector = normalized_selector[..project_separator_index]
            .trim()
            .to_lowercase();
        let title_selector = normalized_selector[project_separator_index + 1..]
            .trim()
            .to_lowercase();
        let filtered: Vec<&Value> = scoped_sessions
            .iter()
            .filter(|session| {
                session
                    .get("projectName")
                    .and_then(Value::as_str)
                    .map(|name| name.to_lowercase() == project_selector)
                    .unwrap_or(false)
                    || session
                        .get("projectPath")
                        .and_then(Value::as_str)
                        .map(|path| path.to_lowercase().contains(&project_selector))
                        .unwrap_or(false)
            })
            .copied()
            .collect();
        return Ok(rank_session_title_matches(&filtered, &title_selector));
    }
    Ok(rank_session_title_matches(
        &scoped_sessions,
        &normalized_selector.to_lowercase(),
    ))
}

fn project_scoped_sessions<'a>(sessions_list: &'a [Value], flags: &Flags) -> Vec<&'a Value> {
    let project_id = flags
        .text("projectId")
        .unwrap_or_default()
        .trim()
        .to_string();
    if project_id.is_empty() {
        return sessions_list.iter().collect();
    }
    sessions_list
        .iter()
        .filter(|session| session_project_id(session) == project_id)
        .collect()
}

fn session_project_id(session: &Value) -> String {
    // String(session.projectId ?? projectIdFromGlobalRef(session.globalRef) ?? "").trim()
    if let Some(project_id) = session.get("projectId").filter(|value| !value.is_null()) {
        return js_string(project_id).trim().to_string();
    }
    let global_ref = session
        .get("globalRef")
        .and_then(Value::as_str)
        .unwrap_or("");
    project_id_from_global_ref(global_ref)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn rank_provider_session_matches(sessions_list: &[&Value], selector_text: &str) -> Vec<Value> {
    let normalized_selector = selector_text.trim();
    if normalized_selector.is_empty() {
        return Vec::new();
    }
    if let Some(slash_index) = normalized_selector.find('/').filter(|index| *index > 0) {
        let provider = normalized_selector[..slash_index].trim().to_lowercase();
        let provider_session_name = normalized_selector[slash_index + 1..].trim();
        if provider.is_empty() || provider_session_name.is_empty() {
            return Vec::new();
        }
        return sessions_list
            .iter()
            .filter(|session| {
                session
                    .get("provider")
                    .and_then(Value::as_str)
                    .map(|value| value.to_lowercase() == provider)
                    .unwrap_or(false)
                    && session.get("providerSessionName").and_then(Value::as_str)
                        == Some(provider_session_name)
            })
            .map(|session| (*session).clone())
            .collect();
    }
    sessions_list
        .iter()
        .filter(|session| {
            session.get("providerSessionName").and_then(Value::as_str) == Some(normalized_selector)
        })
        .map(|session| (*session).clone())
        .collect()
}

fn rank_session_title_matches(sessions_list: &[&Value], selector_lower: &str) -> Vec<Value> {
    let exact: Vec<Value> = sessions_list
        .iter()
        .filter(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.to_lowercase() == selector_lower)
                .unwrap_or(false)
                || session
                    .get("displayTitle")
                    .and_then(Value::as_str)
                    .map(|title| title.to_lowercase() == selector_lower)
                    .unwrap_or(false)
        })
        .map(|session| (*session).clone())
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    sessions_list
        .iter()
        .filter(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.to_lowercase().contains(selector_lower))
                .unwrap_or(false)
                || session
                    .get("displayTitle")
                    .and_then(Value::as_str)
                    .map(|title| title.to_lowercase().contains(selector_lower))
                    .unwrap_or(false)
        })
        .map(|session| (*session).clone())
        .collect()
}

fn format_session_matches(sessions_list: &[Value]) -> String {
    sessions_list
        .iter()
        .map(|session| {
            let title = match session.get("displayTitle") {
                Some(value) if !value.is_null() => js_string(value),
                _ => js_display(session.get("title")),
            };
            format!(
                "{}. {} - {}",
                js_display(session.get("alias")),
                js_display(session.get("projectName")),
                title
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_session_alias_cache() -> Option<Value> {
    let path = ghostex_home().join("cli").join("session-aliases.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// JS truthiness of an optional JSON value (missing/undefined → false).
fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// JS String(value) coercion for JSON values.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                int.to_string()
            } else if let Some(int) = number.as_u64() {
                int.to_string()
            } else {
                js_f64_string(number.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// String(value ?? "") — null/undefined become "".
fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(other) => js_string(other),
    }
}

/// Template interpolation of a possibly-missing value (undefined → "undefined").
fn js_display(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(other) => js_string(other),
    }
}

/// JS number-to-string for finite doubles (integers print without a decimal).
fn js_f64_string(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        return format!("{}", value as i64);
    }
    format!("{value}")
}

mod js_regex {
    /*!
    Private ECMAScript-flavored regex engine for `wait-for-text` (the `regex`
    crate is not a dependency and `new RegExp(pattern)` accepts user-supplied
    JS patterns). Backtracking matcher supporting the subset agents actually
    use in sentinel patterns: literals and escaped literals, `.`, `^`, `$`,
    `\b`/`\B`, `\d \D \w \W \s \S`, character classes with ranges and negation,
    groups `( )`, `(?: )`, named groups, lookahead `(?= ) (?! )`, alternation
    `|`, and quantifiers `* + ? {n} {n,} {n,m}` with lazy `?` variants.
    Unsupported (compile error): backreferences `\1`-`\9` and lookbehind.
    `test()` is unanchored per-line search like JS RegExp.prototype.test.
    */

    #[derive(Debug)]
    pub struct Regex {
        root: Node,
    }

    #[derive(Debug)]
    enum Node {
        Char(char),
        Any,
        Class {
            negated: bool,
            items: Vec<ClassItem>,
        },
        StartAnchor,
        EndAnchor,
        WordBoundary {
            negated: bool,
        },
        Seq(Vec<Node>),
        Alt(Vec<Node>),
        Repeat {
            node: Box<Node>,
            min: u32,
            max: Option<u32>,
            lazy: bool,
        },
        Look {
            negated: bool,
            node: Box<Node>,
        },
    }

    #[derive(Debug, Clone, Copy)]
    enum ClassItem {
        Char(char),
        Range(char, char),
        Digit,
        NotDigit,
        Word,
        NotWord,
        Space,
        NotSpace,
    }

    impl Regex {
        pub fn new(pattern: &str) -> Result<Regex, String> {
            let mut parser = Parser {
                chars: pattern.chars().collect(),
                pos: 0,
            };
            let root = parser
                .parse_alternation()
                .map_err(|detail| format!("Invalid regular expression: /{pattern}/: {detail}"))?;
            if parser.pos < parser.chars.len() {
                // Only an unmatched ')' can stop the top-level parse.
                return Err(format!(
                    "Invalid regular expression: /{pattern}/: Unmatched ')'"
                ));
            }
            Ok(Regex { root })
        }

        pub fn test(&self, text: &str) -> bool {
            let chars: Vec<char> = text.chars().collect();
            (0..=chars.len()).any(|start| match_node(&self.root, &chars, start, &mut |_| true))
        }
    }

    struct Parser {
        chars: Vec<char>,
        pos: usize,
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.pos).copied()
        }

        fn advance(&mut self) -> Option<char> {
            let character = self.peek();
            if character.is_some() {
                self.pos += 1;
            }
            character
        }

        fn eat(&mut self, expected: char) -> bool {
            if self.peek() == Some(expected) {
                self.pos += 1;
                true
            } else {
                false
            }
        }

        fn parse_alternation(&mut self) -> Result<Node, String> {
            let mut branches = vec![self.parse_sequence()?];
            while self.eat('|') {
                branches.push(self.parse_sequence()?);
            }
            if branches.len() == 1 {
                Ok(branches.pop().expect("one branch"))
            } else {
                Ok(Node::Alt(branches))
            }
        }

        fn parse_sequence(&mut self) -> Result<Node, String> {
            let mut nodes: Vec<Node> = Vec::new();
            loop {
                match self.peek() {
                    None | Some('|') | Some(')') => break,
                    _ => {}
                }
                let atom = self.parse_atom()?;
                let node = match self.parse_quantifier()? {
                    Some((min, max)) => {
                        // V8: quantifiers on ^ $ \b \B are "Nothing to repeat"
                        // (lookaheads stay quantifiable per Annex B).
                        if matches!(
                            atom,
                            Node::StartAnchor | Node::EndAnchor | Node::WordBoundary { .. }
                        ) {
                            return Err("Nothing to repeat".to_string());
                        }
                        let lazy = self.eat('?');
                        Node::Repeat {
                            node: Box::new(atom),
                            min,
                            max,
                            lazy,
                        }
                    }
                    None => atom,
                };
                nodes.push(node);
            }
            Ok(Node::Seq(nodes))
        }

        fn parse_atom(&mut self) -> Result<Node, String> {
            match self.advance().expect("caller checked peek") {
                '(' => self.parse_group(),
                '^' => Ok(Node::StartAnchor),
                '$' => Ok(Node::EndAnchor),
                '.' => Ok(Node::Any),
                '[' => self.parse_class(),
                '\\' => self.parse_escape_atom(),
                '*' | '+' | '?' => Err("Nothing to repeat".to_string()),
                '{' => {
                    // V8: a brace that forms a valid quantifier with nothing
                    // before it is "Nothing to repeat"; otherwise it is a
                    // literal '{' (e.g. "a{", "{x}", "{,3}").
                    self.pos -= 1;
                    if self.parse_quantifier()?.is_some() {
                        return Err("Nothing to repeat".to_string());
                    }
                    self.pos += 1;
                    Ok(Node::Char('{'))
                }
                // '}' and ']' fall through as literals like Annex B.
                other => Ok(Node::Char(other)),
            }
        }

        fn parse_group(&mut self) -> Result<Node, String> {
            if self.eat('?') {
                match self.peek() {
                    Some(':') => {
                        self.pos += 1;
                        let inner = self.parse_alternation()?;
                        self.expect_group_close()?;
                        Ok(inner)
                    }
                    Some('=') => {
                        self.pos += 1;
                        let inner = self.parse_alternation()?;
                        self.expect_group_close()?;
                        Ok(Node::Look {
                            negated: false,
                            node: Box::new(inner),
                        })
                    }
                    Some('!') => {
                        self.pos += 1;
                        let inner = self.parse_alternation()?;
                        self.expect_group_close()?;
                        Ok(Node::Look {
                            negated: true,
                            node: Box::new(inner),
                        })
                    }
                    Some('<') => {
                        self.pos += 1;
                        match self.peek() {
                            Some('=') | Some('!') => Err(
                                "lookbehind assertions are not supported by this CLI's regex engine"
                                    .to_string(),
                            ),
                            _ => {
                                // (?<name>...) named capture: treat as a plain group.
                                while let Some(character) = self.advance() {
                                    if character == '>' {
                                        break;
                                    }
                                }
                                let inner = self.parse_alternation()?;
                                self.expect_group_close()?;
                                Ok(inner)
                            }
                        }
                    }
                    _ => Err("Invalid group".to_string()),
                }
            } else {
                let inner = self.parse_alternation()?;
                self.expect_group_close()?;
                Ok(inner)
            }
        }

        fn expect_group_close(&mut self) -> Result<(), String> {
            if self.eat(')') {
                Ok(())
            } else {
                Err("Unterminated group".to_string())
            }
        }

        fn parse_quantifier(&mut self) -> Result<Option<(u32, Option<u32>)>, String> {
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    Ok(Some((0, None)))
                }
                Some('+') => {
                    self.pos += 1;
                    Ok(Some((1, None)))
                }
                Some('?') => {
                    self.pos += 1;
                    Ok(Some((0, Some(1))))
                }
                Some('{') => {
                    let saved = self.pos;
                    self.pos += 1;
                    let Some(min) = self.parse_decimal() else {
                        self.pos = saved;
                        return Ok(None);
                    };
                    let max = if self.eat(',') {
                        self.parse_decimal()
                    } else {
                        Some(min)
                    };
                    if !self.eat('}') {
                        self.pos = saved;
                        return Ok(None);
                    }
                    if let Some(upper) = max {
                        if upper < min {
                            return Err("numbers out of order in {} quantifier".to_string());
                        }
                    }
                    Ok(Some((min, max)))
                }
                _ => Ok(None),
            }
        }

        fn parse_decimal(&mut self) -> Option<u32> {
            let start = self.pos;
            let mut value: u64 = 0;
            while let Some(digit) = self.peek().and_then(|c| c.to_digit(10)) {
                value = (value * 10 + digit as u64).min(u32::MAX as u64);
                self.pos += 1;
            }
            if self.pos == start {
                None
            } else {
                Some(value as u32)
            }
        }

        fn parse_escape_atom(&mut self) -> Result<Node, String> {
            let Some(escaped) = self.advance() else {
                return Err("\\ at end of pattern".to_string());
            };
            match escaped {
                'd' => Ok(shorthand_class(ClassItem::Digit)),
                'D' => Ok(shorthand_class(ClassItem::NotDigit)),
                'w' => Ok(shorthand_class(ClassItem::Word)),
                'W' => Ok(shorthand_class(ClassItem::NotWord)),
                's' => Ok(shorthand_class(ClassItem::Space)),
                'S' => Ok(shorthand_class(ClassItem::NotSpace)),
                'b' => Ok(Node::WordBoundary { negated: false }),
                'B' => Ok(Node::WordBoundary { negated: true }),
                '1'..='9' => Err(
                    "backreferences (\\1-\\9) are not supported by this CLI's regex engine"
                        .to_string(),
                ),
                'c' => match self.peek() {
                    Some(letter) if letter.is_ascii_alphabetic() => {
                        self.pos += 1;
                        Ok(Node::Char((((letter as u8) & 0x1f) as u8) as char))
                    }
                    _ => Ok(Node::Seq(vec![Node::Char('\\'), Node::Char('c')])),
                },
                other => Ok(Node::Char(self.escaped_char(other))),
            }
        }

        fn escaped_char(&mut self, escaped: char) -> char {
            match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'f' => '\u{c}',
                'v' => '\u{b}',
                '0' => '\0',
                'x' => self.parse_hex_escape(2).unwrap_or('x'),
                'u' => self.parse_hex_escape(4).unwrap_or('u'),
                other => other,
            }
        }

        fn parse_hex_escape(&mut self, digits: usize) -> Option<char> {
            let saved = self.pos;
            let mut value: u32 = 0;
            for _ in 0..digits {
                let Some(digit) = self.peek().and_then(|c| c.to_digit(16)) else {
                    self.pos = saved;
                    return None;
                };
                value = value * 16 + digit;
                self.pos += 1;
            }
            match char::from_u32(value) {
                Some(character) => Some(character),
                None => {
                    self.pos = saved;
                    None
                }
            }
        }

        fn parse_class(&mut self) -> Result<Node, String> {
            let negated = self.eat('^');
            let mut items: Vec<ClassItem> = Vec::new();
            loop {
                let Some(character) = self.peek() else {
                    return Err("Unterminated character class".to_string());
                };
                if character == ']' {
                    self.pos += 1;
                    break;
                }
                let first = self.parse_class_element()?;
                if let ClassItem::Char(low) = first {
                    let dash_next = self.peek() == Some('-')
                        && self.chars.get(self.pos + 1).copied() != Some(']')
                        && self.chars.get(self.pos + 1).is_some();
                    if dash_next {
                        self.pos += 1; // consume '-'
                        let second = self.parse_class_element()?;
                        match second {
                            ClassItem::Char(high) => {
                                if (high as u32) < (low as u32) {
                                    return Err("Range out of order in character class".to_string());
                                }
                                items.push(ClassItem::Range(low, high));
                                continue;
                            }
                            other => {
                                // Annex B: shorthand adjacent to '-' keeps '-' literal.
                                items.push(ClassItem::Char(low));
                                items.push(ClassItem::Char('-'));
                                items.push(other);
                                continue;
                            }
                        }
                    }
                }
                items.push(first);
            }
            Ok(Node::Class { negated, items })
        }

        fn parse_class_element(&mut self) -> Result<ClassItem, String> {
            let character = self.advance().expect("caller checked peek");
            if character != '\\' {
                return Ok(ClassItem::Char(character));
            }
            let Some(escaped) = self.advance() else {
                return Err("\\ at end of pattern".to_string());
            };
            Ok(match escaped {
                'd' => ClassItem::Digit,
                'D' => ClassItem::NotDigit,
                'w' => ClassItem::Word,
                'W' => ClassItem::NotWord,
                's' => ClassItem::Space,
                'S' => ClassItem::NotSpace,
                'b' => ClassItem::Char('\u{8}'),
                '0'..='7' => {
                    // Octal escape (Annex B), up to 3 octal digits total.
                    let mut value = escaped.to_digit(8).expect("octal digit");
                    let mut count = 1;
                    while count < 3 {
                        let Some(digit) = self.peek().and_then(|c| c.to_digit(8)) else {
                            break;
                        };
                        value = value * 8 + digit;
                        self.pos += 1;
                        count += 1;
                    }
                    ClassItem::Char(char::from_u32(value).unwrap_or('\0'))
                }
                other => ClassItem::Char(self.escaped_char(other)),
            })
        }
    }

    fn shorthand_class(item: ClassItem) -> Node {
        Node::Class {
            negated: false,
            items: vec![item],
        }
    }

    fn match_node(
        node: &Node,
        chars: &[char],
        pos: usize,
        cont: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        match node {
            Node::Char(expected) => pos < chars.len() && chars[pos] == *expected && cont(pos + 1),
            Node::Any => pos < chars.len() && !is_line_terminator(chars[pos]) && cont(pos + 1),
            Node::Class { negated, items } => {
                pos < chars.len() && (class_matches(items, chars[pos]) != *negated) && cont(pos + 1)
            }
            Node::StartAnchor => pos == 0 && cont(pos),
            Node::EndAnchor => pos == chars.len() && cont(pos),
            Node::WordBoundary { negated } => {
                let before = pos > 0 && is_word_char(chars[pos - 1]);
                let after = pos < chars.len() && is_word_char(chars[pos]);
                ((before != after) != *negated) && cont(pos)
            }
            Node::Seq(nodes) => match_seq(nodes, chars, pos, cont),
            Node::Alt(branches) => {
                for branch in branches {
                    if match_node(branch, chars, pos, &mut *cont) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat {
                node,
                min,
                max,
                lazy,
            } => match_repeat(node, *min, *max, *lazy, 0, chars, pos, cont),
            Node::Look { negated, node } => {
                let matched = match_node(node, chars, pos, &mut |_| true);
                (matched != *negated) && cont(pos)
            }
        }
    }

    fn match_seq(
        nodes: &[Node],
        chars: &[char],
        pos: usize,
        cont: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        match nodes.split_first() {
            None => cont(pos),
            Some((first, rest)) => match_node(first, chars, pos, &mut |next| {
                match_seq(rest, chars, next, cont)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn match_repeat(
        node: &Node,
        min: u32,
        max: Option<u32>,
        lazy: bool,
        count: u32,
        chars: &[char],
        pos: usize,
        cont: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        let can_more = max.map_or(true, |limit| count < limit);
        if lazy {
            if count >= min && cont(pos) {
                return true;
            }
            if can_more {
                return match_node(node, chars, pos, &mut |next| {
                    if next == pos && count + 1 >= min {
                        // Zero-width iteration adds nothing once min is reached.
                        return false;
                    }
                    match_repeat(node, min, max, lazy, count + 1, chars, next, cont)
                });
            }
            false
        } else {
            if can_more
                && match_node(node, chars, pos, &mut |next| {
                    if next == pos {
                        // Zero-width progress: stop expanding to avoid loops,
                        // but finish through the continuation once min is met.
                        if count + 1 >= min {
                            return cont(pos);
                        }
                    }
                    match_repeat(node, min, max, lazy, count + 1, chars, next, cont)
                })
            {
                return true;
            }
            count >= min && cont(pos)
        }
    }

    fn class_matches(items: &[ClassItem], character: char) -> bool {
        items.iter().any(|item| match item {
            ClassItem::Char(expected) => character == *expected,
            ClassItem::Range(low, high) => (*low..=*high).contains(&character),
            ClassItem::Digit => character.is_ascii_digit(),
            ClassItem::NotDigit => !character.is_ascii_digit(),
            ClassItem::Word => is_word_char(character),
            ClassItem::NotWord => !is_word_char(character),
            ClassItem::Space => is_js_whitespace(character),
            ClassItem::NotSpace => !is_js_whitespace(character),
        })
    }

    fn is_word_char(character: char) -> bool {
        character.is_ascii_alphanumeric() || character == '_'
    }

    fn is_line_terminator(character: char) -> bool {
        matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}')
    }

    /// JS \s character class membership.
    fn is_js_whitespace(character: char) -> bool {
        matches!(
            character,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flags_from(args: &[&str]) -> (Flags, Vec<String>) {
        let owned: Vec<String> = args.iter().map(|value| value.to_string()).collect();
        let parsed = parse_args(&owned);
        (parsed.flags, parsed.rest)
    }

    #[test]
    fn parse_wait_for_text_defaults_and_clamps() {
        let (flags, rest) = flags_from(&["my-session", "PHASE", "1", "COMPLETE"]);
        let parsed = parse_wait_for_text(&rest, &flags);
        assert_eq!(parsed.interval_seconds, 20.0);
        assert_eq!(parsed.lines, 200.0);
        assert_eq!(parsed.timeout_seconds, 1800.0);
        assert_eq!(parsed.pattern, "PHASE 1 COMPLETE");
        assert_eq!(parsed.selector, Some("my-session".to_string()));

        let (flags, rest) = flags_from(&[
            "--interval-seconds",
            "1",
            "--lines",
            "99999",
            "--timeout-seconds",
            "2",
            "--pattern",
            "DONE$",
            "--session-id",
            "s1",
        ]);
        let parsed = parse_wait_for_text(&rest, &flags);
        assert_eq!(parsed.interval_seconds, 2.0);
        assert_eq!(parsed.lines, 2000.0);
        assert_eq!(parsed.timeout_seconds, 5.0);
        assert_eq!(parsed.pattern, "DONE$");
        // --session-id suppresses positional selector parsing entirely.
        assert_eq!(parsed.selector, None);

        // Non-numeric values fall back to defaults.
        let (flags, rest) = flags_from(&["s", "p", "--interval-seconds", "soon"]);
        assert_eq!(parse_wait_for_text(&rest, &flags).interval_seconds, 20.0);
    }

    #[test]
    fn limit_text_lines_keeps_tail_window() {
        assert_eq!(limit_text_lines("a\nb\nc\nd", Some(2.0)), "c\nd");
        assert_eq!(limit_text_lines("a\r\nb\r\nc", Some(2.0)), "b\nc");
        assert_eq!(limit_text_lines("a\nb", Some(10.0)), "a\nb");
        assert_eq!(limit_text_lines("a\nb", None), "a\nb");
        assert_eq!(limit_text_lines("a\nb", Some(0.0)), "a\nb");
        assert_eq!(limit_text_lines("a\nb", Some(-3.0)), "a\nb");
        assert_eq!(limit_text_lines("a\nb\nc", Some(2.5)), "b\nc");
    }

    #[test]
    fn wait_for_text_match_scans_from_last_line() {
        let regex = js_regex::Regex::new("^\\s*PHASE 1 (COMPLETE|BLOCKED)").unwrap();
        let text = "noise\n  PHASE 1 COMPLETE\nmore noise\n  PHASE 1 BLOCKED\ntail";
        assert_eq!(
            find_wait_for_text_match(text, &regex),
            Some("  PHASE 1 BLOCKED")
        );
        // The anchor binds to line starts, so mid-line mentions never match.
        let text = "the agent said PHASE 1 COMPLETE in its reasoning";
        assert_eq!(find_wait_for_text_match(text, &regex), None);
        assert_eq!(find_wait_for_text_match("", &regex), None);
    }

    #[test]
    fn js_regex_supports_sentinel_patterns() {
        let test = |pattern: &str, input: &str| js_regex::Regex::new(pattern).unwrap().test(input);
        assert!(test(
            "^\\s*PHASE 1 (COMPLETE|BLOCKED)",
            "   PHASE 1 COMPLETE"
        ));
        assert!(!test(
            "^\\s*PHASE 1 (COMPLETE|BLOCKED)",
            "x PHASE 1 COMPLETE"
        ));
        assert!(test("READY$", "worker READY"));
        assert!(!test("READY$", "READY to go"));
        assert!(test("PHASE(?= 2)", "PHASE 2 START"));
        assert!(!test("PHASE(?= 2)", "PHASE 1 START"));
        assert!(test("PHASE(?! 1)", "PHASE 2"));
        assert!(!test("PHASE(?! 1)", "PHASE 1"));
        assert!(test("\\bDONE\\b", "task DONE."));
        assert!(!test("\\bDONE\\b", "ABANDONED"));
        assert!(test("[0-9]+m?s", "took 250ms"));
        assert!(test("a{2,3}", "caaad"));
        assert!(!test("^a{2,3}$", "aaaa"));
        assert!(test("colou?r", "color"));
        assert!(test("colou?r", "colour"));
        assert!(test("^[^#].*done", "all done"));
        assert!(!test("^[^#].*done", "# all done"));
        assert!(test("a.c", "abc"));
        assert!(!test("a.c", "a\nc"));
        assert!(test("x|y|z{2}", "wzz"));
        assert!(test("\\d\\d:\\d\\d", "at 10:42 today"));
        assert!(test("(?:ab)+c", "ababc"));
        assert!(test("a+?b", "aaab"));
        assert!(test("^$", ""));
        assert!(!test("^$", "x"));
        // Literal braces that do not form quantifiers.
        assert!(test("\\{3}", "{3}"));
        assert!(test("a\\*b", "a*b"));
        assert!(test("a{", "a{"));
        assert!(test("{x}", "{x}"));
        assert!(test("a{,3}", "a{,3}"));
        // Lookaheads stay quantifiable (Annex B), assertions do not.
        assert!(js_regex::Regex::new("(?=a)*").is_ok());
        assert!(test("\\x1", "x1"));
        assert!(test("\\u12", "u12"));
        assert!(test("[a-]", "-"));
        assert!(test("[\\d-x]", "-"));
        assert!(test("[^]", "x"));
        assert!(!test("[]", "x"));
    }

    #[test]
    fn js_regex_rejects_invalid_patterns() {
        let error = js_regex::Regex::new("(").unwrap_err();
        assert!(error.contains("Unterminated group"), "{error}");
        let error = js_regex::Regex::new("a)").unwrap_err();
        assert!(error.contains("Unmatched ')'"), "{error}");
        let error = js_regex::Regex::new("[a").unwrap_err();
        assert!(error.contains("Unterminated character class"), "{error}");
        let error = js_regex::Regex::new("*a").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("a{3,1}").unwrap_err();
        assert!(error.contains("numbers out of order"), "{error}");
        let error = js_regex::Regex::new("a\\").unwrap_err();
        assert!(error.contains("\\ at end of pattern"), "{error}");
        let error = js_regex::Regex::new("(a)\\1").unwrap_err();
        assert!(error.contains("backreferences"), "{error}");
        let error = js_regex::Regex::new("(?<=a)b").unwrap_err();
        assert!(error.contains("lookbehind"), "{error}");
        // V8 rejects bare quantifiers and quantified assertions.
        let error = js_regex::Regex::new("{3}").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("{2,}").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("^*").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("\\b*").unwrap_err();
        assert!(error.contains("Nothing to repeat"), "{error}");
        let error = js_regex::Regex::new("[b-a]").unwrap_err();
        assert!(
            error.contains("Range out of order in character class"),
            "{error}"
        );
        let error = js_regex::Regex::new("(?xa)").unwrap_err();
        assert!(error.contains("Invalid group"), "{error}");
    }

    #[test]
    fn js_regex_zero_width_repeats_terminate() {
        let test = |pattern: &str, input: &str| js_regex::Regex::new(pattern).unwrap().test(input);
        assert!(test("(?:)*", ""));
        assert!(test("(?:a?)*b", "b"));
        assert!(test("(a*)*b", "aab"));
        assert!(!test("(a+)+c", "aaab"));
    }

    // Differential battery: expected values generated with Node's RegExp on
    // the same (pattern, input) pairs; the private engine must agree.
    #[test]
    fn js_regex_matches_node_regexp_battery() {
        let cases: &[(&str, &str, bool)] = &[
            (
                "^\\s*PHASE \\d+ (COMPLETE|BLOCKED)$",
                "PHASE 12 BLOCKED",
                true,
            ),
            (
                "^\\s*PHASE \\d+ (COMPLETE|BLOCKED)$",
                "\tPHASE 3 COMPLETE",
                true,
            ),
            (
                "^\\s*PHASE \\d+ (COMPLETE|BLOCKED)$",
                "PHASE 3 COMPLETE.",
                false,
            ),
            ("error|warning", "no problems here", false),
            ("error|warning", "1 warning generated", true),
            ("\\$\\d+\\.\\d{2}", "cost $14.99 total", true),
            ("\\$\\d+\\.\\d{2}", "cost $14.9 total", false),
            ("(?=.*foo)(?=.*bar)", "bar then foo", true),
            ("(?=.*foo)(?=.*bar)", "only foo", false),
            ("^\\[worker-[0-9]+\\] ready$", "[worker-2] ready", true),
            ("a[^bc]d", "axd", true),
            ("a[^bc]d", "abd", false),
            ("^\\W+$", "!!  ??", true),
            ("\\S+@\\S+\\.[a-z]{2,}", "mail me at x@y.io ok", true),
            ("^(foo)?bar", "bar", true),
            ("^(foo)?bar", "foobar", true),
            ("z{0}", "anything", true),
            ("^(a|b)+$", "abab", true),
            ("^(a|b)+$", "abcab", false),
            ("done(?!!)", "done!", false),
            ("done(?!!)", "done.", true),
            ("^.{3,5}$", "abcd", true),
            ("^.{3,5}$", "ab", false),
            ("[A-Fa-f0-9]{6}", "color a1B2c3 here", true),
            ("\\bv\\d+\\.\\d+\\.\\d+\\b", "release v1.22.3 shipped", true),
            ("  +", "double  space", true),
            ("  +", "single space", false),
            ("^\\d*$", "", true),
            ("ab*?c", "abbbc", true),
            ("^[-+]?\\d+$", "-42", true),
            ("\\u0041BC", "ABC", true),
            ("\\x41BC", "ABC", true),
            ("\\t\\w", "\tx", true),
            ("\\0", "a b", false),
        ];
        for (pattern, input, expected) in cases {
            let regex = js_regex::Regex::new(pattern)
                .unwrap_or_else(|error| panic!("pattern {pattern:?} failed to compile: {error}"));
            assert_eq!(
                regex.test(input),
                *expected,
                "pattern {pattern:?} on {input:?}"
            );
        }
    }

    fn fixture_sessions() -> Vec<Value> {
        vec![
            json!({
                "alias": 1,
                "sessionId": "sess-1",
                "globalRef": "gx:p1:sess-1",
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "provider": "zmx",
                "providerSessionName": "g-0713-090001",
                "title": "Fix sidebar drag",
                "displayTitle": "Fix sidebar drag",
            }),
            json!({
                "alias": 2,
                "sessionId": "sess-2",
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "provider": "zmx",
                "providerSessionName": "g-0713-090002",
                "title": "Port CLI",
            }),
            json!({
                "alias": 3,
                "sessionId": "sess-3",
                "projectId": "p2",
                "projectName": "Zephyr",
                "projectPath": "/Users/dev/zephyr",
                "title": "Port CLI",
            }),
        ]
    }

    #[test]
    fn resolve_listed_sessions_matches_by_id_ref_provider_and_title() {
        let sessions_list = fixture_sessions();
        let flags = Flags::default();
        let by_id = resolve_listed_sessions("sess-2", &sessions_list, &flags).unwrap();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0]["alias"], json!(2));
        let by_ref = resolve_listed_sessions("gx:p1:sess-1", &sessions_list, &flags).unwrap();
        assert_eq!(by_ref[0]["alias"], json!(1));
        let by_provider_name =
            resolve_listed_sessions("g-0713-090002", &sessions_list, &flags).unwrap();
        assert_eq!(by_provider_name[0]["alias"], json!(2));
        let by_provider_pair =
            resolve_listed_sessions("zmx/g-0713-090001", &sessions_list, &flags).unwrap();
        assert_eq!(by_provider_pair[0]["alias"], json!(1));
        let by_title = resolve_listed_sessions("port cli", &sessions_list, &flags).unwrap();
        assert_eq!(by_title.len(), 2);
        let by_partial = resolve_listed_sessions("sidebar", &sessions_list, &flags).unwrap();
        assert_eq!(by_partial.len(), 1);
        assert_eq!(by_partial[0]["alias"], json!(1));
        let by_project_title =
            resolve_listed_sessions("zephyr:port cli", &sessions_list, &flags).unwrap();
        assert_eq!(by_project_title.len(), 1);
        assert_eq!(by_project_title[0]["alias"], json!(3));
        let none = resolve_listed_sessions("no-such-thing", &sessions_list, &flags).unwrap();
        assert!(none.is_empty());
        assert!(resolve_listed_sessions("   ", &sessions_list, &flags).is_err());
    }

    #[test]
    fn resolve_listed_sessions_honors_project_scope() {
        let sessions_list = fixture_sessions();
        let mut flags = Flags::default();
        flags.insert_text("projectId", "p2");
        let scoped = resolve_listed_sessions("port cli", &sessions_list, &flags).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0]["alias"], json!(3));
    }

    #[test]
    fn resolve_one_listed_session_error_messages() {
        let sessions_list = fixture_sessions();
        let flags = Flags::default();
        let missing = resolve_one_listed_session("nope", &sessions_list, &flags).unwrap_err();
        assert_eq!(
            missing.to_string(),
            "No matching session found for \"nope\". Run \"ghostex sessions\" or \"gx sessions\" to list sessions."
        );
        let ambiguous = resolve_one_listed_session("port cli", &sessions_list, &flags).unwrap_err();
        assert_eq!(
            ambiguous.to_string(),
            "Multiple sessions matched \"port cli\":\n2. Ghostex - Port CLI\n3. Zephyr - Port CLI"
        );
    }

    #[test]
    fn format_session_matches_uses_display_title_fallbacks() {
        let sessions_list = vec![
            json!({ "alias": 5, "projectName": "P", "displayTitle": "D", "title": "T" }),
            json!({ "alias": 6, "projectName": "P", "title": "T" }),
            json!({ "alias": 7 }),
        ];
        assert_eq!(
            format_session_matches(&sessions_list),
            "5. P - D\n6. P - T\n7. undefined - undefined"
        );
    }

    #[test]
    fn json_finite_number_mirrors_json_stringify() {
        assert_eq!(json_finite_number(Some(5000.0)), json!(5000));
        assert_eq!(json_finite_number(Some(2.5)), json!(2.5));
        assert_eq!(json_finite_number(None), Value::Null);
        assert_eq!(json_finite_number(Some(f64::INFINITY)), Value::Null);
    }

    #[test]
    fn result_error_message_prefers_error_field() {
        let fallback = || "Could not focus X.".to_string();
        assert_eq!(
            result_error_message(&json!({ "error": "boom" }), fallback),
            "boom"
        );
        let fallback = || "Could not focus X.".to_string();
        assert_eq!(
            result_error_message(&json!({ "error": null }), fallback),
            "Could not focus X."
        );
        let fallback = || "Could not focus X.".to_string();
        assert_eq!(
            result_error_message(&json!({}), fallback),
            "Could not focus X."
        );
    }

    #[test]
    fn js_helpers_coerce_like_javascript() {
        assert_eq!(js_display(Some(&json!(4))), "4");
        assert_eq!(js_display(None), "undefined");
        assert_eq!(js_display(Some(&json!(null))), "null");
        assert_eq!(js_string_or_empty(Some(&json!(null))), "");
        assert_eq!(js_f64_string(1800.0), "1800");
        assert_eq!(js_f64_string(7.5), "7.5");
        assert!(js_truthy(Some(&json!("x"))));
        assert!(!js_truthy(Some(&json!(""))));
        assert!(!js_truthy(None));
    }
}
