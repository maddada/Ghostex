use super::*;

/// CDXC:AgentScreenDetection 2026-10-04 DECISION:
/// User: after the new-thread hotkey the composer must show its details as instantly as possible. What the session's agent command was started with (`--model`, `--effort`, Codex's `-m` and `model_reasoning_effort`, Claude's `--permission-mode` and `--dangerously-skip-permissions`) is known the moment the session exists, so it fills the pills before the agent has painted. It is the weakest evidence and holds only until the agent reports through its transcript or statusline; anything the command does not name stays loading, never guessed from settings files (Claude's own settings disagreed with the effort it actually ran).
/// The launch flags Ghostex adds for OMP (`--thinking`), Copilot and Grok Build (`--reasoning-effort`) and Hermes Agent (`--reasoning`) are read the same way since 2026-11-01.
/// SEE-ALSO: `merge_session_chat_option_selections`, `apply_detected_choice` in packages/gx-chat-core/src/menus/option_values.rs (launch evidence never replaces a pick made in the chat).
pub(crate) fn read_session_chat_launch_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent: Option<SessionChatOptionAgent>,
) -> Option<SessionChatDetectedSelection> {
    let agent = agent?;
    let session = repository.get_session(project_id, session_id).ok()??;
    let command = session
        .pointer("/launchSettings/agentLaunchPlan/command")
        .or_else(|| session.pointer("/runtimeSettings/agentCommand"))
        .and_then(Value::as_str);
    let mut selection = command
        .and_then(|command| launch_command_selection(agent, command))
        .unwrap_or_default();
    with_remembered_launch_choice(agent, &session, project_id, session_id, &mut selection);
    (selection.model.is_some()
        || selection.effort.is_some()
        || selection.mode.is_some()
        || selection.permissions.is_some())
    .then_some(selection)
}

/// What the next launch of this session runs that its command line does not spell out, or runs
/// instead of what it spells out: a resume or wake is launched on the conversation's remembered
/// model and effort (see `with_resume_model_pin`), and Cursor's effort is the one it keeps for the
/// pinned model.
/// SEE-ALSO: CDXC:SessionChat 2026-10-08 and CDXC:AgentProviders 2026-10-07 in agent_model_pins.rs.
fn with_remembered_launch_choice(
    agent: SessionChatOptionAgent,
    session: &Value,
    project_id: &str,
    session_id: &str,
    selection: &mut SessionChatDetectedSelection,
) {
    let Some(family) = pinned_family(agent) else {
        return;
    };
    if let Some(remembered) = remembered_selection(family, session, project_id, session_id) {
        selection.model = remembered.model;
        if remembered.effort.is_some() {
            selection.effort = remembered.effort;
        }
        return;
    }
    if family != "cursor" || selection.effort.is_some() {
        return;
    }
    let model = selection.model.as_ref().map(|choice| choice.value.clone());
    let marker = session.pointer(&format!(
        "/runtimeSettings/{}",
        crate::agent_model_pins::SESSION_PIN_KEY
    ));
    selection.effort = marker
        .filter(|marker| marker.get("model").and_then(Value::as_str) == model.as_deref())
        .and_then(|marker| marker.get("effort"))
        .and_then(Value::as_str)
        .and_then(|effort| launch_effort_choice(family, effort));
}

/// The conversation's remembered model and effort alone. A slept session's transcript and
/// statusline must not outrank them in its pills, because its next wake runs them.
pub(crate) fn read_session_chat_remembered_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent: SessionChatOptionAgent,
) -> Option<SessionChatDetectedSelection> {
    let family = pinned_family(agent)?;
    let session = repository.get_session(project_id, session_id).ok()??;
    remembered_selection(family, &session, project_id, session_id)
}

fn pinned_family(agent: SessionChatOptionAgent) -> Option<&'static str> {
    match agent {
        SessionChatOptionAgent::Claude => Some("claude"),
        SessionChatOptionAgent::Codex => Some("codex"),
        SessionChatOptionAgent::Cursor => Some("cursor"),
        _ => None,
    }
}

fn remembered_selection(
    family: &str,
    session: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatDetectedSelection> {
    let agent_session_id = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str);
    let choice =
        crate::agent_model_pins::session_choice(project_id, session_id, family, agent_session_id)?;
    Some(SessionChatDetectedSelection {
        model: launch_model_choice(family, &choice.model),
        effort: choice
            .effort
            .as_deref()
            .and_then(|effort| launch_effort_choice(family, effort)),
        ..SessionChatDetectedSelection::default()
    })
    .filter(|selection| selection.model.is_some())
}

/// The options one agent command line names, read word by word so a quoted prompt or another
/// option's value that happens to look like a flag is never taken for one.
pub(crate) fn launch_command_selection(
    agent: SessionChatOptionAgent,
    command: &str,
) -> Option<SessionChatDetectedSelection> {
    let family = match agent {
        SessionChatOptionAgent::Claude => "claude",
        SessionChatOptionAgent::Codex => "codex",
        SessionChatOptionAgent::Pi => "pi",
        SessionChatOptionAgent::Cursor => "cursor",
        SessionChatOptionAgent::Antigravity => "antigravity",
        SessionChatOptionAgent::Omp => "omp",
        SessionChatOptionAgent::Copilot => "copilot",
        SessionChatOptionAgent::Grok => "grok",
        SessionChatOptionAgent::Hermes => "hermes-agent",
        _ => return None,
    };
    let mut words = Vec::new();
    let mut offset = 0;
    while !command[offset..].trim().is_empty() {
        let (start, end, word) = crate::agents::command_word(command, offset)?;
        offset = end;
        words.push((command[start..end].to_string(), word));
    }
    let mut selection = SessionChatDetectedSelection::default();
    let mut codex_flags: Vec<(String, Option<String>)> = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let (literal, word) = &words[index];
        index += 1;
        if !crate::agents::is_option_word(literal, word) || !word.starts_with('-') {
            continue;
        }
        let (flag, inline) = match word.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_string())),
            None => (word.as_str(), None),
        };
        let takes_value = inline.is_none() && crate::agents::option_takes_value(family, flag);
        let value = match inline {
            Some(value) => Some(value),
            None if takes_value => {
                let value = words.get(index).map(|(_, value)| value.clone());
                index += 1;
                value
            }
            None => None,
        };
        let value = value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if agent == SessionChatOptionAgent::Codex {
            codex_flags.push((flag.to_string(), value.map(str::to_string)));
        }
        match (agent, flag, value) {
            (_, "--model", Some(model))
            | (
                SessionChatOptionAgent::Codex
                | SessionChatOptionAgent::Grok
                | SessionChatOptionAgent::Hermes,
                "-m",
                Some(model),
            ) => {
                selection.model = launch_model_choice(family, model);
            }
            (
                SessionChatOptionAgent::Claude
                | SessionChatOptionAgent::Antigravity
                | SessionChatOptionAgent::Grok,
                "--effort",
                Some(effort),
            )
            | (
                SessionChatOptionAgent::Pi | SessionChatOptionAgent::Omp,
                "--thinking",
                Some(effort),
            )
            | (
                SessionChatOptionAgent::Copilot | SessionChatOptionAgent::Grok,
                "--reasoning-effort",
                Some(effort),
            )
            | (SessionChatOptionAgent::Hermes, "--reasoning", Some(effort)) => {
                selection.effort = launch_effort_choice(family, effort);
            }
            (SessionChatOptionAgent::Codex, "-c" | "--config", Some(setting)) => {
                if let Some(effort) = setting.strip_prefix("model_reasoning_effort=") {
                    selection.effort =
                        launch_effort_choice(family, effort.trim_matches(['"', '\'']));
                }
            }
            (SessionChatOptionAgent::Claude, "--permission-mode", Some(mode)) => {
                selection.mode = launch_mode_choice(mode);
            }
            (SessionChatOptionAgent::Claude, "--dangerously-skip-permissions", None) => {
                selection.mode = launch_mode_choice("bypassPermissions");
            }
            _ => {}
        }
    }
    if agent == SessionChatOptionAgent::Codex {
        selection.permissions = codex_launch_permissions(&codex_flags);
    }
    (selection.model.is_some()
        || selection.effort.is_some()
        || selection.mode.is_some()
        || selection.permissions.is_some())
    .then_some(selection)
}

fn launch_choice(value: &str, label: &str) -> SessionChatDetectedChoice {
    SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Launch,
    }
}

/// A catalog value keeps the catalog's own label; a Claude model id or alias outside it is named
/// the way a transcript record of the same id would be.
/// The label the agent itself last showed for the value wins over both, so the pill's text does
/// not change when the agent's own report replaces the launch value a few seconds in.
fn launch_model_choice(family: &str, model: &str) -> Option<SessionChatDetectedChoice> {
    let choice = if let Some(entry) = crate::agent_model_catalog::catalog_model(family, model) {
        let label = entry.get("label").and_then(Value::as_str).unwrap_or(model);
        launch_choice(model, label)
    } else {
        match family {
            "claude" => claude_transcript_model_choice(model)
                .map(|choice| launch_choice(&choice.value, &choice.label))?,
            _ => launch_choice(model, model),
        }
    };
    let label = crate::agent_model_pins::remembered_model_label(family, &choice.value)
        .unwrap_or(choice.label);
    Some(launch_choice(&choice.value, &label))
}

fn launch_effort_choice(family: &str, effort: &str) -> Option<SessionChatDetectedChoice> {
    let effort = effort.trim().to_ascii_lowercase();
    (!effort.is_empty() && effort.chars().all(|ch| ch.is_ascii_alphanumeric())).then(|| {
        let label = crate::agent_model_pins::remembered_effort_label(family, &effort)
            .unwrap_or_else(|| effort.clone());
        launch_choice(&effort, &label)
    })
}

/// Claude's `--permission-mode` takes the same names as the transcript's `permissionMode`.
fn launch_mode_choice(mode: &str) -> Option<SessionChatDetectedChoice> {
    claude_transcript_mode_choice(mode).map(|choice| launch_choice(&choice.value, &choice.label))
}
