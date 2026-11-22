//! Linear calls made on request rather than by the background pass: a ticket's branch name, the
//! teams and Linear projects the Create Linear ticket dialog offers, and creating the ticket.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use super::linear::{linear_graphql, project_linear_key};

/// Whether the background pass found a Linear key for this project (`PresentationProject.workLinear`).
/// A cache read, so the projection never opens the credentials file.
pub(crate) fn project_has_linear_key(project: &Value) -> bool {
    project
        .get("projectId")
        .and_then(Value::as_str)
        .and_then(project_linear_key)
        .is_some()
}

/// A ticket as Linear answers it after a create or a lookup.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearTicket {
    pub(crate) identifier: String,
    pub(crate) url: Option<String>,
    pub(crate) branch_name: Option<String>,
    pub(crate) title: Option<String>,
}

fn parse_ticket(issue: &Value) -> Option<LinearTicket> {
    let text = |key: &str| {
        issue
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(LinearTicket {
        identifier: text("identifier")?,
        url: text("url"),
        branch_name: text("branchName"),
        title: text("title"),
    })
}

/// The branch Linear suggests for an issue (`yahia/spx-1245-copy-link`), as its "Copy git
/// branch name" button gives it.
pub(crate) fn linear_issue_ticket(api_key: &str, identifier: &str) -> Result<LinearTicket, String> {
    let body = linear_graphql(
        api_key,
        "query($id: String!) { issue(id: $id) { identifier url branchName title } }",
        json!({ "id": identifier }),
    )?;
    body.pointer("/data/issue")
        .and_then(parse_ticket)
        .ok_or_else(|| format!("Linear has no issue {identifier}."))
}

/// `[{ id, key, name }]`, in Linear's order.
pub(crate) fn list_linear_teams(api_key: &str) -> Result<Vec<Value>, String> {
    let body = linear_graphql(
        api_key,
        "query { teams(first: 250) { nodes { id key name } } }",
        json!({}),
    )?;
    Ok(body
        .pointer("/data/teams/nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter(|node| node.get("id").and_then(Value::as_str).is_some())
                .map(|node| {
                    json!({
                        "id": node.get("id").cloned().unwrap_or(Value::Null),
                        "key": node.get("key").cloned().unwrap_or(Value::Null),
                        "name": node.get("name").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// The team a new ticket goes to when the dialog names none: the team whose key this project's
/// sessions use most, else Linear's first team.
pub(crate) fn default_linear_team_id(
    teams: &[Value],
    used_identifiers: &[String],
) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for identifier in used_identifiers {
        if let Some((key, _)) = identifier.split_once('-') {
            *counts.entry(key.to_ascii_uppercase()).or_default() += 1;
        }
    }
    let team_id = |team: &Value| team.get("id").and_then(Value::as_str).map(str::to_string);
    teams
        .iter()
        .filter_map(|team| {
            let key = team
                .get("key")
                .and_then(Value::as_str)?
                .to_ascii_uppercase();
            counts.get(&key).map(|count| (*count, team))
        })
        // Ties keep Linear's order: `max_by_key` takes the last maximum, so compare reversed.
        .rev()
        .max_by_key(|(count, _)| *count)
        .and_then(|(_, team)| team_id(team))
        .or_else(|| teams.first().and_then(team_id))
}

/// Open Linear projects (releases), most recently updated first: one team's when `team_id` is
/// given, else the whole workspace's. `[{ id, name }]`.
pub(crate) fn list_linear_projects(
    api_key: &str,
    team_id: Option<&str>,
) -> Result<Vec<Value>, String> {
    const FIELDS: &str = "nodes { id name completedAt canceledAt }";
    let (body, pointer) = match team_id {
        Some(team_id) => (
            linear_graphql(
                api_key,
                &format!(
                    "query($teamId: String!) {{ team(id: $teamId) {{ projects(first: 100, orderBy: updatedAt) {{ {FIELDS} }} }} }}"
                ),
                json!({ "teamId": team_id }),
            )?,
            "/data/team/projects/nodes",
        ),
        None => (
            linear_graphql(
                api_key,
                &format!("query {{ projects(first: 100, orderBy: updatedAt) {{ {FIELDS} }} }}"),
                json!({}),
            )?,
            "/data/projects/nodes",
        ),
    };
    Ok(body
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter(|node| {
                    node.get("completedAt").is_none_or(Value::is_null)
                        && node.get("canceledAt").is_none_or(Value::is_null)
                })
                .filter(|node| node.get("id").and_then(Value::as_str).is_some())
                .map(|node| {
                    json!({
                        "id": node.get("id").cloned().unwrap_or(Value::Null),
                        "name": node.get("name").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// What the Create Linear ticket dialog sends.
pub(crate) struct NewLinearTicket<'a> {
    pub(crate) team_id: &'a str,
    pub(crate) title: &'a str,
    pub(crate) description: Option<&'a str>,
    pub(crate) linear_project_id: Option<&'a str>,
    pub(crate) assign_to_me: bool,
}

/// `issueCreate`; "assign to me" first asks Linear who the key belongs to.
pub(crate) fn create_linear_issue(
    api_key: &str,
    ticket: &NewLinearTicket<'_>,
) -> Result<LinearTicket, String> {
    let mut input = Map::new();
    input.insert("teamId".to_string(), json!(ticket.team_id));
    input.insert("title".to_string(), json!(ticket.title));
    if let Some(description) = ticket.description {
        input.insert("description".to_string(), json!(description));
    }
    if let Some(project_id) = ticket.linear_project_id {
        input.insert("projectId".to_string(), json!(project_id));
    }
    if ticket.assign_to_me {
        let viewer = linear_graphql(api_key, "query { viewer { id } }", json!({}))?;
        if let Some(viewer_id) = viewer.pointer("/data/viewer/id").and_then(Value::as_str) {
            input.insert("assigneeId".to_string(), json!(viewer_id));
        }
    }
    let body = linear_graphql(
        api_key,
        "mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { identifier url branchName title } } }",
        json!({ "input": Value::Object(input) }),
    )?;
    if body.pointer("/data/issueCreate/success") != Some(&Value::Bool(true)) {
        return Err("Linear did not create the ticket.".to_string());
    }
    body.pointer("/data/issueCreate/issue")
        .and_then(parse_ticket)
        .ok_or_else(|| "Linear created the ticket but did not say which one.".to_string())
}
