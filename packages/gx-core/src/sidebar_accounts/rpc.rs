//! Where an account request goes and what its answer reads as.
//!
//! Ported from the runtime's `requestGroupAccounts` and `requestSessionAccounts`,
//! `gxserver-runtime/client.ts` (`rpc`) and `gxserver-runtime/helpers/records.ts`
//! (`gpuiGxserverRpcErrorMessage`), all deleted 2026-09-25 (see git history).

use serde_json::Value;

use crate::keys::{MachineId, SessionKey};
use crate::sidebar_view::text::{is_js_line_terminator, is_js_whitespace, utf16_prefix};

/// The daemon path both pages call.
pub const AGENT_ACCOUNTS_PATH: &str = "/api/agentAccounts";

/// Which daemon an account request goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountsTarget {
    /// This computer's gxserver.
    Local,
    /// A remote machine's gxserver, down its tunnel.
    Remote(String),
}

/// `parseGpuiRemotePresentationGroupId`'s machine (`^remote:([^:]+):group:(.+)$`), else this
/// computer: the launcher asks the machine the project lives on.
pub fn group_accounts_target(group_id: &str) -> AccountsTarget {
    let remote = group_id
        .strip_prefix("remote:")
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(machine, rest)| Some((machine, rest.strip_prefix("group:")?)))
        .filter(|(machine, project)| {
            !machine.is_empty()
                && !project.is_empty()
                && !project.chars().any(is_js_line_terminator)
        });
    match remote {
        Some((machine, _)) => AccountsTarget::Remote(machine.to_string()),
        None => AccountsTarget::Local,
    }
}

/// `parseGpuiRemotePresentationSessionId(id) ?? parseGxserverPresentationProjectSessionId(id)`:
/// the session a row's account request names, or `None`, which the TypeScript answers with
/// [`SESSION_COMPUTER_UNAVAILABLE`].
pub fn session_accounts_target(session_id: &str) -> Option<SessionKey> {
    let key = SessionKey::parse_sidebar_session_id(session_id)?;
    // The remote pattern's last group is `.+`, which stops at a line terminator.
    if matches!(key.machine, MachineId::Remote(_))
        && key.session_id.chars().any(is_js_line_terminator)
    {
        return None;
    }
    Some(key)
}

/// What a row whose id names no session answers.
pub const SESSION_COMPUTER_UNAVAILABLE: &str = "The session’s computer is unavailable.";

/// `GpuiGxserverClient.rpc` from the HTTP status and body onward: the `result` of a successful
/// envelope, else the daemon's own bounded `message`, else a sentence naming the status.
pub fn agent_accounts_http_answer(status: u16, body: &str) -> Result<Value, String> {
    // `readJson`: an empty body is `undefined`. A body that does not parse throws an engine error
    // in the TypeScript, which has no counterpart; it reads as no body here.
    let parsed: Option<Value> = if body.trim().is_empty() {
        None
    } else {
        serde_json::from_str(body).ok()
    };
    let ok = (200..300).contains(&status);
    let success = parsed
        .as_ref()
        .and_then(Value::as_object)
        .filter(|envelope| {
            envelope.get("ok") == Some(&Value::Bool(true))
                && envelope.get("product").and_then(Value::as_str) == Some("gxserver")
                && envelope.contains_key("result")
        });
    let Some(envelope) = success.filter(|_| ok) else {
        return Err(rpc_error_message(parsed.as_ref()).unwrap_or_else(|| {
            let status = if status > 0 {
                status.to_string()
            } else {
                "no response".to_string()
            };
            format!("gxserver rejected {AGENT_ACCOUNTS_PATH} ({status}).")
        }));
    };
    if envelope.get("protocolVersion").and_then(Value::as_f64) != Some(1.0) {
        return Err("gxserver protocol mismatch.".to_string());
    }
    Ok(envelope.get("result").cloned().unwrap_or(Value::Null))
}

/// `gpuiGxserverRpcErrorMessage`: the `message` of an explicit failed envelope, with control
/// characters and whitespace runs collapsed, trimmed and cut to 500 UTF-16 units.
fn rpc_error_message(body: Option<&Value>) -> Option<String> {
    let envelope = body?.as_object()?;
    if envelope.get("ok") != Some(&Value::Bool(false)) {
        return None;
    }
    let message = envelope.get("message")?.as_str()?;
    let mut collapsed = String::with_capacity(message.len());
    let mut in_space = false;
    for character in message.chars() {
        // `/[\u0000-\u001f\u007f-\u009f]+/gu` to a space, then `/\s+/gu` to one space.
        let control = matches!(character, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}');
        if control || is_js_whitespace(character) {
            if !in_space {
                collapsed.push(' ');
            }
            in_space = true;
        } else {
            collapsed.push(character);
            in_space = false;
        }
    }
    let trimmed = collapsed.trim_matches(is_js_whitespace);
    Some(utf16_prefix(trimmed, 500).to_string())
}
