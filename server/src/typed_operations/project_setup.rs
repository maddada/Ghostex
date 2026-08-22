use std::fs;
use std::io::Read;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::platform::shell::command_shell;

use super::values::{
    expand_user_path, is_path_inside, json_value_to_javascript_string, normalize_path_string,
    run_process_command, typed_result, CommandSummary, ProcessCommand, TypedOperationContext,
    TypedOperationError, SETUP_COMMAND_LIMIT_BYTES,
};

pub(crate) async fn run_project_setup_command(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = params.get("action").and_then(Value::as_str);
    if action != Some("worktreeSetupCommand") {
        return Err(TypedOperationError::bad_request(format!(
            "Unsupported project setup action: {}",
            setup_action_value_to_string(params.get("action"))
        )));
    }
    let action = action.expect("validated setup action");
    let setup_project = resolve_project_setup_command_project(params, context)?;
    /*
    CDXC:GlobalProjectDefaults 2026-08-02:
    A project without its own worktree command now runs the Global Default. With
    no global configured this resolves to None and the operation still no-ops
    below, which is the pre-feature behavior.
    */
    let resolved_setup_command = crate::global_project_defaults::resolve_with_global_default(
        setup_project
            .get("gitConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("worktreeCommand"))
            .and_then(Value::as_str),
        &crate::global_project_defaults::read_global_project_defaults().worktree_command,
    );
    let command_text =
        normalize_project_setup_command(resolved_setup_command.map(Value::String).as_ref())?;
    if command_text.is_empty() {
        return Ok(json!({
            "action": action,
            "exitCode": 0,
            "stderr": "",
            "stdout": "",
        }));
    }
    let shell = command_shell();
    let command = ProcessCommand {
        args: vec![shell.command_flag(false).to_string(), command_text],
        cwd: context.cwd.clone(),
        env: Vec::new(),
        executable: shell.executable.clone(),
        preserve_stdout_whitespace: false,
        result_command: Some(CommandSummary {
            args: vec![
                shell.command_flag(false).to_string(),
                "<worktree setup command>".to_string(),
            ],
            cwd: context.cwd.clone(),
            executable: shell.executable.clone(),
        }),
        stdin: None,
    };
    let output = run_process_command(&command, context).await?;
    Ok(typed_result(action, &command, output))
}

fn normalize_project_setup_command(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let Some(text) = input.and_then(Value::as_str) else {
        return Ok(String::new());
    };
    if text.len() > SETUP_COMMAND_LIMIT_BYTES {
        return Err(TypedOperationError::bad_request(
            "worktreeCommand exceeds the 16384-byte limit.",
        ));
    }
    Ok(text.trim().to_string())
}

fn resolve_project_setup_command_project(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    if let Some(project_id) = params
        .get("setupCommandProjectId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return context
            .projects
            .iter()
            .find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .cloned()
            .ok_or_else(|| {
                TypedOperationError::not_found(format!("Project {project_id} does not exist."))
            });
    }
    if let Some(path) = params
        .get("setupCommandProjectPath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        let setup_path =
            normalize_project_setup_existing_directory_path(path, "setupCommandProjectPath")?;
        for candidate in &context.projects {
            if let Some(candidate_path) = candidate.get("path").and_then(Value::as_str) {
                if normalize_project_setup_existing_directory_path(candidate_path, "project.path")?
                    == setup_path
                {
                    return Ok(candidate.clone());
                }
            }
        }
        return Err(TypedOperationError::forbidden(
            "setupCommandProjectPath must be a registered gxserver project path.",
        ));
    }
    resolve_project_setup_command_for_context_cwd(context)
}

fn resolve_project_setup_command_for_context_cwd(
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let context_cwd =
        normalize_project_setup_existing_directory_path(&context.cwd, "project.path")?;
    for candidate in &context.projects {
        if let Some(candidate_path) = candidate.get("path").and_then(Value::as_str) {
            if normalize_project_setup_existing_directory_path(candidate_path, "project.path")?
                == context_cwd
            {
                return Ok(candidate.clone());
            }
        }
    }
    Err(TypedOperationError::forbidden(
        "Project setup command target must be a registered gxserver project.",
    ))
}

fn normalize_project_setup_existing_directory_path(
    input: &str,
    field: &str,
) -> Result<String, TypedOperationError> {
    let path = normalize_project_setup_absolute_path(input, field)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| TypedOperationError::not_found(format!("{field} does not exist: {path}")))?;
    if !metadata.is_dir() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} is not a directory."
        )));
    }
    Ok(path)
}

fn normalize_project_setup_absolute_path(
    input: &str,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.trim();
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

fn setup_action_value_to_string(input: Option<&Value>) -> String {
    match input {
        None => "undefined".to_string(),
        Some(value) => json_value_to_javascript_string(value),
    }
}

pub(crate) fn count_project_file_lines(
    cwd: &str,
    file_paths: &[String],
) -> Result<usize, TypedOperationError> {
    let mut total = 0;
    for file_path in file_paths {
        let absolute = Path::new(cwd).join(file_path);
        if !is_path_inside(cwd, &absolute.to_string_lossy()) {
            return Err(TypedOperationError::forbidden(
                "filePath must stay inside the project.",
            ));
        }
        let mut file = fs::File::open(&absolute)
            .map_err(|error| TypedOperationError::not_found(error.to_string()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| TypedOperationError::bad_request(error.to_string()))?;
        total += bytes.iter().filter(|byte| **byte == b'\n').count();
    }
    Ok(total)
}
