use super::*;

pub(crate) async fn handle_generate_commit_message_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match generate_commit_message_for_project(state, &params).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) async fn handle_create_pull_request_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let projects = {
        let db = match open_gxserver_database(&state.paths) {
            Ok(db) => db,
            Err(error) => {
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "internalError",
                        message: format!("SQLite gxserver state error: {error}"),
                    },
                );
            }
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        match repository.list_projects() {
            Ok(projects) => projects,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        }
    };
    match create_pull_request_for_project(&params, projects).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => typed_operation_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) struct CommitMessageGenerationAgent {
    agent_id: String,
    command: String,
    is_default: bool,
    name: String,
}

/*
CDXC:Git 2026-06-24-16:11:
GPUI blank commit-message generation is a local gxserver operation over a
registered project, not renderer shell execution. The endpoint stages only the
review-approved project-relative paths, derives branch/diff text from fixed Git
actions, resolves prompt-agent commands from stored gxserver project/settings
state, and returns only parsed subject/body text to the commit pipeline.
*/
pub(crate) async fn generate_commit_message_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let project_id = read_project_id(params)?;
    let requested_file_paths = read_commit_message_generation_file_paths(params)?;
    let (project, project_path, projects, settings) = {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository.get_project(&project_id)?.ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
        let project_path = project
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| DomainStateError::bad_request("Project has no filesystem path."))?
            .to_string();
        let projects = repository.list_projects()?;
        let settings = read_agent_settings(&db)?;
        /*
        CDXC:Build 2026-06-24-20:22:
        Axum requires the HTTP fallback future to be Send. Read commit-message
        generation metadata from SQLite up front and leave the rusqlite-backed
        repository inside this block before awaiting Git actions or agent output.
        */
        (project, project_path, projects, settings)
    };

    let status =
        run_commit_message_generation_git_action(&projects, &project_id, "statusPorcelainZ", None)
            .await?;
    ensure_commit_message_generation_git_success(&status, "Could not inspect selected changes.")?;
    let mut file_paths = retain_current_commit_message_generation_paths(
        &requested_file_paths,
        typed_result_stdout_raw(&status),
    )?;

    let mut add = run_commit_message_generation_git_action(
        &projects,
        &project_id,
        "addAll",
        Some(&file_paths),
    )
    .await?;
    for retry_delay_ms in [40_u64, 120] {
        if commit_message_generation_git_succeeded(&add) {
            break;
        }
        let stderr = add
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !commit_message_generation_stage_failure_is_transient(stderr) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
        let refreshed_status = run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "statusPorcelainZ",
            None,
        )
        .await?;
        ensure_commit_message_generation_git_success(
            &refreshed_status,
            "Could not refresh selected changes before staging.",
        )?;
        file_paths = retain_current_commit_message_generation_paths(
            &file_paths,
            typed_result_stdout_raw(&refreshed_status),
        )?;
        add = run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "addAll",
            Some(&file_paths),
        )
        .await?;
    }
    ensure_commit_message_generation_git_success(
        &add,
        commit_message_generation_stage_failure_message(&add),
    )?;

    let (summary, patch, branch) = tokio::try_join!(
        run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "diffCachedStatFiles",
            Some(&file_paths),
        ),
        run_commit_message_generation_git_action(
            &projects,
            &project_id,
            "diffCachedFiles",
            Some(&file_paths),
        ),
        run_commit_message_generation_git_action(&projects, &project_id, "branch", None),
    )?;
    ensure_commit_message_generation_git_success(
        &summary,
        "Could not inspect selected staged changes.",
    )?;
    ensure_commit_message_generation_git_success(
        &patch,
        "Could not inspect selected staged changes.",
    )?;

    let staged_summary = typed_result_stdout(&summary);
    let staged_patch = typed_result_stdout(&patch);
    if staged_summary.trim().is_empty() && staged_patch.trim().is_empty() {
        return Err(DomainStateError::bad_request(
            "No staged changes are available for commit message generation.",
        ));
    }

    let agent = resolve_commit_message_generation_agent(&project, params, &settings)?;
    let prompt = build_gxserver_commit_message_generation_prompt(
        typed_result_stdout(&branch).trim(),
        read_project_generate_commit_body(&project),
        &staged_summary,
        &staged_patch,
    );
    let stdout = run_commit_message_generation_agent(state, &project_path, &agent, &prompt).await?;
    parse_gxserver_generated_commit_message(&stdout, &agent.name)
}

pub(crate) fn read_commit_message_generation_file_paths(
    params: &Map<String, Value>,
) -> std::result::Result<Vec<String>, DomainStateError> {
    let file_paths = params
        .get("filePaths")
        .and_then(Value::as_array)
        .ok_or_else(|| DomainStateError::bad_request("filePaths must be a non-empty array."))?;
    if file_paths.is_empty() {
        return Err(DomainStateError::bad_request(
            "filePaths must include at least one changed file.",
        ));
    }
    if file_paths.len() > 500 {
        return Err(DomainStateError::bad_request(
            "filePaths exceeds the 500-file limit.",
        ));
    }
    let mut normalized = Vec::with_capacity(file_paths.len());
    for value in file_paths {
        let text = value
            .as_str()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request("filePaths must contain relative paths.")
            })?;
        if text.contains('\0')
            || Path::new(text).is_absolute()
            || text.split(['/', '\\']).any(|part| part == "..")
        {
            return Err(DomainStateError::bad_request(
                "filePaths must contain relative paths inside the project.",
            ));
        }
        let path = text.replace('\\', "/").trim_start_matches('/').to_string();
        if !normalized.contains(&path) {
            normalized.push(path);
        }
    }
    Ok(normalized)
}

pub(crate) async fn run_commit_message_generation_git_action(
    projects: &[Value],
    project_id: &str,
    action: &str,
    file_paths: Option<&[String]>,
) -> std::result::Result<Value, DomainStateError> {
    let mut params = Map::new();
    params.insert("action".to_string(), json!(action));
    params.insert("projectId".to_string(), json!(project_id));
    if let Some(file_paths) = file_paths {
        params.insert(
            "filePaths".to_string(),
            Value::Array(file_paths.iter().map(|path| json!(path)).collect()),
        );
    }
    dispatch_typed_operation_endpoint("/api/runGitAction", &params, projects.to_vec())
        .await
        .map_err(|error| typed_operation_commit_generation_error(error, "Git inspection failed."))
}

pub(crate) fn ensure_commit_message_generation_git_success(
    result: &Value,
    message: &str,
) -> std::result::Result<(), DomainStateError> {
    let exit_code = result.get("exitCode").and_then(Value::as_i64).unwrap_or(1);
    if exit_code == 0 && result.get("error").is_none() {
        return Ok(());
    }
    Err(DomainStateError {
        code: "badRequest",
        message: message.to_string(),
    })
}

pub(crate) fn commit_message_generation_git_succeeded(result: &Value) -> bool {
    result.get("exitCode").and_then(Value::as_i64) == Some(0) && result.get("error").is_none()
}

pub(crate) fn commit_message_generation_stage_failure_is_transient(stderr: &str) -> bool {
    let normalized = stderr.to_ascii_lowercase();
    normalized.contains("index.lock")
        || normalized.contains("another git process")
        || normalized.contains("pathspec")
}

pub(crate) fn commit_message_generation_stage_failure_message(result: &Value) -> &'static str {
    let stderr = result
        .get("stderr")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if stderr.contains("index.lock") || stderr.contains("another git process") {
        "Could not stage selected changes because another Git operation is still running."
    } else if stderr.contains("pathspec") {
        "Could not stage selected changes because reviewed files changed again."
    } else {
        "Could not stage selected changes."
    }
}

pub(crate) fn typed_operation_commit_generation_error(
    error: TypedOperationError,
    message: &str,
) -> DomainStateError {
    DomainStateError {
        code: error.code,
        message: message.to_string(),
    }
}

pub(crate) fn typed_result_stdout(result: &Value) -> String {
    result
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub(crate) fn typed_result_stdout_raw(result: &Value) -> &str {
    result
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/*
CDXC:Git 2026-06-24-16:11:
The generation endpoint must re-derive the current changed-file set from
gxserver-owned Git status before staging. The renderer's review request chooses
from that set, but Rust rejects stale or arbitrary file paths at the writer
boundary so prompt generation cannot inspect unrelated project content.
*/
pub(crate) fn retain_current_commit_message_generation_paths(
    file_paths: &[String],
    status_stdout: &str,
) -> std::result::Result<Vec<String>, DomainStateError> {
    /*
    CDXC:Git 2026-07-11-06:23:
    A commit review is path-trusted when the modal opens, but other agents can
    finish or replace one of those files before the user confirms. Keep only
    reviewed paths that are still changed at the authoritative gxserver check;
    never add newly changed paths and never allow an arbitrary requested path
    through. Reject only when the entire reviewed selection is now stale.
    */
    let changed_paths = parse_commit_message_generation_status_paths(status_stdout);
    let current_paths = file_paths
        .iter()
        .filter(|path| changed_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    if !current_paths.is_empty() {
        return Ok(current_paths);
    }
    Err(DomainStateError::bad_request(
        "None of the selected files are still part of the current Git review.",
    ))
}

pub(crate) fn parse_commit_message_generation_status_paths(status_stdout: &str) -> HashSet<String> {
    let mut changed_paths = HashSet::new();
    let mut entries = status_stdout.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let status = &entry[..2];
        if let Some(path) = normalize_commit_message_generation_status_path(&entry[3..]) {
            changed_paths.insert(path);
        }
        if status.contains('R') || status.contains('C') {
            let _ = entries.next();
        }
    }
    changed_paths
}

pub(crate) fn normalize_commit_message_generation_status_path(path: &str) -> Option<String> {
    let text = path.trim();
    if text.is_empty()
        || text.contains('\0')
        || Path::new(text).is_absolute()
        || text.split(['/', '\\']).any(|part| part == "..")
    {
        return None;
    }
    let normalized = text.replace('\\', "/").trim_start_matches('/').to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub(crate) fn read_project_generate_commit_body(project: &Value) -> bool {
    project
        .get("gitConfig")
        .and_then(Value::as_object)
        .and_then(|config| config.get("generateCommitBody"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

pub(crate) fn resolve_commit_message_generation_agent(
    project: &Value,
    params: &Map<String, Value>,
    settings: &Map<String, Value>,
) -> std::result::Result<CommitMessageGenerationAgent, DomainStateError> {
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| read_text_from_map(settings, "defaultPromptAgentId"))
        .unwrap_or_else(|| "codex".to_string());
    let agent_config = resolve_project_agent_config(project, &agent_id, None);
    let command = read_text_from_map(&agent_config, "command")
        .or_else(|| default_agent_command(&agent_id).map(str::to_string))
        .ok_or_else(|| DomainStateError {
            code: "dependencyUnavailable",
            message: "Choose a configured prompt agent before generating a commit message."
                .to_string(),
        })?;
    let is_default = default_agent_command(&agent_id).is_some();
    let name = read_text_from_map(&agent_config, "name").unwrap_or_else(|| {
        default_agent_name(&agent_id)
            .unwrap_or(agent_id.as_str())
            .to_string()
    });
    Ok(CommitMessageGenerationAgent {
        agent_id,
        command,
        is_default,
        name,
    })
}

pub(crate) fn default_agent_name(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some("Amp CLI"),
        "antigravity" => Some("Antigravity CLI"),
        "claude" => Some("Claude"),
        "codebuddy" => Some("CodeBuddy"),
        "codex" => Some("Codex"),
        "command-code" => Some("Command Code"),
        "copilot" => Some("Copilot"),
        "cursor" => Some("Cursor CLI"),
        "devin" => Some("Devin"),
        "droid" => Some("Factory Droid"),
        "empryo" => Some("Empryo"),
        "grok" => Some("Grok Build"),
        "hermes-agent" => Some("Hermes Agent"),
        "kimi" => Some("Kimi Code"),
        "kiro" => Some("Kiro CLI"),
        "omp" => Some("OMP"),
        "openclaude" => Some("OpenClaude"),
        "opencode" => Some("OpenCode"),
        "pi" => Some("Pi Agent"),
        "qoder" => Some("Qoder"),
        "rovodev" => Some("Rovo Dev"),
        _ => None,
    }
}

pub(crate) fn build_gxserver_commit_message_generation_prompt(
    branch: &str,
    generate_body: bool,
    staged_summary: &str,
    staged_patch: &str,
) -> String {
    let branch_line = format!(
        "Branch: {}",
        if branch.trim().is_empty() {
            "(detached)"
        } else {
            branch.trim()
        }
    );
    let summary = staged_summary.chars().take(6_000).collect::<String>();
    let patch = staged_patch.chars().take(40_000).collect::<String>();
    [
        "You write concise git commit messages.",
        "Return only a JSON object with keys: subject, body.",
        "Rules:",
        "- subject must be imperative, <= 72 chars, and no trailing period",
        if generate_body {
            "- body can be empty string or short bullet points"
        } else {
            "- body must be an empty string"
        },
        "- capture the primary user-visible or developer-visible change",
        "",
        &branch_line,
        "",
        "Staged files:",
        &summary,
        "",
        "Staged patch:",
        &patch,
    ]
    .join("\n")
}

pub(crate) async fn run_commit_message_generation_agent(
    state: &AppState,
    cwd: &str,
    agent: &CommitMessageGenerationAgent,
    prompt: &str,
) -> std::result::Result<String, DomainStateError> {
    // The job names its own account; see `one_shot_command` in accounts/launch.rs.
    let account_agent = CommitMessageGenerationAgent {
        agent_id: agent.agent_id.clone(),
        command: crate::accounts::launch::one_shot_command(
            state,
            &agent.agent_id,
            &agent.command,
            None,
        ),
        is_default: agent.is_default,
        name: agent.name.clone(),
    };
    let (shell_command, stdin) =
        build_commit_message_generation_shell_command(&account_agent, prompt)?;
    let shell = command_shell();
    let mut child = Command::new(&shell.executable);
    crate::platform::process::NoConsoleWindow::no_console_window(&mut child);
    child
        .args(shell.interactive_script_args(&shell_command))
        .current_dir(cwd)
        .envs(internal_prompt_generation_environment(
            &state.paths.home_dir,
        ))
        .kill_on_drop(true)
        .stdin(if stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let start_failed = || DomainStateError {
        code: "dependencyUnavailable",
        message: "Could not start commit message generation.".to_string(),
    };
    let mut process = child.spawn().map_err(|_| start_failed())?;
    if let (Some(prompt), Some(mut pipe)) = (stdin, process.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        // A CLI that exits before reading its prompt closes the pipe; its exit status says why.
        let _ = pipe.write_all(prompt.as_bytes()).await;
        drop(pipe);
    }
    let output = tokio::time::timeout(
        Duration::from_millis(GXSERVER_COMMIT_MESSAGE_GENERATION_TIMEOUT_MS),
        process.wait_with_output(),
    )
    .await
    .map_err(|_| DomainStateError {
        code: "dependencyUnavailable",
        message: "Commit message generation timed out.".to_string(),
    })?
    .map_err(|_| start_failed())?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = commit_message_generation_failure_message(
            &agent.name,
            output.status.code(),
            &stdout,
            &stderr,
        );
        let _ = state.logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "commitMessageGenerationFailed".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(message.clone()),
            details: None,
        });
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message,
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// CDXC:Git 2026-10-01 WHY:
/// A failed agent run used to surface only "<agent> commit message generation failed." and threw the real reason away (Claude prints "Failed to authenticate: OAuth session expired..." on stdout, most CLIs use stderr). The toast and the gxserver log carry the exit code plus the last error lines of each stream.
pub(crate) fn commit_message_generation_failure_message(
    agent_name: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> String {
    let mut message = match exit_code {
        Some(code) => format!("{agent_name} commit message generation failed (exit {code})."),
        None => format!("{agent_name} commit message generation was stopped by a signal."),
    };
    for stream in [stdout, stderr] {
        let tail = commit_message_generation_output_tail(stream);
        if !tail.is_empty() {
            message.push('\n');
            message.push_str(&tail);
        }
    }
    message
}

fn commit_message_generation_output_tail(output: &str) -> String {
    const MAX_LINES: usize = 2;
    const MAX_LINE_CHARS: usize = 160;
    let lines = output
        .lines()
        .map(|line| {
            crate::session_chat_options::strip_ansi_sgr(line)
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    // CLIs often end with a usage or help footer after the error, and Codex echoes the whole prompt (diff included) to stderr first, so look for error lines only near the end.
    let recent = &lines[lines.len().saturating_sub(8)..];
    let error_lines = recent
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error") || lower.contains("failed")
        })
        .collect::<Vec<_>>();
    let picked = if error_lines.is_empty() {
        recent.iter().collect::<Vec<_>>()
    } else {
        error_lines
    };
    picked[picked.len().saturating_sub(MAX_LINES)..]
        .iter()
        .map(|line| {
            if line.chars().count() > MAX_LINE_CHARS {
                format!(
                    "{}...",
                    line.chars().take(MAX_LINE_CHARS).collect::<String>()
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The shell command for the commit message agent, and the prompt to write to its stdin when it reads
/// it from there (`None` when the prompt is part of the command).
///
/// CDXC:Git 2026-10-17 WHY:
/// The prompt used to reach Codex, Claude and a custom command through a POSIX heredoc (`codex exec … <<'DELIM'`), which PowerShell, the Windows command shell, cannot run, so commit message generation failed on Windows for every agent but Cursor. It now goes to the CLI's stdin, like the title job and Catch me up, and Cursor's argument is quoted for the shell that runs it.
pub(crate) fn build_commit_message_generation_shell_command(
    agent: &CommitMessageGenerationAgent,
    prompt: &str,
) -> std::result::Result<(String, Option<String>), DomainStateError> {
    let stdin = Some(prompt.to_string());
    Ok(match agent.agent_id.as_str() {
        "codex" => {
            let command = one_shot_text_job_command(&agent.command, "codex");
            /*
            CDXC:Git 2026-10-01 DECISION:
            Commit message generation runs Codex on gpt-6-luna with high reasoning effort, and Claude on Sonnet 5.5 (claude-sonnet-5-5) at medium effort.
            */
            let command = format!(
                "{command} exec --sandbox read-only --ephemeral --skip-git-repo-check -m gpt-6-luna -c 'model_reasoning_effort=\"high\"'"
            );
            (command, stdin)
        }
        "cursor" => (
            format!(
                "{} --print --mode ask --trust --output-format text {}",
                one_shot_text_job_command(&agent.command, "cursor"),
                crate::session_chat_catch_up::quote_prompt_argument(prompt)
            ),
            None,
        ),
        "claude" => {
            let command = one_shot_text_job_command(&agent.command, "claude");
            (
                format!(
                    "{command} --permission-mode dontAsk --model claude-sonnet-5-5 --effort medium -p"
                ),
                stdin,
            )
        }
        _ if !agent.is_default => (agent.command.clone(), stdin),
        _ => {
            return Err(DomainStateError {
                code: "badRequest",
                message: format!(
                    "{} does not support background commit message generation.",
                    agent.name
                ),
            })
        }
    })
}

pub(crate) fn parse_gxserver_generated_commit_message(
    stdout: &str,
    agent_name: &str,
) -> std::result::Result<Value, DomainStateError> {
    let start = stdout.find('{').ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        ))
    })?;
    let end = stdout.rfind('}').ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        ))
    })?;
    if end < start {
        return Err(DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        )));
    }
    let parsed = serde_json::from_str::<Value>(&stdout[start..=end]).map_err(|_| {
        DomainStateError::bad_request(format!(
            "{agent_name} did not return a commit message JSON object."
        ))
    })?;
    let subject = parsed
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '。'])
        .chars()
        .take(72)
        .collect::<String>()
        .trim()
        .to_string();
    if subject.is_empty() {
        return Err(DomainStateError::bad_request(format!(
            "{agent_name} returned an empty commit subject."
        )));
    }
    let body = parsed
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(json!({ "body": body, "subject": subject }))
}
