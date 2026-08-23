// C1 wave-1 deferred split: apps/desktop/src/app/helpers/project.rs (~4.3k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the project board beads bridge request
// handling and beads title generation. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::{Duration, Instant};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn run_project_beads_bridge_request_for_context(
    payload: &str,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> serde_json::Value {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    match project_beads_bridge_result_for_request(&request, context) {
        Ok(response) => response,
        Err(error) => project_beads_bridge_error_response(&request_id, &error),
    }
}

pub(crate) fn project_beads_bridge_result_for_request(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> Result<serde_json::Value, String> {
    let context = context.ok_or_else(|| "No active Kanban project is available.".to_string())?;
    let request_id = manage_request_string(request, "requestId").unwrap_or_default();
    let action = manage_request_string(request, "action").unwrap_or_default();
    if action == "generateTitle" {
        return gpui_project_beads_generate_title(request, context, &request_id);
    }

    let gxserver_action = project_beads_gxserver_action_for_board_action(&action)?;
    let mut params = serde_json::Map::new();
    params.insert(
        "action".to_string(),
        serde_json::Value::String(gxserver_action.to_string()),
    );
    params.insert(
        "projectBoardScope".to_string(),
        serde_json::Value::Bool(true),
    );
    params.insert(
        "projectPath".to_string(),
        serde_json::Value::String(context.project_path.clone()),
    );
    let request_project_id = manage_request_string(request, "projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let (Some(request_project_id), Some(active_project_id)) =
        (request_project_id.as_ref(), context.project_id.as_ref())
    {
        if request_project_id != active_project_id {
            return Err("Project board request was not sent by this active project.".to_string());
        }
    }
    if let Some(project_id) = request_project_id.or_else(|| context.project_id.clone()) {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id),
        );
    }

    for key in [
        "comment",
        "dependsOnId",
        "depType",
        "description",
        "issueId",
        "label",
        "priority",
        "query",
        "status",
        "title",
        "value",
    ] {
        if let Some(value) = manage_request_string(request, key) {
            params.insert(key.to_string(), serde_json::Value::String(value));
        }
    }
    if let Some(estimate) = request.get("estimate").filter(|value| value.is_number()) {
        params.insert("estimate".to_string(), estimate.clone());
    }
    if let Some(labels) = request.get("labels").and_then(serde_json::Value::as_array) {
        let labels = labels
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|label| serde_json::Value::String(label.to_string()))
            .collect::<Vec<_>>();
        params.insert("labels".to_string(), serde_json::Value::Array(labels));
    }

    let params = serde_json::Value::Object(params);
    if context.remote_machine_id.is_some() {
        let target = context
            .remote_target
            .as_ref()
            .ok_or_else(|| "The remote gxserver is not connected.".to_string())?;
        let result = gpui_remote_gxserver_rpc_result(
            target,
            "/api/runBeadsAction",
            &params,
            Duration::from_secs(60),
        )?;
        return Ok(project_beads_bridge_response_from_result(
            &result,
            &request_id,
        ));
    }
    let (status_code, body) =
        gxserver_post_typed_operation("/api/runBeadsAction", &params, Duration::from_secs(60))?;
    project_beads_bridge_response_from_gxserver(status_code, &body, &request_id)
}

pub(crate) fn project_beads_bridge_response_from_result(
    result: &serde_json::Value,
    request_id: &str,
) -> serde_json::Value {
    let exit_code = result
        .get("exitCode")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(1);
    serde_json::json!({
        "error": null,
        "exitCode": exit_code,
        "requestId": request_id,
        "stderr": result.get("stderr").and_then(serde_json::Value::as_str).unwrap_or(""),
        "stdout": result.get("stdout").and_then(serde_json::Value::as_str).unwrap_or(""),
    })
}

pub(crate) fn project_beads_bridge_error_response(request_id: &str, error: &str) -> serde_json::Value {
    serde_json::json!({
        "error": error,
        "exitCode": 127,
        "requestId": request_id,
        "stderr": error,
        "stdout": "",
    })
}

pub(crate) fn gpui_project_beads_generate_title(
    request: &serde_json::Value,
    context: &ProjectBoardBridgeRuntimeContext,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    // Scenario-gated (native.project.board) title-generation diagnostics:
    // agent id, prompt size, duration, and outcome only — never prompt text,
    // generated titles, paths, or process output.
    let agent_id = manage_request_string(request, "agentId").unwrap_or_default();
    let prompt_chars = manage_request_string(request, "prompt")
        .map(|prompt| prompt.chars().count())
        .unwrap_or(0);
    let started_at = Instant::now();
    support_logs::append(
        support_logs::GpuiSupportLog::ProjectBoard,
        "gpui.projectBoard.generateTitleStarted",
        serde_json::json!({ "agentId": agent_id, "promptChars": prompt_chars }),
    );
    let result = gpui_project_beads_generate_title_inner(request, context, request_id);
    support_logs::append(
        support_logs::GpuiSupportLog::ProjectBoard,
        if result.is_ok() {
            "gpui.projectBoard.generateTitleFinished"
        } else {
            "gpui.projectBoard.generateTitleFailed"
        },
        serde_json::json!({
            "durationMs": started_at.elapsed().as_millis() as u64,
            "ok": result.is_ok(),
        }),
    );
    result
}

pub(crate) fn gpui_project_beads_generate_title_inner(
    request: &serde_json::Value,
    context: &ProjectBoardBridgeRuntimeContext,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    /*
    macOS `projectBeadsGenerateTitle` parity (TerminalWorkspaceView.swift):
    board ticket title generation is a local prompt-agent subprocess, not a
    gxserver endpoint. The selected/default prompt agent runs once through a
    login zsh with the title prompt fed as a heredoc, and the first non-empty
    stdout line becomes the sanitized 39-char board title.
    */
    let prompt = manage_request_string(request, "prompt")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Missing required Beads field: prompt".to_string())?;
    let agent_command = manage_request_string(request, "agentCommand");
    let agent_id = manage_request_string(request, "agentId");
    let cwd = std::path::PathBuf::from(&context.project_path);
    if !cwd.is_dir() {
        return Err(format!(
            "Project path does not exist: {}",
            context.project_path
        ));
    }
    let generation_command = gpui_project_beads_prompt_generation_command(
        agent_command.as_deref(),
        agent_id.as_deref(),
    )?;
    let source_text: String = prompt.chars().take(4_000).collect();
    let generation_prompt = format!(
        "Write a concise session title that summarizes the user's text.\n\
         Return plain text only.\n\
         Rules:\n\
         - keep it specific and scannable\n\
         - must be fewer than 60 characters\n\
         - do not use quotes, markdown, or commentary\n\
         - do not end with punctuation\n\
         - focus on the task, bug, feature, or topic\n\
         \n\
         User text:\n\
         {source_text}\n\
         \n\
         Output handling:\n\
         - Produce only the final session title.\n\
         - Do not wrap the result in backticks.\n\
         - Print only the final result to stdout."
    );
    let delimiter = format!(
        "ghostex_SESSION_TITLE_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    );
    let command =
        format!("{generation_command} <<'{delimiter}'\n{generation_prompt}\n{delimiter}\n");
    let mut process = std::process::Command::new("/bin/zsh");
    process
        .arg("-lc")
        .arg(&command)
        .current_dir(&cwd)
        .env("GHOSTEX_INTERNAL_PROMPT_GENERATION", "1")
        .env("GHOSTEX_INTERNAL_TITLE_GENERATION", "1")
        .env(
            "PATH",
            gpui_project_beads_title_generation_path(std::env::var("PATH").ok().as_deref()),
        );
    for key in GPUI_PROJECT_BEADS_TITLE_GENERATION_STRIPPED_ENV_KEYS {
        process.env_remove(key);
    }
    // macOS waits on the process with no deadline (the page's own 60s bridge
    // timeout owns UI responsiveness); the beads bridge already runs on the
    // background executor.
    let output = process
        .output()
        .map_err(|_| "Could not launch the prompt-agent title generation process.".to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let stderr = stderr.trim();
        return Err(if stderr.is_empty() {
            "Prompt-agent title generation failed.".to_string()
        } else {
            stderr.to_string()
        });
    }
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "Prompt-agent title generation returned an empty title.".to_string())?;
    let sanitized = gpui_project_beads_sanitize_generated_title(line);
    if sanitized.is_empty() {
        return Err("Prompt-agent title generation returned an empty title.".to_string());
    }
    let title: String = sanitized.chars().take(39).collect();
    Ok(serde_json::json!({
        "error": null,
        "exitCode": 0,
        "requestId": request_id,
        "stderr": "",
        "stdout": serde_json::json!({ "title": title }).to_string(),
    }))
}

pub(crate) const GPUI_PROJECT_BEADS_TITLE_GENERATION_STRIPPED_ENV_KEYS: [&str; 22] = [
    // macOS `projectBoardInternalPromptGenerationEnvironmentKeys`: internal
    // prompt-agent work must not inherit Ghostex session-binding environment,
    // so hooks cannot turn the background job into a restorable user session.
    "GHOSTEX_AGENT",
    "GHOSTEX_GLOBAL_SESSION_REF",
    "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
    "GHOSTEX_GXSERVER_BASE_URL",
    "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
    "GHOSTEX_NATIVE_SESSION_ID",
    "GHOSTEX_SESSION_ID",
    "GHOSTEX_SESSION_STATE_FILE",
    "GHOSTEX_WORKSPACE_ID",
    "GHOSTEX_WORKSPACE_ROOT",
    "VSMUX_AGENT",
    "VSMUX_SESSION_ID",
    "VSMUX_SESSION_STATE_FILE",
    "VSMUX_WORKSPACE_ID",
    "VSMUX_WORKSPACE_ROOT",
    "ZMX_SESSION",
    "ZMX_SESSION_PREFIX",
    "ghostex_AGENT",
    "ghostex_SESSION_ID",
    "ghostex_SESSION_STATE_FILE",
    "ghostex_WORKSPACE_ID",
    "ghostex_WORKSPACE_ROOT",
];

pub(crate) fn gpui_project_beads_prompt_generation_command(
    agent_command: Option<&str>,
    agent_id: Option<&str>,
) -> Result<String, String> {
    // macOS `projectBeadsPromptGenerationCommand` parity, including the
    // ephemeral Codex exec profile so a title prompt can never become a
    // restorable Codex transcript.
    const CODEX_EXEC_ARGS: &str = "exec --ephemeral --skip-git-repo-check -m gpt-5.6-luna -c 'model_reasoning_effort=\"low\"'";
    let normalized_agent_id = agent_id
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();
    let command = agent_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if normalized_agent_id.is_empty() {
        return Ok(format!("codex {CODEX_EXEC_ARGS}"));
    }
    if let Some(command) = command {
        return Ok(match normalized_agent_id.as_str() {
            "codex" => format!("{command} {CODEX_EXEC_ARGS}"),
            "cursor" => format!(
                "{command} --print --mode ask --trust --model cursor-grok-4.5-low --output-format text"
            ),
            "claude" => format!("{command} -p --model haiku --effort low"),
            "gemini" => format!("{command} -p"),
            _ => command.to_string(),
        });
    }
    match normalized_agent_id.as_str() {
        "codex" => Ok(format!("codex {CODEX_EXEC_ARGS}")),
        "claude" => Ok("claude -p --model haiku --effort low".to_string()),
        "cursor" => Ok(
            "cursor-agent --print --mode ask --trust --model cursor-grok-4.5-low --output-format text"
                .to_string(),
        ),
        "gemini" => Ok("gemini -p".to_string()),
        _ => Err(format!(
            "{normalized_agent_id} does not support background title generation."
        )),
    }
}

pub(crate) fn gpui_project_beads_title_generation_path(existing: Option<&str>) -> String {
    // macOS `projectBoardNativeProcessPath`: common tool directories lead so
    // agent CLIs resolve even under minimal login-shell PATH files.
    let home = std::env::var("HOME").unwrap_or_default();
    let default_entries = [
        format!("{home}/.local/share/mise/shims"),
        format!("{home}/.local/bin"),
        format!("{home}/.asdf/shims"),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ];
    let mut seen = std::collections::HashSet::new();
    default_entries
        .into_iter()
        .chain(existing.unwrap_or_default().split(':').map(str::to_string))
        .filter(|entry| {
            let normalized = entry.trim().to_string();
            !normalized.is_empty() && seen.insert(normalized)
        })
        .collect::<Vec<_>>()
        .join(":")
}

pub(crate) fn gpui_project_beads_sanitize_generated_title(line: &str) -> String {
    let stripped = line
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | '`'));
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_end_matches(|character| matches!(character, '.' | '…'))
        .trim()
        .to_string()
}

pub(crate) fn gpui_normalized_project_path_for_comparison(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

