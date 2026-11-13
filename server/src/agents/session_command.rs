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

/// Whether a shell word read by [`command_word`] can be an option: a quoted instruction containing a
/// flag is an argument, not an option.
pub(crate) fn is_option_word(literal: &str, word: &str) -> bool {
    literal == word || literal.starts_with('-')
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
        let literal = &command[*start..*end];
        let selector = is_option_word(literal, word)
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

pub(crate) fn option_takes_value(agent: &str, word: &str) -> bool {
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
        "cursor" => matches!(
            word,
            "--model"
                | "--api-key"
                | "-H"
                | "--header"
                | "-e"
                | "--endpoint"
                | "--output-format"
                | "--mode"
                | "--sandbox"
                | "--workspace"
                | "--add-dir"
                | "--plugin-dir"
                | "--worktree-base"
        ),
        "pi" => matches!(
            word,
            "--provider"
                | "--model"
                | "--api-key"
                | "--system-prompt"
                | "--append-system-prompt"
                | "--mode"
                | "--session"
                | "--session-id"
                | "--fork"
                | "--session-dir"
                | "--name"
                | "-n"
                | "--models"
                | "--tools"
                | "-t"
                | "--exclude-tools"
                | "-xt"
                | "--thinking"
                | "--extension"
                | "-e"
                | "--skill"
                | "--prompt-template"
                | "--theme"
                | "--use-theme"
                | "--export"
                | "--tui-mode"
        ),
        "antigravity" => matches!(
            word,
            "--model"
                | "--effort"
                | "--agent"
                | "--add-dir"
                | "--conversation"
                | "--project"
                | "--mode"
                | "--log-file"
                | "--input-format"
                | "--output-format"
                | "--json-schema"
                | "--print-timeout"
                | "-p"
                | "--print"
                | "--prompt"
                | "-i"
                | "--prompt-interactive"
        ),
        // CDXC:AgentProviders 2026-11-01: the value options each CLI's `--help` lists (OMP
        // 18.13.2, Copilot 1.0.103, Grok Build 1.0.70 and Hermes Agent
        // 0.22.5), so a model or effort override never mistakes another option's value
        // for a flag.
        "omp" => matches!(
            word,
            "--model"
                | "--smol"
                | "--slow"
                | "--plan"
                | "--goal"
                | "--prewalk-into"
                | "--plan-yolo-into"
                | "--provider"
                | "--api-key"
                | "--system-prompt"
                | "--system-prompt-template"
                | "--append-system-prompt"
                | "--profile"
                | "--alias"
                | "--cwd"
                | "--mode"
                | "--config"
                | "--add-dir"
                | "-r"
                | "--resume"
                | "--restore"
                | "--session"
                | "--session-dir"
                | "--models"
                | "--tools"
                | "--thinking"
                | "--service-tier"
                | "--hook"
                | "-e"
                | "--extension"
                | "--skills"
                | "--export"
                | "--max-time"
                | "--warn-cost"
                | "--max-cost"
                | "--approval-mode"
        ),
        "copilot" => matches!(
            word,
            "--model"
                | "--reasoning-effort"
                | "-i"
                | "--interactive"
                | "-p"
                | "--prompt"
                | "--context"
                | "--auto-tier"
                | "--agent"
                | "-n"
                | "--name"
                | "--session-id"
                | "--log-dir"
                | "--log-level"
                | "--stream"
                | "--output-format"
                | "--add-dir"
                | "--attachment"
                | "--disable-mcp-server"
                | "--enable-mcp-server"
                | "--add-github-mcp-toolset"
                | "--add-github-mcp-tool"
                | "--plugin-dir"
                | "--additional-mcp-config"
                | "--mode"
                | "--max-autopilot-continues"
                | "--max-ai-credits"
        ),
        "grok" => matches!(
            word,
            "-m" | "--model"
                | "--reasoning-effort"
                | "--effort"
                | "--add-dir"
                | "--agent"
                | "--agents"
                | "--allow"
                | "--deny"
                | "--cwd"
                | "--disallowed-tools"
                | "--json-schema"
                | "--max-turns"
                | "--output-format"
                | "-p"
                | "--single"
                | "--permission-mode"
                | "--prompt-file"
                | "--prompt-json"
                | "--rules"
                | "-s"
                | "--session-id"
                | "--sandbox"
                | "--system-prompt-override"
                | "--tools"
        ),
        "hermes-agent" => matches!(
            word,
            "-m" | "--model"
                | "--provider"
                | "--reasoning"
                | "-z"
                | "--usage-file"
                | "-t"
                | "--toolsets"
                | "-s"
                | "--skills"
                | "--resume"
                | "--in"
        ),
        _ => false,
    }
}

/// CDXC:AgentProviders 2026-09-17 WHY:
/// Codex rejects a repeated `--model`, and a custom agent command may already pin one, so a launch-time choice replaces the existing option instead of appending a second copy.
/// Values of other options are skipped so a quoted instruction such as `--append-system-prompt '--model'` stays untouched.
pub(crate) fn with_agent_model_options(
    command: &str,
    agent: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<String, DomainStateError> {
    validate_model_option_command(command)?;
    let mut words = Vec::new();
    let mut offset = 0;
    while !command[offset..].trim().is_empty() {
        let word = command_word(command, offset).ok_or_else(|| {
            DomainStateError::bad_request("The agent command has unfinished shell quoting.")
        })?;
        offset = word.1;
        words.push(word);
    }
    let codex_effort = |value: &str| value.starts_with("model_reasoning_effort=");
    let mut removed = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let (start, _, word) = &words[index];
        let is_flag = word.starts_with('-');
        let next_value = words.get(index + 1).map(|(_, _, value)| value.as_str());
        let (remove, takes_value) = match (agent, word.as_str()) {
            _ if !is_flag => (false, false),
            (
                "claude" | "codex" | "cursor" | "antigravity" | "copilot" | "grok" | "hermes-agent",
                "--model",
            )
            | ("codex" | "grok" | "hermes-agent", "-m")
                if model.is_some() =>
            {
                (true, true)
            }
            (
                "claude" | "codex" | "cursor" | "antigravity" | "copilot" | "grok" | "hermes-agent",
                value,
            ) if model.is_some() && value.starts_with("--model=") => (true, false),
            ("claude" | "antigravity" | "grok", "--effort")
            | ("copilot" | "grok", "--reasoning-effort")
            | ("hermes-agent", "--reasoning")
                if effort.is_some() =>
            {
                (true, true)
            }
            ("claude" | "antigravity" | "grok", value)
                if effort.is_some() && value.starts_with("--effort=") =>
            {
                (true, false)
            }
            ("copilot" | "grok", value)
                if effort.is_some() && value.starts_with("--reasoning-effort=") =>
            {
                (true, false)
            }
            ("hermes-agent", value) if effort.is_some() && value.starts_with("--reasoning=") => {
                (true, false)
            }
            // Ghostex's Pi model is `provider/id`, so a pinned `--provider` goes with the old model.
            // OMP is Pi's fork: its `--model` takes `provider/id` too and `--provider` is legacy.
            ("pi" | "omp", "--model" | "--provider") if model.is_some() => (true, true),
            ("pi" | "omp", value)
                if model.is_some()
                    && (value.starts_with("--model=") || value.starts_with("--provider=")) =>
            {
                (true, false)
            }
            ("pi" | "omp", "--thinking") if effort.is_some() => (true, true),
            ("pi" | "omp", value) if effort.is_some() && value.starts_with("--thinking=") => {
                (true, false)
            }
            ("codex", "-c" | "--config")
                if effort.is_some() && next_value.is_some_and(codex_effort) =>
            {
                (true, true)
            }
            ("codex", value)
                if effort.is_some()
                    && value.strip_prefix("--config=").is_some_and(codex_effort) =>
            {
                (true, false)
            }
            (_, value) => (false, option_takes_value(agent, value)),
        };
        if remove
            && takes_value
            && words
                .get(index + 1)
                .is_none_or(|(start, end, _)| command[*start..*end].starts_with('-'))
        {
            return Err(DomainStateError::bad_request(format!(
                "The agent command option {word} needs a value before a model or effort override can be applied."
            )));
        }
        let last = if takes_value && index + 1 < words.len() {
            index + 1
        } else {
            index
        };
        if remove {
            removed.push((command[..*start].trim_end().len(), words[last].1));
        }
        index = last + 1;
    }
    let mut result = command.to_string();
    for (start, end) in removed.into_iter().rev() {
        result.replace_range(start..end, "");
    }
    let mut result = result.trim().to_string();
    if let Some(model) = model {
        result.push_str(&format!(" --model {}", shell_word(model)));
    }
    if let Some(effort) = effort {
        match agent {
            "codex" => result.push_str(&format!(
                " -c {}",
                shell_word(&format!("model_reasoning_effort={effort}"))
            )),
            "pi" | "omp" => result.push_str(&format!(" --thinking {}", shell_word(effort))),
            "copilot" | "grok" => {
                result.push_str(&format!(" --reasoning-effort {}", shell_word(effort)))
            }
            "hermes-agent" => result.push_str(&format!(" --reasoning {}", shell_word(effort))),
            // Cursor takes no effort flag; the launch plan refuses an effort for it before here.
            "cursor" => {}
            _ => result.push_str(&format!(" --effort {}", shell_word(effort))),
        }
    }
    Ok(result)
}

/// Whether the command already chooses a model (`--model`, Codex's `-m`), which a remembered pin
/// must not replace: it is the session's own choice.
pub(crate) fn command_names_model(command: &str, agent: &str) -> bool {
    let mut offset = 0;
    let mut skip_value = false;
    while let Some((start, end, word)) = command_word(command, offset) {
        offset = end;
        if std::mem::take(&mut skip_value) || !is_option_word(&command[start..end], &word) {
            continue;
        }
        // A Codex profile can name its own model.
        let codex_choice = agent == "codex"
            && (matches!(word.as_str(), "-m" | "-p" | "--profile")
                || word.starts_with("--profile="));
        if word == "--model" || word.starts_with("--model=") || codex_choice {
            return true;
        }
        skip_value = !word.contains('=') && option_takes_value(agent, &word);
    }
    false
}

/// Model ids such as `opus[1m]` carry shell glob characters, so only plain words stay unquoted.
fn shell_word(value: &str) -> String {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-._:/=".contains(&byte))
    {
        value.to_string()
    } else {
        super::quote_shell_arg(value)
    }
}

/// CDXC:AgentProviders 2026-09-18 WHY:
/// Appending selectors to a shell list can pass them to a later command, and a trailing comment can swallow them entirely. Only rewrite a single invocation; quoted or escaped prompt text remains literal.
fn validate_model_option_command(command: &str) -> Result<(), DomainStateError> {
    let unsupported_command = || {
        DomainStateError::bad_request(
            "Model and effort overrides require a single agent command without shell operators, command substitutions, comments, or line continuations.",
        )
    };
    // CDXC:AgentProviders 2026-10-08 WHY: a Windows account session's command starts with PowerShell's call operator (`& 'C:\…\cswap.exe' run '3' --share-history -- …`, see accounts/launch.rs), which is not a shell operator. Refusing it left every chat pick off those sessions' saved command (logged as `sessionChatModelResumeCommandUnchanged`) and their resume back on the launch model.
    let command = command.trim();
    let command = command
        .strip_prefix("& ")
        .filter(|_| cfg!(windows))
        .unwrap_or(command);
    let mut quote = None;
    let mut escaped = false;
    let mut word_start = true;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        if escaped {
            if matches!(ch, '\n' | '\r') {
                return Err(unsupported_command());
            }
            escaped = false;
            continue;
        }
        if quote == Some('\'') {
            if ch == '\'' {
                quote = None;
            }
            continue;
        }
        if ch == '\\' {
            escaped = true;
            word_start = false;
            continue;
        }
        let shell_expansion = ch == '`' || (ch == '$' && chars.peek() == Some(&'('));
        let shell_boundary = quote.is_none()
            && (matches!(ch, ';' | '&' | '|' | '<' | '>' | '(' | ')' | '\n' | '\r')
                || (ch == '#' && word_start));
        if shell_expansion || shell_boundary {
            return Err(unsupported_command());
        }
        if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
        }
        word_start = quote.is_none() && ch.is_whitespace();
    }
    Ok(())
}
