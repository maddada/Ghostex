//! One ticket's details for the Work page (`/api/readWorkItem`): the Linear issue or GitHub issue
//! with its comments, the PR with each check by name and its review state, the videos and links
//! people attached, the sessions on this computer that link it, and where it is in the team flow.
//! Blocking (Linear and `gh` are network calls); the route runs it on a blocking worker with a
//! time limit, and every source is read at the same time.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;
use crate::session_git_status::{gh_cli_is_available, run_gh_command};
use crate::team_sync::{team_for_page, team_ticket_facts};

use super::*;

/// Re-opening a ticket within this long shows what was read a moment ago.
const DETAIL_TTL: Duration = Duration::from_secs(45);
const MAX_COMMENTS: usize = 30;
const MAX_TEXT_CHARS: usize = 20_000;

/// The item a request names.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WorkItemRef {
    Linear(String),
    GithubIssue(u64),
    /// A PR number (as text) or URL.
    PullRequest(String),
}

impl WorkItemRef {
    pub(crate) fn from_params(params: &Map<String, Value>) -> Option<Self> {
        if let Some(identifier) = params
            .get("linearIssue")
            .and_then(Value::as_str)
            .and_then(normalize_linear_identifier)
        {
            return Some(Self::Linear(identifier));
        }
        if let Some(number) = params.get("githubIssue").and_then(|value| {
            value.as_u64().or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.trim().trim_start_matches('#').parse().ok())
            })
        }) {
            return Some(Self::GithubIssue(number));
        }
        params.get("pullRequest").and_then(|value| {
            value
                .as_u64()
                .map(|number| number.to_string())
                .or_else(|| {
                    value
                        .as_str()
                        .map(|text| text.trim().trim_start_matches('#').to_string())
                })
                .filter(|selector| !selector.is_empty())
                .map(Self::PullRequest)
        })
    }
}

struct Cached {
    value: Result<Value, String>,
    fetched_at: Instant,
}

fn detail_cache() -> &'static Mutex<HashMap<String, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reads through the detail cache.
fn cached_detail(
    key: String,
    force: bool,
    fetch: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    if !force {
        if let Some(cached) = detail_cache().lock().ok().and_then(|cache| {
            cache
                .get(&key)
                .filter(|cached| cached.fetched_at.elapsed() < DETAIL_TTL)
                .map(|cached| cached.value.clone())
        }) {
            return cached;
        }
    }
    let value = fetch();
    if let Ok(mut cache) = detail_cache().lock() {
        cache.retain(|_, cached| cached.fetched_at.elapsed() < DETAIL_TTL);
        cache.insert(
            key,
            Cached {
                value: value.clone(),
                fetched_at: Instant::now(),
            },
        );
    }
    value
}

/// Everything the details page shows. `projects` are the work-mode projects the window shows,
/// `project_id` the one the request named (or the item's own).
pub(crate) fn read_work_item(
    paths: &GxserverPaths,
    projects: &[WorkProjectInput],
    plan: &WorkFeedPlan,
    project_id: Option<&str>,
    item_ref: &WorkItemRef,
    force: bool,
) -> Value {
    let list = build_work_list(projects, plan);
    // A worktree project shares its repo with the main checkout, whose row the list holds.
    let wanted_repo = project_id
        .and_then(|id| projects.iter().find(|input| input.project_id == id))
        .and_then(|input| input.repo.clone());
    let wanted_key_matches = |item: &Value| match item_ref {
        WorkItemRef::Linear(identifier) => {
            item.get("linearIssue").and_then(Value::as_str) == Some(identifier.as_str())
        }
        WorkItemRef::GithubIssue(number) => {
            item.get("githubIssue").and_then(Value::as_u64) == Some(*number)
                && match &wanted_repo {
                    Some(repo) => {
                        item.get("key").and_then(Value::as_str)
                            == Some(format!("issue:{repo}#{number}").as_str())
                    }
                    None => project_id
                        .is_none_or(|id| item.get("projectId").and_then(Value::as_str) == Some(id)),
                }
        }
        WorkItemRef::PullRequest(selector) => {
            let number = item.pointer("/pullRequest/number").and_then(Value::as_u64);
            let url = item.pointer("/pullRequest/url").and_then(Value::as_str);
            (url == Some(selector.as_str())
                || number.is_some_and(|number| number.to_string() == *selector))
                && (selector.starts_with("http")
                    || project_id
                        .is_none_or(|id| item.get("projectId").and_then(Value::as_str) == Some(id)))
        }
    };
    let mut item = list
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find(|item| wanted_key_matches(item)))
        .cloned();
    let project = item
        .as_ref()
        .and_then(|item| item.get("projectId").and_then(Value::as_str))
        .or(project_id)
        .and_then(|id| projects.iter().find(|input| input.project_id == id))
        .or_else(|| projects.first());
    let cwd = project.and_then(|input| input.path.clone());
    let linear_key = project
        .and_then(|input| input.linear_api_key.clone())
        .or_else(|| {
            projects
                .iter()
                .find_map(|input| input.linear_api_key.clone())
        });
    let gh = gh_cli_is_available();

    // The PR to read: the item's folded one, or the request's own.
    let mut pull_request_selector = item
        .as_ref()
        .and_then(|item| {
            item.pointer("/pullRequest/url")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    item.pointer("/pullRequest/number")
                        .and_then(Value::as_u64)
                        .map(|number| number.to_string())
                })
        })
        .or_else(|| match item_ref {
            WorkItemRef::PullRequest(selector) => Some(selector.clone()),
            _ => None,
        });

    // A PR's tickets (pull_request_tickets.rs): the ones known here now, the rest once it is read.
    let mut pr_tickets = match item_ref {
        WorkItemRef::PullRequest(selector) => pull_request_tickets_known_locally(
            selector,
            project.and_then(|input| input.repo.as_deref()),
            item.as_ref(),
            projects,
        ),
        _ => Vec::new(),
    };
    // The item the ticket facts and the team's data are about: a PR's ticket when it has one.
    let ticket_ref = pr_tickets.first().cloned();
    let ticket_ref = ticket_ref.as_ref().unwrap_or(item_ref);

    // A Work workspace's team: the ticket's Slack threads, working thread and team sessions.
    let team_ticket = project.and_then(|input| {
        let repo = input.repo.as_deref().map(str::to_ascii_lowercase);
        let ticket = match ticket_ref {
            WorkItemRef::Linear(identifier) => Some(identifier.clone()),
            WorkItemRef::GithubIssue(number) => repo.map(|repo| format!("{repo}#{number}")),
            WorkItemRef::PullRequest(selector) => pull_request_url_parts(selector)
                .map(|(repo, number)| format!("{}#{number}", repo.to_ascii_lowercase()))
                .or_else(|| repo.map(|repo| format!("{repo}#{selector}"))),
        }?;
        Some((input.workspace_id.clone(), ticket))
    });

    let mut errors: Vec<String> = Vec::new();
    let mut linear: Option<Value> = None;
    let mut github_issue: Option<Value> = None;
    let mut pull_request: Option<Value> = None;
    let mut team: Option<Value> = None;

    // The Linear issue first: it can name the PR (Linear's GitHub integration).
    std::thread::scope(|scope| {
        let team_task = team_ticket.as_ref().map(|(workspace_id, ticket)| {
            scope.spawn(move || {
                crate::team_sync::read_team_ticket(paths, workspace_id, ticket, force)
            })
        });
        let linear_task = match (ticket_ref, linear_key.as_deref()) {
            (WorkItemRef::Linear(identifier), Some(api_key)) => {
                let identifier = identifier.clone();
                let api_key = api_key.to_string();
                Some(scope.spawn(move || {
                    cached_detail(
                        format!("linear:{}:{identifier}", linear_key_fingerprint(&api_key)),
                        force,
                        || fetch_linear_issue_detail(&api_key, &identifier),
                    )
                }))
            }
            _ => None,
        };
        let issue_task = match (ticket_ref, cwd.as_deref(), gh) {
            (WorkItemRef::GithubIssue(number), Some(cwd), true) => {
                let number = *number;
                let cwd = cwd.to_string();
                Some(scope.spawn(move || {
                    cached_detail(format!("issue:{cwd}#{number}"), force, || {
                        fetch_github_issue_detail(&cwd, number)
                    })
                }))
            }
            _ => None,
        };
        // Started now only when the PR is already known; otherwise after the Linear answer.
        let early_pr = pull_request_selector
            .clone()
            .filter(|_| gh)
            .map(|selector| {
                let cwd = cwd.clone().unwrap_or_default();
                scope.spawn(move || {
                    cached_detail(format!("pr:{cwd}:{selector}"), force, || {
                        fetch_pull_request_detail(&cwd, &selector)
                    })
                })
            });
        if let Some(task) = linear_task {
            match task
                .join()
                .unwrap_or_else(|_| Err("Linear read failed.".to_string()))
            {
                Ok(value) => linear = Some(value),
                Err(error) => errors.push(format!("Linear: {error}")),
            }
        }
        if let Some(task) = issue_task {
            match task
                .join()
                .unwrap_or_else(|_| Err("gh failed.".to_string()))
            {
                Ok(value) => github_issue = Some(value),
                Err(error) => errors.push(format!("GitHub: {error}")),
            }
        }
        let late_pr = if early_pr.is_none() && gh {
            pull_request_selector = linear
                .as_ref()
                .and_then(|linear| linear.get("pullRequestUrls"))
                .and_then(Value::as_array)
                .and_then(|urls| urls.first())
                .and_then(Value::as_str)
                .map(str::to_string);
            pull_request_selector.clone().map(|selector| {
                let cwd = cwd.clone().unwrap_or_default();
                scope.spawn(move || {
                    cached_detail(format!("pr:{cwd}:{selector}"), force, || {
                        fetch_pull_request_detail(&cwd, &selector)
                    })
                })
            })
        } else {
            None
        };
        if let Some(task) = early_pr.or(late_pr) {
            match task
                .join()
                .unwrap_or_else(|_| Err("gh failed.".to_string()))
            {
                Ok(value) => pull_request = Some(value),
                Err(error) => errors.push(format!("GitHub: {error}")),
            }
        }
        match team_task.and_then(|task| task.join().ok()).flatten() {
            Some(Ok(value)) => team = Some(value),
            Some(Err(error)) => errors.push(error),
            None => {}
        }
    });

    if let (WorkItemRef::PullRequest(_), Some(pr)) = (item_ref, pull_request.as_ref()) {
        let mut team_keys: Vec<String> = plan
            .linear
            .iter()
            .flat_map(|request| request.team_keys.iter().cloned())
            .collect();
        team_keys.extend(
            linear_key
                .as_deref()
                .map(linear_key_fingerprint)
                .and_then(cached_linear_team_keys)
                .unwrap_or_default(),
        );
        for ticket in pull_request_branch_tickets(pr, &team_keys) {
            push_ticket(&mut pr_tickets, ticket);
        }
        // Only adds tickets; a failed read leaves the ones found above.
        if let (Some(api_key), Some(url)) =
            (linear_key.as_deref(), pr.get("url").and_then(Value::as_str))
        {
            let attached = cached_detail(
                format!("linear-attached:{}:{url}", linear_key_fingerprint(api_key)),
                force,
                || linear_issues_attached_to_url(api_key, url).map(|ids| json!(ids)),
            );
            for identifier in attached
                .ok()
                .as_ref()
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                push_ticket(&mut pr_tickets, WorkItemRef::Linear(identifier.to_string()));
            }
        }
        // No ticket was known before the PR was read: read the first one it names now.
        if matches!(ticket_ref, WorkItemRef::PullRequest(_)) {
            match pr_tickets.first() {
                Some(WorkItemRef::Linear(identifier)) => {
                    if let Some(api_key) = linear_key.as_deref() {
                        match cached_detail(
                            format!("linear:{}:{identifier}", linear_key_fingerprint(api_key)),
                            force,
                            || fetch_linear_issue_detail(api_key, identifier),
                        ) {
                            Ok(value) => linear = Some(value),
                            Err(error) => errors.push(format!("Linear: {error}")),
                        }
                    }
                }
                Some(WorkItemRef::GithubIssue(number)) => {
                    if let (Some(cwd), true) = (cwd.as_deref(), gh) {
                        match cached_detail(format!("issue:{cwd}#{number}"), force, || {
                            fetch_github_issue_detail(cwd, *number)
                        }) {
                            Ok(value) => github_issue = Some(value),
                            Err(error) => errors.push(format!("GitHub: {error}")),
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let ticket_ref = match (ticket_ref, pr_tickets.first()) {
        (WorkItemRef::PullRequest(_), Some(ticket)) => ticket,
        _ => ticket_ref,
    };

    // A ticket the list does not have (closed, or someone else's team): its row from the details.
    if item.is_none() {
        item = synthesized_item(
            item_ref,
            project,
            linear.as_ref(),
            github_issue.as_ref(),
            pull_request.as_ref(),
        );
    }

    let mut texts: Vec<String> = Vec::new();
    let mut links: Vec<Value> = Vec::new();
    for source in [
        linear.as_ref(),
        github_issue.as_ref(),
        pull_request.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(body) = source
            .get("description")
            .or_else(|| source.get("body"))
            .and_then(Value::as_str)
        {
            texts.push(body.to_string());
        }
        if let Some(comments) = source.get("comments").and_then(Value::as_array) {
            texts.extend(
                comments
                    .iter()
                    .filter_map(|comment| comment.get("body").and_then(Value::as_str))
                    .map(str::to_string),
            );
        }
    }
    if let Some(attachments) = linear
        .as_ref()
        .and_then(|linear| linear.get("attachments"))
        .and_then(Value::as_array)
    {
        for attachment in attachments {
            if let Some(url) = attachment.get("url").and_then(Value::as_str) {
                texts.push(url.to_string());
                if !is_github_pull_request_url(url) && media_embed(url).is_none() {
                    links.push(attachment.clone());
                }
            }
        }
    }
    let media = media_in_texts(&texts);

    let local_sessions: Vec<&str> = item
        .as_ref()
        .and_then(|item| item.get("sessions"))
        .and_then(Value::as_array)
        .map(|sessions| {
            sessions
                .iter()
                .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    // Cloud sessions started here on the ticket (cloud_work.rs), by its team key and a PR's own.
    let mut cloud_keys: Vec<String> = team_ticket
        .as_ref()
        .map(|(_, ticket)| ticket.clone())
        .into_iter()
        .collect();
    if let Some((repo, number)) = pull_request
        .as_ref()
        .and_then(|pr| pr.get("url").and_then(Value::as_str))
        .and_then(pull_request_url_parts)
    {
        cloud_keys.push(format!("{}#{number}", repo.to_ascii_lowercase()));
    }
    let mut cloud_sessions: Vec<Value> = Vec::new();
    for key in &cloud_keys {
        for record in cloud_sessions_for_ticket(paths, key) {
            if !cloud_sessions
                .iter()
                .any(|known| known.get("sessionUrl") == record.get("sessionUrl"))
            {
                cloud_sessions.push(record);
            }
        }
    }
    let cloud_urls: Vec<&str> = cloud_sessions
        .iter()
        .filter_map(|record| record.get("sessionUrl").and_then(Value::as_str))
        .collect();
    let team = team.map(|team| team_for_page(team, &local_sessions, &cloud_urls));
    let mut facts = team_flow_facts(
        ticket_ref,
        local_sessions.len() + cloud_sessions.len(),
        linear.as_ref(),
        github_issue.as_ref(),
        pull_request.as_ref(),
    );
    if let Some(team) = &team {
        facts.session_count += team
            .get("sessions")
            .and_then(Value::as_array)
            .map(|sessions| {
                sessions
                    .iter()
                    .filter(|session| {
                        matches!(
                            session.get("status").and_then(Value::as_str),
                            Some("starting" | "running")
                        )
                    })
                    .count()
            })
            .unwrap_or(0);
        facts.team = Some(team_ticket_facts(team));
    }
    let (steps, source) = resolve_team_flow(
        paths,
        project.map(|input| input.project_id.as_str()),
        project.map(|input| input.workspace_id.as_str()),
    );

    json!({
        "item": item,
        "linear": linear,
        "githubIssue": github_issue,
        "pullRequest": pull_request,
        "tickets": pr_tickets.iter().map(work_item_ref_label).collect::<Vec<_>>(),
        "media": media,
        "links": links,
        "teamFlow": { "source": source, "steps": evaluate_team_flow(&steps, &facts) },
        "team": team,
        "cloudSessions": cloud_sessions,
        "cloudProviders": cloud_providers_wire(),
        "projects": list.get("projects").cloned().unwrap_or(Value::Array(Vec::new())),
        "errors": errors,
    })
}

/// A ticket's own record through the details cache (what the page just read): the Linear issue,
/// the GitHub issue or the PR, for a cloud session's task (cloud_work.rs).
pub(crate) fn work_ticket_detail(
    linear_api_key: Option<&str>,
    cwd: &str,
    ticket: &WorkTicket,
) -> Result<Value, String> {
    match ticket {
        WorkTicket::Linear(identifier) => {
            let api_key = linear_api_key.ok_or_else(|| {
                "Set a Linear API key first (Settings, or ghostex work-mode linear-key)."
                    .to_string()
            })?;
            cached_detail(
                format!("linear:{}:{identifier}", linear_key_fingerprint(api_key)),
                false,
                || fetch_linear_issue_detail(api_key, identifier),
            )
        }
        WorkTicket::Github(number) => cached_detail(format!("issue:{cwd}#{number}"), false, || {
            fetch_github_issue_detail(cwd, *number)
        }),
        WorkTicket::PullRequest(selector) => {
            cached_detail(format!("pr:{cwd}:{selector}"), false, || {
                fetch_pull_request_detail(cwd, selector)
            })
        }
    }
}

/// `SPX-1245` or `#218`, as the team flow's ticket step names it.
fn work_item_ref_label(ticket: &WorkItemRef) -> String {
    match ticket {
        WorkItemRef::Linear(identifier) => identifier.clone(),
        WorkItemRef::GithubIssue(number) => format!("#{number}"),
        WorkItemRef::PullRequest(selector) => selector.clone(),
    }
}

fn synthesized_item(
    item_ref: &WorkItemRef,
    project: Option<&WorkProjectInput>,
    linear: Option<&Value>,
    github_issue: Option<&Value>,
    pull_request: Option<&Value>,
) -> Option<Value> {
    let text = |value: Option<&Value>, key: &str| {
        value
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let mut item = json!({
        "sessions": [],
        "labels": [],
        "assignedToMe": false,
        "noTicket": false,
        "projectId": project.map(|input| input.project_id.clone()),
        "projectName": project.map(|input| input.name.clone()),
    });
    match item_ref {
        WorkItemRef::Linear(identifier) => {
            let status = linear_status(
                text(linear, "stateType").as_deref(),
                text(linear, "stateName").as_deref(),
            );
            item["key"] = json!(format!("linear:{identifier}"));
            item["kind"] = json!("linearIssue");
            item["id"] = json!(identifier);
            item["linearIssue"] = json!(identifier);
            item["title"] = json!(text(linear, "title").unwrap_or_else(|| identifier.clone()));
            item["url"] = json!(text(linear, "url"));
            item["status"] = json!({ "group": status.group, "name": status.name });
            item["branchName"] = json!(text(linear, "branchName"));
            if let Some(linear) = linear {
                for key in ["assignee", "linearProject", "cycle", "labels"] {
                    if let Some(value) = linear.get(key) {
                        item[key] = value.clone();
                    }
                }
                item["assignedToMe"] = json!(linear
                    .pointer("/assignee/isMe")
                    .and_then(Value::as_bool)
                    .unwrap_or(false));
            }
        }
        WorkItemRef::GithubIssue(number) => {
            let closed = text(github_issue, "state").as_deref() == Some("closed");
            item["key"] = json!(format!(
                "issue:{}#{number}",
                project.map(|p| p.project_id.as_str()).unwrap_or_default()
            ));
            item["kind"] = json!("githubIssue");
            item["id"] = json!(format!("#{number}"));
            item["githubIssue"] = json!(number);
            item["title"] =
                json!(text(github_issue, "title").unwrap_or_else(|| format!("Issue #{number}")));
            item["url"] = json!(text(github_issue, "url"));
            item["status"] = json!({ "group": if closed { "closed" } else { "todo" }, "name": if closed { "Closed" } else { "Open" } });
        }
        WorkItemRef::PullRequest(selector) => {
            let number = pull_request
                .and_then(|pr| pr.get("number"))
                .and_then(Value::as_u64);
            let state = text(pull_request, "state").unwrap_or_else(|| "open".to_string());
            item["key"] = json!(format!("pr:{selector}"));
            item["kind"] = json!("pullRequest");
            item["id"] = json!(number
                .map(|n| format!("#{n}"))
                .unwrap_or_else(|| selector.clone()));
            item["pullRequestRef"] = json!(selector);
            item["title"] =
                json!(text(pull_request, "title").unwrap_or_else(|| "Pull request".to_string()));
            item["url"] = json!(text(pull_request, "url"));
            item["status"] = json!({ "group": state, "name": state });
            item["noTicket"] = json!(true);
        }
    }
    Some(item)
}

fn team_flow_facts(
    item_ref: &WorkItemRef,
    sessions: usize,
    linear: Option<&Value>,
    github_issue: Option<&Value>,
    pull_request: Option<&Value>,
) -> TeamFlowFacts {
    let text = |value: Option<&Value>, key: &str| {
        value
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let ticket_id = match item_ref {
        WorkItemRef::Linear(identifier) => Some(identifier.clone()),
        WorkItemRef::GithubIssue(number) => Some(format!("#{number}")),
        WorkItemRef::PullRequest(_) => None,
    };
    let count = |key: &str| {
        pull_request
            .and_then(|pr| pr.pointer(&format!("/checksSummary/{key}")))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize
    };
    TeamFlowFacts {
        ticket_id,
        ticket_state_name: text(linear, "stateName").or_else(|| text(github_issue, "state")),
        ticket_state_type: text(linear, "stateType"),
        session_count: sessions,
        pull_request_number: pull_request
            .and_then(|pr| pr.get("number"))
            .and_then(Value::as_u64),
        pull_request_state: text(pull_request, "state"),
        pull_request_labels: pull_request
            .and_then(|pr| pr.get("labels"))
            .and_then(Value::as_array)
            .map(|labels| {
                labels
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        review_decision: text(pull_request, "reviewDecision"),
        unresolved_review_threads: pull_request
            .and_then(|pr| pr.get("unresolvedReviewThreads"))
            .and_then(Value::as_u64)
            .map(|count| count as usize),
        checks_total: count("total"),
        checks_passed: count("passed"),
        checks_failed: count("failed"),
        team: None,
    }
}

fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_string(),
    }
}

fn fetch_linear_issue_detail(api_key: &str, identifier: &str) -> Result<Value, String> {
    let body = linear_graphql(
        api_key,
        &format!(
            "query($id: String!) {{ issue(id: $id) {{ identifier title url description updatedAt createdAt branchName priorityLabel state {{ name type }} assignee {{ name displayName avatarUrl isMe }} creator {{ name displayName }} team {{ key name }} project {{ name url }} cycle {{ name number }} labels(first: 20) {{ nodes {{ name color }} }} comments(first: {MAX_COMMENTS}) {{ nodes {{ body createdAt user {{ name displayName avatarUrl }} }} }} attachments(first: 30) {{ nodes {{ title subtitle url sourceType }} }} }} }}"
        ),
        json!({ "id": identifier }),
    )?;
    let issue = body
        .pointer("/data/issue")
        .filter(|issue| issue.is_object())
        .ok_or_else(|| format!("Linear has no issue {identifier}."))?;
    let text = |pointer: &str| {
        issue
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let attachments: Vec<Value> = issue
        .pointer("/attachments/nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| {
                    let url = node.get("url").and_then(Value::as_str)?;
                    Some(json!({
                        "title": node.get("title").and_then(Value::as_str).unwrap_or(url),
                        "subtitle": node.get("subtitle"),
                        "url": url,
                        "source": node.get("sourceType"),
                    }))
                })
                .collect()
        })
        .unwrap_or_default();
    let pull_request_urls: Vec<&str> = attachments
        .iter()
        .filter_map(|attachment| attachment.get("url").and_then(Value::as_str))
        .filter(|url| is_github_pull_request_url(url))
        .collect();
    let comments: Vec<Value> = issue
        .pointer("/comments/nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .map(|node| {
                    json!({
                        "author": node.pointer("/user/displayName").or_else(|| node.pointer("/user/name")),
                        "avatarUrl": node.pointer("/user/avatarUrl"),
                        "body": cut(node.get("body").and_then(Value::as_str).unwrap_or_default(), MAX_TEXT_CHARS),
                        "createdAt": node.get("createdAt"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let assignee = issue
        .get("assignee")
        .filter(|assignee| assignee.is_object())
        .map(|assignee| {
            json!({
                "name": assignee.get("displayName").or_else(|| assignee.get("name")),
                "avatarUrl": assignee.get("avatarUrl"),
                "isMe": assignee.get("isMe").and_then(Value::as_bool).unwrap_or(false),
            })
        });
    let cycle = issue
        .get("cycle")
        .filter(|cycle| cycle.is_object())
        .map(|cycle| {
            cycle
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "Cycle {}",
                        cycle.get("number").and_then(Value::as_u64).unwrap_or(0)
                    )
                })
        });
    Ok(json!({
        "identifier": text("/identifier").unwrap_or_else(|| identifier.to_string()),
        "title": text("/title"),
        "url": text("/url"),
        "description": text("/description").map(|description| cut(&description, MAX_TEXT_CHARS)),
        "updatedAt": text("/updatedAt"),
        "createdAt": text("/createdAt"),
        "branchName": text("/branchName"),
        "priority": text("/priorityLabel"),
        "stateName": text("/state/name"),
        "stateType": text("/state/type"),
        "assignee": assignee,
        "creator": issue.pointer("/creator/displayName").or_else(|| issue.pointer("/creator/name")),
        "team": issue.get("team"),
        "linearProject": issue.get("project").filter(|project| project.is_object()),
        "cycle": cycle,
        "labels": issue.pointer("/labels/nodes").and_then(Value::as_array).map(|labels| {
            labels.iter().filter_map(|label| label.get("name").and_then(Value::as_str)).collect::<Vec<_>>()
        }),
        "comments": comments,
        "commentCount": comments.len(),
        "attachments": attachments,
        "pullRequestUrls": pull_request_urls,
    }))
}

fn fetch_github_issue_detail(cwd: &str, number: u64) -> Result<Value, String> {
    let number_text = number.to_string();
    let output = run_gh_command(
        Some(cwd),
        &[
            "issue",
            "view",
            number_text.as_str(),
            "--json",
            "number,title,url,state,body,author,assignees,labels,comments,updatedAt",
        ],
    )
    .ok_or_else(|| format!("gh could not read issue #{number}."))?;
    let issue: Value = serde_json::from_str(output.trim())
        .map_err(|_| format!("gh sent an unreadable answer for issue #{number}."))?;
    let comments: Vec<Value> = issue
        .get("comments")
        .and_then(Value::as_array)
        .map(|comments| {
            comments
                .iter()
                .rev()
                .take(MAX_COMMENTS)
                .rev()
                .map(|comment| {
                    json!({
                        "author": comment.pointer("/author/login"),
                        "body": cut(comment.get("body").and_then(Value::as_str).unwrap_or_default(), MAX_TEXT_CHARS),
                        "createdAt": comment.get("createdAt"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "number": number,
        "title": issue.get("title"),
        "url": issue.get("url"),
        "state": issue.get("state").and_then(Value::as_str).map(str::to_ascii_lowercase),
        "body": cut(issue.get("body").and_then(Value::as_str).unwrap_or_default(), MAX_TEXT_CHARS),
        "author": issue.pointer("/author/login"),
        "assignees": issue.get("assignees").and_then(Value::as_array).map(|people| {
            people.iter().filter_map(|person| person.get("login").and_then(Value::as_str)).collect::<Vec<_>>()
        }),
        "labels": issue.get("labels").and_then(Value::as_array).map(|labels| {
            labels.iter().filter_map(|label| label.get("name").and_then(Value::as_str)).collect::<Vec<_>>()
        }),
        "commentCount": issue.get("comments").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "comments": comments,
        "updatedAt": issue.get("updatedAt"),
    }))
}

fn fetch_pull_request_detail(cwd: &str, selector: &str) -> Result<Value, String> {
    let cwd = (!cwd.is_empty()).then_some(cwd);
    let output = run_gh_command(
        cwd,
        &[
            "pr",
            "view",
            selector,
            "--json",
            "number,title,url,state,isDraft,body,author,headRefName,baseRefName,labels,reviewDecision,latestReviews,statusCheckRollup,updatedAt,additions,deletions,changedFiles",
        ],
    )
    .ok_or_else(|| format!("gh could not read pull request {selector}."))?;
    let pr: Value = serde_json::from_str(output.trim())
        .map_err(|_| format!("gh sent an unreadable answer for pull request {selector}."))?;
    let is_draft = pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false);
    let state = pr
        .get("state")
        .and_then(Value::as_str)
        .and_then(|state| crate::session_git_status::parse_gh_pull_request_state(state, is_draft))
        .map(|state| state.as_wire())
        .unwrap_or("open");
    let checks: Vec<Value> = pr
        .get("statusCheckRollup")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(check_row).collect())
        .unwrap_or_default();
    let summary = |status: &str| {
        checks
            .iter()
            .filter(|check| check["status"] == status)
            .count()
    };
    let reviews = pr.get("latestReviews").and_then(Value::as_array);
    let review_count = |state: &str| {
        reviews
            .map(|reviews| {
                reviews
                    .iter()
                    .filter(|review| review.get("state").and_then(Value::as_str) == Some(state))
                    .count()
            })
            .unwrap_or(0)
    };
    let url = pr
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let unresolved = pull_request_url_parts(&url)
        .and_then(|(repo, number)| unresolved_review_threads(&repo, number));
    Ok(json!({
        "number": pr.get("number"),
        "title": pr.get("title"),
        "url": url,
        "state": state,
        "body": cut(pr.get("body").and_then(Value::as_str).unwrap_or_default(), MAX_TEXT_CHARS),
        "author": pr.pointer("/author/login"),
        "headBranch": pr.get("headRefName"),
        "baseBranch": pr.get("baseRefName"),
        "labels": pr.get("labels").and_then(Value::as_array).map(|labels| {
            labels.iter().filter_map(|label| label.get("name").and_then(Value::as_str)).collect::<Vec<_>>()
        }),
        "reviewDecision": pr.get("reviewDecision").and_then(Value::as_str).filter(|decision| !decision.is_empty()),
        "reviews": {
            "approved": review_count("APPROVED"),
            "changesRequested": review_count("CHANGES_REQUESTED"),
            "commented": review_count("COMMENTED"),
        },
        "unresolvedReviewThreads": unresolved,
        "checks": checks,
        "checksSummary": {
            "total": checks.len(),
            "passed": summary("passed"),
            "failed": summary("failed"),
            "pending": summary("pending"),
            "skipped": summary("skipped"),
        },
        "additions": pr.get("additions"),
        "deletions": pr.get("deletions"),
        "changedFiles": pr.get("changedFiles"),
        "updatedAt": pr.get("updatedAt"),
    }))
}

/// One check, named the way GitHub names it: a check run (`name`, `conclusion`) or a commit
/// status (`context`, `state`).
fn check_row(item: &Value) -> Value {
    let upper = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase()
    };
    let conclusion = upper("conclusion");
    let state = upper("state");
    let row_status = if matches!(
        conclusion.as_str(),
        "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE"
    ) || matches!(state.as_str(), "FAILURE" | "ERROR")
    {
        "failed"
    } else if matches!(conclusion.as_str(), "SKIPPED" | "NEUTRAL" | "STALE") {
        "skipped"
    } else if conclusion == "SUCCESS" || state == "SUCCESS" {
        "passed"
    } else {
        "pending"
    };
    let name = item
        .get("name")
        .or_else(|| item.get("context"))
        .and_then(Value::as_str)
        .unwrap_or("check");
    let duration_seconds = match (
        item.get("startedAt")
            .and_then(Value::as_str)
            .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok()),
        item.get("completedAt")
            .and_then(Value::as_str)
            .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok()),
    ) {
        (Some(started), Some(completed)) if completed > started => {
            Some((completed - started).num_seconds())
        }
        _ => None,
    };
    json!({
        "name": name,
        "workflow": item.get("workflowName").and_then(Value::as_str).filter(|name| !name.is_empty()),
        "status": row_status,
        "url": item.get("detailsUrl").or_else(|| item.get("targetUrl")).and_then(Value::as_str),
        "durationSeconds": duration_seconds,
    })
}

/// Review threads nobody resolved yet, through GitHub's GraphQL API (`gh api graphql`).
fn unresolved_review_threads(repo: &str, number: u64) -> Option<usize> {
    let (owner, name) = repo.split_once('/')?;
    let owner_arg = format!("owner={owner}");
    let name_arg = format!("name={name}");
    let number_arg = format!("number={number}");
    let output = run_gh_command(
        None,
        &[
            "api",
            "graphql",
            "-f",
            "query=query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100) { nodes { isResolved } } } } }",
            "-f",
            owner_arg.as_str(),
            "-f",
            name_arg.as_str(),
            "-F",
            number_arg.as_str(),
        ],
    )?;
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    let threads = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")?
        .as_array()?;
    Some(
        threads
            .iter()
            .filter(|thread| thread.get("isResolved").and_then(Value::as_bool) == Some(false))
            .count(),
    )
}

/// The embeddable player for a Loom, YouTube or video-file link.
pub(crate) fn media_embed(url: &str) -> Option<Value> {
    let trimmed = url.trim_end_matches(['.', ',', ')', ']', '>']);
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.split_once("loom.com/share/").map(|(_, rest)| rest) {
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !id.is_empty() {
            return Some(
                json!({ "kind": "loom", "url": trimmed, "embedUrl": format!("https://www.loom.com/embed/{id}") }),
            );
        }
    }
    let youtube_id = if let Some(rest) = trimmed.split_once("youtu.be/").map(|(_, rest)| rest) {
        Some(rest)
    } else if lower.contains("youtube.com/") {
        trimmed
            .split_once("v=")
            .map(|(_, rest)| rest)
            .or_else(|| trimmed.split_once("/shorts/").map(|(_, rest)| rest))
    } else {
        None
    }
    .map(|rest| {
        rest.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>()
    })
    .filter(|id| id.len() >= 6);
    if let Some(id) = youtube_id {
        return Some(
            json!({ "kind": "youtube", "url": trimmed, "embedUrl": format!("https://www.youtube-nocookie.com/embed/{id}") }),
        );
    }
    let path = lower.split(['?', '#']).next().unwrap_or_default();
    if [".mp4", ".webm", ".mov", ".m4v"]
        .iter()
        .any(|ext| path.ends_with(ext))
    {
        return Some(json!({ "kind": "video", "url": trimmed, "embedUrl": trimmed }));
    }
    None
}

/// Every video link in these texts, once each, in the order they appear.
fn media_in_texts(texts: &[String]) -> Vec<Value> {
    let mut seen = Vec::new();
    let mut media = Vec::new();
    for text in texts {
        for word in
            text.split(|c: char| c.is_whitespace() || c == '(' || c == '<' || c == '"' || c == '\'')
        {
            let Some(start) = word.find("https://").or_else(|| word.find("http://")) else {
                continue;
            };
            let url = &word[start..];
            if let Some(embed) = media_embed(url) {
                let key = embed["embedUrl"].as_str().unwrap_or_default().to_string();
                if !seen.contains(&key) {
                    seen.push(key);
                    media.push(embed);
                }
            }
        }
    }
    media
}
