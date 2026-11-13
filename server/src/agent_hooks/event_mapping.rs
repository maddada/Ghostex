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
        "mastra" | "mastracode" | "mastra code" => "mastra",
        "devin" => "devin",
        "pi" | "π" => "pi",
        "zcode" | "zcode-cli" => "zcode",
        "omp" => "omp",
        "opencode" | "open code" => "opencode",
        "grok" | "grok build" => "grok",
        "amp" | "amp cli" => "amp",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "copilot" | "github copilot" => "copilot",
        "codebuddy" | "code buddy" => "codebuddy",
        "droid" | "factory" | "factory droid" => "droid",
        "kiro" | "kiro-cli" | "kiro cli" => "kiro",
        "qoder" | "qodercli" => "qoder",
        "empryo" | "em" => "empryo",
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
    if agent_key == "mastra" {
        return mastra_hook_activity(event_name, payload);
    }
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
    /*
    CDXC:Notifications 2026-10-18 WHY:
    Ghostex's Pi and OMP extensions send Stop only for a turn the agent finished on its own, and never for an Esc (Interrupt) or a provider failure (StopFailure): Pi's once `agent_settled` says no retry or queued follow-up continues it, OMP's (which has no `agent_settled`) for an `agent_end` not marked `willContinue`. OMP marks every end it continues past that way (a retry, a compaction or todo continuation, a wait on a background job); sending Stop for those made an orchestrator's OMP thread report "waiting on job" mid-task (Discord, 2026-10-18). Its Amp plugin does the same from `agent.end`'s status (`done`, `cancelled`, `error`). That makes their Stop the same completed-turn boundary as Claude's and Codex's, so a finished Pi, OMP or Amp turn enters attention and rings instead of settling silently to idle.
    SEE-ALSO: build_pi_extension_source, build_omp_extension_source and build_amp_plugin_source in server/src/agent_hooks/plugin_sources.rs, normalize_agent_hook_activity in server/src/agents/activity.rs.
    */
    if matches!(agent_key, "pi" | "omp" | "amp") && lower == "stop" {
        return Some("attention".to_string());
    }
    // OMP's question tool waits for the user's answer, as Claude's AskUserQuestion does; Claude
    // raises it through a PermissionRequest for the asking call, which OMP does not send.
    if agent_key == "omp"
        && compact == "pretooluse"
        && first_string([payload.get("tool_name"), payload.get("toolName")]).as_deref()
            == Some("ask")
    {
        return Some("attention".to_string());
    }
    /*
    CDXC:Notifications 2026-10-17 WHY:
    Cursor CLI (2026.10.16) fires `stop` at the end of every agent loop and names how it ended: `status: "completed"` for a turn it finished, `"aborted"` for a Ctrl+C (chat Stop), `"error"` for a failed request. The generic table settled all three to idle, so a finished Cursor turn never rang (agent flow tests, Windows, 2026-10-17). A completed turn enters attention like Claude's and Codex's; an aborted or failed one only ends the turn. gxserver's table reads the same field as `stopStatus`, because its `status` is the hook's own posted status.
    SEE-ALSO: normalize_agent_hook_event_activity in server/src/agents/activity.rs, the cursor params in server/src/agent_hooks/notify_runtime.rs.
    */
    if agent_key == "cursor" && lower == "stop" {
        let status = first_string([payload.get("stopStatus"), payload.get("status")]);
        return Some(
            match status.as_deref() {
                Some("aborted" | "error") => "idle",
                _ => "attention",
            }
            .to_string(),
        );
    }
    /*
    CDXC:SessionStatus 2026-10-17 WHY:
    Cursor (2026.10.16) reports a finished tool asynchronously, so a command cancelled by Stop arrives as a `postToolUse` (exit code 0) or a `postToolUseFailure` (`is_interrupt: true`) after the `stop` that ended the turn. Mapped to working like any tool event, it re-claimed the session: Cursor sat at its prompt saying "Cancelled" while the chat kept its spinner and Stop button until the next message (agent flow tests, Windows and Mac, 2026-10-17). A finished tool never starts a turn (`beforeSubmitPrompt` and `preToolUse` do), so it changes nothing.
    */
    if agent_key == "cursor" && matches!(compact.as_str(), "posttooluse" | "posttoolusefailure") {
        return None;
    }
    // OpenClaude emits Claude's hook contract verbatim, and Empryo runs Claude-format hooks
    // (CDXC:AgentHooks 2026-10-06 in config.rs), so both share every Claude-specific rule below
    // instead of falling through to the generic tables (which have no PostCompact trigger check
    // and no StopFailure arm).
    if matches!(agent_key, "claude" | "openclaude" | "empryo") {
        /*
        CDXC:Notifications 2026-09-15 DECISION:
        User: suppress completion attention while Claude reports background work remaining; notify when Claude finishes after that work completes, and preserve question/permission alerts.
        This narrows the 2026-09-04 decision to ring on every finished Claude turn: Stop also fires after progress updates while background agents and monitors continue.
        Claude's background_tasks array contains in-flight tasks; an empty or absent array retains ordinary completion attention.
        SEE-ALSO: normalize_agent_hook_event_activity in server/src/agents/activity.rs and background_tasks forwarding in server/src/agent_hooks/notify_runtime.rs.
        */
        if lower == "stop" {
            return Some(
                if payload
                    .get("background_tasks")
                    .and_then(Value::as_array)
                    .is_some_and(|tasks| !tasks.is_empty())
                {
                    "working"
                } else {
                    "attention"
                }
                .to_string(),
            );
        }
        /*
        CDXC:AgentHooks 2026-08-27:
        Claude skips Stop after a model error and emits StopFailure instead, so
        without this arm the pane spins "working" forever on every failed turn.
        */
        if matches!(lower.as_str(), "stopfailure" | "idle" | "sessionend") {
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
    /*
    CDXC:Notifications 2026-10-17 WHY:
    Copilot CLI (1.0.98) fires Stop only when a turn finishes on its own (`stop_reason: end_turn`; a Ctrl+C abort fires none), so its Stop is the same completed-turn boundary as Claude's and enters attention. Its Notification names why it fired: `permission_prompt` and `elicitation_dialog` mean Copilot is waiting on the user (an approval panel or an ask_user form), while `shell_completed` and the rest are information that must not move the session. Copilot's PermissionRequest hook fires for every tool, auto-approved ones included, and carries no event name, so it is never the attention signal.
    */
    if agent_key == "copilot" {
        if lower == "stop" {
            return Some("attention".to_string());
        }
        if lower == "notification" {
            let kind = first_string([
                payload.get("notification_type"),
                payload.get("notificationType"),
            ]);
            return matches!(
                kind.as_deref(),
                Some("permission_prompt" | "elicitation_dialog")
            )
            .then(|| "attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit" | "pretooluse" | "posttooluse" | "posttoolusefailure"
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
        if lower == "stop" && antigravity_stop_awaits_background_work(payload) {
            return Some("working".to_string());
        }
        // agy's question tool waits for the user's answer, as Claude's AskUserQuestion does.
        if lower == "pretooluse"
            && first_string([nested_get(payload, &["toolCall", "name"])]).as_deref()
                == Some("ask_question")
        {
            return Some("attention".to_string());
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
    /*
    CDXC:AgentHooks 2026-09-28 WHY:
    SessionStart (Kiro: agentSpawn) fires while the CLI waits at its input prompt: startup, resume, a new or cleared conversation, and it is the only hook Claude fires when /clear finishes. Most agents settled idle on it only through the notify hook's stateless default status, which also invented idle for every mid-turn event nothing maps, so the rule is explicit now. Two exceptions stay unmapped: OpenCode's TUI plugin reports SessionStart whenever the TUI opens another conversation, which may be mid-turn, and Claude's compaction SessionStart (source compact) also fires mid-turn after an auto-compact, while a manual /compact settles through PostCompact. gxserver's table leaves SessionStart to the status posted here, because the Claude rule needs the payload.
    */
    if compact == "agentspawn"
        || (compact == "sessionstart"
            && agent_key != "opencode"
            && !(matches!(agent_key, "claude" | "openclaude")
                && payload.get("source").and_then(Value::as_str) == Some("compact")))
    {
        return Some("idle".to_string());
    }
    /*
    CDXC:AgentHooks 2026-09-28 WHY:
    Hermes fires post_tool_call after every tool, mid-turn; only post_llm_call and on_session_end end its turn. Left unmapped, the hook posted its stateless default status (idle) and gxserver applied it, so each finished tool dropped the session to idle until the next tool began, and a long execute_code or the final reply showed no dot (observed 2026-09-28 in the Dobby bot, session G9eas). Its clarify tool asks the user and waits in the terminal, like an approval, so that pre_tool_call needs the user.
    */
    if compact == "pretoolcall"
        && first_string([payload.get("tool_name"), payload.get("toolName")])
            .is_some_and(|tool| crate::session_chat_interactive::is_ask_user_question_tool(&tool))
    {
        return Some("attention".to_string());
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
            | "posttoolcall"
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
CDXC:SessionChat 2026-08-24:
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

/// CDXC:AgentHooks 2026-10-23 WHY:
/// Antigravity CLI's hook payloads never name their event (its hook contract: the stdin JSON is the common fields plus the event's own arguments, and the command is given no event name either; CDXC:AgentProviders 2026-09-03 in notify_runtime.rs), so every agy rule in this table, which is keyed by event name, matched nothing: under the Detection status source nothing said agy was working, the chat fell back to Send as soon as the prompt reached agy's step log, and the turn read as finished (GitHub #271). The event is read off the arguments agy documents for each hook: Stop carries `terminationReason`, `fullyIdle`, `executionNum` or `finalModelOutput`; PostToolUse adds `result` or `error` to PreToolUse's `toolCall`; PostInvocation adds `modelOutput` or `modelThinking` to PreInvocation's `invocationNum`. A `hookType` (agy's own `HOOK_TYPE_*` enum) wins when a build sends one. A payload with none of these (SessionStart's common fields) stays unnamed.
pub(crate) fn antigravity_hook_event_name(payload: &Value) -> Option<&'static str> {
    let has = |key: &str| payload.get(key).is_some_and(|value| !value.is_null());
    if let Some(kind) = first_string([payload.get("hookType")]) {
        return match kind.trim_start_matches("HOOK_TYPE_") {
            "PRE_INVOCATION" => Some("PreInvocation"),
            "POST_INVOCATION" => Some("PostInvocation"),
            "PRE_TOOL_USE" => Some("PreToolUse"),
            "POST_TOOL_USE" => Some("PostToolUse"),
            "STOP" => Some("Stop"),
            "SESSION_START" => Some("SessionStart"),
            _ => None,
        };
    }
    if [
        "terminationReason",
        "fullyIdle",
        "executionNum",
        "finalModelOutput",
    ]
    .iter()
    .any(|key| has(key))
    {
        return Some("Stop");
    }
    if has("toolCall") {
        return Some(if has("result") || has("error") {
            "PostToolUse"
        } else {
            "PreToolUse"
        });
    }
    if has("modelOutput") || has("modelThinking") {
        return Some("PostInvocation");
    }
    if has("invocationNum") || has("initialNumSteps") {
        return Some("PreInvocation");
    }
    None
}

/// CDXC:AgentHooks 2026-10-23 WHY:
/// Antigravity CLI moves a command that outlives `WaitMsBeforeAsync` (often 1s) into a background task and tells the model to say it will wait and end the turn; its loop then stops (`terminationReason: NO_TOOL_CALL`) and the CLI wakes the model again when the task finishes, so the turn's real answer comes later (GitHub #271: the chat dropped to Send and "Worked" a few seconds in, and the replies popped up minutes later). The Stop payload says so with `fullyIdle`, but it is a plain proto3 bool that agy's protojson leaves out when false, so only `fullyIdle: true` means the agent is done. Any other stop reason (a cancel, an error, a limit) ends the turn as before, and a payload without a stop reason keeps the old idle.
/// SEE-ALSO: server/src/session_chat_antigravity_mirror.rs keeps the transcript's turn open across the same wait; server/src/agents/activity.rs re-derives this Stop from the forwarded fields.
pub(crate) fn antigravity_stop_awaits_background_work(payload: &Value) -> bool {
    let fully_idle = payload_boolean(payload, &["fullyIdle", "fully_idle"]);
    let reason = first_string([
        payload.get("terminationReason"),
        payload.get("termination_reason"),
    ]);
    fully_idle != Some(true) && reason.as_deref() == Some("NO_TOOL_CALL")
}

pub(crate) fn payload_boolean(payload: &Value, keys: &[&str]) -> Option<bool> {
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

/// CDXC:AgentHooks 2026-09-05 WHY:
/// Mastra uses Pi's TUI components but its own root-level JSON hooks and user_message payload.
/// AgentEnd preserves suspended/aborted/error reasons, while Mastra 0.35's Stop reports suspended runs as complete, so Stop is deliberately not installed.
/// The agent_done Notification duplicates AgentEnd and must not ring a second time; child events must not move the lead session.
/// SEE-ALSO: server/src/agents/activity.rs and server/src/agent_hooks/notify_runtime.rs.
fn mastra_hook_activity(event_name: &str, payload: &Value) -> Option<String> {
    let activity = match event_name.to_ascii_lowercase().as_str() {
        "sessionstart" | "sessionend" | "interrupt" => "idle",
        "userpromptsubmit" | "agentstart" | "pretooluse" | "posttooluse" => "working",
        "permissionrequest" => "attention",
        "permissionresult" => match payload.get("decision").and_then(Value::as_str) {
            Some("dismissed") => "idle",
            Some("approved" | "auto_approved" | "declined") => "working",
            _ => return None,
        },
        "agentend" => match payload.get("stop_reason").and_then(Value::as_str) {
            Some("complete" | "suspended") => "attention",
            Some("aborted" | "error") => "idle",
            _ => return None,
        },
        "notification" if payload.get("reason").and_then(Value::as_str) != Some("agent_done") => {
            "attention"
        }
        _ => return None,
    };
    Some(activity.to_string())
}
