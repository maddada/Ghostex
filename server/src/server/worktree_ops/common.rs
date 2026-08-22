use super::*;

pub(crate) fn is_allowed_git_remote_name(value: &str) -> bool {
    value.chars().enumerate().all(|(index, ch)| {
        (index == 0 && ch.is_ascii_alphanumeric())
            || (index > 0 && (ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')))
    })
}

pub(crate) fn normalize_branch_name(branch: &str) -> Option<String> {
    let value = branch.trim();
    if value.is_empty() || value == "HEAD" || value == "detached" {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn has_porcelain_status_changes(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with("##")
    })
}

/// git refuses `worktree remove` and `worktree move` on a checkout with
/// initialised submodules through the same message, so both verbs classify here.
pub(crate) fn is_submodule_worktree_refusal(result: &Value) -> bool {
    let text = format!(
        "{}\n{}",
        result
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
    );
    text.to_lowercase()
        .contains("working trees containing submodules cannot be moved or removed")
}

pub(crate) fn operation_failure_message(result: &Value, fallback: &str) -> String {
    result
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| result.get("stderr").and_then(Value::as_str).map(str::trim))
        .filter(|value| !value.is_empty())
        .or_else(|| result.get("stdout").and_then(Value::as_str).map(str::trim))
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn exit_code(result: &Value) -> i64 {
    result.get("exitCode").and_then(Value::as_i64).unwrap_or(1)
}
