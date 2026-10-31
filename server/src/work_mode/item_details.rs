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

/// The item a request names: a source's ticket, or a PR (the code host's).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WorkItemRef {
    Ticket(TicketRef),
    /// A PR number (as text) or URL.
    PullRequest(String),
}

impl WorkItemRef {
    /// `source` + `ticketId`, or `pullRequest`.
    pub(crate) fn from_params(params: &Map<String, Value>) -> Result<Self, String> {
        if let Some(ticket) = TicketRef::from_params(params) {
            return ticket.map(Self::Ticket);
        }
        let Some(value) = params.get("pullRequest").filter(|value| !value.is_null()) else {
            return Err(
                "Pass source and ticketId (linear SPX-1245, github 218) or pullRequest (412 or its URL)."
                    .to_string(),
            );
        };
        value
            .as_u64()
            .map(|number| number.to_string())
            .or_else(|| {
                let text = value.as_str()?.trim().trim_start_matches('#');
                let valid = text.parse::<u64>().is_ok_and(|number| number > 0)
                    || (text.starts_with("https://") && text.contains("/pull/"));
                valid.then(|| text.to_string())
            })
            .map(Self::PullRequest)
            .ok_or_else(|| "pullRequest must be a PR number or URL.".to_string())
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
pub(crate) fn cached_detail(
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

/// The scope a source reads the details with, in the project's checkout: the project's own, or,
/// for a source that borrows credentials (`borrows_other_projects_credentials`) and cannot read
/// with the project's (no Linear key of its own), the first project's it can read with.
pub(crate) fn source_scope<'a>(
    rules: &dyn WorkSource,
    project: Option<&'a WorkProjectInput>,
    projects: &'a [WorkProjectInput],
    cwd: Option<&'a str>,
) -> TicketScope<'a> {
    let in_checkout = |owner: &'a WorkProjectInput| TicketScope {
        cwd,
        ..owner.ticket_scope()
    };
    match project {
        Some(own) if rules.borrows_other_projects_credentials() => project
            .into_iter()
            .chain(projects)
            .map(in_checkout)
            .find(|scope| rules.can_read(scope))
            .unwrap_or_else(|| in_checkout(own)),
        Some(own) => in_checkout(own),
        None => TicketScope {
            cwd,
            ..TicketScope::default()
        },
    }
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
        WorkItemRef::Ticket(ticket) => {
            item.get("source").and_then(Value::as_str) == Some(ticket.source.as_wire())
                && item.get("ticketId").and_then(Value::as_str) == Some(ticket.id.as_str())
                && match &wanted_repo {
                    // A Linear key names one ticket wherever it is worked.
                    _ if !work_source(ticket.source).keys_name_a_repo() => true,
                    Some(repo) => {
                        item.get("key").and_then(Value::as_str) == Some(ticket.key(repo).as_str())
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
    let gh = gh_cli_is_available();
    let scope_for =
        |source: WorkSourceId| source_scope(work_source(source), project, projects, cwd.as_deref());

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
            WorkItemRef::Ticket(ticket) => match ticket.source {
                WorkSourceId::Linear => Some(ticket.id.clone()),
                WorkSourceId::Github => repo.map(|repo| format!("{repo}#{}", ticket.id)),
            },
            WorkItemRef::PullRequest(selector) => pull_request_url_parts(selector)
                .map(|(repo, number)| format!("{}#{number}", repo.to_ascii_lowercase()))
                .or_else(|| repo.map(|repo| format!("{repo}#{selector}"))),
        }?;
        Some((input.workspace_id.clone(), ticket))
    });

    let mut errors: Vec<String> = Vec::new();
    let mut ticket: Option<Value> = None;
    let mut pull_request: Option<Value> = None;
    let mut team: Option<Value> = None;

    // The Linear issue first: it can name the PR (Linear's GitHub integration).
    std::thread::scope(|threads| {
        let team_task = team_ticket.as_ref().map(|(workspace_id, ticket)| {
            threads.spawn(move || {
                crate::team_sync::read_team_ticket(paths, workspace_id, ticket, force)
            })
        });
        let ticket_task = match ticket_ref {
            WorkItemRef::Ticket(wanted)
                if work_source(wanted.source).can_read(&scope_for(wanted.source)) =>
            {
                let scope = scope_for(wanted.source);
                Some(threads.spawn(move || {
                    work_source(wanted.source).read_detail(&scope, &wanted.id, force)
                }))
            }
            _ => None,
        };
        // Started now only when the PR is already known; otherwise after the ticket's answer.
        let early_pr = pull_request_selector
            .clone()
            .filter(|_| gh)
            .map(|selector| {
                let cwd = cwd.clone().unwrap_or_default();
                threads.spawn(move || {
                    cached_detail(format!("pr:{cwd}:{selector}"), force, || {
                        fetch_pull_request_detail(&cwd, &selector)
                    })
                })
            });
        if let (Some(task), WorkItemRef::Ticket(wanted)) = (ticket_task, ticket_ref) {
            let name = work_source(wanted.source).name();
            match task
                .join()
                .unwrap_or_else(|_| Err(format!("{name} read failed.")))
            {
                Ok(value) => ticket = Some(value),
                Err(error) => errors.push(format!("{name}: {error}")),
            }
        }
        let late_pr = if early_pr.is_none() && gh {
            pull_request_selector = ticket
                .as_ref()
                .and_then(|ticket| ticket.get("pullRequestUrls"))
                .and_then(Value::as_array)
                .and_then(|urls| urls.first())
                .and_then(Value::as_str)
                .map(str::to_string);
            pull_request_selector.clone().map(|selector| {
                let cwd = cwd.clone().unwrap_or_default();
                threads.spawn(move || {
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
        let known_keys: Vec<(WorkSourceId, Vec<String>)> = WorkSourceId::BUILT_IN
            .into_iter()
            .map(|source| {
                (
                    source,
                    work_source(source).known_keys(&scope_for(source), plan),
                )
            })
            .collect();
        for ticket in pull_request_branch_tickets(pr, &known_keys) {
            push_ticket(&mut pr_tickets, ticket);
        }
        if let Some(url) = pr.get("url").and_then(Value::as_str) {
            for source in WorkSourceId::BUILT_IN {
                let source_rules = work_source(source);
                let scope = scope_for(source);
                if !source_rules.can_read(&scope) {
                    continue;
                }
                // Only adds tickets; a failed read leaves the ones found above.
                for id in source_rules
                    .tickets_attached_to(&scope, url, force)
                    .unwrap_or_default()
                {
                    push_ticket(
                        &mut pr_tickets,
                        WorkItemRef::Ticket(TicketRef::new(source, id)),
                    );
                }
            }
        }
        // No ticket was known before the PR was read: read the first one it names now.
        if matches!(ticket_ref, WorkItemRef::PullRequest(_)) {
            if let Some(WorkItemRef::Ticket(first)) = pr_tickets.first() {
                let source = work_source(first.source);
                let scope = scope_for(first.source);
                if source.can_read(&scope) {
                    match source.read_detail(&scope, &first.id, force) {
                        Ok(value) => ticket = Some(value),
                        Err(error) => errors.push(format!("{}: {error}", source.name())),
                    }
                }
            }
        }
    }
    let ticket_ref = match (ticket_ref, pr_tickets.first()) {
        (WorkItemRef::PullRequest(_), Some(ticket)) => ticket,
        _ => ticket_ref,
    };

    // A ticket the list does not have (closed, or someone else's team): its row from the details.
    if item.is_none() {
        item = synthesized_item(item_ref, project, ticket.as_ref(), pull_request.as_ref());
    }

    let mut texts: Vec<String> = Vec::new();
    let mut links: Vec<Value> = Vec::new();
    for source in [ticket.as_ref(), pull_request.as_ref()]
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
    if let Some(attachments) = ticket
        .as_ref()
        .and_then(|ticket| ticket.get("attachments"))
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
        ticket.as_ref(),
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
        "ticket": ticket,
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
pub(crate) fn work_ticket_detail(scope: &TicketScope, item: &WorkItemRef) -> Result<Value, String> {
    match item {
        WorkItemRef::Ticket(ticket) => {
            work_source(ticket.source).read_detail(scope, &ticket.id, false)
        }
        WorkItemRef::PullRequest(selector) => {
            let cwd = scope.cwd.unwrap_or_default();
            cached_detail(format!("pr:{cwd}:{selector}"), false, || {
                fetch_pull_request_detail(cwd, selector)
            })
        }
    }
}

/// `SPX-1245` or `#218`, as the team flow's ticket step names it.
fn work_item_ref_label(ticket: &WorkItemRef) -> String {
    match ticket {
        WorkItemRef::Ticket(ticket) => ticket.label(),
        WorkItemRef::PullRequest(selector) => selector.clone(),
    }
}

fn synthesized_item(
    item_ref: &WorkItemRef,
    project: Option<&WorkProjectInput>,
    ticket: Option<&Value>,
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
        WorkItemRef::Ticket(wanted) => {
            let state = ticket.and_then(|ticket| ticket.get("state"));
            item["key"] =
                json!(wanted.key(project.map(|p| p.project_id.as_str()).unwrap_or_default()));
            item["kind"] = json!("ticket");
            item["source"] = json!(wanted.source.as_wire());
            item["ticketId"] = json!(wanted.id);
            item["id"] = json!(wanted.label());
            item["title"] = json!(text(ticket, "title").unwrap_or_else(|| wanted.untitled()));
            item["url"] = json!(text(ticket, "url"));
            item["status"] = json!({
                "group": text(state, "group").unwrap_or_else(|| "todo".to_string()),
                "name": text(state, "name").unwrap_or_else(|| "Open".to_string()),
            });
            if wanted.source == WorkSourceId::Linear {
                item["branchName"] = json!(text(ticket, "branchName"));
            }
            if let Some(ticket) = ticket {
                for key in ["assignee", "cycle", "labels"] {
                    // An unassigned or uncycled ticket leaves the key out, like the list row does.
                    if let Some(value) = ticket.get(key).filter(|value| !value.is_null()) {
                        item[key] = value.clone();
                    }
                }
                if let (Some(value), Some(kind)) = (
                    ticket.get("project"),
                    work_source(wanted.source).project_kind(),
                ) {
                    item[kind.field()] = value.clone();
                }
                item["assignedToMe"] = json!(ticket
                    .pointer("/assignee/isMe")
                    .and_then(Value::as_bool)
                    .unwrap_or(false));
            }
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
    ticket: Option<&Value>,
    pull_request: Option<&Value>,
) -> TeamFlowFacts {
    let text = |value: Option<&Value>, key: &str| {
        value
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let ticket_id = match item_ref {
        WorkItemRef::Ticket(ticket) => Some(ticket.label()),
        WorkItemRef::PullRequest(_) => None,
    };
    let state = ticket.and_then(|ticket| ticket.get("state"));
    let count = |key: &str| {
        pull_request
            .and_then(|pr| pr.pointer(&format!("/checksSummary/{key}")))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize
    };
    TeamFlowFacts {
        ticket_id,
        // A GitHub step names the state as `gh` does (`open`, `closed`).
        ticket_state_name: match ticket
            .and_then(|ticket| ticket.get("source"))
            .and_then(Value::as_str)
        {
            Some("github") => text(state, "type"),
            _ => text(state, "name"),
        },
        ticket_state_type: text(state, "type"),
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

/// `text` cut to `limit` characters, with an ellipsis when it was longer.
pub(crate) fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_string(),
    }
}

pub(crate) fn fetch_linear_issue_detail(api_key: &str, identifier: &str) -> Result<Value, String> {
    let body = linear_graphql(
        api_key,
        &format!(
            "query($id: String!) {{ issue(id: $id) {{ identifier title url description updatedAt createdAt branchName priorityLabel state {{ name type }} assignee {{ name displayName avatarUrl isMe }} creator {{ name displayName }} team {{ key name }} project {{ name url }} cycle {{ name number }} labels(first: 20) {{ nodes {{ name color }} }} comments(first: {MAX_COMMENTS}) {{ nodes {{ body createdAt user {{ name displayName avatarUrl }} }} }} attachments(first: 30) {{ nodes {{ title subtitle url sourceType }} }} }} }}"
        ),
        json!({ "id": identifier }),
    )?;
    linear_issue_detail_of(identifier, &body)
}

/// The details page's Linear issue from Linear's answer to the query above.
fn linear_issue_detail_of(identifier: &str, body: &Value) -> Result<Value, String> {
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
    let fields = json!({
        "updatedAt": text("/updatedAt"),
        "createdAt": text("/createdAt"),
        "branchName": text("/branchName"),
        "priority": text("/priorityLabel"),
        "assignee": assignee,
        "author": issue.pointer("/creator/displayName").or_else(|| issue.pointer("/creator/name")),
        "team": issue.get("team"),
        "project": issue.get("project").filter(|project| project.is_object()),
        "cycle": cycle,
        "labels": issue.pointer("/labels/nodes").and_then(Value::as_array).map(|labels| {
            labels.iter().filter_map(|label| label.get("name").and_then(Value::as_str)).collect::<Vec<_>>()
        }),
        "attachments": attachments,
        "pullRequestUrls": pull_request_urls,
    });
    let ticket = TicketRef::new(
        WorkSourceId::Linear,
        text("/identifier").unwrap_or_else(|| identifier.to_string()),
    );
    Ok(ticket_detail_value(TicketDetail {
        ticket: &ticket,
        title: text("/title"),
        url: text("/url"),
        description: text("/description").map(|description| cut(&description, MAX_TEXT_CHARS)),
        status: linear_status(
            text("/state/type").as_deref(),
            text("/state/name").as_deref(),
        ),
        state: text("/state/type"),
        fields: fields.as_object().cloned().unwrap_or_default(),
        comment_count: comments.len(),
        comments,
    }))
}

pub(crate) fn fetch_github_issue_detail(cwd: &str, number: u64) -> Result<Value, String> {
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
    Ok(github_issue_detail_of(
        number,
        &issue,
        cached_github_viewer_login().as_deref(),
    ))
}

/// The details page's GitHub issue from `gh issue view --json`. `viewer` is the signed-in
/// GitHub login, so the first assignee carries the same `isMe` the list row does.
fn github_issue_detail_of(number: u64, issue: &Value, viewer: Option<&str>) -> Value {
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
    let text = |key: &str| issue.get(key).and_then(Value::as_str).map(str::to_string);
    let state = text("state").map(|state| state.to_ascii_lowercase());
    let closed = state.as_deref() == Some("closed");
    let assignee = issue
        .get("assignees")
        .and_then(Value::as_array)
        .and_then(|people| people.first())
        .and_then(|person| person.get("login").and_then(Value::as_str))
        .map(|login| {
            json!({
                "name": login,
                "isMe": viewer.is_some_and(|viewer| viewer.eq_ignore_ascii_case(login)),
            })
        });
    let labels: Vec<&str> = issue
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|label| label.get("name").and_then(Value::as_str))
        .collect();
    ticket_detail_value(TicketDetail {
        ticket: &TicketRef::new(WorkSourceId::Github, number.to_string()),
        title: text("title"),
        url: text("url"),
        description: Some(cut(&text("body").unwrap_or_default(), MAX_TEXT_CHARS)),
        status: WorkItemStatus {
            group: if closed { "closed" } else { "todo" },
            name: if closed { "Closed" } else { "Open" }.to_string(),
        },
        state,
        fields: json!({
            "author": issue.pointer("/author/login"),
            "updatedAt": issue.get("updatedAt"),
            "assignee": assignee,
            "labels": labels,
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
        comment_count: issue
            .get("comments")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        comments,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Linear's answer to the details query, with `attachments` as given.
    fn linear_answer(identifier: &str, attachments: Value) -> Value {
        json!({ "data": { "issue": {
            "identifier": identifier,
            "title": "Checkout fails on Safari",
            "url": format!("https://linear.app/acme/issue/{identifier}"),
            "description": "Steps: open checkout.",
            "updatedAt": "2026-10-20T09:00:00.000Z",
            "createdAt": "2026-10-01T09:00:00.000Z",
            "branchName": "leandro/acm-1-checkout",
            "priorityLabel": "High",
            "state": { "name": "In Review", "type": "started" },
            "assignee": { "name": "leandro", "displayName": "Leandro", "avatarUrl": "https://avatars/l.png", "isMe": true },
            "creator": { "name": "ana", "displayName": "Ana" },
            "team": { "key": "ACM", "name": "Acme" },
            "project": { "name": "Checkout revamp", "url": "https://linear.app/acme/project/checkout" },
            "cycle": { "name": "", "number": 12 },
            "labels": { "nodes": [{ "name": "Bug", "color": "#f00" }] },
            "comments": { "nodes": [
                { "body": "Repro on 17.2", "createdAt": "2026-10-02T09:00:00.000Z", "user": { "name": "ana", "displayName": "Ana", "avatarUrl": null } },
                { "body": "On it", "createdAt": "2026-10-03T09:00:00.000Z", "user": { "name": "leandro", "displayName": null, "avatarUrl": "https://avatars/l.png" } },
            ] },
            "attachments": { "nodes": attachments },
        } } })
    }

    #[track_caller]
    fn assert_published(actual: &Value, expected: Value) {
        assert!(
            *actual == expected,
            "published value changed:\n{}",
            serde_json::to_string_pretty(actual).unwrap()
        );
    }

    #[test]
    fn a_linear_issue_reads_into_the_details_shape() {
        let answer = linear_answer(
            "ACM-1",
            json!([
                { "title": "PR 40", "subtitle": "Open", "url": "https://github.com/acme/app/pull/40", "sourceType": "github" },
                { "title": null, "subtitle": null, "url": "https://docs.acme.dev/spec", "sourceType": null },
            ]),
        );
        assert_published(
            &linear_issue_detail_of("ACM-1", &answer).unwrap(),
            json!({
              "updatedAt": "2026-10-20T09:00:00.000Z",
              "createdAt": "2026-10-01T09:00:00.000Z",
              "branchName": "leandro/acm-1-checkout",
              "priority": "High",
              "assignee": {
                "name": "Leandro",
                "avatarUrl": "https://avatars/l.png",
                "isMe": true
              },
              "author": "Ana",
              "team": {
                "key": "ACM",
                "name": "Acme"
              },
              "project": {
                "name": "Checkout revamp",
                "url": "https://linear.app/acme/project/checkout"
              },
              "cycle": "Cycle 12",
              "labels": [
                "Bug"
              ],
              "attachments": [
                {
                  "title": "PR 40",
                  "subtitle": "Open",
                  "url": "https://github.com/acme/app/pull/40",
                  "source": "github"
                },
                {
                  "title": "https://docs.acme.dev/spec",
                  "subtitle": null,
                  "url": "https://docs.acme.dev/spec",
                  "source": null
                }
              ],
              "pullRequestUrls": [
                "https://github.com/acme/app/pull/40"
              ],
              "source": "linear",
              "id": "ACM-1",
              "label": "ACM-1",
              "title": "Checkout fails on Safari",
              "url": "https://linear.app/acme/issue/ACM-1",
              "description": "Steps: open checkout.",
              "state": {
                "group": "review",
                "name": "In Review",
                "type": "started"
              },
              "comments": [
                {
                  "author": "Ana",
                  "avatarUrl": null,
                  "body": "Repro on 17.2",
                  "createdAt": "2026-10-02T09:00:00.000Z"
                },
                {
                  "author": null,
                  "avatarUrl": "https://avatars/l.png",
                  "body": "On it",
                  "createdAt": "2026-10-03T09:00:00.000Z"
                }
              ],
              "commentCount": 2
            }),
        );
        assert_eq!(
            linear_issue_detail_of("ACM-9", &json!({ "data": { "issue": null } })),
            Err("Linear has no issue ACM-9.".to_string())
        );
    }

    #[test]
    fn a_github_issue_reads_into_the_details_shape_with_its_last_thirty_comments() {
        let comments: Vec<Value> = (1..=32)
            .map(|n| json!({ "author": { "login": format!("user{n}") }, "body": format!("comment {n}"), "createdAt": format!("2026-10-{:02}T09:00:00Z", n % 28 + 1) }))
            .collect();
        let issue = json!({
            "number": 218, "title": "Arabic plan cards", "url": "https://github.com/acme/web/issues/218",
            "state": "OPEN", "body": "Cards overflow.", "author": { "login": "yahia" },
            "assignees": [{ "login": "octo-me" }], "labels": [{ "name": "bug" }, { "name": "rtl" }],
            "comments": comments, "updatedAt": "2026-10-21T09:00:00Z",
        });
        let mut detail = github_issue_detail_of(218, &issue, Some("octo-me"));
        let comments = detail["comments"].as_array().unwrap().clone();
        assert_eq!(comments.len(), MAX_COMMENTS);
        assert_eq!(
            comments[0],
            json!({ "author": "user3", "body": "comment 3", "createdAt": "2026-10-04T09:00:00Z" })
        );
        assert_eq!(comments[29]["body"], json!("comment 32"));
        detail.as_object_mut().unwrap().remove("comments");
        assert_published(
            &detail,
            json!({
              "author": "yahia",
              "updatedAt": "2026-10-21T09:00:00Z",
              "assignee": { "name": "octo-me", "isMe": true },
              "labels": ["bug", "rtl"],
              "source": "github",
              "id": "218",
              "label": "#218",
              "title": "Arabic plan cards",
              "url": "https://github.com/acme/web/issues/218",
              "description": "Cards overflow.",
              "state": {
                "group": "todo",
                "name": "Open",
                "type": "open"
              },
              "commentCount": 32
            }),
        );
    }

    #[test]
    fn a_linear_tickets_details_hand_its_new_pr_to_the_sidebar_card() {
        let api_key = "lin-key-details-share";
        let fingerprint = linear_key_fingerprint(api_key);
        seed_linear_issue(
            fingerprint,
            "ACM-8",
            Some(LinearIssueInfo {
                identifier: "ACM-8".to_string(),
                title: Some("Checkout".to_string()),
                url: None,
                state_type: Some("started".to_string()),
                state_name: Some("In Progress".to_string()),
                project_name: None,
                project_url: None,
                pull_request_urls: Vec::new(),
            }),
        );
        let ticket = linear_issue_detail_of(
            "ACM-8",
            &linear_answer(
                "ACM-8",
                json!([{ "title": "PR 77", "subtitle": null, "url": "https://github.com/acme/app/pull/77", "sourceType": "github" }]),
            ),
        )
        .unwrap();
        let project = WorkProjectInput {
            project_id: "PCharShare".to_string(),
            name: "Share app".to_string(),
            path: None,
            project: json!({ "projectId": "PCharShare" }),
            sessions: Vec::new(),
            credentials: std::sync::Arc::new(WorkCredentials::with_project_linear_key(
                "PCharShare",
                Some(api_key),
            )),
            repo: None,
            workspace_id: crate::workspaces::DEFAULT_WORKSPACE_ID.to_string(),
            tracker: WorkSourceId::Linear,
        };
        assert!(share_work_item_with_sidebar(
            &[project],
            &json!({ "ticket": ticket })
        ));
        assert_eq!(
            cached_linear_issue(fingerprint, "ACM-8")
                .unwrap()
                .pull_request_urls,
            vec!["https://github.com/acme/app/pull/77".to_string()]
        );
    }

    #[test]
    fn the_team_flow_names_a_ticket_state_as_its_tracker_does() {
        let github = github_issue_detail_of(218, &json!({ "state": "OPEN" }), None);
        let linear = linear_issue_detail_of("ACM-1", &linear_answer("ACM-1", json!([]))).unwrap();
        let facts = |ticket: &Value, id: TicketRef| {
            let facts = team_flow_facts(&WorkItemRef::Ticket(id), 0, Some(ticket), None);
            (
                facts.ticket_id,
                facts.ticket_state_name,
                facts.ticket_state_type,
            )
        };
        assert_eq!(
            facts(&github, TicketRef::new(WorkSourceId::Github, "218")),
            (
                Some("#218".to_string()),
                Some("open".to_string()),
                Some("open".to_string())
            )
        );
        assert_eq!(
            facts(&linear, TicketRef::new(WorkSourceId::Linear, "ACM-1")),
            (
                Some("ACM-1".to_string()),
                Some("In Review".to_string()),
                Some("started".to_string())
            )
        );
    }

    #[test]
    fn a_ticket_missing_from_the_list_gets_a_row_from_its_details() {
        let project = WorkProjectInput {
            project_id: "PCharDetails".to_string(),
            name: "Details app".to_string(),
            path: Some("/gx-char/details".to_string()),
            project: json!({ "projectId": "PCharDetails" }),
            sessions: Vec::new(),
            credentials: std::sync::Arc::new(WorkCredentials::with_project_linear_key(
                "PCharDetails",
                None,
            )),
            repo: Some("acme/app".to_string()),
            workspace_id: crate::workspaces::DEFAULT_WORKSPACE_ID.to_string(),
            tracker: WorkSourceId::Linear,
        };
        let linear = linear_issue_detail_of("ACM-1", &linear_answer("ACM-1", json!([]))).unwrap();
        let github_issue = github_issue_detail_of(
            218,
            &json!({ "number": 218, "title": "Arabic plan cards", "url": "https://github.com/acme/web/issues/218", "state": "CLOSED" }),
            None,
        );
        let ticket = |source, id: &str| WorkItemRef::Ticket(TicketRef::new(source, id));
        let rows = json!({
            "linear": synthesized_item(&ticket(WorkSourceId::Linear, "ACM-1"), Some(&project), Some(&linear), None),
            "linearUnread": synthesized_item(&ticket(WorkSourceId::Linear, "ACM-2"), None, None, None),
            "githubIssue": synthesized_item(&ticket(WorkSourceId::Github, "218"), Some(&project), Some(&github_issue), None),
            "githubUnread": synthesized_item(&ticket(WorkSourceId::Github, "219"), None, None, None),
        });
        assert_published(
            &rows,
            json!({
              "linear": {
                "sessions": [],
                "labels": [
                  "Bug"
                ],
                "assignedToMe": true,
                "noTicket": false,
                "projectId": "PCharDetails",
                "projectName": "Details app",
                "key": "linear:ACM-1",
                "kind": "ticket",
                "source": "linear",
                "ticketId": "ACM-1",
                "id": "ACM-1",
                "title": "Checkout fails on Safari",
                "url": "https://linear.app/acme/issue/ACM-1",
                "status": {
                  "group": "review",
                  "name": "In Review"
                },
                "branchName": "leandro/acm-1-checkout",
                "assignee": {
                  "name": "Leandro",
                  "avatarUrl": "https://avatars/l.png",
                  "isMe": true
                },
                "cycle": "Cycle 12",
                "linearProject": {
                  "name": "Checkout revamp",
                  "url": "https://linear.app/acme/project/checkout"
                }
              },
              "linearUnread": {
                "sessions": [],
                "labels": [],
                "assignedToMe": false,
                "noTicket": false,
                "projectId": null,
                "projectName": null,
                "key": "linear:ACM-2",
                "kind": "ticket",
                "source": "linear",
                "ticketId": "ACM-2",
                "id": "ACM-2",
                "title": "ACM-2",
                "url": null,
                "status": {
                  "group": "todo",
                  "name": "Open"
                },
                "branchName": null
              },
              "githubIssue": {
                "sessions": [],
                "labels": [],
                "assignedToMe": false,
                "noTicket": false,
                "projectId": "PCharDetails",
                "projectName": "Details app",
                "key": "issue:PCharDetails#218",
                "kind": "ticket",
                "source": "github",
                "ticketId": "218",
                "id": "#218",
                "title": "Arabic plan cards",
                "url": "https://github.com/acme/web/issues/218",
                "status": {
                  "group": "closed",
                  "name": "Closed"
                }
              },
              "githubUnread": {
                "sessions": [],
                "labels": [],
                "assignedToMe": false,
                "noTicket": false,
                "projectId": null,
                "projectName": null,
                "key": "issue:#219",
                "kind": "ticket",
                "source": "github",
                "ticketId": "219",
                "id": "#219",
                "title": "Issue #219",
                "url": null,
                "status": {
                  "group": "todo",
                  "name": "Open"
                }
              }
            }),
        );
    }

    #[test]
    fn reading_a_linear_ticket_returns_its_details_and_row() {
        let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        let api_key = "lin-key-details-read";
        let project = WorkProjectInput {
            project_id: "PCharRead".to_string(),
            name: "Read app".to_string(),
            path: Some("/gx-char/details-read".to_string()),
            project: json!({ "projectId": "PCharRead" }),
            sessions: Vec::new(),
            credentials: std::sync::Arc::new(WorkCredentials::with_project_linear_key(
                "PCharRead",
                Some(api_key),
            )),
            repo: Some("acme/app".to_string()),
            workspace_id: crate::workspaces::DEFAULT_WORKSPACE_ID.to_string(),
            tracker: WorkSourceId::Linear,
        };
        let answer = linear_answer(
            "ACM-5",
            json!([{ "title": "Demo", "subtitle": null, "url": "https://www.loom.com/share/abc123", "sourceType": "loom" }]),
        );
        let detail = linear_issue_detail_of("ACM-5", &answer).unwrap();
        // What the page read a moment ago, so nothing asks Linear.
        cached_detail(
            format!("linear:{}:ACM-5", linear_key_fingerprint(api_key)),
            true,
            || Ok(detail),
        )
        .unwrap();
        let projects = vec![project];
        let read = read_work_item(
            &paths,
            &projects,
            &WorkFeedPlan::default(),
            Some("PCharRead"),
            &WorkItemRef::Ticket(TicketRef::new(WorkSourceId::Linear, "ACM-5")),
            false,
        );
        let wanted: Map<String, Value> = [
            "item",
            "ticket",
            "pullRequest",
            "tickets",
            "media",
            "links",
            "errors",
        ]
        .into_iter()
        .map(|key| (key.to_string(), read[key].clone()))
        .collect();
        assert_published(
            &Value::Object(wanted),
            json!({
              "item": {
                "sessions": [],
                "labels": [
                  "Bug"
                ],
                "assignedToMe": true,
                "noTicket": false,
                "projectId": "PCharRead",
                "projectName": "Read app",
                "key": "linear:ACM-5",
                "kind": "ticket",
                "source": "linear",
                "ticketId": "ACM-5",
                "id": "ACM-5",
                "title": "Checkout fails on Safari",
                "url": "https://linear.app/acme/issue/ACM-5",
                "status": {
                  "group": "review",
                  "name": "In Review"
                },
                "branchName": "leandro/acm-1-checkout",
                "assignee": {
                  "name": "Leandro",
                  "avatarUrl": "https://avatars/l.png",
                  "isMe": true
                },
                "cycle": "Cycle 12",
                "linearProject": {
                  "name": "Checkout revamp",
                  "url": "https://linear.app/acme/project/checkout"
                }
              },
              "ticket": {
                "updatedAt": "2026-10-20T09:00:00.000Z",
                "createdAt": "2026-10-01T09:00:00.000Z",
                "branchName": "leandro/acm-1-checkout",
                "priority": "High",
                "assignee": {
                  "name": "Leandro",
                  "avatarUrl": "https://avatars/l.png",
                  "isMe": true
                },
                "author": "Ana",
                "team": {
                  "key": "ACM",
                  "name": "Acme"
                },
                "project": {
                  "name": "Checkout revamp",
                  "url": "https://linear.app/acme/project/checkout"
                },
                "cycle": "Cycle 12",
                "labels": [
                  "Bug"
                ],
                "attachments": [
                  {
                    "title": "Demo",
                    "subtitle": null,
                    "url": "https://www.loom.com/share/abc123",
                    "source": "loom"
                  }
                ],
                "pullRequestUrls": [],
                "source": "linear",
                "id": "ACM-5",
                "label": "ACM-5",
                "title": "Checkout fails on Safari",
                "url": "https://linear.app/acme/issue/ACM-5",
                "description": "Steps: open checkout.",
                "state": {
                  "group": "review",
                  "name": "In Review",
                  "type": "started"
                },
                "comments": [
                  {
                    "author": "Ana",
                    "avatarUrl": null,
                    "body": "Repro on 17.2",
                    "createdAt": "2026-10-02T09:00:00.000Z"
                  },
                  {
                    "author": null,
                    "avatarUrl": "https://avatars/l.png",
                    "body": "On it",
                    "createdAt": "2026-10-03T09:00:00.000Z"
                  }
                ],
                "commentCount": 2
              },
              "pullRequest": null,
              "tickets": [],
              "media": [
                {
                  "kind": "loom",
                  "url": "https://www.loom.com/share/abc123",
                  "embedUrl": "https://www.loom.com/embed/abc123"
                }
              ],
              "links": [],
              "errors": []
            }),
        );
    }

    #[test]
    fn reading_a_github_issue_returns_its_details_and_row() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        let cwd = "/gx-char/details-read-github";
        let project = WorkProjectInput {
            project_id: "PCharReadGh".to_string(),
            name: "Read web".to_string(),
            path: Some(cwd.to_string()),
            project: json!({ "projectId": "PCharReadGh" }),
            sessions: Vec::new(),
            credentials: std::sync::Arc::new(WorkCredentials::with_project_linear_key(
                "PCharReadGh",
                None,
            )),
            repo: Some("acme/web".to_string()),
            workspace_id: crate::workspaces::DEFAULT_WORKSPACE_ID.to_string(),
            tracker: WorkSourceId::Github,
        };
        let issue = github_issue_detail_of(
            218,
            &json!({
                "number": 218, "title": "Arabic plan cards", "url": "https://github.com/acme/web/issues/218",
                "state": "OPEN", "body": "Cards overflow. Demo: https://www.loom.com/share/abc123",
                "author": { "login": "yahia" }, "assignees": [{ "login": "octo-me" }],
                "labels": [{ "name": "bug" }],
                "comments": [{ "author": { "login": "ana" }, "body": "Repro on 17.2", "createdAt": "2026-10-02T09:00:00Z" }],
                "updatedAt": "2026-10-21T09:00:00Z",
            }),
            Some("octo-me"),
        );
        // What the page read a moment ago, so nothing asks GitHub.
        cached_detail(format!("issue:{cwd}#218"), true, || Ok(issue)).unwrap();
        // `gh` is signed in, without asking it.
        let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
        let projects = vec![project];
        let read = read_work_item(
            &paths,
            &projects,
            &WorkFeedPlan::default(),
            Some("PCharReadGh"),
            &WorkItemRef::Ticket(TicketRef::new(WorkSourceId::Github, "218")),
            false,
        );
        let wanted: Map<String, Value> = [
            "item",
            "ticket",
            "pullRequest",
            "tickets",
            "media",
            "links",
            "errors",
        ]
        .into_iter()
        .map(|key| (key.to_string(), read[key].clone()))
        .collect();
        assert_published(
            &Value::Object(wanted),
            json!({
              "item": {
                "sessions": [],
                "labels": ["bug"],
                "assignee": { "name": "octo-me", "isMe": true },
                "assignedToMe": true,
                "noTicket": false,
                "projectId": "PCharReadGh",
                "projectName": "Read web",
                // The key is `issue:<projectId>#218`, not the repo form `issue:acme/web#218`; a fix is expected to change this assertion.
                "key": "issue:PCharReadGh#218",
                "kind": "ticket",
                "source": "github",
                "ticketId": "218",
                "id": "#218",
                "title": "Arabic plan cards",
                "url": "https://github.com/acme/web/issues/218",
                "status": {
                  "group": "todo",
                  "name": "Open"
                }
              },
              "ticket": {
                "author": "yahia",
                "updatedAt": "2026-10-21T09:00:00Z",
                "assignee": { "name": "octo-me", "isMe": true },
                "labels": ["bug"],
                "source": "github",
                "id": "218",
                "label": "#218",
                "title": "Arabic plan cards",
                "url": "https://github.com/acme/web/issues/218",
                "description": "Cards overflow. Demo: https://www.loom.com/share/abc123",
                "state": {
                  "group": "todo",
                  "name": "Open",
                  "type": "open"
                },
                "comments": [
                  {
                    "author": "ana",
                    "body": "Repro on 17.2",
                    "createdAt": "2026-10-02T09:00:00Z"
                  }
                ],
                "commentCount": 1
              },
              "pullRequest": null,
              "tickets": [],
              "media": [
                {
                  "kind": "loom",
                  "url": "https://www.loom.com/share/abc123",
                  "embedUrl": "https://www.loom.com/embed/abc123"
                }
              ],
              "links": [],
              "errors": []
            }),
        );
    }
}
