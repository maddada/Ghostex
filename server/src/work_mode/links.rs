//! The links a person set by hand on a session, stored as `runtimeSettings.workLinks`.
//!
//! A key that is present overrides what the branch says; a key that is absent leaves that kind
//! of link automatic. So `linearIssues: []` means "no Linear issue, whatever the branch says",
//! while no `linearIssues` key means "whatever the branch says".

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::{parse_github_project_reference, WorkSourceId};

/// A session's hand-set links. `None` = automatic, `Some(empty)` = explicitly none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ManualWorkLinks {
    pub(crate) pull_request: Option<Option<ManualPullRequest>>,
    pub(crate) linear_issues: Option<Vec<String>>,
    pub(crate) github_issues: Option<Vec<u64>>,
    pub(crate) linear_project: Option<Option<String>>,
    /// A GitHub Project as `owner/number`.
    pub(crate) github_project: Option<Option<String>>,
}

impl ManualWorkLinks {
    /// One source's hand-set ticket ids; `None` = automatic.
    pub(crate) fn ticket_ids(&self, source: WorkSourceId) -> Option<Vec<String>> {
        match source {
            WorkSourceId::Linear => self.linear_issues.clone(),
            WorkSourceId::Github => self
                .github_issues
                .as_ref()
                .map(|numbers| numbers.iter().map(u64::to_string).collect()),
        }
    }
}

/// The request key that adds tickets of a source to the ones a session already links.
pub(crate) fn added_work_link_field(source: WorkSourceId) -> &'static str {
    match source {
        WorkSourceId::Linear => "addLinearIssues",
        WorkSourceId::Github => "addGithubIssues",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ManualPullRequest {
    Number(u64),
    Url(String),
}

pub(crate) fn manual_work_links(session: &Value) -> ManualWorkLinks {
    let Some(links) = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("workLinks"))
        .and_then(Value::as_object)
    else {
        return ManualWorkLinks::default();
    };
    ManualWorkLinks {
        pull_request: links.get("pullRequest").map(parse_pull_request),
        linear_issues: links.get("linearIssues").map(|value| {
            value
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .filter_map(normalize_linear_identifier)
                        .collect()
                })
                .unwrap_or_default()
        }),
        github_issues: links.get("githubIssues").map(|value| {
            value
                .as_array()
                .map(|items| items.iter().filter_map(issue_number).collect())
                .unwrap_or_default()
        }),
        linear_project: links.get("linearProject").map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
        }),
        github_project: links.get("githubProject").map(|value| {
            value
                .as_str()
                .and_then(parse_github_project_reference)
                .map(|(owner, number)| format!("{owner}/{number}"))
        }),
    }
}

fn parse_pull_request(value: &Value) -> Option<ManualPullRequest> {
    if let Some(number) = issue_number(value) {
        return Some(ManualPullRequest::Number(number));
    }
    let text = value.as_str()?.trim();
    if text.starts_with("https://") || text.starts_with("http://") {
        return Some(ManualPullRequest::Url(text.to_string()));
    }
    None
}

fn issue_number(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().trim_start_matches('#').parse().ok())
        })
        .filter(|number| *number > 0)
}

/// `spx-1245` → `SPX-1245`; anything that is not `<key>-<number>` → `None`.
pub(crate) fn normalize_linear_identifier(text: &str) -> Option<String> {
    let text = text.trim();
    let (key, number) = text.split_once('-')?;
    let valid = !key.is_empty()
        && key.len() <= 10
        && key.chars().all(|c| c.is_ascii_alphanumeric())
        && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit());
    valid.then(|| format!("{}-{}", key.to_ascii_uppercase(), number))
}

/// Applies a request to the stored links. Each kind the request names is set (`null` = back to
/// automatic); `clear: true` first puts every kind back to automatic.
pub(crate) fn merge_work_links(
    existing: Option<&Map<String, Value>>,
    request: &Map<String, Value>,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut links = if request.get("clear") == Some(&Value::Bool(true)) {
        Map::new()
    } else {
        existing.cloned().unwrap_or_default()
    };
    if let Some(value) = request.get("pullRequest") {
        match value {
            Value::Null => {
                links.remove("pullRequest");
            }
            Value::String(text) if text.trim().eq_ignore_ascii_case("none") => {
                links.insert("pullRequest".to_string(), Value::Null);
            }
            other => {
                let parsed = parse_pull_request(other).ok_or_else(|| {
                    DomainStateError::bad_request("pullRequest must be a PR number or URL.")
                })?;
                links.insert(
                    "pullRequest".to_string(),
                    match parsed {
                        ManualPullRequest::Number(number) => json!(number),
                        ManualPullRequest::Url(url) => json!(url),
                    },
                );
            }
        }
    }
    if let Some(value) = request.get("linearIssues") {
        match value {
            Value::Null => {
                links.remove("linearIssues");
            }
            other => {
                let items = list_of(other);
                let mut identifiers = Vec::new();
                for item in items {
                    let text = item.as_str().unwrap_or_default();
                    let identifier = normalize_linear_identifier(text).ok_or_else(|| {
                        DomainStateError::bad_request(format!(
                            "\"{text}\" is not a Linear issue ID like SPX-1245."
                        ))
                    })?;
                    if !identifiers.contains(&identifier) {
                        identifiers.push(identifier);
                    }
                }
                links.insert("linearIssues".to_string(), json!(identifiers));
            }
        }
    }
    if let Some(value) = request.get("githubIssues") {
        match value {
            Value::Null => {
                links.remove("githubIssues");
            }
            other => {
                let mut numbers = Vec::new();
                for item in list_of(other) {
                    let number = issue_number(&item).ok_or_else(|| {
                        DomainStateError::bad_request("githubIssues must be issue numbers.")
                    })?;
                    if !numbers.contains(&number) {
                        numbers.push(number);
                    }
                }
                links.insert("githubIssues".to_string(), json!(numbers));
            }
        }
    }
    if let Some(value) = request.get("linearProject") {
        match value {
            Value::Null => {
                links.remove("linearProject");
            }
            Value::String(name) if !name.trim().is_empty() => {
                links.insert("linearProject".to_string(), json!(name.trim()));
            }
            _ => {
                links.insert("linearProject".to_string(), Value::Null);
            }
        }
    }
    // `owner/number` or the project's URL; `"none"` or `""` = explicitly none.
    if let Some(value) = request.get("githubProject") {
        match value {
            Value::Null => {
                links.remove("githubProject");
            }
            Value::String(text)
                if text.trim().is_empty() || text.trim().eq_ignore_ascii_case("none") =>
            {
                links.insert("githubProject".to_string(), Value::Null);
            }
            other => {
                let (owner, number) = other
                    .as_str()
                    .and_then(parse_github_project_reference)
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "githubProject must be owner/number (like acme/12) or the project's link.",
                        )
                    })?;
                links.insert(
                    "githubProject".to_string(),
                    json!(format!("{owner}/{number}")),
                );
            }
        }
    }
    Ok(links)
}

/// A scalar becomes a one-item list; `"none"` and `[]` become an empty list.
fn list_of(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::String(text) if text.trim().eq_ignore_ascii_case("none") => Vec::new(),
        Value::String(text) => text
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| json!(part))
            .collect(),
        other => vec![other.clone()],
    }
}

/// Writes `runtimeSettings.workLinks` in one statement.
///
/// CDXC:WorkMode 2026-10-09 WHY:
/// Hooks, titles and activity rewrite the same runtime settings JSON all the time, so the links
/// are patched in place (as agentbox does) instead of reading and rewriting the whole object.
pub(crate) fn write_work_links(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    links: &Map<String, Value>,
) -> Result<bool, DomainStateError> {
    let changed = if links.is_empty() {
        db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.workLinks') \
             WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
        )
    } else {
        db.execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.workLinks', json(?1)) \
             WHERE projectId = ?2 AND sessionId = ?3",
            params![Value::Object(links.clone()).to_string(), project_id, session_id],
        )
    }
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite work links error: {error}"),
    })?;
    Ok(changed > 0)
}
