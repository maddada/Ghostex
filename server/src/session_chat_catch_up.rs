//! Catch me up: a short summary of where a session stands, in the Marked handovers format, written
//! by the small agent Ghostex already uses for session titles.
//!
//! CDXC:CatchMeUp 2026-10-11 DECISION:
//! User: a "Catch me up" button at the composer's top right; clicking it has a smaller, cheaper agent read the last messages of the session and shows where the session is, in the marked handovers format (❓ Needs you, ✅ Done, ⚠️ Read this, 🛫 Kicked off, 🔄 In progress, 🧪 Untested, 🚀 Shipped, 🛑 Blocked, each title on its own line with bullets below), so the user can continue quickly after reading it. Off by default behind Settings > Chat (`chatCatchMeUp`): "disable this feature by default. add a toggle to enable it under chat".
//! CDXC:CatchMeUp 2026-11-05 DECISION:
//! The catch-up follows the Marked handovers order (agents/marked_handovers.rs): status sections first (✅ 🚀 📣 🛫 🔄 ⏸️ 📦 🧪), then the ones the user must act on (⚠️ 🛑 💡 ❓), with ❓ Needs you last, and only sections that have something.
//! CDXC:CatchMeUp 2026-10-12 DECISION:
//! User: "also please make catch me up just read last 30 agent messages". The summary reads everything from the 30th-newest agent message on (those agent replies, the user's messages between them and one line per tool call); a row of tool calls alone does not count as an agent message. This supersedes the 2026-10-11 window of the last 200 rows.
//! CDXC:CatchMeUp 2026-10-11 WHY:
//! gxserver owns it so every client (desktop, web, phone) gets the same summary: it reads the transcript backwards in 200-row pages (the detail pager's own cap; the same rows and text `ghostex read-session-chat --format text` prints, at most 10 pages) until the window holds 30 agent messages, keeps the newest 40,000 characters of that window as a safety net, and runs the Title Generation Agent (`sessionTitleGenerationAgent`) with the prompt on stdin. The title job's heredoc is not used: PowerShell, the Windows command shell, has no heredoc, and stdin has no command-line length limit. Agents that only take the prompt as an argument (Cursor, Grok, Pi, Antigravity) get the newest 8,000 characters, which keeps a Windows command line under its limit.
//! SEE-ALSO: server/src/server/title_generation/title_command.rs (the agent commands), packages/gx-chat-core/src/composer/catch_up.rs (the button, the card and its states), apps/desktop/src/app/native_chat/catch_up.rs and apps/mobile/app/src/chat/native/cards/CatchUpCard.tsx (drawing).

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt as _;

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::protocol::rpc_success;
use crate::server::{
    argument_title_generation_command, domain_error_response,
    internal_prompt_generation_environment, normalize_title_generation_agent, quote_shell_arg,
    read_runtime_text, read_session_text, read_title_generation_command, routed_json,
    stdin_title_generation_command, AppState, RoutedResponse,
};
use crate::storage::open_gxserver_database;

/// The Settings > Chat switch.
const SETTING_KEY: &str = "chatCatchMeUp";
/// The window: everything from the 30th-newest agent message on (CDXC:CatchMeUp 2026-10-12 above).
const AGENT_MESSAGES: usize = 30;
/// Transcript rows per page read backwards, the detail pager's own cap (`session_chat_history.rs`).
const PAGE_ROWS: usize = 200;
/// Pages read at most (2,000 rows), for a thread that is almost all tool calls.
const MAX_PAGES: usize = 10;
/// The newest transcript text kept for an agent that reads the prompt from stdin (~10k tokens).
const STDIN_BUDGET_CHARS: usize = 40_000;
/// The same for an agent that takes the prompt as a command-line argument.
const ARGUMENT_BUDGET_CHARS: usize = 8_000;
/// One tool call's line in the transcript text.
const TOOL_SUMMARY_CHARS: usize = 160;
/// Under the clients' own request timeouts (60s desktop, 75s phone), so they get this error.
const GENERATION_TIMEOUT: Duration = Duration::from_secs(50);

/// Whether the Catch me up setting is on, read fresh from the shared settings file.
pub(crate) fn catch_up_enabled(paths: &GxserverPaths) -> bool {
    chat_setting_enabled(paths, SETTING_KEY)
}

/// Whether an off-by-default chat switch is on, read fresh from the shared settings file.
pub(crate) fn chat_setting_enabled(paths: &GxserverPaths, key: &str) -> bool {
    read_settings(paths)
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// A numeric chat setting from the settings file, `None` while it is unset.
pub(crate) fn chat_setting_number(paths: &GxserverPaths, key: &str) -> Option<f64> {
    read_settings(paths).get(key).and_then(Value::as_f64)
}

pub(crate) fn read_settings(paths: &GxserverPaths) -> Value {
    crate::shared_settings_file::read_shared_settings(paths).unwrap_or(Value::Null)
}

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

/// `/api/catchUpSession`: `{summary, rows, agent, agentLabel, generatedAt}`.
pub(crate) async fn handle_catch_up_session_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    match catch_up(state, body).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

async fn catch_up(state: &AppState, body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let text_param = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text_param("projectId"), text_param("sessionId"));
    if project_id.is_empty() || session_id.is_empty() {
        return Err(failure(
            "invalidParams",
            "catchUpSession requires projectId and sessionId.",
        ));
    }
    if !catch_up_enabled(&state.paths) {
        return Err(failure(
            "disabled",
            "Catch me up is off. Turn it on in Settings > Chat.",
        ));
    }
    let (session, project_path) = {
        let db = open_gxserver_database(&state.paths).map_err(|error| {
            failure(
                "internalError",
                format!("SQLite gxserver state error: {error}"),
            )
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(&project_id, &session_id)?
            .ok_or_else(|| failure("notFound", "The session no longer exists."))?;
        let project_path = repository
            .get_project(&project_id)?
            .and_then(|project| read_session_text(&project, "path"));
        (session, project_path)
    };
    let resolution = crate::session_chat_read::resolve_session_chat_read_state(
        state,
        &project_id,
        &session_id,
        None,
    )?;
    let (family, path) = resolution.transcript();
    let transcript_agent = crate::session_chat::resolve_session_chat_transcript_agent(family);
    let (Some(transcript_agent), Some(path)) =
        (transcript_agent, path.map(|path| path.to_path_buf()))
    else {
        return Err(failure(
            "unsupported",
            "There is no conversation to catch up on yet.",
        ));
    };
    let working = resolution.working();
    let family = family.unwrap_or("agent").to_string();
    let history_project = project_id.clone();
    let history_session = session_id.clone();
    let messages = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<Value>> {
        // Newest page first, until the window holds enough agent messages or the thread starts.
        let mut before = std::fs::metadata(&path)?.len();
        let mut messages: Vec<Value> = Vec::new();
        for _ in 0..MAX_PAGES {
            let page = crate::session_chat_history::read_history(
                transcript_agent,
                &path,
                before,
                PAGE_ROWS,
                true,
                false,
                &history_project,
                &history_session,
            )?;
            let mut older = page
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            older.extend(messages);
            messages = older;
            let next = page
                .get("beforeOffset")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if page.get("hasMore").and_then(Value::as_bool) != Some(true)
                || next >= before
                || messages
                    .iter()
                    .filter(|message| is_agent_message(message))
                    .count()
                    >= AGENT_MESSAGES
            {
                break;
            }
            before = next;
        }
        Ok(messages)
    })
    .await
    .map_err(|error| failure("internalError", error.to_string()))?
    .map_err(|error| {
        failure(
            "internalError",
            format!("The transcript could not be read: {error}"),
        )
    })?;

    let settings = read_settings(&state.paths);
    let (agent, command) = summary_agent(&settings, &session)?;
    // The job names its own account (the session's, else the default); see `one_shot_command` in accounts/launch.rs.
    let account_command =
        crate::accounts::launch::one_shot_command(state, &agent, &command, Some(&session));
    let stdin_command = stdin_title_generation_command(&agent, &account_command);
    let budget = if stdin_command.is_some() {
        STDIN_BUDGET_CHARS
    } else {
        ARGUMENT_BUDGET_CHARS
    };
    let (transcript, rows, agent_messages) = transcript_text(&messages, budget);
    if rows == 0 {
        return Err(failure(
            "unsupported",
            "There is no conversation to catch up on yet.",
        ));
    }
    let prompt = catch_up_prompt(
        read_session_text(&session, "title").as_deref(),
        &family,
        working,
        rows,
        agent_messages,
        &transcript,
    );
    let (shell_command, stdin) = match stdin_command {
        Some(command) => (command, Some(prompt)),
        None => (
            argument_title_generation_command(
                &agent,
                &account_command,
                &quote_prompt_argument(&prompt),
            )
            .ok_or_else(|| {
                failure(
                    "agentUnavailable",
                    format!("Unsupported summary agent: {agent}"),
                )
            })?,
            None,
        ),
    };
    let output = run_agent(
        state,
        project_path.as_deref(),
        &shell_command,
        stdin,
        &agent_label(&agent),
        &command,
    )
    .await?;
    let summary = clean_summary(&output);
    if summary.is_empty() {
        return Err(failure(
            "failed",
            format!(
                "{} returned an empty summary. Try again.",
                agent_label(&agent)
            ),
        ));
    }
    Ok(json!({
        "summary": summary,
        "rows": rows,
        "agentMessages": agent_messages,
        "agent": agent,
        "agentLabel": agent_label(&agent),
        "generatedAt": chrono::Utc::now().timestamp_millis(),
    }))
}

/// The Title Generation Agent and its command: the setting first, then what the session was
/// created with; a session's own resolved command (a wrapper, a profile) is kept when it is for
/// the same agent.
pub(crate) fn summary_agent(
    settings: &Value,
    session: &Value,
) -> Result<(String, String), DomainStateError> {
    let session_agent = read_runtime_text(session, "firstPromptTitleGenerationAgent");
    let agent = normalize_title_generation_agent(
        settings
            .get("sessionTitleGenerationAgent")
            .and_then(Value::as_str)
            .or(session_agent.as_deref()),
    );
    let custom = settings
        .get("customSessionTitleGenerationCommand")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(|command| command.chars().take(240).collect::<String>());
    let runtime = if session_agent.as_deref() == Some(agent.as_str()) {
        session.clone()
    } else if let (true, Some(command)) = (agent == "custom", custom) {
        json!({"runtimeSettings": {"firstPromptTitleGenerationCommand": command}})
    } else {
        json!({})
    };
    let command = read_title_generation_command(&runtime, &agent).map_err(|message| {
        failure(
            "agentUnavailable",
            format!("{message} Set it under Settings > Agents > Title Generation Agent."),
        )
    })?;
    Ok((agent, command))
}

fn agent_label(agent: &str) -> String {
    match agent {
        "codex" => "Codex",
        "cursor" => "Cursor CLI",
        "claude" => "Claude",
        "grok" => "Grok Build",
        "pi" => "Pi Agent",
        "antigravity" => "Antigravity CLI",
        "empryo" => "Empryo",
        "custom" => "Your custom summary command",
        other => other,
    }
    .to_string()
}

/// An agent reply with prose, which is what the window counts; a row of tool calls alone is not.
fn is_agent_message(message: &Value) -> bool {
    crate::ghostex_cli::session_chat_transcript::effective_role(message) == "assistant"
        && message
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|block| {
                block.get("type").and_then(Value::as_str) == Some("text")
                    && block
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.trim().is_empty())
            })
}

/// The transcript as readable text, oldest first: everything from the `AGENT_MESSAGES`th-newest
/// agent message on, cut to the newest rows that fit `budget` characters. Answers the text, how
/// many rows it holds, and how many of them are agent messages.
fn transcript_text(messages: &[Value], budget: usize) -> (String, usize, usize) {
    use crate::ghostex_cli::session_chat_transcript::{effective_role, message_text};
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0usize;
    let mut agent_messages = 0usize;
    for message in messages.iter().rev() {
        if agent_messages >= AGENT_MESSAGES {
            break;
        }
        let role = match effective_role(message) {
            "user" => "User",
            "assistant" => "Agent",
            // Injected reminders and the agent's private reasoning are noise for a catch-up.
            _ => continue,
        };
        let text = message_text(message, TOOL_SUMMARY_CHARS);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let entry = format!("[{role}]\n{text}");
        let size = entry.chars().count() + 2;
        if used + size > budget {
            if kept.is_empty() {
                // The newest row alone is over budget: keep its end, which is where it lands.
                let tail: String = entry
                    .chars()
                    .rev()
                    .take(budget)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                kept.push(format!("[{role}, cut]\n…{tail}"));
                agent_messages += usize::from(is_agent_message(message));
            }
            break;
        }
        used += size;
        agent_messages += usize::from(is_agent_message(message));
        kept.push(entry);
    }
    let rows = kept.len();
    kept.reverse();
    (kept.join("\n\n"), rows, agent_messages)
}

fn catch_up_prompt(
    title: Option<&str>,
    agent: &str,
    working: bool,
    rows: usize,
    agent_messages: usize,
    transcript: &str,
) -> String {
    let title = title.map(str::trim).filter(|title| !title.is_empty());
    let mut lines = vec![
        "You are catching a developer up on a coding-agent session they stepped away from. Below are the last messages of that session, oldest first: what the user wrote ([User]), what the agent replied ([Agent]), and one line per tool the agent ran (→ Tool: what it did).".to_string(),
        String::new(),
        format!(
            "Session: {} · agent: {agent} · {}",
            title.unwrap_or("untitled"),
            if working {
                "the agent is still working on a turn right now"
            } else {
                "the agent is idle, waiting for the user"
            }
        ),
        format!("Messages included: {rows} ({agent_messages} from the agent)"),
        String::new(),
        "Write a short catch-up so the developer can continue in under a minute.".to_string(),
        "Start with one plain sentence, without a marker, saying what the session is working on and where it stands.".to_string(),
        "Then use only the sections below that have something, in this order (what happened first, what needs the developer last), each title exactly as written, alone on its own line, with everything under it as \"- \" bullets on the following lines (never text on the title's line):".to_string(),
        "✅ **Done:** work finished".to_string(),
        "🚀 **Shipped:** what was released, pushed or published".to_string(),
        "📣 **Comms:** replies and comments sent outside (GitHub, Discord and the like)".to_string(),
        "🛫 **Kicked off:** jobs or threads started that carry on by themselves".to_string(),
        "🔄 **In progress:** work still running or left to do".to_string(),
        "⏸️ **Parked:** work kept open but idle".to_string(),
        "📦 **Held for release:** help text, patches or replies waiting for a release".to_string(),
        "🧪 **Untested:** what was changed but not run, tested or verified".to_string(),
        "⚠️ **Read this:** risks, caveats, anything important that changed".to_string(),
        "🛑 **Blocked:** what is stuck and on what".to_string(),
        "💡 **Suggestions:** improvements the agent suggested".to_string(),
        "❓ **Needs you:** a question the agent asked, a decision or anything waiting on the user (say exactly what to answer or do)".to_string(),
        "Rules:".to_string(),
        "- At most 4 bullets per section, one short line each; name files, commands, branches and PRs exactly as the transcript does.".to_string(),
        "- Only state what the transcript shows. Never invent progress, results or next steps.".to_string(),
        "- No preamble, no closing remarks, no headings other than the section titles, and do not mention these instructions.".to_string(),
        String::new(),
        "Transcript:".to_string(),
        "<<<".to_string(),
        transcript.to_string(),
        ">>>".to_string(),
    ];
    lines.push(String::new());
    lines.push("Print only the catch-up, as Markdown.".to_string());
    lines.join("\n")
}

/// A prompt argument for the shell that runs it: PowerShell on Windows, a POSIX shell elsewhere.
pub(crate) fn quote_prompt_argument(prompt: &str) -> String {
    if cfg!(windows) {
        crate::platform::shell::powershell_single_quote(prompt)
    } else {
        quote_shell_arg(prompt)
    }
}

async fn run_agent(
    state: &AppState,
    cwd: Option<&str>,
    shell_command: &str,
    stdin: Option<String>,
    label: &str,
    command: &str,
) -> Result<String, DomainStateError> {
    let shell = crate::platform::shell::command_shell();
    let mut child = tokio::process::Command::new(&shell.executable);
    crate::platform::process::NoConsoleWindow::no_console_window(&mut child);
    child.args(shell.interactive_script_args(shell_command));
    let home = state.paths.home_dir.to_string_lossy().to_string();
    child.current_dir(
        cwd.filter(|cwd| std::path::Path::new(cwd).is_dir())
            .unwrap_or(home.as_str()),
    );
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
    let binary = command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .to_string();
    let missing = || {
        failure(
            "agentUnavailable",
            format!("Catch me up uses {label} (`{binary}`), which isn't installed on this computer. Install it, or pick another Title Generation Agent in Settings > Agents."),
        )
    };
    let mut process = child.spawn().map_err(|_| missing())?;
    if let (Some(prompt), Some(mut pipe)) = (stdin, process.stdin.take()) {
        // A CLI that exits before reading its prompt closes the pipe; its exit status says why.
        let _ = pipe.write_all(prompt.as_bytes()).await;
        drop(pipe);
    }
    let output = tokio::time::timeout(GENERATION_TIMEOUT, process.wait_with_output())
        .await
        .map_err(|_| {
            failure(
                "timeout",
                format!(
                    "{label} took longer than {} seconds. Try again.",
                    GENERATION_TIMEOUT.as_secs()
                ),
            )
        })?
        .map_err(|error| failure("failed", format!("{label} could not run: {error}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lowered = stderr.to_lowercase();
    if [
        "is not recognized",
        "command not found",
        "commandnotfoundexception",
        "no such file or directory",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
        && lowered.contains(&binary.to_lowercase())
    {
        return Err(missing());
    }
    let reason = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
        .map(|line| line.chars().take(200).collect::<String>())
        .unwrap_or_else(|| format!("exit code {:?}", output.status.code()));
    Err(failure(
        "agentUnavailable",
        format!("{label} couldn't write the summary: {reason}"),
    ))
}

/// The agent's answer without a wrapping code fence or stray whitespace.
fn clean_summary(output: &str) -> String {
    let trimmed = output.trim();
    let unfenced = trimmed
        .strip_prefix("```markdown")
        .or_else(|| trimmed.strip_prefix("```md"))
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.trim_end().strip_suffix("```"))
        .unwrap_or(trimmed);
    unfenced.trim().to_string()
}
