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
CDXC:GPUISidebarGit 2026-06-24-16:11:
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
        CDXC:GxserverRustBuild 2026-06-24-20:22:
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
CDXC:GPUISidebarGit 2026-06-24-16:11:
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
    CDXC:GPUISidebarGit 2026-07-11-06:23:
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
        "campfire" => Some("Campfire"),
        "claude" => Some("Claude"),
        "codebuddy" => Some("CodeBuddy"),
        "codex" => Some("Codex"),
        "command-code" => Some("Command Code"),
        "copilot" => Some("Copilot"),
        "cursor" => Some("Cursor CLI"),
        "devin" => Some("Devin"),
        "droid" => Some("Factory Droid"),
        "gemini" => Some("Gemini"),
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
    let delimiter = format!(
        "ghostex_GXSERVER_GIT_COMMIT_{}",
        chrono::Utc::now().timestamp_millis()
    );
    let shell_command = build_commit_message_generation_shell_command(agent, &delimiter, prompt)?;
    let shell = command_shell();
    let mut child = Command::new(&shell.executable);
    child
        .args(shell.interactive_script_args(&shell_command))
        .current_dir(cwd)
        .envs(internal_prompt_generation_environment(
            &state.paths.home_dir,
        ))
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = tokio::time::timeout(
        Duration::from_millis(GXSERVER_COMMIT_MESSAGE_GENERATION_TIMEOUT_MS),
        child.output(),
    )
    .await
    .map_err(|_| DomainStateError {
        code: "dependencyUnavailable",
        message: "Commit message generation timed out.".to_string(),
    })?
    .map_err(|_| DomainStateError {
        code: "dependencyUnavailable",
        message: "Could not start commit message generation.".to_string(),
    })?;
    if !output.status.success() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: format!("{} commit message generation failed.", agent.name),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) fn build_commit_message_generation_shell_command(
    agent: &CommitMessageGenerationAgent,
    delimiter: &str,
    prompt: &str,
) -> std::result::Result<String, DomainStateError> {
    Ok(match agent.agent_id.as_str() {
        "codex" => {
            let command = enforce_required_agent_permission_flag(&agent.command, "codex");
            let command = format!(
                "{command} exec --ephemeral --skip-git-repo-check -m gpt-5.4-mini -c 'model_reasoning_effort=\"low\"'"
            );
            create_here_doc_command(&command, delimiter, prompt)
        }
        "cursor" => format!(
            "{} --print --mode ask --trust --output-format text {}",
            agent.command,
            quote_shell_arg(prompt)
        ),
        "claude" => {
            let command = enforce_required_agent_permission_flag(&agent.command, "claude");
            create_here_doc_command(&format!("{command} -p"), delimiter, prompt)
        }
        "gemini" => create_here_doc_command(&format!("{} -p", agent.command), delimiter, prompt),
        _ if !agent.is_default => create_here_doc_command(&agent.command, delimiter, prompt),
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
