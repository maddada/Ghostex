//! The Work page's list (`/api/listWorkItems`): one flat row per piece of work across the
//! work-mode projects a window shows, with the sessions on this computer that link it.
//!
//! CDXC:WorkMode 2026-10-10 DECISION:
//! User: the Work list is sorted by recently updated, with filters on top and "Assigned to me"
//! on when it opens. It is one flat list by default; grouping is an option the page applies on
//! top ("i also want you to please add dropdown that lets me group the list items by different
//! ways that are useful", supersedes the 2026-10-09 "no grouping"; the groups live in
//! apps/desktop/views/work/grouping.ts). A PR linked to a ticket folds into that
//! ticket's row; a PR with no ticket is its own row with a "No ticket" warning. Personal
//! work-mode projects get the Work list too, showing only their issues and PRs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::{presentation_activity, project_session_title};
use crate::server::AppState;
use crate::session_git_status::{gh_cli_is_available, PullRequestState};
use crate::storage::open_gxserver_database;

use super::*;

/// A work-mode project and what the list needs from it, read once per request.
#[derive(Clone, Debug)]
pub(crate) struct WorkProjectInput {
    pub(crate) project_id: String,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
    pub(crate) project: Value,
    /// The sessions in the sidebar (every one that is not closed).
    pub(crate) sessions: Vec<Value>,
    /// Where its tickets' sources look up their credentials, shared by every project of a request.
    pub(crate) credentials: Arc<WorkCredentials>,
    /// `owner/repo` of the project's GitHub `origin`.
    pub(crate) repo: Option<String>,
    /// The workspace the project is in (a worktree project follows its parent checkout).
    pub(crate) workspace_id: String,
    /// The workspace's primary tracker: whose tickets the list shows.
    pub(crate) tracker: WorkSourceId,
}

impl WorkProjectInput {
    pub(crate) fn ticket_scope(&self) -> TicketScope<'_> {
        TicketScope {
            project_id: &self.project_id,
            workspace_id: crate::workspaces::stored_project_workspace_id(&self.project),
            cwd: self.path.as_deref(),
            credentials: Some(&self.credentials),
        }
    }
}

/// The work-mode projects a request names (all of them when it names none).
pub(crate) fn load_work_projects(
    state: &AppState,
    wanted: Option<&[String]>,
) -> Result<Vec<WorkProjectInput>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let all_projects = repository.list_projects()?;
    let workspaces = crate::workspaces::read_sidebar_workspaces(&db)?;
    let credentials = Arc::new(WorkCredentials::new(&state.paths));
    let mut projects = Vec::new();
    for project in all_projects.iter().cloned() {
        if !project_work_mode(&project) {
            continue;
        }
        let Some(project_id) = project
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        if wanted.is_some_and(|wanted| !wanted.contains(&project_id)) {
            continue;
        }
        let path = project
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string);
        projects.push(WorkProjectInput {
            name: project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| project_id.clone()),
            sessions: repository.list_sessions_excluding_stopped(Some(&project_id))?,
            credentials: credentials.clone(),
            repo: path.as_deref().and_then(work_repo_of),
            workspace_id: crate::workspaces::project_workspace_id(
                &workspaces,
                &project,
                &all_projects,
            ),
            tracker: project_work_tracker(&state.paths, &workspaces, &project, &all_projects),
            path,
            project_id,
            project,
        });
    }
    Ok(projects)
}

/// Which feeds a set of projects reads, worked out once per request so the refresh and the list
/// that follows read the same feeds: each source's own (`WorkSource::plan_list`) and one `gh`
/// checkout per GitHub repo for the code host.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorkFeedPlan {
    /// Linear's requests, one per key: the viewer's issues plus the teams that key's projects link
    /// to (source_linear.rs).
    pub(crate) linear: Vec<LinearFeedRequest>,
    /// The checkout `gh` lists each repo from, and the project it stands for.
    pub(crate) github: Vec<(String, String)>,
}

impl WorkFeedPlan {
    pub(crate) fn github_cwds(&self) -> Vec<String> {
        self.github.iter().map(|(cwd, _)| cwd.clone()).collect()
    }
}

pub(crate) fn work_feed_plan(projects: &[WorkProjectInput]) -> WorkFeedPlan {
    let mut plan = WorkFeedPlan::default();
    for source in WorkSourceId::BUILT_IN {
        work_source(source).plan_list(projects, &mut plan);
    }
    // Worktree projects share their repo with the main checkout: the repo is listed once.
    let mut repos_seen = HashSet::new();
    for input in projects {
        if let (Some(path), Some(repo)) = (&input.path, &input.repo) {
            if repos_seen.insert(repo.clone()) {
                plan.github.push((path.clone(), input.project_id.clone()));
            }
        }
    }
    plan
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkItemStatus {
    /// `backlog`, `todo`, `progress`, `review`, `done`, `canceled`, `draft`, `open`, `merged` or
    /// `closed`; the page's status filter and glyph read it.
    pub(crate) group: &'static str,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkItemPerson {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) avatar_url: Option<String>,
    pub(crate) is_me: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkItemLink {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkItemPullRequest {
    pub(crate) number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    pub(crate) state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) checks: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) review_decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkItemSession {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) title: String,
    /// The agent is in a turn right now (the row's live dot).
    pub(crate) working: bool,
    pub(crate) lifecycle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkItem {
    /// Stable across refreshes: `linear:SPX-1234`, `issue:owner/repo#218`, `pr:owner/repo#6552`.
    pub(crate) key: String,
    /// `ticket` (a source's) or `pullRequest` (the code host's).
    pub(crate) kind: &'static str,
    /// A ticket's source: `linear` or `github`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<&'static str>,
    /// What the row shows first: `SPX-1234`, `#218`, `#6552`.
    pub(crate) id: String,
    pub(crate) title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) updated_at: Option<String>,
    pub(crate) status: WorkItemStatus,
    /// The Ghostex project (one folder, one repo) the work happens in, when Ghostex can tell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) linear_project: Option<WorkItemLink>,
    /// The GitHub Project a GitHub issue or PR is in (title only; `gh` lists no link).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) github_project: Option<WorkItemLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cycle: Option<String>,
    pub(crate) labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) assignee: Option<WorkItemPerson>,
    pub(crate) assigned_to_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pull_request: Option<WorkItemPullRequest>,
    /// A PR row's ticket when the ticket itself is not in the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ticket: Option<String>,
    /// A PR that no ticket claims.
    pub(crate) no_ticket: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) branch_name: Option<String>,
    pub(crate) sessions: Vec<WorkItemSession>,
    /// `ticketId` (with `source`) or `pullRequestRef` names the item for `/api/readWorkItem` and
    /// `/api/startWorkOnTicket`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ticket_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pull_request_ref: Option<String>,
}

pub(crate) fn linear_status(state_type: Option<&str>, state_name: Option<&str>) -> WorkItemStatus {
    let name = state_name.unwrap_or("Open").to_string();
    let in_review = name.to_ascii_lowercase().contains("review");
    let group = match state_type {
        Some("started") if in_review => "review",
        Some("started") => "progress",
        Some("unstarted") => "todo",
        Some("backlog") | Some("triage") => "backlog",
        Some("completed") => "done",
        Some("canceled") => "canceled",
        _ => "todo",
    };
    WorkItemStatus { group, name }
}

fn pull_request_status(state: PullRequestState) -> WorkItemStatus {
    let (group, name) = match state {
        PullRequestState::Draft => ("draft", "Draft"),
        PullRequestState::Open => ("open", "Open"),
        PullRequestState::Merged => ("merged", "Merged"),
        PullRequestState::Closed => ("closed", "Closed"),
    };
    WorkItemStatus {
        group,
        name: name.to_string(),
    }
}

fn is_open_pull_request(state: PullRequestState) -> bool {
    matches!(state, PullRequestState::Open | PullRequestState::Draft)
}

/// What one sidebar session on this computer links to, for joining it onto rows.
struct SessionLinks {
    session: WorkItemSession,
    /// Every source's tickets, Linear's first.
    tickets: Vec<TicketRef>,
    /// The `owner/repo` (else the project id) its tickets' keys name.
    repo: String,
    pull_request: Option<(String, u64)>,
}

impl SessionLinks {
    fn ticket_keys(&self) -> impl Iterator<Item = String> + '_ {
        self.tickets.iter().map(|ticket| ticket.key(&self.repo))
    }
}

fn session_links(input: &WorkProjectInput, generated_at: &str) -> Vec<SessionLinks> {
    let repo_key = input
        .repo
        .clone()
        .unwrap_or_else(|| input.project_id.clone());
    input
        .sessions
        .iter()
        .filter_map(|session| {
            let session_id = session.get("sessionId").and_then(Value::as_str)?;
            let targets = work_targets(&input.project, session);
            let title_fields = project_session_title(session);
            let title_source = title_fields.get("titleSource").and_then(Value::as_str);
            let title = work_display_title(&input.project, session, title_source)
                .or_else(|| {
                    title_fields
                        .get("displayTitle")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| session_id.to_string());
            let pull_request = targets
                .pull_request
                .as_deref()
                .and_then(|selector| match selector.parse::<u64>() {
                    Ok(number) => Some((repo_key.clone(), number)),
                    Err(_) => pull_request_url_parts(selector),
                });
            Some(SessionLinks {
                session: WorkItemSession {
                    project_id: input.project_id.clone(),
                    session_id: session_id.to_string(),
                    title,
                    working: presentation_activity(session, generated_at) == "working",
                    lifecycle: crate::presentation::effective_lifecycle_state(session),
                    agent_id: session
                        .get("agentId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                },
                tickets: targets
                    .tickets
                    .into_iter()
                    .map(|linked| linked.ticket)
                    .collect(),
                repo: repo_key.clone(),
                pull_request,
            })
        })
        .collect()
}

fn person(name: &str, is_me: bool) -> WorkItemPerson {
    WorkItemPerson {
        name: name.to_string(),
        avatar_url: None,
        is_me,
    }
}

/// A source's ticket as a list row.
fn ticket_item(
    key: String,
    ticket: Ticket,
    project_id: Option<String>,
    projects_by_id: &HashMap<&str, &WorkProjectInput>,
) -> WorkItem {
    let source = ticket.ticket.source;
    let (linear_project, github_project) = match work_source(source).project_kind() {
        Some(TicketProjectKind::Linear) => (ticket.project, None),
        Some(TicketProjectKind::Github) => (None, ticket.project),
        None => (None, None),
    };
    WorkItem {
        key,
        kind: "ticket",
        source: Some(source.as_wire()),
        id: ticket.ticket.label(),
        title: ticket.title.unwrap_or_else(|| ticket.ticket.label()),
        url: ticket.url,
        updated_at: ticket.updated_at,
        status: ticket.status,
        project_name: project_id
            .as_deref()
            .and_then(|id| projects_by_id.get(id))
            .map(|input| input.name.clone()),
        project_id,
        linear_project,
        github_project,
        cycle: ticket.cycle,
        labels: ticket.labels,
        assignee: ticket.assignee,
        assigned_to_me: ticket.assigned_to_me,
        pull_request: None,
        ticket: None,
        no_ticket: false,
        branch_name: ticket.branch_name,
        sessions: Vec::new(),
        ticket_id: Some(ticket.ticket.id),
        pull_request_ref: None,
    }
}

/// Builds the list from the caches, never from the network. Returns the response body.
pub(crate) fn build_work_list(projects: &[WorkProjectInput], plan: &WorkFeedPlan) -> Value {
    let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let viewer_login = cached_github_viewer_login();
    let is_me = |login: Option<&str>| {
        viewer_login
            .as_deref()
            .zip(login)
            .is_some_and(|(viewer, login)| viewer.eq_ignore_ascii_case(login))
    };
    let projects_by_id: HashMap<&str, &WorkProjectInput> = projects
        .iter()
        .map(|input| (input.project_id.as_str(), input))
        .collect();
    let project_for_repo = |repo: &str| {
        projects
            .iter()
            .find(|input| input.repo.as_deref() == Some(repo))
            .map(|input| input.project_id.clone())
    };
    let mut errors: Vec<String> = Vec::new();
    let mut items: Vec<WorkItem> = Vec::new();
    let mut index_by_key: HashMap<String, usize> = HashMap::new();

    let links: Vec<SessionLinks> = projects
        .iter()
        .flat_map(|input| session_links(input, &generated_at))
        .collect();

    // Which project a Linear team's tickets are worked in, learned from linked sessions.
    let mut team_projects: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for link in &links {
        for ticket in &link.tickets {
            if let Some(team) = work_source(ticket.source).team_of(&ticket.id) {
                *team_projects
                    .entry(team)
                    .or_default()
                    .entry(link.session.project_id.clone())
                    .or_default() += 1;
            }
        }
    }
    let team_project = |team: &str| {
        team_projects.get(team).and_then(|counts| {
            counts
                .iter()
                .max_by_key(|(_, count)| **count)
                .map(|(project_id, _)| project_id.clone())
        })
    };

    // Every source's tickets: what its feeds brought, then the linked ones they did not.
    // Each source's known ticket-id prefixes, in `BUILT_IN` order, for matching PRs below.
    let mut known_keys: Vec<(WorkSourceId, Vec<String>)> = Vec::new();
    let mut pull_request_url_tickets: HashMap<(String, u64), String> = HashMap::new();
    for source in WorkSourceId::BUILT_IN {
        let source_rules = work_source(source);
        let list = source_rules.cached_list(projects, plan);
        errors.extend(list.errors);
        known_keys.push((source, list.known_keys.into_iter().collect()));
        for listed in list.tickets {
            let key = listed.ticket.ticket.key(&listed.repo);
            if index_by_key.contains_key(&key) {
                continue;
            }
            let mut project_id = listed.project_id;
            for url in &listed.ticket.pull_request_urls {
                if let Some(parts) = pull_request_url_parts(url) {
                    project_id = project_id.or_else(|| project_for_repo(&parts.0));
                    pull_request_url_tickets.insert(parts, key.clone());
                }
            }
            let project_id =
                project_id.or_else(|| listed.ticket.team.as_deref().and_then(team_project));
            index_by_key.insert(key.clone(), items.len());
            items.push(ticket_item(key, listed.ticket, project_id, &projects_by_id));
        }
        // Tickets a sidebar session links that the feeds did not bring (another Linear team's, or
        // one assigned to someone else), from the card caches.
        if source_rules.list_holds_every_open_ticket() {
            continue;
        }
        for link in &links {
            let Some(input) = projects_by_id
                .get(link.session.project_id.as_str())
                .filter(|input| input.tracker == source)
            else {
                continue;
            };
            for linked in link.tickets.iter().filter(|ticket| ticket.source == source) {
                let key = linked.key(&link.repo);
                if index_by_key.contains_key(&key) {
                    continue;
                }
                let TicketRead::Found(ticket) =
                    source_rules.cached_ticket(&input.ticket_scope(), &linked.id)
                else {
                    continue;
                };
                if matches!(ticket.status.group, "done" | "canceled") {
                    continue;
                }
                for url in &ticket.pull_request_urls {
                    if let Some(parts) = pull_request_url_parts(url) {
                        pull_request_url_tickets.insert(parts, key.clone());
                    }
                }
                index_by_key.insert(key.clone(), items.len());
                items.push(ticket_item(
                    key,
                    ticket,
                    Some(input.project_id.clone()),
                    &projects_by_id,
                ));
            }
        }
    }

    // Each repo's PRs, from the code host (GitHub) whatever the tracker; they fold into the
    // tickets above.
    let gh = gh_cli_is_available();
    let mut pull_requests: Vec<(String, String, GithubListPullRequest)> = Vec::new();
    for (cwd, project_id) in &plan.github {
        let Some(input) = projects_by_id.get(project_id.as_str()) else {
            continue;
        };
        let Some(feed) = cached_github_feed(cwd) else {
            continue;
        };
        let feed = match feed {
            Ok(feed) => feed,
            Err(error) => {
                errors.push(format!("GitHub ({}): {error}", input.name));
                continue;
            }
        };
        let repo = feed
            .repo
            .clone()
            .or_else(|| input.repo.clone())
            .unwrap_or_else(|| input.project_id.clone());
        for pull_request in feed.pull_requests {
            pull_requests.push((repo.clone(), input.project_id.clone(), pull_request));
        }
    }

    // A session that links both a PR and a ticket ties the two together.
    let mut session_pull_request_tickets: HashMap<(String, u64), String> = HashMap::new();
    for link in &links {
        let Some(pull_request) = &link.pull_request else {
            continue;
        };
        if let Some(key) = link.ticket_keys().next() {
            session_pull_request_tickets.insert(pull_request.clone(), key);
        }
    }

    for (repo, project_id, pull_request) in pull_requests {
        let id = (repo.clone(), pull_request.number);
        let github_project = pull_request
            .project
            .as_ref()
            .filter(|_| {
                projects_by_id
                    .get(project_id.as_str())
                    .is_some_and(|input| input.tracker == WorkSourceId::Github)
            })
            .map(|project| WorkItemLink {
                name: project.title.clone(),
                url: project.url.clone(),
            });
        let head = pull_request.head_branch.as_deref().unwrap_or_default();
        let mut tickets: Vec<String> = Vec::new();
        if let Some(key) = session_pull_request_tickets.get(&id) {
            tickets.push(key.clone());
        }
        if let Some(key) = pull_request_url_tickets.get(&id) {
            tickets.push(key.clone());
        }
        let text = PullRequestText {
            head,
            body: Some(&pull_request.body),
        };
        for (source, keys) in &known_keys {
            for id in work_source(*source).tickets_in_pull_request(keys, &text) {
                tickets.push(TicketRef::new(*source, id).key(&repo));
            }
        }
        let mine = is_me(pull_request.author.as_deref())
            || pull_request
                .assignees
                .iter()
                .any(|login| is_me(Some(login)));
        let summary = WorkItemPullRequest {
            number: pull_request.number,
            url: pull_request.url.clone(),
            state: pull_request.state.as_wire(),
            checks: pull_request.checks,
            review_decision: pull_request.review_decision.clone(),
            title: Some(pull_request.title.clone()),
        };
        if let Some(index) = tickets
            .iter()
            .find_map(|key| index_by_key.get(key).copied())
        {
            let item = &mut items[index];
            if item.pull_request.is_none() {
                item.pull_request = Some(summary);
            }
            if item.project_id.is_none() {
                item.project_name = projects_by_id
                    .get(project_id.as_str())
                    .map(|input| input.name.clone());
                item.project_id = Some(project_id.clone());
            }
            let ticket_source = item.source.and_then(WorkSourceId::parse).map(work_source);
            if item.github_project.is_none()
                && ticket_source
                    .is_some_and(|source| source.project_kind() == Some(TicketProjectKind::Github))
            {
                item.github_project = github_project;
            }
            item.assigned_to_me |= mine && item.assignee.is_none();
            if sort_time(pull_request.updated_at.as_deref()) > sort_time(item.updated_at.as_deref())
            {
                item.updated_at = pull_request.updated_at.clone();
            }
            if ticket_source.is_some_and(|source| source.status_follows_pull_request()) {
                item.status = WorkItemStatus {
                    group: if pull_request.state == PullRequestState::Draft {
                        "progress"
                    } else {
                        "review"
                    },
                    name: "Open".to_string(),
                };
            }
            index_by_key.insert(format!("pr:{repo}#{}", pull_request.number), index);
            continue;
        }
        let ticket = tickets
            .first()
            .and_then(|key| key.strip_prefix("linear:"))
            .map(str::to_string);
        let key = format!("pr:{repo}#{}", pull_request.number);
        index_by_key.insert(key.clone(), items.len());
        items.push(WorkItem {
            key,
            kind: "pullRequest",
            source: None,
            id: format!("#{}", pull_request.number),
            title: pull_request.title.clone(),
            url: pull_request.url.clone(),
            updated_at: pull_request.updated_at.clone(),
            status: pull_request_status(pull_request.state),
            project_name: projects_by_id
                .get(project_id.as_str())
                .map(|input| input.name.clone()),
            project_id: Some(project_id.clone()),
            linear_project: None,
            github_project,
            cycle: None,
            labels: Vec::new(),
            assignee: pull_request
                .author
                .as_deref()
                .map(|login| person(login, is_me(Some(login)))),
            assigned_to_me: mine,
            pull_request: Some(summary),
            no_ticket: ticket.is_none() && tickets.is_empty(),
            ticket,
            branch_name: pull_request.head_branch.clone(),
            sessions: Vec::new(),
            ticket_id: None,
            pull_request_ref: pull_request
                .url
                .clone()
                .or_else(|| Some(pull_request.number.to_string())),
        });
    }

    // A session's own PR that no feed brought (another repo, or one the list cut off).
    for link in &links {
        let Some((repo, number)) = &link.pull_request else {
            continue;
        };
        let pr_key = format!("pr:{repo}#{number}");
        if index_by_key.contains_key(&pr_key) {
            continue;
        }
        let Some(input) = projects_by_id.get(link.session.project_id.as_str()) else {
            continue;
        };
        let selector = number.to_string();
        let cwd = input.path.clone().unwrap_or_default();
        let Some(info) = cached_work_pull_request(&cwd, &selector)
            .filter(|info| is_open_pull_request(info.state))
        else {
            continue;
        };
        let summary = WorkItemPullRequest {
            number: info.number,
            url: info.url.clone(),
            state: info.state.as_wire(),
            checks: info.checks,
            review_decision: None,
            title: None,
        };
        let ticket_index = link
            .ticket_keys()
            .find_map(|key| index_by_key.get(&key).copied());
        if let Some(index) = ticket_index {
            if items[index].pull_request.is_none() {
                items[index].pull_request = Some(summary);
            }
            index_by_key.insert(pr_key, index);
            continue;
        }
        index_by_key.insert(pr_key.clone(), items.len());
        items.push(WorkItem {
            key: pr_key,
            kind: "pullRequest",
            source: None,
            id: format!("#{number}"),
            title: format!("Pull request #{number}"),
            url: info.url.clone(),
            updated_at: None,
            status: pull_request_status(info.state),
            project_id: Some(input.project_id.clone()),
            project_name: Some(input.name.clone()),
            linear_project: None,
            github_project: None,
            cycle: None,
            labels: Vec::new(),
            assignee: None,
            assigned_to_me: true,
            pull_request: Some(summary),
            ticket: None,
            no_ticket: link.tickets.is_empty(),
            branch_name: None,
            sessions: Vec::new(),
            ticket_id: None,
            pull_request_ref: info.url.clone().or_else(|| Some(number.to_string())),
        });
    }

    // Join the sessions.
    for link in links {
        let mut keys: Vec<String> = link.ticket_keys().collect();
        if let Some((repo, number)) = &link.pull_request {
            keys.push(format!("pr:{repo}#{number}"));
        }
        let mut joined = HashSet::new();
        for key in keys {
            if let Some(index) = index_by_key.get(&key).copied() {
                if joined.insert(index) {
                    items[index].sessions.push(link.session.clone());
                }
            }
        }
    }

    items.sort_by(|a, b| {
        sort_time(b.updated_at.as_deref())
            .cmp(&sort_time(a.updated_at.as_deref()))
            .then_with(|| a.key.cmp(&b.key))
    });
    // Only a Linear workspace needs a Linear key; a GitHub one never shows the notice.
    let linear_projects: Vec<&WorkProjectInput> = projects
        .iter()
        .filter(|input| input.tracker == WorkSourceId::Linear)
        .collect();
    let linear_configured = linear_projects.is_empty()
        || linear_projects
            .iter()
            .any(|input| work_source(WorkSourceId::Linear).can_read(&input.ticket_scope()));
    let tracker = if projects.is_empty() || !linear_projects.is_empty() {
        WorkSourceId::Linear
    } else {
        WorkSourceId::Github
    };
    json!({
        "tracker": tracker.as_wire(),
        "items": items,
        "projects": projects.iter().map(|input| json!({
            "projectId": input.project_id,
            "name": input.name,
            "repo": input.repo,
        })).collect::<Vec<_>>(),
        "viewer": { "githubLogin": viewer_login },
        "linearConfigured": linear_configured,
        "ghAvailable": gh,
        "errors": errors,
        "generatedAt": generated_at,
    })
}

/// A GitHub issue's status: its project board's Status column when it has one (`In progress`,
/// `In review`, `Done`), otherwise just open.
pub(crate) fn github_issue_status(project: Option<&GithubListProject>) -> WorkItemStatus {
    let Some(name) = project.and_then(|project| project.status.clone()) else {
        return WorkItemStatus {
            group: "todo",
            name: "Open".to_string(),
        };
    };
    let lower = name.to_ascii_lowercase();
    let group = if lower.contains("review") {
        "review"
    } else if lower.contains("progress") || lower.contains("doing") {
        "progress"
    } else if lower.contains("done") || lower.contains("complete") || lower.contains("shipped") {
        "done"
    } else if lower.contains("backlog") || lower.contains("triage") || lower.contains("icebox") {
        "backlog"
    } else {
        "todo"
    };
    WorkItemStatus { group, name }
}

/// Milliseconds since the epoch for sorting; rows without a time sort last.
fn sort_time(time: Option<&str>) -> i64 {
    time.and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
        .map(|time| time.timestamp_millis())
        .unwrap_or(i64::MIN)
}
