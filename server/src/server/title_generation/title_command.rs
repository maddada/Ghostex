use super::*;

/// CDXC:SessionTitles 2026-09-15 WHY:
/// History lookup needs the transcript family, including custom Claude profiles, using the same resolver as chat without changing the session identity helper shared by other callers.
pub(crate) fn session_history_title_source(session: &Value) -> Option<String> {
    let agent = crate::session_chat_follower::session_chat_agent_for_session(session)?;
    if !crate::agent_transcripts::agent_supports_session_history_title_source(Some(agent.as_str()))
    {
        return None;
    }
    let prompts = crate::agent_transcripts::recent_session_user_prompts(
        &agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    );
    build_session_history_title_source(&prompts)
}

pub(crate) fn build_session_history_title_source(prompts: &[String]) -> Option<String> {
    let mut recent: Vec<String> = prompts
        .iter()
        .rev()
        .take(GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_COUNT)
        .map(|prompt| {
            js_string_slice_prefix(
                crate::coordinators::strip_agent_message_header(prompt),
                GXSERVER_SESSION_HISTORY_TITLE_SOURCE_MESSAGE_MAX_LENGTH,
            )
            .trim()
            .to_string()
        })
        .collect();
    recent.reverse();
    let joined = recent.join("\n\n");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub(crate) async fn generate_first_prompt_session_title(
    state: &AppState,
    cwd: Option<&str>,
    prompt: &str,
    source_max_length: usize,
    session: &Value,
) -> Result<String, String> {
    let source_text = js_string_slice_prefix(prompt, source_max_length);
    let generation_prompt = build_first_prompt_title_generation_prompt(&source_text);
    let agent = normalize_title_generation_agent(
        read_runtime_text(session, "firstPromptTitleGenerationAgent").as_deref(),
    );
    let command = read_title_generation_command(session, &agent)?;
    // The job names its own account; see `one_shot_command` in accounts/launch.rs.
    let command = crate::accounts::launch::one_shot_command(state, &agent, &command, Some(session));
    // See CDXC:SessionTitles 2026-10-17 on `title_generation_shell_command`.
    let (shell_command, stdin) =
        title_generation_shell_command(&agent, &command, generation_prompt)?;
    let shell = command_shell();
    let mut child = Command::new(&shell.executable);
    crate::platform::process::NoConsoleWindow::no_console_window(&mut child);
    child.args(shell.interactive_script_args(&shell_command));
    child.current_dir(cwd.unwrap_or_else(|| state.paths.home_dir.to_str().unwrap_or(".")));
    child.envs(internal_prompt_generation_environment(
        &state.paths.home_dir,
    ));
    child.stdin(if stdin.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    });
    child.stdout(std::process::Stdio::piped());
    child.stderr(std::process::Stdio::piped());
    child.kill_on_drop(true);
    let mut process = child
        .spawn()
        .map_err(|error| format!("title generation failed: {error}"))?;
    if let (Some(prompt), Some(mut pipe)) = (stdin, process.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        // A CLI that exits before reading its prompt closes the pipe; its exit status says why.
        let _ = pipe.write_all(prompt.as_bytes()).await;
        drop(pipe);
    }
    let output = tokio::time::timeout(
        Duration::from_millis(GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS),
        process.wait_with_output(),
    )
    .await
    .map_err(|_| "title generation timed out".to_string())?
    .map_err(|error| format!("title generation failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "title generation exited {:?}",
            output.status.code()
        ));
    }
    parse_generated_session_title_text(&String::from_utf8_lossy(&output.stdout))
}

/// CDXC:SessionTitles 2026-09-11 WHY:
/// Pi and Antigravity CLI generate titles through their non-interactive print modes (`pi -p`, `agy -p`), verified against the real binaries; before that a machine with only those CLIs fell back to the Codex default, which was not installed (GitHub issue #125).
/// Pi gets `--no-tools --no-context-files --no-session` so a title prompt never runs tools, loads AGENTS.md, or leaves a session behind; Antigravity gets `--disable-slash-commands` so a prompt starting with `/` is not expanded.
/// CDXC:SessionTitles 2026-10-07 WHY:
/// Empryo generates titles through `empryo --headless` (verified against 3.9.1-beta: a one-line title in about ten seconds, read from stdin); `--no-genome` and `--marionette-mode none` skip the repo map and the prompt pre-pass, `--max-steps 1` keeps it from running tools, and it saves no session without `--save-session`. Without it the Rename dialog offered Empryo, remembered it, and every Generate Name failed with "Choose a configured agent that supports name generation" (seen live 2026-10-07). It passes no `--effort`: the title runs on the user's default model, and Empryo rejects an effort that model lacks.
/// SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/agents/model.rs `title_generation_preview`, which must preview the same commands.
pub(crate) fn normalize_title_generation_agent(value: Option<&str>) -> String {
    match value {
        Some("cursor" | "claude" | "grok" | "pi" | "antigravity" | "empryo" | "custom") => {
            value.unwrap().to_string()
        }
        _ => "codex".to_string(),
    }
}

pub(crate) fn read_title_generation_command(
    session: &Value,
    agent: &str,
) -> Result<String, String> {
    if let Some(command) = read_runtime_text(session, "firstPromptTitleGenerationCommand") {
        return Ok(command);
    }
    match agent {
        "codex" => Ok("codex".to_string()),
        "cursor" => Ok("cursor-agent".to_string()),
        "claude" => Ok("claude".to_string()),
        "grok" => Ok("grok".to_string()),
        "pi" => Ok("pi".to_string()),
        "antigravity" => Ok("agy".to_string()),
        "empryo" => Ok("empryo".to_string()),
        "custom" => Err("Custom title generation command is not configured.".to_string()),
        _ => Ok("codex".to_string()),
    }
}

/// CDXC:SessionTitles 2026-10-17 WHY:
/// The title job gave Codex, Claude, Empryo and a custom command their prompt through a POSIX heredoc (`codex exec … <<'DELIM'`), which PowerShell, the Windows command shell, cannot run, so Generate Name and the automatic title failed silently for them on Windows (seen with Codex 0.163 on 2026-10-17). The prompt now goes to the CLI's stdin, as Catch me up does (session_chat_catch_up.rs), and an argument prompt is quoted for the shell that runs it.
fn title_generation_shell_command(
    agent: &str,
    command: &str,
    prompt: String,
) -> Result<(String, Option<String>), String> {
    if let Some(stdin_command) = stdin_title_generation_command(agent, command) {
        return Ok((stdin_command, Some(prompt)));
    }
    argument_title_generation_command(
        agent,
        command,
        &crate::session_chat_catch_up::quote_prompt_argument(&prompt),
    )
    .map(|command| (command, None))
    .ok_or_else(|| format!("Unsupported title generation agent: {agent}"))
}

/// The command for an agent that takes its prompt as an argument (Cursor, Grok, Pi and
/// Antigravity), with `quoted_prompt` already quoted for the shell that runs it.
pub(crate) fn argument_title_generation_command(
    agent: &str,
    command: &str,
    quoted_prompt: &str,
) -> Option<String> {
    Some(match agent {
        "cursor" => format!(
            "{} --print --mode ask --trust --model cursor-grok-4.5-low --output-format text {quoted_prompt}",
            one_shot_text_job_command(command, "cursor")
        ),
        "grok" => format!(
            "{command} --model grok-4.5 --reasoning-effort low --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 --single {quoted_prompt}"
        ),
        "pi" => format!(
            "{command} -p --no-session --no-tools --no-context-files --thinking low {quoted_prompt}"
        ),
        "antigravity" => format!(
            "{command} -p {quoted_prompt} --output-format text --effort low --disable-slash-commands"
        ),
        _ => return None,
    })
}

/// The command for an agent that reads its prompt from stdin (Codex, Claude, Empryo and a custom
/// command), without the prompt; `None` for the agents that take it as an argument.
pub(crate) fn stdin_title_generation_command(agent: &str, command: &str) -> Option<String> {
    Some(match agent {
        "codex" => {
            let command = one_shot_text_job_command(command, "codex");
            format!(
                "{command} exec --sandbox read-only --ephemeral --skip-git-repo-check -m gpt-6-luna -c 'model_reasoning_effort=\"low\"'"
            )
        }
        "claude" => {
            let command = one_shot_text_job_command(command, "claude");
            // CDXC:SessionTitles 2026-10-08 DECISION: User: "I want you to switch from haiku 4.5 to 5.5 for anything we used haiku for in this app (auto title etc)". The explicit id keeps an older CLI, whose `haiku` alias still means 4.5, from titling with 4.5.
            format!("{command} -p --permission-mode dontAsk --model claude-haiku-5-5 --effort low")
        }
        "empryo" => {
            format!("{command} --headless --quiet --no-genome --marionette-mode none --max-steps 1")
        }
        "custom" => command.to_string(),
        _ => return None,
    })
}

pub(crate) fn build_first_prompt_title_generation_prompt(source_text: &str) -> String {
    [
        "Write a concise session title that summarizes the user's text.",
        "Return plain text only.",
        "Rules:",
        "- keep it specific and scannable",
        "- prefer 2 to 4 words when possible",
        &format!(
            "- must be fewer than {} characters",
            GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH + 1
        ),
        "- do not abbreviate with ellipses",
        "- do not use quotes, markdown, or commentary",
        "- do not end with punctuation",
        "- focus on the task, bug, feature, or topic",
        "",
        "User text:",
        source_text,
        "",
        "Output handling:",
        "- Produce only the final session title.",
        "- Do not wrap the result in backticks.",
        "- Print only the final result to stdout.",
    ]
    .join("\n")
}

pub(crate) fn parse_generated_session_title_text(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let normalized = if trimmed.starts_with("```") && trimmed.ends_with("```") {
        trimmed
            .trim_start_matches('`')
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches('`')
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    let Some(line) = normalized.lines().find(|line| !line.trim().is_empty()) else {
        return Err("Title generation returned an empty session title.".to_string());
    };
    let sanitized = line
        .trim()
        .trim_matches(['"', '\'', '`'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '…'])
        .trim()
        .to_string();
    if sanitized.is_empty() {
        return Err("Title generation returned an empty session title.".to_string());
    }
    Ok(clamp_generated_session_title_length(&sanitized))
}

pub(crate) fn clamp_generated_session_title_length(value: &str) -> String {
    if js_string_length(value) <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH {
        return value.to_string();
    }
    let mut candidate = String::new();
    for word in value.split_whitespace() {
        let next = if candidate.is_empty() {
            word.to_string()
        } else {
            format!("{candidate} {word}")
        };
        if js_string_length(&next) > GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH {
            break;
        }
        candidate = next;
    }
    if candidate.is_empty() {
        js_string_slice_prefix(value, GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH)
            .trim()
            .to_string()
    } else {
        candidate
    }
}

/*
CDXC:SessionTitles 2026-06-22-07:21:
TypeScript title caps use JavaScript string length and slice semantics, so Rust must count UTF-16 code units rather than Unicode scalar values for first-prompt source text and generated session titles. Rust strings cannot store lone surrogate halves; when a JS slice would expose one, use the replacement character that Node writes at the UTF-8 boundary.
*/
pub(crate) fn js_string_length(text: &str) -> usize {
    text.encode_utf16().count()
}

pub(crate) fn js_string_slice_prefix(text: &str, max_code_units: usize) -> String {
    let mut output = String::new();
    let mut code_units = 0usize;
    for ch in text.chars() {
        let width = ch.len_utf16();
        if code_units + width > max_code_units {
            if code_units < max_code_units {
                output.push(char::REPLACEMENT_CHARACTER);
            }
            break;
        }
        output.push(ch);
        code_units += width;
        if code_units == max_code_units {
            break;
        }
    }
    output
}

pub(crate) fn internal_prompt_generation_environment(
    home_dir: &std::path::Path,
) -> Vec<(String, String)> {
    /*
    CDXC:SessionTitles 2026-06-24-16:11:
    Background title and commit-message generation must not inherit active
    Ghostex session identity. Clear session-binding variables and mark the
    process as internal so installed agent hooks do not attach generated prompt
    runs to user-restorable terminal sessions.
    */
    let mut environment = std::env::vars().collect::<std::collections::HashMap<_, _>>();
    for key in [
        "ANSI_COLORS_DISABLED",
        "NO_COLOR",
        "NODE_DISABLE_COLORS",
        "GHOSTEX_GLOBAL_SESSION_REF",
        "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
        "GHOSTEX_GXSERVER_BASE_URL",
        "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
        "GHOSTEX_SESSION_ID",
        "GHOSTEX_SESSION_STATE_FILE",
        "GHOSTEX_WORKSPACE_ID",
        "GHOSTEX_WORKSPACE_ROOT",
        "VSMUX_SESSION_ID",
        "VSMUX_SESSION_STATE_FILE",
        "VSMUX_WORKSPACE_ID",
        "VSMUX_WORKSPACE_ROOT",
        "ghostex_SESSION_STATE_FILE",
        "ghostex_WORKSPACE_ID",
        "ghostex_WORKSPACE_ROOT",
    ] {
        environment.remove(key);
    }
    environment.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());
    environment.insert(
        "GHOSTEX_INTERNAL_PROMPT_GENERATION".to_string(),
        "1".to_string(),
    );
    environment.insert(
        "GHOSTEX_INTERNAL_TITLE_GENERATION".to_string(),
        "1".to_string(),
    );
    environment.into_iter().collect()
}

pub(crate) fn quote_shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
