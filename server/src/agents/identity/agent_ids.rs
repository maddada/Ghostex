use serde_json::Value;

use super::*;

pub(crate) fn normalize_agent_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase().replace('_', " ");
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let mapped = match normalized.as_str() {
        "codex" | "openai codex" | "codex cli" => "codex",
        "claude" | "claude code" => "claude",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "opencode" | "open code" => "opencode",
        "pi" | "π" => "pi",
        "zcode" | "zcode-cli" => "zcode",
        "omp" => "omp",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "amp" | "amp cli" => "amp",
        "copilot" | "github copilot" => "copilot",
        "droid" | "factory" | "factory droid" => "droid",
        "grok" | "grok build" => "grok",
        "kiro" | "kiro cli" | "kiro-cli" => "kiro",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        "codebuddy" | "code buddy" => "codebuddy",
        "qoder" | "qodercli" => "qoder",
        "empryo" | "em" => "empryo",
        "dsh" | "dsh-tui" | "dst" | "deepseek" | "deepseek harness" => "dsh",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        // Keep these folds identical to the agent-hooks resolver's alias set so
        // a hook payload and a sidebar launch resolve to the same agent id.
        "kimi" | "kimi code" | "kimi-code" | "kimicode" => "kimi",
        "openclaude" | "open claude" | "open-claude" | "openclaude cli" => "openclaude",
        "command-code" | "command code" | "commandcode" => "command-code",
        "mastra" | "mastra code" | "mastracode" => "mastra",
        "devin" => "devin",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                char
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

pub(crate) fn normalize_status_agent_name(value: Option<&str>) -> Option<String> {
    let agent = normalize_agent_id(value)?;
    matches!(
        agent.as_str(),
        "antigravity" | "claude" | "codex" | "copilot" | "cursor" | "gemini" | "opencode" | "pi"
    )
    .then_some(agent)
}

pub(crate) fn infer_agent_id_from_path(path: Option<&str>) -> Option<String> {
    let lower = path?.replace('\\', "/").to_ascii_lowercase();
    if lower.ends_with("/.zcode/cli/db/db.sqlite") {
        return Some("zcode".to_string());
    }
    if lower.ends_with("/session.jsonl") && lower.contains("/.empryo/sessions/") {
        return Some("empryo".to_string());
    }
    if lower.ends_with("/chat-messages.json") && lower.contains("/manicode/projects/") {
        return Some("freebuff".to_string());
    }
    if lower.contains("/dsh-session-mirror/") || lower.contains("/.dsh/sessions/") {
        return Some("dsh".to_string());
    }
    if lower.contains("/.cursor/") && (lower.ends_with(".json") || lower.ends_with(".jsonl")) {
        return Some("cursor".to_string());
    }
    if lower.contains("/.claude/") && lower.ends_with(".jsonl") {
        return Some("claude".to_string());
    }
    if lower.contains("/.codex/") || lower.contains("/.codex-profiles/") {
        return Some("codex".to_string());
    }
    if lower.contains("/.opencode/") || lower.contains("/.config/opencode/") {
        return Some("opencode".to_string());
    }
    if lower.contains("/.pi/agent/") {
        return Some("pi".to_string());
    }
    None
}

/*
CDXC:SessionIdentity 2026-06-24-04:49:
Passive hook and sidecar events must not let stale Droid metadata replace a row gxserver already owns as Pi or another agent.
Treat stored agentId/runtime agentName as an identity lock when older rows do not yet have launchAgentId, while still allowing unowned terminal rows to be promoted by first matching observations.
*/
pub(crate) fn locked_session_agent_id(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    normalize_agent_id(
        runtime_settings
            .get("launchAgentId")
            .and_then(Value::as_str),
    )
    .or_else(|| normalize_agent_id(read_text_value(session, "agentId").as_deref()))
    .or_else(|| normalize_agent_id(runtime_settings.get("agentName").and_then(Value::as_str)))
    .or_else(|| {
        launch_settings
            .get("agentLaunchPlan")
            .and_then(Value::as_object)
            .and_then(|plan| {
                plan.get("agentCommand")
                    .and_then(Value::as_str)
                    .or_else(|| plan.get("command").and_then(Value::as_str))
            })
            .and_then(infer_agent_id_from_command)
    })
    .or_else(|| {
        launch_settings
            .get("startupText")
            .and_then(Value::as_str)
            .and_then(infer_agent_id_from_command)
    })
}

pub(crate) fn launch_agent_mismatch(session: &Value, incoming_agent_id: Option<&str>) -> bool {
    let Some(incoming) = normalize_agent_id(incoming_agent_id) else {
        return false;
    };
    locked_session_agent_id(session)
        .map(|locked| {
            locked != incoming
                && session_launch_agent_provider_id(session).as_deref() != Some(incoming.as_str())
        })
        .unwrap_or(false)
}

pub(crate) fn session_launch_agent_provider_id(session: &Value) -> Option<String> {
    normalize_agent_id(
        object_field(session, "launchSettings")
            .get("icon")
            .and_then(Value::as_str),
    )
}

pub(crate) fn align_observed_identity_with_launch_profile(
    session: &Value,
    mut identity: ResolvedIdentity,
) -> ResolvedIdentity {
    let observed_agent_id = normalize_agent_id(identity.agent_id.as_deref());
    if observed_agent_id.is_some() && observed_agent_id == session_launch_agent_provider_id(session)
    {
        /*
        CDXC:SessionIdentity 2026-09-02:
        The substitution maps the observed CLI family onto the sidebar
        CONFIGURATION of that family (`custom-…` built on Claude), which is the
        only case where the locked id is a different spelling of the same
        agent. A locked id that names another canonical agent is not a profile
        of this provider: a live-process scan that had misread a tool child as
        the session's agent stamped it into launchAgentId, and substituting it
        here turned every later Claude observation (scan and hook alike) back
        into that wrong agent, so the row could never recover.
        */
        if let Some(launch_agent_id) = locked_session_agent_id(session).filter(|locked| {
            Some(locked.as_str()) == observed_agent_id.as_deref() || locked.starts_with("custom-")
        }) {
            identity.agent_id = Some(launch_agent_id);
        }
    }
    identity
}

/// Infer the first known CLI from shell words, preserving quoted paths on every host.
/// CDXC:AgentProviders 2026-09-16 WHY:
/// Resume validation must recognize quoted Unix paths and Windows launchers, including account wrappers, or a stale command can evade the family check.
pub(crate) fn infer_agent_id_from_command(command: &str) -> Option<String> {
    let command = command.to_ascii_lowercase();
    let mut quote = None;
    let tokens = command.split(|character: char| {
        if quote == Some(character) {
            quote = None;
        } else if quote.is_none() {
            if matches!(character, '\'' | '"') {
                quote = Some(character);
            } else {
                return character.is_whitespace()
                    || matches!(character, ';' | '&' | '|' | '(' | ')');
            }
        }
        false
    });
    for token in tokens {
        let token = token.trim_matches(['\'', '"']);
        if token.is_empty() || token.contains('=') || token.starts_with('-') {
            continue;
        }
        let basename = token.rsplit(['/', '\\']).next().unwrap_or(token);
        let executable = [".exe", ".cmd", ".bat", ".ps1"]
            .iter()
            .find_map(|suffix| basename.strip_suffix(suffix))
            .unwrap_or(basename);
        let agent = match executable {
            "cursor-agent" => "cursor",
            "hermes" => "hermes-agent",
            "codebuddy" => "codebuddy",
            "agy" => "antigravity",
            "opencode" => "opencode",
            "omp" => "omp",
            "rovodev" => "rovodev",
            "qodercli" => "qoder",
            "empryo" | "em" => "empryo",
            "commandcode" => "command-code",
            "openclaude" => "openclaude",
            "mastracode" => "mastra",
            "devin" => "devin",
            "kimi" => "kimi",
            "kiro-cli" => "kiro",
            "claude" | "cswap" => "claude",
            "copilot" => "copilot",
            "gemini" => "gemini",
            "codex" | "xswap" => "codex",
            "zcode" | "zcode-cli" => "zcode",
            "droid" => "droid",
            "freebuff" => "freebuff",
            "dsh-tui" | "dst" => "dsh",
            "grok" => "grok",
            "amp" => "amp",
            "pi" => "pi",
            _ => continue,
        };
        return Some(agent.to_string());
    }
    None
}

/// The agent CLI a command line starts, read from its executable word only (environment assignments
/// and `env`/`exec`/`command` prefixes skipped), so `git log | grep claude` names no agent.
pub(crate) fn infer_agent_id_from_command_executable(command: &str) -> Option<String> {
    let mut offset = 0;
    while let Some((_, end, word)) = crate::agents::command_word(command, offset) {
        offset = end;
        if word.contains('=') || matches!(word.as_str(), "env" | "exec" | "command") {
            continue;
        }
        return infer_agent_id_from_command(&word);
    }
    None
}

pub(crate) fn is_agent_associated(session: &Value, identity: &ResolvedIdentity) -> bool {
    session.get("kind").and_then(Value::as_str) == Some("agent")
        || session.get("agentId").and_then(Value::as_str).is_some()
        || identity.agent_id.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some()
        || read_text_from_map(&object_field(session, "runtimeSettings"), "agentName").is_some()
}
