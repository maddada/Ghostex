use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::ghostex_cli::args::{FlagValue, Flags, parse_args};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::{actions, selector, sessions};

use super::*;

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
    let lines = if flags.contains("lines") {
        flags.number("lines")
    } else if flags.truthy("all") {
        None
    } else {
        Some(40.0)
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
    if !limited.ends_with('\n') {
        println!();
    }
    if !flags.contains("lines") && !flags.truthy("all") && limited.len() < text.len() {
        eprintln!("Showing the last 40 lines. Use --lines <n> or --all for more.");
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub(super) struct WaitForTextParams {
    pub(super) interval_seconds: f64,
    pub(super) lines: f64,
    pub(super) pattern: String,
    pub(super) selector: Option<String>,
    pub(super) timeout_seconds: f64,
}

fn clamp_number(value: Option<f64>, fallback: f64, min: f64, max: f64) -> f64 {
    match value {
        Some(parsed) if parsed.is_finite() => parsed.clamp(min, max),
        _ => fallback,
    }
}

pub(super) fn parse_wait_for_text(rest: &[String], flags: &Flags) -> WaitForTextParams {
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

pub(super) fn find_wait_for_text_match<'a>(
    text: &'a str,
    regex: &js_regex::Regex,
) -> Option<&'a str> {
    let lines: Vec<&str> = text.split('\n').collect();
    lines
        .iter()
        .rev()
        .find(|&line| regex.test(line))
        .map(|v| v as _)
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
