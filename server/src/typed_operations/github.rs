use ghostex_gx_protocol::git_provider::GitHostProvider;
use serde_json::{json, Map, Value};

use crate::source_control::pull_request::{
    resolve_repo_provider, CreatePullRequestOptions, PullRequestViewState, ResolvedRepoProvider,
};

use super::values::{
    command_summary_json, display_unknown_value, run_process_command, typed_result, ProcessCommand,
    TypedOperationContext, TypedOperationError,
};

pub(crate) async fn run_github_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_github_action(params.get("action"))?;
    let resolved = resolve_repo_provider(context).await;
    let command = build_provider_command(&resolved, &action, &context.cwd);
    let mut result = match run_process_command(&command, context).await {
        Ok(output) => typed_result(&action, &command, output),
        Err(error) => json!({
            "action": action,
            "command": command_summary_json(&command.summary()),
            "exitCode": 1,
            "stderr": error.message,
            "stdout": "",
        }),
    };
    if let Some(object) = result.as_object_mut() {
        object.insert("provider".to_string(), json!(resolved.kind.wire_name()));
    }
    Ok(result)
}

pub(crate) fn build_provider_command(
    resolved: &ResolvedRepoProvider,
    action: &str,
    cwd: &str,
) -> ProcessCommand {
    let provider = resolved.provider();
    match action {
        "prCreateFill" => {
            provider.build_create_command(cwd, &CreatePullRequestOptions { fill: true })
        }
        "prView" => provider.build_view_command(cwd, true),
        "version" => provider.build_version_command(cwd),
        _ => unreachable!("validated provider action"),
    }
}

pub(crate) enum PullRequestProbe {
    Failed,
    Invalid,
    NotOpen,
    Open(Value),
}

pub(crate) async fn probe_open_pull_request(
    context: &TypedOperationContext,
    resolved: &ResolvedRepoProvider,
) -> Result<PullRequestProbe, TypedOperationError> {
    let provider = resolved.provider();
    let command = provider.build_view_command(context.cwd.as_str(), false);
    let output = match run_process_command(&command, context).await {
        Ok(output) => output,
        Err(_) => return Ok(PullRequestProbe::Failed),
    };
    if output.exit_code != 0 || output.error.is_some() || output.stdout.trim().is_empty() {
        return Ok(PullRequestProbe::Failed);
    }
    let Some(view) = provider.parse_view_output(&output.stdout, resolved.expected_host()) else {
        return Ok(PullRequestProbe::Invalid);
    };
    if view.state == PullRequestViewState::Open {
        Ok(PullRequestProbe::Open(view.to_summary_json()))
    } else {
        Ok(PullRequestProbe::NotOpen)
    }
}

pub(crate) fn pull_request_success(provider: GitHostProvider, created: bool, pr: Value) -> Value {
    json!({
        "created": created,
        "ok": true,
        "pr": pr,
        "provider": provider.wire_name(),
    })
}

pub(crate) fn pull_request_failure(provider: GitHostProvider, reason: &str) -> Value {
    json!({
        "created": false,
        "ok": false,
        "provider": provider.wire_name(),
        "reason": reason,
    })
}

fn normalize_github_action(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "prCreateFill" | "prView" | "version" => Ok(action.to_string()),
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported GitHub action: {}",
            display_unknown_value(input)
        ))),
    }
}
