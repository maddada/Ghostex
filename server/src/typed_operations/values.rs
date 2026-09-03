use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{json, Map, Value};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    time::{timeout, Instant},
};

const TYPED_OPERATION_TIMEOUT_MS: u64 = 120_000;
const TYPED_OPERATION_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const TYPED_OPERATION_STDERR_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const COMMIT_MESSAGE_LIMIT_BYTES: usize = 64 * 1024;
pub(crate) const SETUP_COMMAND_LIMIT_BYTES: usize = 16 * 1024;
const FILE_PATH_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct TypedOperationError {
    pub code: &'static str,
    pub details: Option<Value>,
    pub message: String,
    pub scope_rejection: bool,
}

impl TypedOperationError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    pub(crate) fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "dependencyUnavailable",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self {
            code: "forbidden",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    pub(crate) fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub(crate) fn with_scope_rejection(mut self) -> Self {
        self.scope_rejection = true;
        self
    }
}

impl fmt::Display for TypedOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for TypedOperationError {}

#[derive(Clone, Debug)]
pub struct TypedOperationContext {
    pub(crate) beads_cwd: Option<String>,
    pub(crate) cwd: String,
    pub(crate) env_path: Option<String>,
    pub(crate) projects: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessCommand {
    pub(crate) args: Vec<String>,
    pub(crate) cwd: String,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) executable: String,
    pub(crate) preserve_stdout_whitespace: bool,
    pub(crate) result_command: Option<CommandSummary>,
    pub(crate) stdin: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CommandSummary {
    pub(crate) args: Vec<String>,
    pub(crate) cwd: String,
    pub(crate) executable: String,
}

#[derive(Debug)]
pub(crate) struct CommandOutput {
    pub(crate) error: Option<Value>,
    pub(crate) exit_code: i32,
    pub(crate) stderr: String,
    pub(crate) stdout: String,
}

impl ProcessCommand {
    pub(crate) fn new<S, A, C>(executable: S, args: Vec<A>, cwd: C) -> Self
    where
        S: Into<String>,
        A: Into<String>,
        C: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.into(),
            env: Vec::new(),
            executable: executable.into(),
            preserve_stdout_whitespace: false,
            result_command: None,
            stdin: None,
        }
    }

    pub(crate) fn with_result_args(mut self, args: Vec<&str>) -> Self {
        self.result_command = Some(CommandSummary {
            args: args.into_iter().map(str::to_string).collect(),
            cwd: self.cwd.clone(),
            executable: self.executable.clone(),
        });
        self
    }

    pub(crate) fn with_result_command(mut self, result_command: Option<CommandSummary>) -> Self {
        self.result_command = result_command;
        self
    }

    pub(crate) fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    pub(crate) fn with_stdin(mut self, stdin: String) -> Self {
        self.stdin = Some(stdin);
        self
    }

    pub(crate) fn with_preserved_stdout_whitespace(mut self) -> Self {
        self.preserve_stdout_whitespace = true;
        self
    }

    pub(crate) fn summary(&self) -> CommandSummary {
        self.result_command
            .clone()
            .unwrap_or_else(|| CommandSummary {
                args: self.args.clone(),
                cwd: self.cwd.clone(),
                executable: self.executable.clone(),
            })
    }
}

pub(crate) trait StringOrElse {
    fn or_else(self, fallback: &str) -> String;
}

impl StringOrElse for String {
    fn or_else(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

pub(crate) async fn run_process_command(
    command: &ProcessCommand,
    context: &TypedOperationContext,
) -> Result<CommandOutput, TypedOperationError> {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(typed_operation_environment(
            context.env_path.as_deref(),
            &command.env,
        ))
        .kill_on_drop(true)
        .stdin(if command.stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = process.spawn().map_err(|error| {
        TypedOperationError::dependency_unavailable(format!(
            "Could not start typed operation command: {error}"
        ))
    })?;

    if let Some(stdin) = command.stdin.as_deref() {
        if let Some(mut child_stdin) = child.stdin.take() {
            if child_stdin.write_all(stdin.as_bytes()).await.is_err() {
                let _ = child.start_kill();
                return Ok(CommandOutput {
                    error: Some(json!({
                        "code": "stdinFailed",
                        "message": "Typed operation stdin write failed.",
                    })),
                    exit_code: 1,
                    stderr: String::new(),
                    stdout: String::new(),
                });
            }
            let _ = child_stdin.shutdown().await;
        }
    }

    let started = Instant::now();
    let result = timeout(
        Duration::from_millis(TYPED_OPERATION_TIMEOUT_MS),
        child.wait_with_output(),
    )
    .await;
    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(TypedOperationError::dependency_unavailable(format!(
                "Typed operation command failed: {error}"
            )))
        }
        Err(_) => {
            return Ok(CommandOutput {
                error: Some(json!({
                    "code": "timeout",
                    "message": format!("Typed operation timed out after {TYPED_OPERATION_TIMEOUT_MS}ms."),
                    "timeoutMs": TYPED_OPERATION_TIMEOUT_MS,
                })),
                exit_code: 1,
                stderr: String::new(),
                stdout: String::new(),
            })
        }
    };
    /*
    CDXC:Git 2026-07-24-17:38:
    Git porcelain status uses the first two columns as data, including a
    leading space for an unstaged change. Preserve stdout exactly for those
    commands so the first path is not shifted left and truncated before the
    reviewed-file staging boundary.
    */
    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let mut stdout = if command.preserve_stdout_whitespace {
        stdout_text.to_string()
    } else {
        stdout_text.trim().to_string()
    };
    let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.stdout.len() > TYPED_OPERATION_STDOUT_LIMIT_BYTES {
        let truncated_stdout =
            String::from_utf8_lossy(&output.stdout[..TYPED_OPERATION_STDOUT_LIMIT_BYTES]);
        stdout = if command.preserve_stdout_whitespace {
            truncated_stdout.to_string()
        } else {
            truncated_stdout.trim().to_string()
        };
        return Ok(CommandOutput {
            error: Some(json!({
                "capturedBytes": TYPED_OPERATION_STDOUT_LIMIT_BYTES,
                "code": "stdoutLimitExceeded",
                "limitBytes": TYPED_OPERATION_STDOUT_LIMIT_BYTES,
                "message": format!("Typed operation stdout exceeded {TYPED_OPERATION_STDOUT_LIMIT_BYTES} bytes."),
                "stream": "stdout",
            })),
            exit_code: 1,
            stderr,
            stdout,
        });
    }
    if output.stderr.len() > TYPED_OPERATION_STDERR_LIMIT_BYTES {
        stderr = String::from_utf8_lossy(&output.stderr[..TYPED_OPERATION_STDERR_LIMIT_BYTES])
            .trim()
            .to_string();
        return Ok(CommandOutput {
            error: Some(json!({
                "capturedBytes": TYPED_OPERATION_STDERR_LIMIT_BYTES,
                "code": "stderrLimitExceeded",
                "limitBytes": TYPED_OPERATION_STDERR_LIMIT_BYTES,
                "message": format!("Typed operation stderr exceeded {TYPED_OPERATION_STDERR_LIMIT_BYTES} bytes."),
                "stream": "stderr",
            })),
            exit_code: 1,
            stderr,
            stdout,
        });
    }
    let _duration_ms = started.elapsed().as_millis();
    Ok(CommandOutput {
        error: None,
        exit_code: output.status.code().unwrap_or(1),
        stderr,
        stdout,
    })
}

pub(crate) fn typed_operation_environment(
    env_path: Option<&str>,
    extra_env: &[(String, String)],
) -> Vec<(String, String)> {
    let mut environment: Vec<(String, String)> = env::vars().collect();
    environment.retain(|(key, _)| !is_typed_operation_color_env_key(key));
    if let Some(path) = env_path {
        environment.retain(|(key, _)| key != "PATH");
        environment.push(("PATH".to_string(), path.to_string()));
    }
    for (key, value) in extra_env {
        if is_typed_operation_color_env_key(key) {
            continue;
        }
        environment.retain(|(existing_key, _)| existing_key != key);
        environment.push((key.clone(), value.clone()));
    }
    environment
}

fn is_typed_operation_color_env_key(key: &str) -> bool {
    matches!(
        key,
        "ANSI_COLORS_DISABLED" | "NO_COLOR" | "NODE_DISABLE_COLORS"
    )
}

pub(crate) fn typed_result(action: &str, command: &ProcessCommand, output: CommandOutput) -> Value {
    let mut result = Map::new();
    result.insert("action".to_string(), json!(action));
    result.insert(
        "command".to_string(),
        command_summary_json(&command.summary()),
    );
    if let Some(error) = output.error {
        result.insert("error".to_string(), error);
    }
    result.insert("exitCode".to_string(), json!(output.exit_code));
    result.insert("stderr".to_string(), json!(output.stderr));
    result.insert("stdout".to_string(), json!(output.stdout));
    Value::Object(result)
}

pub(crate) fn command_summary_json(command: &CommandSummary) -> Value {
    json!({
        "args": command.args,
        "cwd": command.cwd,
        "executable": command.executable,
    })
}

pub(crate) fn read_project_id_value(project: &Value) -> Result<String, TypedOperationError> {
    project
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| TypedOperationError::bad_request("Project is missing projectId."))
}

pub(crate) fn normalize_absolute_path_value(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(text);
    if !expanded.is_absolute() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    Ok(normalize_path_string(expanded))
}

pub(crate) fn normalize_existing_directory_path_value(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let path = normalize_absolute_path_value(input, field)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| TypedOperationError::not_found(format!("{field} does not exist: {path}")))?;
    if !metadata.is_dir() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} is not a directory: {path}"
        )));
    }
    Ok(path)
}

pub(crate) fn normalize_absolute_path(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be a non-empty absolute path or ~/ path."
        )));
    }
    let expanded = expand_user_path(text);
    if !expanded.is_absolute() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be absolute or start with ~/."
        )));
    }
    Ok(normalize_path_string(expanded))
}

pub(crate) fn expand_user_path(input: &str) -> PathBuf {
    if input == "~" {
        return home_dir();
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(input)
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn normalize_path_string(path: PathBuf) -> String {
    /*
    CDXC:Projects 2026-06-22-09:35:
    Typed-operation projectPath, worktreePath, setupCommandProjectPath, and Beads directory checks must match TypeScript `path.resolve` syntax normalization. Collapse `.` and `..` without canonicalizing symlinks so registered-root equality and containment stay lexical while selected paths still get filesystem validation at the operation boundary.
    */
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(segment) => normalized.push(segment),
        }
    }
    if normalized.as_os_str().is_empty() {
        ".".to_string()
    } else {
        normalized.to_string_lossy().to_string()
    }
}

pub(crate) fn is_path_inside(parent_path: &str, candidate_path: &str) -> bool {
    let parent = Path::new(parent_path);
    let candidate = Path::new(candidate_path);
    if parent == candidate {
        return true;
    }
    candidate.starts_with(parent)
}

pub(crate) fn optional_relative_file_path(
    input: Option<&Value>,
) -> Result<Vec<String>, TypedOperationError> {
    if input.is_none() || input == Some(&Value::Null) || input.and_then(Value::as_str) == Some("") {
        return Ok(Vec::new());
    }
    Ok(vec![normalize_relative_file_path(input)?])
}

pub(crate) fn optional_relative_file_paths(
    input: Option<&Value>,
) -> Result<Vec<String>, TypedOperationError> {
    let Some(value) = input else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let array = value.as_array().ok_or_else(|| {
        TypedOperationError::bad_request("filePaths must be an array of relative paths.")
    })?;
    if array.len() > FILE_PATH_LIMIT {
        return Err(TypedOperationError::bad_request(
            "filePaths exceeds the 500-file limit.",
        ));
    }
    array
        .iter()
        .map(|value| normalize_relative_file_path(Some(value)))
        .collect()
}

pub(crate) fn required_relative_file_paths(
    input: Option<&Value>,
) -> Result<Vec<String>, TypedOperationError> {
    let paths = optional_relative_file_paths(input)?;
    if paths.is_empty() {
        return Err(TypedOperationError::bad_request(
            "filePaths must include at least one relative path.",
        ));
    }
    Ok(paths)
}

pub(crate) fn normalize_relative_file_path(
    input: Option<&Value>,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).unwrap_or("");
    if text.is_empty()
        || Path::new(text).is_absolute()
        || text.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(TypedOperationError::bad_request(
            "filePath must be a relative path inside the project.",
        ));
    }
    let normalized = text
        .replace('\\', "/")
        .trim_start_matches('/')
        .trim()
        .to_string();
    if normalized.is_empty() {
        return Err(TypedOperationError::bad_request(
            "filePath must be a relative path inside the project.",
        ));
    }
    Ok(normalized)
}

pub(crate) fn normalize_git_commit_message(
    subject_input: Option<&Value>,
    body_input: Option<&Value>,
) -> Result<String, TypedOperationError> {
    let subject = normalize_required_text(subject_input, "messageSubject")?;
    let body = body_input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let message = if let Some(body) = body {
        format!("{subject}\n\n{body}")
    } else {
        subject
    };
    if message.len() > COMMIT_MESSAGE_LIMIT_BYTES {
        return Err(TypedOperationError::bad_request(
            "Commit message exceeds the 65536-byte limit.",
        ));
    }
    Ok(format!("{message}\n"))
}

pub(crate) fn normalize_git_ref(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let value = normalize_required_text(input, field)?;
    if !is_allowed_git_ref(&value) {
        return Err(TypedOperationError::bad_request(format!(
            "{field} is not an allowed Git ref."
        )));
    }
    Ok(value)
}

pub(crate) fn normalize_optional_git_ref(
    input: Option<&Value>,
    field: &str,
) -> Result<Option<String>, TypedOperationError> {
    if input.is_none() || input == Some(&Value::Null) || input.and_then(Value::as_str) == Some("") {
        return Ok(None);
    }
    normalize_git_ref(input, field).map(Some)
}

pub(crate) fn is_allowed_git_ref(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}

pub(crate) fn normalize_git_remote_name(
    input: Option<&Value>,
) -> Result<String, TypedOperationError> {
    let value = if input.is_none() || input == Some(&Value::Null) {
        "origin".to_string()
    } else {
        normalize_required_text(input, "remoteName")?
    };
    if value.chars().enumerate().all(|(index, ch)| {
        (index == 0 && ch.is_ascii_alphanumeric())
            || (index > 0 && (ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')))
    }) {
        return Ok(value);
    }
    Err(TypedOperationError::bad_request(
        "remoteName is not an allowed Git remote name.",
    ))
}

pub(crate) fn normalize_issue_id(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let issue_id = normalize_required_text(input, "issueId")?;
    let mut chars = issue_id.chars();
    if matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && issue_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
    {
        return Ok(issue_id);
    }
    Err(TypedOperationError::bad_request(
        "issueId contains unsupported characters.",
    ))
}

pub(crate) fn normalize_required_text(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be a non-empty string."
        )));
    }
    Ok(text.to_string())
}

pub(crate) fn resolve_path_against(cwd: &str, path_text: &str) -> PathBuf {
    let path = Path::new(path_text.trim());
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    };
    resolved.components().collect()
}

#[cfg(unix)]
pub(crate) fn chmod_executable_if_supported(path: &Path) -> Result<(), TypedOperationError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
        TypedOperationError::bad_request(format!("Could not chmod Beads Git hook: {error}"))
    })
}

#[cfg(not(unix))]
pub(crate) fn chmod_executable_if_supported(_path: &Path) -> Result<(), TypedOperationError> {
    Ok(())
}

pub(crate) fn json_value_to_javascript_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| match value {
                Value::Null => String::new(),
                Value::Array(_) => json_value_to_javascript_string(value),
                _ => json_value_to_javascript_string(value),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

pub(crate) fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn nullish_value_to_string(input: Option<&Value>, default_value: &str) -> String {
    match input {
        None | Some(Value::Null) => default_value.to_string(),
        Some(value) => value_to_string(value),
    }
}

pub(crate) fn normalize_nullish_required_text(
    input: Option<&Value>,
    field: &str,
    default_value: &str,
) -> Result<String, TypedOperationError> {
    match input {
        None | Some(Value::Null) => Ok(default_value.to_string()),
        Some(value) => normalize_required_text(Some(value), field),
    }
}

pub(crate) fn display_unknown_value(value: Option<&Value>) -> String {
    value
        .map(value_to_string)
        .unwrap_or_else(|| "undefined".to_string())
}
