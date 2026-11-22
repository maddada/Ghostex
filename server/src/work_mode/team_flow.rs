//! A team's flow: the steps a ticket goes through (ticket, Slack thread, session, PR, video,
//! review comments, CI, QC package, validation), each with the rule that says it is done. The
//! steps are data stored per project or per workspace, with the user's team flow as the default,
//! and the Work page's ticket details draw them as a tracker. A workspace connected to a team uses
//! the team's steps from its Convex project instead (crate::team_sync::read_team_flow_steps).
//!
//! CDXC:WorkMode 2026-10-12 DECISION:
//! User: the team-flow steps on each ticket are editable in Team flow settings, with "our flow as
//! default", now in the order ticket, Slack thread, session, PR, video, review comments, CI, QC
//! package, validation ("move video before review comments"). The working-thread step is called
//! "Slack thread" and is optional ("make it optinoal step (question mark, gray) / we dont always
//! have a working thread"): an optional step that is not done shows as unknown and the tracker's
//! next step skips it. Its id stays `working-thread` so saved flows keep matching. Supersedes the
//! 2026-10-09 order (video after CI) and the "Working thread" label.
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! A step whose data Ghostex does not have is "unknown", never "done", so the tracker cannot claim
//! work happened that nobody can see: the video approval always, and the Slack working thread and
//! validation post when the project's workspace has no team (Convex) connection. With one, those
//! two come from the team's Convex project (crate::team_sync::work_page).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

const TEAM_FLOWS_FILE_NAME: &str = "work-team-flows.json";
const MAX_STEPS: usize = 20;
const MAX_LABEL_CHARS: usize = 40;

/// Every rule a step can use. `needsData` rules read sources Ghostex does not have yet, so they
/// evaluate to "unknown".
pub(crate) const TEAM_FLOW_RULES: &[(&str, &str, bool)] = &[
    ("ticketExists", "A Linear or GitHub ticket exists", false),
    ("slackWorkingThread", "A Slack working thread exists", true),
    ("sessionLinked", "A session is linked to the ticket", false),
    ("pullRequestExists", "A pull request exists", false),
    (
        "reviewCommentsResolved",
        "Every review comment is resolved",
        false,
    ),
    ("pullRequestApproved", "The pull request is approved", false),
    ("checksPassing", "Every check passes", false),
    ("pullRequestLabel", "The pull request has a label", false),
    ("pullRequestMerged", "The pull request is merged", false),
    ("ticketState", "The ticket reached a state", false),
    ("videoApproved", "The demo video is approved", true),
    (
        "slackValidationPost",
        "The validation request is posted in Slack",
        true,
    ),
];

/// The user's own team flow, from their Slack bot's rules.
pub(crate) fn default_team_flow_steps() -> Value {
    json!([
        { "id": "ticket", "label": "Ticket", "rule": { "kind": "ticketExists" } },
        { "id": "working-thread", "label": "Slack thread", "optional": true, "rule": { "kind": "slackWorkingThread" } },
        { "id": "session", "label": "Session", "rule": { "kind": "sessionLinked" } },
        { "id": "pr", "label": "PR", "rule": { "kind": "pullRequestExists" } },
        { "id": "video", "label": "Video", "rule": { "kind": "videoApproved" } },
        { "id": "review-comments", "label": "Review comments", "rule": { "kind": "reviewCommentsResolved" } },
        { "id": "ci", "label": "CI", "rule": { "kind": "checksPassing" } },
        { "id": "qc-package", "label": "QC package", "rule": { "kind": "pullRequestLabel", "label": "READY-FOR-QC" } },
        { "id": "validation", "label": "Validation", "rule": { "kind": "slackValidationPost" } }
    ])
}

fn flows_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_config_dir.join(TEAM_FLOWS_FILE_NAME)
}

fn read_flows(paths: &GxserverPaths) -> Map<String, Value> {
    fs::read_to_string(flows_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_flows(paths: &GxserverPaths, flows: &Map<String, Value>) -> std::io::Result<()> {
    fs::create_dir_all(&paths.app_config_dir)?;
    let path = flows_path(paths);
    let temp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(Value::Object(flows.clone()).to_string().as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, &path)
}

/// Where a flow is stored: one project, one workspace, or the default every other project uses.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TeamFlowScope {
    Project(String),
    Workspace(String),
    Default,
}

impl TeamFlowScope {
    pub(crate) fn from_params(params: &Map<String, Value>) -> Self {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        match (
            text("scope").as_deref(),
            text("projectId"),
            text("workspaceId"),
        ) {
            (Some("default"), _, _) => Self::Default,
            (Some("workspace"), _, Some(workspace)) | (None, None, Some(workspace)) => {
                Self::Workspace(workspace)
            }
            (_, Some(project), _) => Self::Project(project),
            _ => Self::Default,
        }
    }

    fn slot(&self) -> Option<(&'static str, &str)> {
        match self {
            Self::Project(id) => Some(("projects", id)),
            Self::Workspace(id) => Some(("workspaces", id)),
            Self::Default => None,
        }
    }
}

/// The steps a project uses and where they came from: its team's (`team`, or `builtIn` while the
/// team has none), else its own, its workspace's, the stored default, or the built-in default.
/// A team that does not answer shows the built-in default as `teamError`.
pub(crate) fn resolve_team_flow(
    paths: &GxserverPaths,
    project_id: Option<&str>,
    workspace_id: Option<&str>,
) -> (Value, &'static str) {
    if let Some(team) =
        workspace_id.and_then(|id| crate::team_sync::read_team_flow_steps(paths, id))
    {
        return match team {
            Ok(crate::team_sync::TeamFlowSteps {
                steps: Some(steps), ..
            }) => (steps, "team"),
            Ok(_) => (default_team_flow_steps(), "builtIn"),
            Err(_) => (default_team_flow_steps(), "teamError"),
        };
    }
    let flows = read_flows(paths);
    let stored = |group: &str, id: &str| {
        flows
            .get(group)
            .and_then(|group| group.get(id))
            .and_then(|flow| flow.get("steps"))
            .filter(|steps| steps.as_array().is_some_and(|steps| !steps.is_empty()))
            .cloned()
    };
    if let Some(steps) = project_id.and_then(|id| stored("projects", id)) {
        return (steps, "project");
    }
    if let Some(steps) = workspace_id.and_then(|id| stored("workspaces", id)) {
        return (steps, "workspace");
    }
    if let Some(steps) = flows
        .get("default")
        .and_then(|flow| flow.get("steps"))
        .filter(|steps| steps.as_array().is_some_and(|steps| !steps.is_empty()))
    {
        return (steps.clone(), "default");
    }
    (default_team_flow_steps(), "builtIn")
}

/// The rule catalog the settings page offers.
pub(crate) fn team_flow_rule_catalog() -> Value {
    Value::Array(
        TEAM_FLOW_RULES
            .iter()
            .map(|(kind, description, needs_data)| {
                json!({ "kind": kind, "description": description, "needsData": needs_data })
            })
            .collect(),
    )
}

/// Checks and normalizes steps a client sends: known rules, short labels, unique ids.
pub(crate) fn validate_team_flow_steps(steps: &Value) -> Result<Value, DomainStateError> {
    let steps = steps
        .as_array()
        .ok_or_else(|| DomainStateError::bad_request("Pass steps as a list."))?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(DomainStateError::bad_request(format!(
            "A team flow has 1 to {MAX_STEPS} steps."
        )));
    }
    let mut ids = Vec::new();
    let mut output = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let label = step
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .ok_or_else(|| {
                DomainStateError::bad_request(format!("Step {} has no label.", index + 1))
            })?;
        if label.chars().count() > MAX_LABEL_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "\"{label}\" is longer than {MAX_LABEL_CHARS} characters."
            )));
        }
        let rule = step
            .get("rule")
            .and_then(Value::as_object)
            .ok_or_else(|| DomainStateError::bad_request(format!("\"{label}\" has no rule.")))?;
        let kind = rule.get("kind").and_then(Value::as_str).unwrap_or_default();
        if !TEAM_FLOW_RULES.iter().any(|(known, _, _)| *known == kind) {
            return Err(DomainStateError::bad_request(format!(
                "\"{label}\" uses an unknown rule \"{kind}\"."
            )));
        }
        let mut clean_rule = Map::new();
        clean_rule.insert("kind".to_string(), json!(kind));
        match kind {
            "pullRequestLabel" => {
                let wanted = rule
                    .get("label")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|wanted| !wanted.is_empty())
                    .ok_or_else(|| {
                        DomainStateError::bad_request(format!(
                            "\"{label}\" needs the PR label to look for."
                        ))
                    })?;
                clean_rule.insert("label".to_string(), json!(wanted));
            }
            "ticketState" => {
                let states: Vec<String> = rule
                    .get("states")
                    .and_then(Value::as_array)
                    .map(|states| {
                        states
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|state| state.trim().to_string())
                            .filter(|state| !state.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                if states.is_empty() {
                    return Err(DomainStateError::bad_request(format!(
                        "\"{label}\" needs the ticket states that count as done."
                    )));
                }
                clean_rule.insert("states".to_string(), json!(states));
            }
            _ => {}
        }
        let mut id = step
            .get("id")
            .and_then(Value::as_str)
            .map(slug)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| slug(label));
        if id.is_empty() || ids.contains(&id) {
            id = format!("step-{}", index + 1);
        }
        ids.push(id.clone());
        let mut clean_step = json!({ "id": id, "label": label, "rule": Value::Object(clean_rule) });
        if step.get("optional").and_then(Value::as_bool) == Some(true) {
            clean_step["optional"] = json!(true);
        }
        output.push(clean_step);
    }
    Ok(Value::Array(output))
}

fn slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').chars().take(40).collect()
}

/// Saves a scope's steps, or with `None` removes them so the scope falls back again.
pub(crate) fn store_team_flow(
    paths: &GxserverPaths,
    scope: &TeamFlowScope,
    steps: Option<Value>,
) -> std::io::Result<()> {
    let mut flows = read_flows(paths);
    flows.insert("version".to_string(), json!(1));
    match scope.slot() {
        Some((group, id)) => {
            let mut entries = flows
                .get(group)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            match steps {
                Some(steps) => {
                    entries.insert(id.to_string(), json!({ "steps": steps }));
                }
                None => {
                    entries.remove(id);
                }
            }
            flows.insert(group.to_string(), Value::Object(entries));
        }
        None => match steps {
            Some(steps) => {
                flows.insert("default".to_string(), json!({ "steps": steps }));
            }
            None => {
                flows.remove("default");
            }
        },
    }
    write_flows(paths, &flows)
}

/// What the rules read about one ticket. `None` means Ghostex could not find out.
#[derive(Clone, Debug, Default)]
pub(crate) struct TeamFlowFacts {
    pub(crate) ticket_id: Option<String>,
    pub(crate) ticket_state_name: Option<String>,
    pub(crate) ticket_state_type: Option<String>,
    pub(crate) session_count: usize,
    pub(crate) pull_request_number: Option<u64>,
    /// `open`, `draft`, `merged` or `closed`.
    pub(crate) pull_request_state: Option<String>,
    pub(crate) pull_request_labels: Vec<String>,
    pub(crate) review_decision: Option<String>,
    /// Unresolved review threads; `None` when they could not be read.
    pub(crate) unresolved_review_threads: Option<usize>,
    pub(crate) checks_total: usize,
    pub(crate) checks_passed: usize,
    pub(crate) checks_failed: usize,
    /// What the team's Convex project knows; `None` without a team connection.
    pub(crate) team: Option<TeamTicketFacts>,
}

/// The Slack facts a Work workspace's team records for a ticket.
#[derive(Clone, Debug, Default)]
pub(crate) struct TeamTicketFacts {
    pub(crate) has_working_thread: bool,
    /// `#kevin-the-bot` when Slack named the channel.
    pub(crate) working_thread_channel: Option<String>,
    pub(crate) working_thread_url: Option<String>,
    pub(crate) validation_posted: bool,
    pub(crate) validation_detail: Option<String>,
    pub(crate) validation_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TeamFlowStepState {
    pub(crate) id: String,
    pub(crate) label: String,
    /// `done`, `current`, `pending`, `failed` or `unknown`.
    pub(crate) status: &'static str,
    pub(crate) detail: String,
    /// The step may be skipped: when it is not done it shows as unknown, never as the next step.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) optional: bool,
    /// Where the step's evidence opens (the working thread, the validation thread).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
}

/// Evaluates every step; the first step that is neither done nor unknown is the current one. An
/// optional step that is not done is unknown, so it never holds up the flow.
pub(crate) fn evaluate_team_flow(steps: &Value, facts: &TeamFlowFacts) -> Vec<TeamFlowStepState> {
    let mut states: Vec<TeamFlowStepState> = steps
        .as_array()
        .map(|steps| steps.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|step| {
            let id = step.get("id").and_then(Value::as_str)?.to_string();
            let label = step.get("label").and_then(Value::as_str)?.to_string();
            let rule = step.get("rule").cloned().unwrap_or(Value::Null);
            let optional = step.get("optional").and_then(Value::as_bool) == Some(true);
            let (status, detail) = match evaluate_rule(&rule, facts) {
                ("done", detail) => ("done", detail),
                (_, detail) if optional => ("unknown", detail),
                outcome => outcome,
            };
            Some(TeamFlowStepState {
                id,
                label,
                status,
                detail,
                optional,
                url: rule_url(&rule, facts),
            })
        })
        .collect();
    if let Some(current) = states
        .iter_mut()
        .find(|state| matches!(state.status, "pending" | "failed"))
    {
        if current.status == "pending" {
            current.status = "current";
        }
    }
    states
}

fn evaluate_rule(rule: &Value, facts: &TeamFlowFacts) -> (&'static str, String) {
    let has_pr = facts.pull_request_number.is_some();
    let pr_text = facts
        .pull_request_number
        .map(|number| format!("#{number}"))
        .unwrap_or_default();
    match rule.get("kind").and_then(Value::as_str).unwrap_or_default() {
        "ticketExists" => match &facts.ticket_id {
            Some(id) => ("done", id.clone()),
            None => ("pending", "no ticket".to_string()),
        },
        "sessionLinked" => match facts.session_count {
            0 => ("pending", "none yet".to_string()),
            1 => ("done", "1 session".to_string()),
            count => ("done", format!("{count} sessions")),
        },
        "pullRequestExists" => {
            if has_pr {
                ("done", pr_text)
            } else {
                ("pending", "not opened".to_string())
            }
        }
        "reviewCommentsResolved" => match (has_pr, facts.unresolved_review_threads) {
            (false, _) => ("pending", "after PR".to_string()),
            (true, Some(0)) => ("done", "0 open".to_string()),
            (true, Some(count)) => ("pending", format!("{count} open")),
            (true, None) => ("unknown", "couldn't read".to_string()),
        },
        "pullRequestApproved" => match (has_pr, facts.review_decision.as_deref()) {
            (false, _) => ("pending", "after PR".to_string()),
            (true, Some("APPROVED")) => ("done", "approved".to_string()),
            (true, Some("CHANGES_REQUESTED")) => ("failed", "changes asked".to_string()),
            (true, _) => ("pending", "waiting".to_string()),
        },
        "checksPassing" => {
            if !has_pr {
                ("pending", "after PR".to_string())
            } else if facts.checks_total == 0 {
                ("unknown", "no checks".to_string())
            } else if facts.checks_failed > 0 {
                ("failed", format!("{} failing", facts.checks_failed))
            } else if facts.checks_passed == facts.checks_total {
                (
                    "done",
                    format!("{} / {}", facts.checks_passed, facts.checks_total),
                )
            } else {
                (
                    "pending",
                    format!("{} / {}", facts.checks_passed, facts.checks_total),
                )
            }
        }
        "pullRequestLabel" => {
            let wanted = rule
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !has_pr {
                ("pending", "after PR".to_string())
            } else if facts
                .pull_request_labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case(wanted))
            {
                ("done", wanted.to_string())
            } else {
                ("pending", format!("no {wanted}"))
            }
        }
        "pullRequestMerged" => match facts.pull_request_state.as_deref() {
            Some("merged") => ("done", "merged".to_string()),
            Some(_) => ("pending", "not merged".to_string()),
            None => ("pending", "after PR".to_string()),
        },
        "ticketState" => {
            let states: Vec<&str> = rule
                .get("states")
                .and_then(Value::as_array)
                .map(|states| states.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let reached = [
                facts.ticket_state_name.as_deref(),
                facts.ticket_state_type.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|state| {
                states
                    .iter()
                    .any(|wanted| wanted.eq_ignore_ascii_case(state))
            });
            match (&facts.ticket_state_name, reached) {
                (_, true) => ("done", facts.ticket_state_name.clone().unwrap_or_default()),
                (Some(name), false) => ("pending", name.clone()),
                (None, false) => ("unknown", String::new()),
            }
        }
        "slackWorkingThread" => match &facts.team {
            Some(team) if team.has_working_thread => (
                "done",
                team.working_thread_channel
                    .clone()
                    .unwrap_or_else(|| "open".to_string()),
            ),
            Some(_) => ("pending", "none yet".to_string()),
            None => ("unknown", "not connected".to_string()),
        },
        "slackValidationPost" => match &facts.team {
            Some(team) if team.validation_posted => (
                "done",
                team.validation_detail
                    .clone()
                    .unwrap_or_else(|| "posted".to_string()),
            ),
            Some(_) => ("pending", "not posted".to_string()),
            None => ("unknown", "not connected".to_string()),
        },
        // Nothing records the video approval yet; nobody can tell.
        "videoApproved" => ("unknown", "not recorded".to_string()),
        _ => ("unknown", String::new()),
    }
}

fn rule_url(rule: &Value, facts: &TeamFlowFacts) -> Option<String> {
    let team = facts.team.as_ref()?;
    match rule.get("kind").and_then(Value::as_str)? {
        "slackWorkingThread" => team.working_thread_url.clone(),
        "slackValidationPost" => team.validation_url.clone(),
        _ => None,
    }
}
