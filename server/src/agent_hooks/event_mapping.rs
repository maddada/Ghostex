use std::env;

use serde_json::{json, Map, Value};

use super::notify_runtime::read_state_string;
use super::probing::{normalize_prompt_text, now_iso};

pub(crate) fn nested_get<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    Some(current)
}

pub(crate) fn first_string<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(normalize_prompt_text)
            .filter(|text| !text.is_empty())
        {
            return Some(text);
        }
    }
    None
}

pub(crate) fn first_path<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

pub(crate) fn env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| normalize_prompt_text(&value))
        .filter(|value| !value.is_empty())
}

pub(crate) fn normalized_hook_agent_key(value: &str) -> String {
    let normalized = normalize_prompt_text(&value.to_ascii_lowercase());
    let mapped = match normalized.as_str() {
        "claude" | "claude code" => "claude",
        "openclaude" | "open claude" | "openclaude cli" => "openclaude",
        "command-code" | "commandcode" | "command code" => "command-code",
        "codex" | "openai codex" | "codex cli" => "codex",
        "kimi" | "kimi code" => "kimi",
        "campfire" => "campfire",
        "devin" => "devin",
        "pi" | "π" => "pi",
        "omp" => "omp",
        "opencode" | "open code" => "opencode",
        "grok" | "grok build" => "grok",
        "amp" | "amp cli" => "amp",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "gemini" | "gemini cli" => "gemini",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "copilot" | "github copilot" => "copilot",
        "codebuddy" | "code buddy" => "codebuddy",
        "droid" | "factory" | "factory droid" => "droid",
        "kiro" | "kiro-cli" | "kiro cli" => "kiro",
        "qoder" | "qodercli" => "qoder",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if cleaned.is_empty() {
        "codex".to_string()
    } else {
        cleaned
    }
}

pub(crate) fn activity_for_hook_event(
    agent_key: &str,
    event_name: &str,
    payload: &Value,
) -> Option<String> {
    let normalized_event_name = normalize_prompt_text(event_name);
    let lower = normalized_event_name.to_ascii_lowercase();
    let compact = lower.replace(['_', '-', '.'], "");
    /*
    CDXC:AgentHooks 2026-08-27:
    Subagent and teammate lifecycle events describe a CHILD of the session, not
    the lead pane, so they must never move the lead session's activity. They are
    roster-only. Returning early — before any agent-specific or generic
    matching — keeps a future generic rule from accidentally claiming them
    (today's compact matching is exact-string, so "subagentstop" would not hit
    the "stop" arm, but the intent should not depend on that).
    */
    if matches!(
        compact.as_str(),
        "subagentstart" | "subagentstop" | "teammateidle"
    ) {
        return None;
    }
    /*
    PreCompact (registered by Copilot) fires before the compaction is validated
    and an aborted compact emits it alone, so it carries no usable activity
    signal.
    */
    if compact == "precompact" {
        return None;
    }
    /*
    Copilot's ErrorOccurred ends the turn unless the runtime says it recovered
    and kept going.
    */
    if compact == "erroroccurred" {
        let recoverable = payload_boolean(
            payload,
            &[
                "recoverable",
                "metadata.recoverable",
                "properties.recoverable",
            ],
        );
        return Some(
            if recoverable == Some(true) {
                "working"
            } else {
                "idle"
            }
            .to_string(),
        );
    }
    if agent_key == "codex" {
        if lower == "stop" {
            return Some("attention".to_string());
        }
        if matches!(lower.as_str(), "interrupt" | "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    // OpenClaude emits Claude's hook contract verbatim, so it shares every
    // Claude-specific rule below instead of falling through to the generic
    // tables (which have no PostCompact trigger check and no StopFailure arm).
    if matches!(agent_key, "claude" | "openclaude") {
        /*
        CDXC:AgentHooks 2026-08-27:
        Claude skips Stop after a model error and emits StopFailure instead, so
        without this arm the pane spins "working" forever on every failed turn.
        */
        if matches!(
            lower.as_str(),
            "stop" | "stopfailure" | "idle" | "sessionend"
        ) {
            return Some("idle".to_string());
        }
        /*
        A MANUAL /compact ends at an idle input prompt with no Stop behind it,
        so it is a real turn boundary. An AUTO-compact fires mid-turn; mapping
        that to idle would blip a working session, so it stays unmapped.
        */
        if lower == "postcompact" {
            return payload_compact_trigger_is_manual(payload).then(|| "idle".to_string());
        }
        /*
        CDXC:SessionChatPromptQueue 2026-08-24:
        SessionStart is the ONLY hook Claude Code fires when /compact or /clear
        finishes — the UserPromptSubmit that submitted the command marked the
        session "working" and no Stop ever follows, which left sessions (and the
        prompt-queue scheduler gating on them) stuck working forever after a
        manual compaction. Every SessionStart source (startup, resume, clear,
        compact) means the CLI is sitting at its input prompt, so map it to
        idle. An AUTO-compact mid-turn also fires this and blips the session
        idle; the next PreToolUse or working-spinner title restores it, and the
        queue stays safe behind its transcript-lifecycle gate.
        */
        if matches!(lower.as_str(), "sessionstart" | "session-start") {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit"
                | "prompt-submit"
                | "pretooluse"
                | "pre-tool-use"
                | "posttooluse"
                | "posttoolusefailure"
        ) {
            return Some("working".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if matches!(agent_key, "copilot" | "codebuddy" | "droid" | "qoder") {
        if matches!(
            lower.as_str(),
            "stop" | "notification" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(lower.as_str(), "pretooluse" | "pre-tool-use") {
            return Some("working".to_string());
        }
    }
    if agent_key == "antigravity" {
        let fully_idle = payload_boolean(
            payload,
            &[
                "fullyIdle",
                "fully_idle",
                "metadata.fullyIdle",
                "properties.fullyIdle",
            ],
        );
        if fully_idle == Some(false)
            && matches!(lower.as_str(), "stop" | "turn-completion" | "notification")
        {
            return Some("working".to_string());
        }
        if matches!(
            lower.as_str(),
            "stop" | "turn-completion" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "preinvocation" | "postinvocation" | "pretooluse" | "posttooluse"
        ) {
            return Some("working".to_string());
        }
    }
    if matches!(
        compact.as_str(),
        "agentstart"
            | "aftertool"
            | "beforeagentstart"
            | "beforeagent"
            | "beforemcpexecution"
            | "beforeshellexecution"
            | "beforesubmitprompt"
            | "beforetool"
            | "messagepart"
            | "onsessionreset"
            | "onsessionstart"
            | "ontoolpermission"
            | "postapprovalresponse"
            // Devin's post-compaction event fires mid-turn, so the turn is
            // still running (unlike Claude's manual PostCompact).
            | "postcompaction"
            | "posttooluse"
            | "posttoolusefailure"
            | "prellmcall"
            | "pretoolcall"
            | "preinvocation"
            | "postinvocation"
            | "pretooluse"
            | "promptsubmit"
            | "sessionbusy"
            | "userpromptsubmit"
    ) {
        return Some("working".to_string());
    }
    if matches!(
        compact.as_str(),
        "askuserquestion" | "notification" | "notify" | "permissionrequest" | "preapprovalrequest"
    ) {
        return Some("attention".to_string());
    }
    if matches!(
        compact.as_str(),
        "afteragent"
            | "afteragentresponse"
            | "agentend"
            | "agentresponse"
            | "oncomplete"
            | "onerror"
            | "onsessionend"
            | "onsessionfinalize"
            | "postllmcall"
            | "release"
            | "interrupt"
            | "sessionend"
            | "sessionidle"
            | "sessionshutdown"
            | "stop"
            | "stopfailure"
            | "turncompletion"
    ) {
        return Some("idle".to_string());
    }
    None
}

/*
CDXC:AgentHooks 2026-08-27:
Claude's PostCompact payload distinguishes a user-run /compact from an
auto-compact through a top-level `trigger` field. Providers that wrap hook
payloads repeat it one level down, so accept the common wrappers too.
*/
fn payload_compact_trigger_is_manual(payload: &Value) -> bool {
    first_string([
        payload.get("trigger"),
        nested_get(payload, &["payload", "trigger"]),
        nested_get(payload, &["metadata", "trigger"]),
        nested_get(payload, &["properties", "trigger"]),
    ])
    .is_some_and(|trigger| trigger.eq_ignore_ascii_case("manual"))
}

/*
CDXC:SessionChatPromptQueue 2026-08-24:
Claude Code sends two very different things through the same Notification hook:
permission requests ("Claude needs your permission to use …"), which are real
attention — a prompt delivered now would be swallowed as the ANSWER — and the
60-second idle reminder ("Claude is waiting for your input"), which means the
input line is empty and waiting. Treating the reminder as attention permanently
blockaded the prompt-queue scheduler whenever a session was stuck "working"
(e.g. after a local command that never fires Stop). This predicate identifies
the reminder so both mapping layers can refuse to escalate a stuck session on
it; genuine permission notifications keep their attention transition.
*/
pub(crate) fn claude_notification_is_idle_input(payload: &Value) -> bool {
    first_string([payload.get("message")])
        .map(|message| {
            message
                .to_ascii_lowercase()
                .contains("waiting for your input")
        })
        .unwrap_or(false)
}

fn payload_boolean(payload: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        let value = if key.contains('.') {
            nested_get(payload, &key.split('.').collect::<Vec<_>>())
        } else {
            payload.get(*key)
        };
        match value {
            Some(Value::Bool(value)) => return Some(*value),
            Some(Value::String(value)) if matches!(value.as_str(), "true" | "1") => {
                return Some(true)
            }
            Some(Value::String(value)) if matches!(value.as_str(), "false" | "0") => {
                return Some(false)
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn update_hook_status(state: &mut Map<String, Value>, status: &str) {
    let timestamp = now_iso();
    state.insert("status".to_string(), json!(status));
    state.insert("statusUpdatedAt".to_string(), json!(timestamp.clone()));
    state.insert("lastActivityAt".to_string(), json!(timestamp.clone()));
    if status == "attention" {
        state.insert(
            "attentionEventId".to_string(),
            json!(format!("{timestamp}:attention")),
        );
        state.insert("attentionAcknowledgedAt".to_string(), json!(""));
        state.insert("attentionAcknowledgedEventId".to_string(), json!(""));
    } else if status == "working" {
        state.insert("attentionAcknowledgedAt".to_string(), json!(timestamp));
        let event_id = read_state_string(state, "attentionEventId").unwrap_or_default();
        state.insert("attentionAcknowledgedEventId".to_string(), json!(event_id));
    }
}

pub(crate) fn is_prompt_event(event_name: &str) -> bool {
    let lower = event_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "userpromptsubmit"
            | "beforeagent"
            | "preinvocation"
            | "pretooluse"
            | "beforesubmitprompt"
            | "beforeshellexecution"
            | "pre_llm_call"
            | "pre_tool_call"
            | "on_tool_permission"
            | "agent_start"
            | "agent.start"
            | "before_agent_start"
    )
}
