use super::*;

pub(crate) fn decide_first_prompt_auto_title(
    session: &Value,
    prompt: Option<&str>,
    allow_running: bool,
) -> FirstPromptAutoTitleDecision {
    let status = read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus");
    let fork_first_prompt_rearmed = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("forkFirstPromptAutoTitlePending"))
        .and_then(Value::as_bool)
        == Some(true);
    let raw_prompt = prompt.map(crate::coordinators::strip_agent_message_header);
    let normalized_prompt = normalize_first_prompt_title_prompt(prompt);
    let cancelled_prompt = normalize_first_prompt_title_prompt(
        read_runtime_text(session, "gxserverFirstPromptAutoTitleCancelledPrompt").as_deref(),
    )
    .or_else(|| {
        normalize_first_prompt_title_prompt(
            read_runtime_text(session, "firstUserMessage").as_deref(),
        )
    });
    let is_cancelled_retry_prompt = status.as_deref() == Some("cancelled")
        && normalized_prompt.is_some()
        && normalized_prompt != cancelled_prompt;
    if (status.as_deref() == Some("running") && !allow_running)
        || matches!(status.as_deref(), Some("applied" | "failed" | "skipped"))
        || (status.as_deref() == Some("cancelled") && !is_cancelled_retry_prompt)
    {
        return FirstPromptAutoTitleDecision {
            normalized_prompt,
            reason: format!("already-{}", status.unwrap_or_default()),
            should_run: false,
            strategy: None,
        };
    }
    if !fork_first_prompt_rearmed
        && session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("autoTitleFromFirstPrompt"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        return decision(normalized_prompt, "alreadyAutoNamed", false, None);
    }
    let agent_name = first_prompt_agent_name(session);
    let strategy = first_prompt_auto_title_strategy(agent_name.as_deref());
    if strategy.is_none() {
        return decision(normalized_prompt, "unsupportedAgent", false, None);
    }
    let Some(prompt) = normalized_prompt.clone() else {
        return decision(normalized_prompt, "emptyPrompt", false, strategy);
    };
    if is_first_prompt_meta_prompt(&prompt) {
        return decision(Some(prompt), "metaPrompt", false, strategy);
    }
    if is_first_prompt_slash_command(raw_prompt, &prompt) {
        return decision(Some(prompt), "slashCommand", false, strategy);
    }
    if strategy == Some("agentAutoTitle") {
        /*
        These agents persist their own first-turn title in their metadata.
        The metadata sync task adopts that canonical name, so Ghostex must not
        start a second model request or inject `/rename <generated title>`.
        */
        return decision(Some(prompt), "agentAutoTitle", false, strategy);
    }
    let current_title = read_session_text(session, "title");
    // CDXC:SessionTitles 2026-09-03: see the claim gate.
    let is_placeholder_title =
        read_runtime_text(session, "titleSource").as_deref() == Some("placeholder");
    if !fork_first_prompt_rearmed
        && !is_placeholder_title
        && !is_terminal_auto_working_directory_title(session)
        && !is_generic_agent_session_title(agent_name.as_deref(), current_title.as_deref())
    {
        return decision(Some(prompt), "nonGenericCurrentTitle", false, strategy);
    }
    decision(Some(prompt), "eligible", true, strategy)
}

pub(crate) fn decision(
    normalized_prompt: Option<String>,
    reason: &str,
    should_run: bool,
    strategy: Option<&'static str>,
) -> FirstPromptAutoTitleDecision {
    FirstPromptAutoTitleDecision {
        normalized_prompt,
        reason: reason.to_string(),
        should_run,
        strategy,
    }
}

pub(crate) fn first_prompt_agent_name(session: &Value) -> Option<String> {
    read_session_text(session, "agentId").or_else(|| read_runtime_text(session, "agentName"))
}

pub(crate) fn first_prompt_auto_title_strategy(agent_name: Option<&str>) -> Option<&'static str> {
    match normalize_agent_name(agent_name).as_deref() {
        /*
        CDXC:SessionTitles 2026-09-11 DECISION:
        User: disable Claude's first-prompt `/rename` after verifying in Ghostex Web that Claude generates its own title without it.
        Let the normal title sync adopt Claude's name without claiming a job or blocking input; manual rename and Generate Name remain available.
        Extended 2026-11-18 (GitHub #664): a Claude session Claude never names gets Codex's quiet fallback (CDXC:SessionTitles 2026-11-18 in title_fallback.rs); the first-prompt job still never runs for Claude.
        */
        Some("claude") => Some("agentAutoTitle"),
        /*
        CDXC:SessionTitles 2026-10-17 DECISION:
        User chose the "quiet fallback": if Codex hasn't named a session about a minute after its first prompt, Ghostex names it in the background, with no typing blocker and no "Generating title..." state; Codex's own name stays the first choice and a name the user set is never overwritten.
        This supersedes the 2026-09-11 decision, which left the naming to Codex alone, so sessions stayed "Codex Session" when codex-cli 0.163 named none of its threads. The first-prompt job still never runs for Codex; the fallback is `schedule_agent_title_fallback` in title_fallback.rs.
        */
        Some("codex") => Some("agentAutoTitle"),
        // Names every conversation itself about a second after the first
        // prompt and writes it to `annotations/<id>.pbtxt`, the same file its
        // `/rename` rewrites; the metadata sync adopts both.
        Some("antigravity") => Some("agentAutoTitle"),
        // Names its own sessions in its state database, in two stages, and the
        // metadata sync adopts both. Generating a second title here would race
        // that with a worse name.
        Some("hermes-agent") => Some("agentAutoTitle"),
        Some("pi") => Some("generateTitleAndName"),
        // CDXC:SessionTitles 2026-10-17 WHY: omp 18.11 names every session itself after its first prompt (the `title` line of its session file, which the metadata sync adopts), so a Ghostex title typed in as `/rename` only raced it and cost a title-model call.
        Some("omp") => Some("agentAutoTitle"),
        _ => None,
    }
}

pub(crate) fn normalize_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "openai codex" | "codex cli" => Some("codex".to_string()),
        "claude code" => Some("claude".to_string()),
        "cursor cli" | "cursor agent" | "cursor-agent" => Some("cursor".to_string()),
        "hermes" | "hermes agent" | "hermes-agent" => Some("hermes-agent".to_string()),
        "π" => Some("pi".to_string()),
        other => Some(other.to_string()),
    }
}

pub(crate) fn agent_session_title_command(agent_name: Option<&str>, title: &str) -> String {
    match normalize_agent_name(agent_name).as_deref() {
        Some("pi") => format!("/name {title}"),
        // CDXC:SessionTitles 2026-10-17 WHY: omp 18.11 has no `/name`, so the model answered `/name <title>` as a prompt (a lost turn and a stray user message); its `/rename <title>` sets the name as user-set, which also stops OMP renaming the session itself.
        Some("omp") => format!("/rename {title}"),
        Some("hermes-agent") => format!("/title {title}"),
        // Empryo has no `/rename`; `/tab rename <name>` names the tab and the session it saves.
        Some("empryo") => format!("/tab rename {title}"),
        _ => format!("/rename {title}"),
    }
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: `/rename <name>` typed in a Hermes chat is sent as Hermes' own `/title <name>`. Hermes has no `/rename` and answered "Unknown command /rename", while the rename dialog already renames a Hermes session with `/title`.
pub(crate) fn chat_rename_as_agent_title_command(
    agent_name: Option<&str>,
    text: &str,
) -> Option<String> {
    let (command, title) =
        crate::session_chat_local_command::parse_session_chat_local_command(text)?;
    (normalize_agent_name(agent_name).as_deref() == Some("hermes-agent")
        && command.eq_ignore_ascii_case("/rename")
        && !title.is_empty())
    .then(|| agent_session_title_command(agent_name, &title))
}

pub(crate) fn requested_agent_title_command_submission(
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) -> Option<(String, String, String)> {
    if endpoint_path != "/api/requestSessionRename"
        || params
            .get("submitAgentRenameCommand")
            .and_then(Value::as_bool)
            != Some(true)
        || result
            .get("shouldSendAgentRenameCommand")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return None;
    }
    let session = result.get("session")?;
    let project_id = read_session_text(session, "projectId")?;
    let session_id = read_session_text(session, "sessionId")?;
    let title = params.get("title")?.as_str()?.trim();
    if title.is_empty() {
        return None;
    }
    let command = agent_session_title_command(first_prompt_agent_name(session).as_deref(), title);
    Some((project_id, session_id, command))
}

pub(crate) fn is_generic_agent_session_title(
    agent_name: Option<&str>,
    title: Option<&str>,
) -> bool {
    let normalized_title = title
        .map(|value| {
            value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if normalized_title.is_empty() {
        return true;
    }
    let normalized_agent = normalize_agent_name(agent_name);
    let generic = [
        "terminal",
        "terminal session",
        "agent",
        "agent session",
        "antigravity cli",
        "antigravity cli session",
        "claude",
        "claude code",
        "claude session",
        "codex",
        "codex cli",
        "codex session",
        "openai codex",
        "openai codex session",
        "pi",
        "π",
        "pi session",
    ];
    if generic.contains(&normalized_title.as_str()) {
        return true;
    }
    let Some(agent) = normalized_agent else {
        return false;
    };
    normalized_title == agent
        || normalized_title == format!("{agent} session")
        || normalized_title == format!("{agent} agent session")
}

pub(crate) fn normalize_first_prompt_title_prompt(prompt: Option<&str>) -> Option<String> {
    let normalized = crate::coordinators::strip_agent_message_header(prompt?)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let stripped = strip_first_prompt_title_prefixes(normalized);
    let cleaned = stripped
        .trim()
        .trim_end_matches(['.', '?', '!', ':', ';', ','])
        .trim();
    Some(
        if cleaned.is_empty() {
            normalized
        } else {
            cleaned
        }
        .to_string(),
    )
}

/*
CDXC:SessionTitles 2026-06-22-08:12:
First-prompt title eligibility must be decided before Rust claims a background job, using the same prompt-normalization and slash-command rules as TypeScript gxserver. Repeated polite prefixes are stripped only for title generation, while slash-command suppression scans the original prompt by line so short command prompts never enter the title job.
*/
pub(crate) fn strip_first_prompt_title_prefixes(value: &str) -> &str {
    let mut stripped = value;
    loop {
        let lower = stripped.to_lowercase();
        let prefix = [
            "please ",
            "kindly ",
            "hey ",
            "hi ",
            "hello ",
            "can you ",
            "could you ",
            "would you ",
            "will you ",
            "can we ",
            "could we ",
            "would we ",
            "help me ",
            "i need you to ",
            "i need to ",
            "i need ",
            "how do i ",
            "how does ",
            "is there any way to ",
            "is there way to ",
        ]
        .into_iter()
        .find(|prefix| lower.starts_with(prefix));
        let Some(prefix) = prefix else {
            return stripped;
        };
        stripped = &stripped[prefix.len()..];
    }
}

pub(crate) fn is_first_prompt_slash_command(
    raw_prompt: Option<&str>,
    normalized_prompt: &str,
) -> bool {
    if js_string_length(normalized_prompt) > 50 {
        return false;
    }
    let Some(raw_prompt) = raw_prompt else {
        return false;
    };
    raw_prompt
        .split('\n')
        .any(is_first_prompt_slash_command_line)
}

pub(crate) fn is_first_prompt_slash_command_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut consumed_bytes = first.len_utf8();
    for (index, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            consumed_bytes = index + ch.len_utf8();
            continue;
        }
        consumed_bytes = index;
        break;
    }
    let suffix = &rest[consumed_bytes..];
    suffix
        .chars()
        .next()
        .map(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ')' | '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '`'
                )
        })
        .unwrap_or(true)
}

pub(crate) fn is_first_prompt_meta_prompt(prompt: &str) -> bool {
    prompt.starts_with("# AGENTS")
        || prompt.contains("tool_use_id")
        || [
            "<command",
            "<environment_context",
            "<permissions instructions>",
            "<user_instructions>",
            "<INSTRUCTIONS>",
            "<collaboration_mode>",
            "<app-context>",
            "<turn_aborted>",
            "<ide_opened_file>",
            "<local-",
            "[Tool Result]",
            "Caveat:",
        ]
        .iter()
        .any(|prefix| prompt.starts_with(prefix))
}
