use serde_json::{json, Map, Value};
use url::Url;

use super::values::{
    command_summary_json, display_unknown_value, run_process_command, typed_result, ProcessCommand,
    TypedOperationContext, TypedOperationError,
};

pub(crate) async fn run_github_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_github_action(params.get("action"))?;
    let command = build_github_command(&action, &context.cwd);
    match run_process_command(&command, context).await {
        Ok(output) => Ok(typed_result(&action, &command, output)),
        Err(error) => Ok(json!({
            "action": action,
            "command": command_summary_json(&command.summary()),
            "exitCode": 1,
            "stderr": error.message,
            "stdout": "",
        })),
    }
}

pub(crate) fn build_github_command(action: &str, cwd: &str) -> ProcessCommand {
    match action {
        "prCreateFill" => ProcessCommand::new("gh", vec!["pr", "create", "--fill"], cwd),
        "prView" => ProcessCommand::new(
            "gh",
            vec!["pr", "view", "--json", "number,state,title,url"],
            cwd,
        ),
        "version" => ProcessCommand::new("gh", vec!["--version"], cwd),
        _ => unreachable!("validated GitHub action"),
    }
}

pub(crate) enum PullRequestProbe {
    Failed,
    Invalid,
    NotOpen,
    Open(Value),
}

pub(crate) async fn probe_open_github_pull_request(
    context: &TypedOperationContext,
) -> Result<PullRequestProbe, TypedOperationError> {
    let command = ProcessCommand::new(
        "gh",
        vec!["pr", "view", "--json", "number,state,url"],
        context.cwd.as_str(),
    );
    let output = match run_process_command(&command, context).await {
        Ok(output) => output,
        Err(_) => return Ok(PullRequestProbe::Failed),
    };
    if output.exit_code != 0 || output.error.is_some() || output.stdout.trim().is_empty() {
        return Ok(PullRequestProbe::Failed);
    }
    let Some(pr) = parse_github_pull_request_summary(&output.stdout) else {
        return Ok(PullRequestProbe::Invalid);
    };
    if pr.get("state").and_then(Value::as_str) == Some("open") {
        Ok(PullRequestProbe::Open(pr))
    } else {
        Ok(PullRequestProbe::NotOpen)
    }
}

fn parse_github_pull_request_summary(stdout: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(stdout.trim()).ok()?;
    let object = value.as_object()?;
    let state_value = object.get("state")?.as_str()?.to_ascii_lowercase();
    let state = match state_value.as_str() {
        "open" => "open",
        "closed" => "closed",
        "merged" => "merged",
        _ => return None,
    };
    let url = validate_github_pull_request_url(object.get("url")?.as_str()?)?;
    let number = object
        .get("number")
        .and_then(Value::as_u64)
        .or_else(|| github_pull_request_number_from_url(&url));
    let mut summary = Map::new();
    if let Some(number) = number {
        summary.insert("number".to_string(), json!(number));
    }
    summary.insert("state".to_string(), json!(state));
    summary.insert("url".to_string(), json!(url));
    Some(Value::Object(summary))
}

fn validate_github_pull_request_url(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 2048
        || trimmed.contains('\\')
        || trimmed
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return None;
    }
    let parsed = Url::parse(trimmed).ok()?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let segments = parsed.path_segments()?.collect::<Vec<_>>();
    if segments.len() != 4
        || segments[0].is_empty()
        || segments[1].is_empty()
        || segments[2] != "pull"
        || segments[3].is_empty()
        || !segments[3].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(parsed.as_str().to_string())
}

fn github_pull_request_number_from_url(url: &str) -> Option<u64> {
    Url::parse(url)
        .ok()?
        .path_segments()?
        .nth(3)?
        .parse::<u64>()
        .ok()
}

pub(crate) fn github_pull_request_success(created: bool, pr: Value) -> Value {
    json!({
        "created": created,
        "ok": true,
        "pr": pr,
    })
}

pub(crate) fn github_pull_request_failure(reason: &str) -> Value {
    json!({
        "created": false,
        "ok": false,
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
