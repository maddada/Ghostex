use super::*;

pub(crate) const GROK_PERMISSION_MODE_FLAG: &str = "--permission-mode";
pub(crate) const GROK_BYPASS_PERMISSIONS_VALUE: &str = "bypassPermissions";
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
        "antigravity" | "claude" => AcceptAllSpec::Flag {
            aliases: vec!["--dangerously-skip-permissions".to_string()],
            canonical: "--dangerously-skip-permissions",
        },
        "codex" => AcceptAllSpec::Flag {
            aliases: vec!["--yolo".to_string()],
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
        "opencode" => AcceptAllSpec::Environment {
            assignments: vec!["OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}'".to_string()],
            legacy_aliases: vec![
                "--dangerously-skip-permissions".to_string(),
                "--yolo".to_string(),
            ],
        },
        _ => return None,
    })
}

pub(crate) fn strip_accept_all_markers(command: &str, assignments: &[String], aliases: &[String]) -> String {
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

