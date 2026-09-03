use serde_json::{Map, Value};

use super::*;

pub(crate) fn build_agent_resume_plan(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Value {
    let input = to_agent_resume_input(project, session, settings);
    let primary_command =
        build_agent_resume_command(&input, ResumeCommandOptions { display: false });
    let display_command = primary_command
        .as_ref()
        .and_then(|_| build_agent_resume_command(&input, ResumeCommandOptions { display: true }))
        .or_else(|| primary_command.clone());
    let fallback_command = build_agent_resume_fallback_command(&input);
    let copy_command = build_agent_resume_copy_command(&input);
    let mut plan = Map::new();
    insert_optional_string(&mut plan, "agentId", input.agent_id.clone());
    insert_optional_string(&mut plan, "baseCommand", input.agent_lookup_command.clone());
    insert_optional_string(&mut plan, "copyCommand", copy_command);
    insert_optional_string(&mut plan, "displayCommand", display_command.clone());
    insert_optional_string(&mut plan, "fallbackCommand", fallback_command.clone());
    insert_optional_string(
        &mut plan,
        "lookupCommand",
        input.agent_lookup_command.clone(),
    );
    insert_optional_string(&mut plan, "primaryCommand", primary_command.clone());
    insert_optional_string(&mut plan, "runtimeCommand", input.agent_command.clone());
    if let Some(command) = primary_command {
        /*
        CDXC:GxserverZmxLifecycle 2026-06-22-06:58:
        Provider startup must feed zmx the same restored-session startup script shape as TypeScript gxserver. Wrap daemon-owned resume commands before they reach attach metadata or `startSessionProvider` so wake/start paths print restore context and keep the command in the initial provider startup text instead of changing zmx lifecycle decisions.

        CDXC:AgentResume 2026-06-22-07:47:
        Resume planning must keep TypeScript's separate primary/display/copy/fallback command roles. Exact Codex restores validate the stored id first, then the startup wrapper can try a trusted-title fallback without making Copy Resume include lookup shell code.
        */
        let startup_text = wrap_restored_terminal_resume_command(
            &command,
            display_command.as_deref().unwrap_or(&command),
            fallback_command.as_deref(),
        );
        plan.insert(
            "startupText".to_string(),
            Value::String(as_atuin_ignored_shell_input(&startup_text)),
        );
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("queueAfterTerminalReady".to_string()),
        );
    } else {
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("none".to_string()),
        );
    }
    Value::Object(plan)
}

pub(crate) fn get_agent_startup_text_for_session(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Option<String> {
    /*
    CDXC:GxserverSessionIO 2026-06-22-06:53:
    zmx attach metadata must preserve TypeScript's startup-text precedence: explicit renderer text, then queued fresh-launch text, then the daemon-owned agent resume plan shaped by current agent settings. This keeps missing-provider reattach/wake metadata from dropping restorable agent commands after the Rust cutover.
    */
    build_agent_resume_plan(project, session, settings)
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

#[derive(Clone)]
pub(crate) struct AgentResumeInput {
    agent_command: Option<String>,
    agent_id: Option<String>,
    agent_lookup_command: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    first_user_message: Option<String>,
    project_path: Option<String>,
    stored_command_candidates: Vec<String>,
    title: Option<String>,
    title_source: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct ResumeCommandOptions {
    display: bool,
}

pub(crate) fn to_agent_resume_input(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> AgentResumeInput {
    let configured_agent_id = read_text_value(session, "agentId");
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    let agent_config = resolve_project_agent_config(
        project,
        configured_agent_id.as_deref().unwrap_or(""),
        Some(&launch_settings),
    );
    /*
    CDXC:AgentResume 2026-08-29:
    A `custom-…` agent id names a sidebar CONFIGURATION; the CLI family it runs
    is declared by its icon — the same contract available_draft_agents,
    session_chat_composer_agent_id, and launch_agent_mismatch read. Resume
    planning must speak the family: with the raw configuration id,
    `restorable_agent_id` matched nothing, `build_agent_resume_plan` emitted no
    startup text, and a custom-agent session whose daemon was gone could never
    be woken, restored, or forked — clicking it only flashed the row in the
    sidebar. The configured command (stored `agentCommand` or the custom
    config's `command`) still wins below, so the family only selects the resume
    grammar, not the binary.
    */
    let agent_id = resume_agent_family_id(configured_agent_id, &agent_config, &launch_settings);
    let stored_agent_command = read_text_from_map(&runtime_settings, "agentCommand");
    let configured_agent_command = read_text_from_map(&agent_config, "command");
    let base_command = stored_agent_command
        .clone()
        .filter(|command| !is_one_time_agent_session_command(agent_id.as_deref(), command))
        .or_else(|| {
            configured_agent_command
                .filter(|command| !is_one_time_agent_session_command(agent_id.as_deref(), command))
        })
        .or_else(|| {
            agent_id
                .as_deref()
                .and_then(default_agent_command)
                .map(str::to_string)
        })
        .or(stored_agent_command);
    let runtime_command = agent_id
        .as_ref()
        .and_then(|agent_id| {
            base_command.as_ref().map(|command| {
                resolve_agent_launch_command(
                    agent_id,
                    command,
                    read_text_from_map(&agent_config, "acceptAllMode")
                        .or_else(|| read_text_from_map(&launch_settings, "acceptAllMode"))
                        .as_deref(),
                    settings
                        .get("agentAcceptAllEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    read_text_from_map(&agent_config, "icon")
                        .or_else(|| read_text_from_map(&launch_settings, "icon"))
                        .as_deref(),
                )
            })
        })
        .or_else(|| base_command.clone());
    AgentResumeInput {
        agent_command: runtime_command,
        agent_id,
        agent_lookup_command: base_command,
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        first_user_message: read_text_from_map(&runtime_settings, "firstUserMessage")
            .or_else(|| read_text_from_map(&launch_settings, "firstUserMessage")),
        project_path: read_text_value(session, "cwd").or_else(|| read_text_value(project, "path")),
        stored_command_candidates: collect_stored_agent_resume_command_candidates(session),
        title: read_text_value(session, "title"),
        title_source: read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .or_else(|| Some("user".to_string())),
    }
}

fn is_one_time_agent_session_command(agent_id: Option<&str>, command: &str) -> bool {
    /*
    CDXC:AgentResume 2026-08-28:
    Find can create a terminal from an exact Codex history launch such as
    `codex --yolo resume <session>`. Once passive identity detection promotes that
    terminal to a Codex session, the one-time launch command remains in
    runtimeSettings.agentCommand. It is not a reusable agent base command: if
    full reload feeds it back into the ordinary resume planner, the planner
    appends another `resume <uuid>` and Codex rejects the duplicate positional
    arguments. Codex `resume` and `fork` are one-time session launch commands,
    not reusable base commands, so use the configured or built-in base command
    instead. Treat the invalid legacy `--resume` and `--fork` spellings the
    same way: they must never survive as the base command and get replayed by
    Full reload or Fork.
    Claude history launches have the same persistence shape. Its canonical
    `--resume` and `--continue` selectors, plus `--fork-session`, are also
    one-time session invocations rather than reusable agent commands.
    Existing affected rows are repaired at read time without mutating their
    saved metadata.
    */
    let tokens = command.split_whitespace();
    match agent_id {
        Some("codex") => tokens.into_iter().any(|token| {
            matches!(token, "resume" | "fork" | "--resume" | "--fork")
                || token.starts_with("--resume=")
                || token.starts_with("--fork=")
        }),
        Some("claude") => tokens.into_iter().any(|token| {
            matches!(token, "--resume" | "--continue" | "--fork-session")
                || token.starts_with("--resume=")
                || token.starts_with("--continue=")
                || token.starts_with("--fork-session=")
        }),
        _ => false,
    }
}

fn build_codex_resume_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} resume {shell_reference}")
}

pub(crate) fn build_codex_fork_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} fork {shell_reference}")
}

fn build_claude_resume_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} --resume {shell_reference}")
}

pub(crate) fn build_claude_fork_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} --resume {shell_reference} --fork-session")
}

pub(crate) fn build_agent_resume_command(
    input: &AgentResumeInput,
    options: ResumeCommandOptions,
) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let agent_lookup_command = input
        .agent_lookup_command
        .as_deref()
        .unwrap_or(agent_command);
    let resume_title = if agent_id == "pi" {
        None
    } else {
        trusted_resume_title_for_input(input)
    };
    let exact_reference = get_exact_agent_session_reference(agent_id, input);
    let codex_exact_reference = (agent_id == "codex")
        .then(|| get_codex_session_reference(input))
        .flatten();
    let codex_reference = if agent_id == "codex" {
        codex_exact_reference
            .clone()
            .or_else(|| resume_title.clone())
    } else {
        None
    };
    let claude_exact_reference = (agent_id == "claude")
        .then(|| get_claude_session_reference(input))
        .flatten();
    let cursor_reference = (agent_id == "cursor")
        .then(|| get_cursor_session_reference(input))
        .flatten();
    let opencode_reference = (agent_id == "opencode")
        .then(|| get_opencode_session_reference(input))
        .flatten();
    let pi_reference = (agent_id == "pi")
        .then(|| get_pi_session_reference(input))
        .flatten();

    match agent_id {
        "amp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} threads continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "antigravity" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --conversation {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codebuddy" | "copilot" | "droid" | "gemini" | "hermes-agent" | "qoder" => exact_reference
            .map(|reference| {
                format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                )
            }),
        "grok" => exact_reference
            .map(|reference| format!("{agent_command} -r {}", quote_shell_double_arg(&reference))),
        "kiro" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume-id {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "omp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codex" => {
            let reference = codex_reference?;
            if options.display {
                return Some(if let Some(exact) = codex_exact_reference {
                    build_codex_resume_invocation(agent_command, &quote_shell_double_arg(&exact))
                } else {
                    format!(
                        "{}  # lookup Codex session id by title",
                        build_codex_resume_invocation(
                            agent_command,
                            &quote_shell_double_arg(&reference)
                        )
                    )
                });
            }
            if let Some(exact) = codex_exact_reference {
                Some(build_codex_validated_resume_command(agent_command, &exact))
            } else {
                Some(build_codex_resume_lookup_command(agent_command, &reference))
            }
        }
        "claude" => {
            if let Some(exact) = claude_exact_reference {
                return Some(build_claude_resume_invocation(
                    agent_command,
                    &quote_shell_double_arg(&exact),
                ));
            }
            let resume_title = resume_title?;
            if options.display {
                Some(format!(
                    "{}  # lookup Claude session id by title",
                    build_claude_resume_invocation(
                        agent_command,
                        &quote_shell_double_arg(&resume_title)
                    )
                ))
            } else {
                Some(build_claude_resume_lookup_command(
                    agent_command,
                    input,
                    &resume_title,
                ))
            }
        }
        "cursor" => {
            if let Some(reference) = cursor_reference {
                return Some(format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                ));
            }
            let resume_title = resume_title?;
            let project_path = input.project_path.as_deref()?;
            if options.display {
                Some(format!(
                    "{agent_command} --resume {}  # lookup chat id in Cursor chat store",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_cursor_resume_lookup_command(
                    agent_command,
                    project_path,
                    &resume_title,
                ))
            }
        }
        "opencode" => {
            if let Some(reference) = opencode_reference {
                return Some(format!(
                    "{agent_command} --session {}",
                    quote_shell_double_arg(&reference)
                ));
            }
            let resume_title = resume_title?;
            if options.display {
                Some(format!(
                    "{agent_command} -s {}  # lookup session id in OpenCode session list",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_opencode_resume_command(
                    agent_command,
                    &resume_title,
                    agent_lookup_command,
                ))
            }
        }
        "pi" => pi_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "rovodev" => {
            exact_reference.map(|reference| build_rovodev_resume_command(agent_command, &reference))
        }
        _ => None,
    }
}

pub(crate) fn build_agent_resume_copy_command(input: &AgentResumeInput) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let exact_reference = get_exact_agent_session_reference(agent_id, input);
    match agent_id {
        "amp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} threads continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "antigravity" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --conversation {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codebuddy" | "copilot" | "droid" | "gemini" | "hermes-agent" | "qoder" => exact_reference
            .map(|reference| {
                format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                )
            }),
        "grok" => exact_reference
            .map(|reference| format!("{agent_command} -r {}", quote_shell_double_arg(&reference))),
        "kiro" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume-id {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "omp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codex" => get_codex_session_reference(input).map(|reference| {
            build_codex_resume_invocation(agent_command, &quote_shell_double_arg(&reference))
        }),
        "claude" => get_claude_session_reference(input).map(|reference| {
            build_claude_resume_invocation(agent_command, &quote_shell_double_arg(&reference))
        }),
        "cursor" => get_cursor_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "opencode" => get_opencode_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "pi" => get_pi_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "rovodev" => {
            exact_reference.map(|reference| build_rovodev_resume_command(agent_command, &reference))
        }
        _ => None,
    }
}

pub(crate) fn build_agent_resume_fallback_command(input: &AgentResumeInput) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let agent_lookup_command = input
        .agent_lookup_command
        .as_deref()
        .unwrap_or(agent_command);
    let resume_title = trusted_resume_title_for_input(input)?;
    match agent_id {
        "codex" => {
            let exact = get_codex_session_reference(input)?;
            (exact != resume_title)
                .then(|| build_codex_resume_lookup_command(agent_command, &resume_title))
        }
        "claude" => {
            let _exact = get_claude_session_reference(input)?;
            Some(build_claude_resume_lookup_command(
                agent_command,
                input,
                &resume_title,
            ))
        }
        "opencode" => {
            let exact = get_opencode_session_reference(input)?;
            (exact != resume_title).then(|| {
                build_opencode_resume_command(agent_command, &resume_title, agent_lookup_command)
            })
        }
        "cursor" => {
            let _exact = get_cursor_session_reference(input)?;
            let project_path = input.project_path.as_deref()?;
            Some(build_cursor_resume_lookup_command(
                agent_command,
                project_path,
                &resume_title,
            ))
        }
        _ => None,
    }
}

/// The CLI-family agent id resume planning keys off. A built-in id passes
/// through; a `custom-…` configuration id resolves to the family its icon
/// declares (custom config icon first, then the session's launch icon), using
/// the same icon-to-id mapping as accept-all resolution. An unresolvable id is
/// returned unchanged so it still fails `restorable_agent_id` explicitly.
pub(crate) fn resume_agent_family_id(
    agent_id: Option<String>,
    agent_config: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
) -> Option<String> {
    let configured = agent_id?;
    if !configured
        .trim()
        .to_ascii_lowercase()
        .starts_with("custom-")
    {
        return Some(configured);
    }
    read_text_from_map(agent_config, "icon")
        .or_else(|| read_text_from_map(launch_settings, "icon"))
        .and_then(|icon| {
            default_agent_icon_to_id(&icon)
                .map(str::to_string)
                .or_else(|| normalize_agent_id(Some(&icon)))
        })
        .or(Some(configured))
}

pub(crate) fn restorable_agent_id(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    match value {
        "amp" | "antigravity" | "campfire" | "claude" | "codebuddy" | "codex" | "command-code"
        | "copilot" | "cursor" | "devin" | "droid" | "gemini" | "grok" | "hermes-agent"
        | "kimi" | "kiro" | "omp" | "openclaude" | "opencode" | "pi" | "qoder" | "rovodev" => {
            Some(value)
        }
        _ => None,
    }
}

pub(crate) fn get_exact_agent_session_reference(
    agent_id: &str,
    input: &AgentResumeInput,
) -> Option<String> {
    match agent_id {
        "codex" => get_codex_session_reference(input),
        "cursor" => get_cursor_session_reference(input),
        "pi" => get_pi_session_reference(input),
        _ => input.agent_session_id.clone(),
    }
}

pub(crate) fn get_codex_session_reference(input: &AgentResumeInput) -> Option<String> {
    let session_id = input.agent_session_id.as_deref()?.trim();
    if session_id.is_empty() {
        return None;
    }
    get_uuid_from_text(session_id).or_else(|| Some(session_id.to_string()))
}

pub(crate) fn get_claude_session_reference(input: &AgentResumeInput) -> Option<String> {
    input
        .agent_session_id
        .clone()
        .or_else(|| get_claude_session_id(input.agent_session_path.as_deref()))
        .or_else(|| get_claude_session_id_from_stored_commands(&input.stored_command_candidates))
}

pub(crate) fn get_opencode_session_reference(input: &AgentResumeInput) -> Option<String> {
    input.agent_session_id.clone()
}

pub(crate) fn get_pi_session_reference(input: &AgentResumeInput) -> Option<String> {
    input
        .agent_session_path
        .clone()
        .or_else(|| input.agent_session_id.clone())
}

pub(crate) fn get_cursor_session_reference(input: &AgentResumeInput) -> Option<String> {
    get_cursor_chat_session_id(input.agent_session_id.as_deref())
        .or_else(|| get_cursor_chat_session_id(input.agent_session_path.as_deref()))
        .or_else(|| {
            get_cursor_chat_session_id_from_stored_commands(&input.stored_command_candidates)
        })
}

pub(crate) fn get_cursor_chat_session_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    if is_uuid(normalized) {
        return Some(normalized.to_ascii_lowercase());
    }
    let normalized_path = normalized.replace('\\', "/");
    let marker = "/agent-transcripts/";
    let index = normalized_path.to_ascii_lowercase().find(marker)?;
    let tail = &normalized_path[index + marker.len()..];
    let segment = tail.split('/').next()?.trim();
    is_uuid(segment).then(|| segment.to_ascii_lowercase())
}

pub(crate) fn get_cursor_chat_session_id_from_stored_commands(
    candidates: &[String],
) -> Option<String> {
    candidates
        .iter()
        .filter_map(|candidate| get_resume_flag_value_from_stored_command(candidate, "--resume"))
        .find_map(|value| get_cursor_chat_session_id(Some(&value)))
}

pub(crate) fn get_claude_session_id_from_stored_commands(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .filter_map(|candidate| get_resume_flag_value_from_stored_command(candidate, "--resume"))
        .find_map(|value| get_claude_session_id(Some(&value)))
}

pub(crate) fn get_claude_session_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    if let Some(uuid) = get_uuid_from_text(normalized) {
        return Some(uuid);
    }
    let cleaned = normalized.trim_end_matches(".jsonl");
    if is_claude_ses_id(cleaned) {
        return Some(cleaned.to_string());
    }
    let normalized_path = normalized.replace('\\', "/");
    normalized_path
        .split('/')
        .filter_map(|part| {
            let candidate = part.trim_end_matches(".jsonl");
            is_claude_ses_id(candidate).then(|| candidate.to_string())
        })
        .next_back()
}

pub(crate) fn is_claude_ses_id(value: &str) -> bool {
    value.strip_prefix("ses_").is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|char| char.is_ascii_alphanumeric())
    })
}

pub(crate) fn get_resume_flag_value_from_stored_command(
    command: &str,
    flag: &str,
) -> Option<String> {
    let bytes = command.as_bytes();
    let flag_bytes = flag.as_bytes();
    let mut index = 0;
    while index + flag_bytes.len() <= bytes.len() {
        if &bytes[index..index + flag_bytes.len()] != flag_bytes {
            index += 1;
            continue;
        }
        if index > 0 && !bytes[index - 1].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let mut cursor = index + flag_bytes.len();
        if bytes.get(cursor) == Some(&b'=') {
            cursor += 1;
        } else if bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
        } else {
            index += 1;
            continue;
        }
        if cursor >= bytes.len() {
            return None;
        }
        let quote = match bytes[cursor] {
            b'\'' | b'"' => {
                cursor += 1;
                Some(bytes[cursor - 1])
            }
            _ => None,
        };
        let start = cursor;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if quote == Some(byte)
                || (quote.is_none()
                    && (byte.is_ascii_whitespace() || matches!(byte, b';' | b'&' | b'|')))
            {
                break;
            }
            cursor += 1;
        }
        let value = command[start..cursor].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
        index = cursor.saturating_add(1);
    }
    None
}

pub(crate) fn collect_stored_agent_resume_command_candidates(session: &Value) -> Vec<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    let launch_plan = launch_settings
        .get("agentLaunchPlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let resume_plan = launch_settings
        .get("agentResumePlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let values = [
        read_text_from_map(&runtime_settings, "agentResumeCommand"),
        read_text_from_map(&runtime_settings, "resumeCommand"),
        read_text_from_map(&runtime_settings, "resumeFallbackCommand"),
        read_text_from_map(&runtime_settings, "copyCommand"),
        read_text_from_map(&runtime_settings, "startupText"),
        read_text_from_map(&launch_settings, "agentResumeCommand"),
        read_text_from_map(&launch_settings, "resumeCommand"),
        read_text_from_map(&launch_settings, "resumeFallbackCommand"),
        read_text_from_map(&launch_settings, "copyCommand"),
        read_text_from_map(&launch_settings, "startupText"),
        read_text_from_map(&launch_plan, "command"),
        read_text_from_map(&launch_plan, "startupText"),
        read_text_from_map(&resume_plan, "primaryCommand"),
        read_text_from_map(&resume_plan, "copyCommand"),
        read_text_from_map(&resume_plan, "displayCommand"),
        read_text_from_map(&resume_plan, "startupText"),
    ];
    let mut output = Vec::new();
    for value in values.into_iter().flatten() {
        if !output.iter().any(|candidate| candidate == &value) {
            output.push(value);
        }
    }
    output
}

pub(crate) fn trusted_resume_title_for_input(input: &AgentResumeInput) -> Option<String> {
    let title = input.title.as_deref()?;
    let title_source = normalize_title_source(input.title_source.as_deref(), title);
    if title_source == "placeholder" {
        return None;
    }
    let visible = get_visible_terminal_title(title)?.trim().to_string();
    (!visible.is_empty() && !is_rejected_resume_title(&visible)).then_some(visible)
}

pub(crate) fn build_rovodev_resume_command(agent_command: &str, session_reference: &str) -> String {
    let quoted = quote_shell_double_arg(session_reference);
    if agent_command
        .split_whitespace()
        .any(|token| token == "rovodev")
    {
        format!("{agent_command} --restore {quoted}")
    } else {
        format!("{agent_command} rovodev run --restore {quoted}")
    }
}

pub(crate) fn build_claude_resume_lookup_command(
    agent_command: &str,
    input: &AgentResumeInput,
    resume_title: &str,
) -> String {
    let args = [
        quote_shell_arg(input.project_path.as_deref().unwrap_or_default()),
        quote_shell_arg(resume_title),
        quote_shell_arg(input.first_user_message.as_deref().unwrap_or_default()),
    ]
    .join(" ");
    let resume_invocation =
        build_claude_resume_invocation(agent_command, "\"$CLAUDE_RESUME_SESSION_ID\"");
    [
        "CLAUDE_RESUME_SESSION_ID=\"$(".to_string(),
        format!("{} claude {args}", build_resume_lookup_command()),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CLAUDE_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        resume_invocation,
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to find restorable Claude session id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

pub(crate) fn build_cursor_resume_lookup_command(
    agent_command: &str,
    project_path: &str,
    resume_title: &str,
) -> String {
    [
        "CURSOR_CHAT_ID=\"$(".to_string(),
        format!(
            "{} cursor {} {}",
            build_resume_lookup_command(),
            quote_shell_arg(project_path),
            quote_shell_arg(resume_title)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CURSOR_CHAT_ID\"".to_string(),
        "&&".to_string(),
        format!("{agent_command} --resume \"$CURSOR_CHAT_ID\""),
        "||".to_string(),
        format!(
            "printf '%s\\n' {}",
            quote_shell_arg(&format!(
                "Unable to find Cursor chat id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

pub(crate) fn build_opencode_resume_command(
    agent_command: &str,
    resume_title: &str,
    lookup_agent_command: &str,
) -> String {
    format!(
        "{agent_command} -s \"$({lookup_agent_command} session list --format json | {} opencode {})\"",
        build_resume_lookup_command(),
        quote_shell_arg(resume_title)
    )
}

pub(crate) fn build_codex_validated_resume_command(
    agent_command: &str,
    session_reference: &str,
) -> String {
    [
        "CODEX_RESUME_SESSION_ID=\"$(".to_string(),
        format!(
            "{} codex --exact {}",
            build_resume_lookup_command(),
            quote_shell_arg(session_reference)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CODEX_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        build_codex_resume_invocation(agent_command, "\"$CODEX_RESUME_SESSION_ID\""),
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to restore Codex session \"{session_reference}\"."
            ))
        ),
    ]
    .join(" ")
}

pub(crate) fn build_codex_resume_lookup_command(agent_command: &str, resume_title: &str) -> String {
    [
        "CODEX_RESUME_SESSION_ID=\"$(".to_string(),
        format!(
            "{} codex --title {}",
            build_resume_lookup_command(),
            quote_shell_arg(resume_title)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CODEX_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        build_codex_resume_invocation(agent_command, "\"$CODEX_RESUME_SESSION_ID\""),
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to find restorable Codex session id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

pub(crate) fn build_resume_lookup_command() -> String {
    /*
    CDXC:RemoteMinimalDeps 2026-07-13:
    Resume lookups used to run as `node -e <script>` against the bundled
    code-server Node, which forced every host (including remote Linux
    packages) to carry a Node runtime for session restore. The lookups are
    now `gxserver resume-lookup <provider> ...` subcommands of this binary,
    resolved the same way agent hooks resolve their notify executable.
    */
    let executable = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "gxserver".to_string());
    format!("{} resume-lookup", quote_shell_arg(&executable))
}

pub(crate) fn get_uuid_from_text(value: &str) -> Option<String> {
    let text = value.as_bytes();
    for start in 0..text.len().saturating_sub(35) {
        let end = start + 36;
        let candidate = &value[start..end];
        if is_uuid(candidate) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}
