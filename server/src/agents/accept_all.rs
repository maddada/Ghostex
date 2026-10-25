use super::*;

pub(crate) const GROK_PERMISSION_MODE_FLAG: &str = "--permission-mode";
pub(crate) const GROK_BYPASS_PERMISSIONS_VALUE: &str = "bypassPermissions";

/// The configured agent command for a one-shot text job (commit message, session title, Catch me
/// up) with every permission-bypass, approval and sandbox flag removed; the job's builder then
/// pins its own read-only mode (Codex `exec --sandbox read-only`, Claude `--permission-mode
/// dontAsk`, Cursor `--mode ask`).
///
/// CDXC:AgentProviders 2026-10-25 WHY:
/// These jobs used to force `--yolo` / `--dangerously-skip-permissions` (and Cursor's title job `--yolo`) whatever the Agent approvals policy said, while their prompt carries text other people write: a staged diff, a session's first message, the last 30 agent messages. A prompt injected there ran tools with the user's full authority and no sandbox (security audit 2026-10-25). The jobs only need text back, and print/exec modes never stop to ask, so they run read-only instead.
pub(crate) fn one_shot_text_job_command(command: &str, agent_id: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let (single_token_alternatives, paired_alternatives): (&[&str], &[&str]) = match agent_id {
        "codex" | "command-code" => (
            &[
                "--yolo",
                "--approve-for-me",
                "--dangerously-bypass-approvals-and-sandbox",
                "--full-auto",
            ],
            &["-a", "--ask-for-approval", "-s", "--sandbox"],
        ),
        "claude" => (
            &[
                "--dangerously-skip-permissions",
                "--allow-dangerously-skip-permissions",
            ],
            &["--permission-mode"],
        ),
        "cursor" => (
            &["--yolo", "--force", "-f", "--auto-review", "--plan"],
            &["--mode"],
        ),
        _ => return trimmed.to_string(),
    };
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if single_token_alternatives.iter().any(|flag| {
            token == *flag
                || token
                    .strip_prefix(flag)
                    .is_some_and(|rest| rest.starts_with('='))
        }) {
            index += 1;
            continue;
        }
        if paired_alternatives.contains(&token) {
            index += if tokens.get(index + 1).is_some() {
                2
            } else {
                1
            };
            continue;
        }
        if paired_alternatives.iter().any(|flag| {
            token
                .strip_prefix(flag)
                .is_some_and(|rest| rest.starts_with('='))
        }) {
            index += 1;
            continue;
        }
        output.push(token);
        index += 1;
    }
    output.join(" ")
}

pub(crate) fn apply_accept_all_spec(
    command: &str,
    agent_id: &str,
    enabled: bool,
    icon: Option<&str>,
    strip_when_disabled: bool,
) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let spec = accept_all_spec(agent_id).or_else(|| {
        icon.and_then(default_agent_icon_to_id)
            .and_then(accept_all_spec)
    });
    let Some(spec) = spec else {
        return trimmed.to_string();
    };
    let without_retired = strip_retired_opencode_accept_all(trimmed);
    let trimmed = without_retired.as_str();
    if let AcceptAllSpec::Environment {
        assignments,
        legacy_aliases,
    } = spec
    {
        let stripped = strip_accept_all_markers(trimmed, &assignments, &legacy_aliases);
        return if enabled {
            format!("{} {}", assignments.join(" "), stripped)
                .trim()
                .to_string()
        } else {
            stripped
        };
    }
    let AcceptAllSpec::Flag { aliases, canonical } = spec else {
        unreachable!();
    };
    if !enabled {
        return if strip_when_disabled {
            strip_accept_all_flags(trimmed, &aliases)
        } else {
            trimmed.to_string()
        };
    }
    let deduped = strip_duplicate_accept_all_flags(trimmed, &aliases);
    if command_includes_accept_all_flag(&deduped, &aliases) {
        deduped
    } else {
        format!("{deduped} {canonical}").trim().to_string()
    }
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// Accept-all for OpenCode used to be `OPENCODE_CONFIG_CONTENT='{"permission":"allow"}'` in front of the command. OpenCode 2's TUI talks to one shared background service, so the variable is ignored, or, when that TUI starts the service, can be inherited by it and allow everything in every OpenCode session on the computer. OpenCode 2's own switch is `--auto`, which only auto-approves in that TUI, so saved commands lose the old variable.
fn strip_retired_opencode_accept_all(command: &str) -> String {
    const RETIRED: &str = "OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}'";
    if !command.contains(RETIRED) {
        return command.to_string();
    }
    command
        .split_whitespace()
        .filter(|token| *token != RETIRED)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) enum AcceptAllSpec {
    Environment {
        assignments: Vec<String>,
        legacy_aliases: Vec<String>,
    },
    Flag {
        aliases: Vec<String>,
        canonical: &'static str,
    },
}

pub(crate) fn accept_all_spec(agent_id: &str) -> Option<AcceptAllSpec> {
    Some(match agent_id {
        "amp" => AcceptAllSpec::Flag {
            aliases: vec!["--dangerously-allow-all".to_string()],
            canonical: "--dangerously-allow-all",
        },
        // OpenClaude is a Claude-shaped fork and takes Claude's bypass flag
        // verbatim, so it shares Claude's spec rather than getting its own.
        "antigravity" | "claude" | "openclaude" => AcceptAllSpec::Flag {
            aliases: vec!["--dangerously-skip-permissions".to_string()],
            canonical: "--dangerously-skip-permissions",
        },
        "codex" | "command-code" => AcceptAllSpec::Flag {
            aliases: vec!["--yolo".to_string()],
            canonical: "--yolo",
        },
        "kimi" => AcceptAllSpec::Flag {
            aliases: vec![
                "--auto-approve".to_string(),
                "--yes".to_string(),
                "-y".to_string(),
                "--yolo".to_string(),
            ],
            canonical: "--yolo",
        },
        "copilot" => AcceptAllSpec::Flag {
            aliases: vec!["--allow-all".to_string(), "--yolo".to_string()],
            canonical: "--yolo",
        },
        "cursor" => AcceptAllSpec::Flag {
            aliases: vec!["--force".to_string(), "--yolo".to_string()],
            canonical: "--yolo",
        },
        "gemini" => AcceptAllSpec::Flag {
            aliases: vec!["-y".to_string(), "--yolo".to_string()],
            canonical: "--yolo",
        },
        "grok" => AcceptAllSpec::Flag {
            aliases: vec!["--always-approve".to_string()],
            canonical: "--always-approve",
        },
        "mastra" => AcceptAllSpec::Environment {
            assignments: vec!["MASTRACODE_YOLO=1".to_string()],
            legacy_aliases: vec![],
        },
        "opencode" => AcceptAllSpec::Flag {
            aliases: vec![
                "--auto".to_string(),
                "--yolo".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ],
            canonical: "--auto",
        },
        _ => return None,
    })
}

pub(crate) fn strip_accept_all_markers(
    command: &str,
    assignments: &[String],
    aliases: &[String],
) -> String {
    command
        .split_whitespace()
        .filter(|token| !assignments.iter().any(|assignment| assignment == token))
        .filter(|token| !is_accept_all_flag_token(token, aliases))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn strip_accept_all_flags(command: &str, aliases: &[String]) -> String {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if is_accept_all_flag_token(token, aliases) {
            index += 1;
            continue;
        }
        if token == GROK_PERMISSION_MODE_FLAG {
            if let Some(value_token) = tokens.get(index + 1) {
                if is_grok_bypass_value_or_assignment_token(value_token) {
                    index += 2;
                    continue;
                }
            }
        }
        if is_grok_permission_mode_equals_token(token) {
            index += 1;
            continue;
        }
        output.push(token);
        index += 1;
    }
    output.join(" ")
}

pub(crate) fn strip_duplicate_accept_all_flags(command: &str, aliases: &[String]) -> String {
    let mut seen = false;
    let mut output = Vec::new();
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if is_accept_all_flag_token(token, aliases) {
            if !seen {
                output.push(token);
                seen = true;
            }
            index += 1;
            continue;
        }
        if token == GROK_PERMISSION_MODE_FLAG
            && tokens.get(index + 1) == Some(&GROK_BYPASS_PERMISSIONS_VALUE)
        {
            if !seen {
                output.push(token);
                output.push(tokens[index + 1]);
                seen = true;
            }
            index += 2;
            continue;
        }
        if is_grok_permission_mode_equals_token(token) {
            if !seen {
                output.push(token);
                seen = true;
            }
            index += 1;
            continue;
        }
        output.push(token);
        index += 1;
    }
    output.join(" ")
}

pub(crate) fn command_includes_accept_all_flag(command: &str, aliases: &[String]) -> bool {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if is_accept_all_flag_token(token, aliases) {
            return true;
        }
        if *token == GROK_PERMISSION_MODE_FLAG
            && tokens.get(index + 1) == Some(&GROK_BYPASS_PERMISSIONS_VALUE)
        {
            return true;
        }
        if is_grok_permission_mode_equals_token(token) {
            return true;
        }
    }
    false
}

pub(crate) fn is_accept_all_flag_token(token: &str, aliases: &[String]) -> bool {
    aliases.iter().any(|alias| {
        token == alias
            || token
                .strip_prefix(alias)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

pub(crate) fn is_grok_permission_mode_equals_token(token: &str) -> bool {
    token
        .strip_prefix(GROK_PERMISSION_MODE_FLAG)
        .and_then(|rest| rest.strip_prefix('='))
        .is_some_and(|value| value == GROK_BYPASS_PERMISSIONS_VALUE)
}

pub(crate) fn is_grok_bypass_value_or_assignment_token(token: &str) -> bool {
    token == GROK_BYPASS_PERMISSIONS_VALUE
        || token
            .strip_prefix(GROK_BYPASS_PERMISSIONS_VALUE)
            .is_some_and(|rest| rest.starts_with('='))
}
