use crate::domain::DomainStateError;

// Read shell words with their original spans so removing a selector preserves quoted arguments.
pub(crate) fn command_word(command: &str, from: usize) -> Option<(usize, usize, String)> {
    let start = from + command.get(from..)?.len() - command.get(from..)?.trim_start().len();
    if start == command.len() {
        return None;
    }
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    for (relative, ch) in command[start..].char_indices() {
        if escaped {
            word.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if quote == Some(ch) {
            quote = None;
            continue;
        }
        if quote.is_none() {
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
                continue;
            }
            if ch.is_whitespace() {
                return Some((start, start + relative, word));
            }
        }
        word.push(ch);
    }
    if quote.is_some() || escaped {
        return None;
    }
    Some((start, command.len(), word))
}

/// CDXC:AgentProviders 2026-09-11 WHY:
/// History launches save a complete resume invocation as agentCommand. Dropping that command after assigning an account discarded cswap's selected login and launched the project's c2 alias instead.
/// Remove only conversation selectors, keeping the account wrapper and the remaining argument spelling; apply this when assigning an account and when reading older saved commands.
pub(crate) fn reusable_account_command(
    command: &str,
    agent: &str,
) -> Result<String, DomainStateError> {
    let mut words = Vec::new();
    let mut offset = 0;
    while !command[offset..].trim().is_empty() {
        let word = command_word(command, offset).ok_or_else(|| {
            DomainStateError::bad_request("The saved agent command has unfinished shell quoting.")
        })?;
        offset = word.1;
        words.push(word);
    }
    let mut removed = Vec::new();
    let mut index = 0;
    let mut codex_selector = false;
    let mut codex_reference_pending = false;
    while index < words.len() {
        let (start, end, word) = &words[index];
        // A quoted instruction containing a flag is an argument, not a selector.
        let literal = &command[*start..*end];
        let selector = (literal == word || literal.starts_with('-'))
            && match agent {
                "claude" => matches!(word.as_str(), "--resume" | "-r" | "--session-id"),
                "codex" => matches!(word.as_str(), "resume" | "fork" | "--resume" | "--fork"),
                _ => false,
            };
        let flag = literal.starts_with('-')
            && match agent {
                "claude" => {
                    matches!(word.as_str(), "--continue" | "-c" | "--fork-session")
                        || word.starts_with("--resume=")
                        || word.starts_with("--session-id=")
                        || word.starts_with("--continue=")
                }
                "codex" => {
                    word.starts_with("--resume=")
                        || word.starts_with("--fork=")
                        || (codex_selector && matches!(word.as_str(), "--last" | "--all"))
                }
                _ => false,
            };
        if selector || flag {
            let mut remove_end = *end;
            if selector {
                codex_selector = agent == "codex";
                codex_reference_pending = codex_selector;
                if let Some((_, end, value)) = words.get(index + 1) {
                    if !value.starts_with('-') {
                        remove_end = *end;
                        codex_reference_pending = false;
                        index += 1;
                    }
                }
            }
            if word == "--last" {
                codex_reference_pending = false;
            }
            removed.push((*start, remove_end));
        } else if option_takes_value(agent, word) {
            // Values such as --append-system-prompt '--resume' and --model resume must remain literal arguments.
            index += 1;
        } else if codex_reference_pending && !word.starts_with('-') {
            removed.push((*start, *end));
            codex_reference_pending = false;
        }
        index += 1;
    }
    let mut result = command.to_string();
    for (start, end) in removed.into_iter().rev() {
        result.replace_range(start..end, "");
    }
    Ok(result.trim().to_string())
}

fn option_takes_value(agent: &str, word: &str) -> bool {
    match agent {
        "claude" => matches!(
            word,
            "--model"
                | "--fallback-model"
                | "--effort"
                | "--agent"
                | "--agents"
                | "--system-prompt"
                | "--system-prompt-file"
                | "--append-system-prompt"
                | "--append-system-prompt-file"
                | "--settings"
                | "--setting-sources"
                | "--mcp-config"
                | "--permission-mode"
                | "--permission-prompt-tool"
                | "--output-format"
                | "--input-format"
                | "--json-schema"
                | "--max-budget-usd"
                | "--max-turns"
                | "--name"
        ),
        "codex" => matches!(
            word,
            "-c" | "--config"
                | "-m"
                | "--model"
                | "-p"
                | "--profile"
                | "-s"
                | "--sandbox"
                | "-a"
                | "--ask-for-approval"
                | "-C"
                | "--cd"
                | "--add-dir"
                | "--enable"
                | "--disable"
                | "-i"
                | "--image"
                | "--local-provider"
        ),
        _ => false,
    }
}
