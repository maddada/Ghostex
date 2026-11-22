//! `ghostex agents role set | show | clear [--session <ref>] [--json]` and `--role` helper for
//! `agents create` and `agents link` (gxserver's `/api/sessionStandingRole`,
//! server/src/agents/standing_role.rs).

use super::{
    arguments::Arguments,
    identity::{self, text},
};
use crate::ghostex_cli::{
    args::Flags,
    rpc::{CliError, CliResult, call_gxserver_rpc},
    selector, sessions,
};
use serde_json::{Value, json};

/// Validates a `--role` or `role set` value before mutating any session.
pub(super) fn validate_role(raw: &str) -> CliResult<String> {
    crate::agents::standing_role::normalize_standing_role(raw)
        .map_err(|error| CliError::Other(error.message))
}

/// Persists `role` on `session` through `/api/sessionStandingRole`.
pub(super) fn set_session_role(session: &Value, role: &str, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc(
        crate::agents::standing_role::SESSION_STANDING_ROLE_ENDPOINT,
        &json!({
            "op": "set",
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "role": role,
        }),
        flags,
    )
}

fn resolve_target(session_ref: Option<&str>, flags: &Flags) -> CliResult<(Value, Flags)> {
    if let Some(reference) = session_ref {
        let flags = identity::inventory_flags(flags, reference)?;
        let rows = sessions::fetch_session_list(&flags, false)?;
        let session = selector::resolve_one_listed_session(reference, &rows, &flags)?;
        if !identity::is_agent(&session) {
            return Err(CliError::Other(format!(
                "{reference} is not an agent session."
            )));
        }
        return Ok((session, flags));
    }
    if flags.contains("computer") || flags.contains("server") {
        return Err(CliError::Other(
            "Pass --session <session-ref> when targeting another server or computer.".into(),
        ));
    }
    let session = identity::caller_with_no_session_hint(
        "Cannot identify the caller. Run inside a Ghostex agent session, or pass --session <session-ref>.",
    )?;
    let flags = identity::inventory_flags(flags, text(&session, "globalRef"))?;
    Ok((session, flags))
}

pub(super) fn run(args: &Arguments) -> CliResult<Value> {
    let action = args.positional[0].as_str();
    match action {
        "set" => {
            let role = validate_role(&args.positional[1])?;
            let (session, flags) = resolve_target(args.session.as_deref(), &args.flags)?;
            let result = set_session_role(&session, &role, &flags)?;
            Ok(json!({
                "ok": true,
                "status": "set",
                "session": identity::summary(&session),
                "role": result["role"],
                "updatedAt": result["updatedAt"],
            }))
        }
        "show" => {
            let (session, flags) = resolve_target(args.session.as_deref(), &args.flags)?;
            let result = call_gxserver_rpc(
                crate::agents::standing_role::SESSION_STANDING_ROLE_ENDPOINT,
                &json!({
                    "op": "show",
                    "projectId": session["projectId"],
                    "sessionId": session["sessionId"],
                }),
                &flags,
            )?;
            Ok(json!({
                "ok": true,
                "session": identity::summary(&session),
                "role": result["role"],
                "updatedAt": result["updatedAt"],
            }))
        }
        "clear" => {
            let (session, flags) = resolve_target(args.session.as_deref(), &args.flags)?;
            let result = call_gxserver_rpc(
                crate::agents::standing_role::SESSION_STANDING_ROLE_ENDPOINT,
                &json!({
                    "op": "clear",
                    "projectId": session["projectId"],
                    "sessionId": session["sessionId"],
                }),
                &flags,
            )?;
            Ok(json!({
                "ok": true,
                "status": result["status"],
                "cleared": result["cleared"],
                "session": identity::summary(&session),
            }))
        }
        _ => unreachable!("validated role action"),
    }
}

pub(super) fn print_text(action: &str, result: &Value) {
    let reference = text(&result["session"], "globalRef");
    match action {
        "set" => println!(
            "Standing role set for {reference}: {}",
            text(result, "role")
        ),
        "show" => {
            let role = text(result, "role");
            if role.is_empty() {
                println!("No standing role set for {reference}.");
            } else {
                println!("{role}");
            }
        }
        "clear" => {
            if result["cleared"] == json!(true) {
                println!("Standing role cleared for {reference}.");
            } else {
                println!("No standing role was set for {reference}.");
            }
        }
        _ => {}
    }
}
