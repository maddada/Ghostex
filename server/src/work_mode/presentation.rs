//! What a work-mode session publishes: which PR, issues and Linear project it is linked to, and
//! its branch title. Pure cache reads, so the presentation projection can call it per row.

use serde_json::{json, Map, Value};

use crate::session_git_status::{cached_session_git_status, effective_session_git_cwd};

use super::*;

/// What a session is linked to, before any status is looked up. The background pass fetches
/// exactly these; the projection reads the answers back from the caches.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WorkTargets {
    pub(crate) cwd: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) linear_key: Option<u64>,
    /// Every source's tickets, Linear's first: each source's hand-set ones, else what the branch
    /// names.
    pub(crate) tickets: Vec<LinkedTicket>,
    /// A PR number (as text) or URL, as `gh pr view` takes it.
    pub(crate) pull_request: Option<String>,
    /// The branch's own PR from the git probe, which also carries its state.
    pub(crate) branch_pull_request: Option<u64>,
    pub(crate) linear_project: Option<Option<String>>,
    /// A hand-set GitHub Project (`owner/number`); `None` = automatic, from the issues' and the
    /// PR's project items.
    pub(crate) github_project: Option<Option<String>>,
}

/// One ticket a session is linked to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinkedTicket {
    pub(crate) ticket: TicketRef,
    /// Set by hand (`workLinks`), not read from the branch.
    pub(crate) by_hand: bool,
}

impl WorkTargets {
    /// One source's ticket ids, in link order.
    pub(crate) fn ids_of(&self, source: WorkSourceId) -> Vec<String> {
        self.tickets
            .iter()
            .filter(|linked| linked.ticket.source == source)
            .map(|linked| linked.ticket.id.clone())
            .collect()
    }

    /// The GitHub issue numbers, for the code host's reads, which share a call with the PR's.
    pub(crate) fn github_issue_numbers(&self) -> Vec<u64> {
        self.tickets
            .iter()
            .filter(|linked| linked.ticket.source == WorkSourceId::Github)
            .filter_map(|linked| linked.ticket.id.parse().ok())
            .collect()
    }
}

/// Resolves a work-mode session's links: hand-set ones first, then what its branch names, then
/// (for the PR) what Linear's GitHub integration attached to its issue.
pub(crate) fn work_targets(project: &Value, session: &Value) -> WorkTargets {
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let cwd = effective_session_git_cwd(session, Some(project));
    let git = cwd.as_deref().and_then(cached_session_git_status);
    let branch =
        work_branch(git.as_ref().and_then(|git| git.branch.as_deref())).map(str::to_string);
    let manual = manual_work_links(session);
    let linear_key = project_linear_key(project_id);
    let scope = TicketScope {
        project_id,
        cwd: cwd.as_deref(),
        linear_api_key: None,
    };
    let in_branch = |source: WorkSourceId| match branch.as_deref() {
        Some(branch) => work_source(source).tickets_in_branch(&scope, branch),
        None => Vec::new(),
    };

    let tickets: Vec<LinkedTicket> = WorkSourceId::BUILT_IN
        .into_iter()
        .flat_map(|source| {
            let hand_set = manual.ticket_ids(source);
            let by_hand = hand_set.is_some();
            hand_set
                .unwrap_or_else(|| in_branch(source))
                .into_iter()
                .map(move |id| LinkedTicket {
                    ticket: TicketRef::new(source, id),
                    by_hand,
                })
        })
        .collect();
    let branch_pull_request = branch.as_ref().and_then(|_| {
        git.as_ref()
            .and_then(|git| git.pull_request.as_ref())
            .and_then(|pull_request| u64::try_from(pull_request.number).ok())
    });
    let pull_request = match manual.pull_request {
        Some(Some(ManualPullRequest::Number(number))) => Some(number.to_string()),
        Some(Some(ManualPullRequest::Url(url))) => Some(url),
        Some(None) => None,
        None => branch_pull_request
            .map(|number| number.to_string())
            .or_else(|| {
                let repo = cwd.as_deref().and_then(cached_work_repo);
                tickets.iter().find_map(|linked| {
                    match work_source(linked.ticket.source).cached_ticket(&scope, &linked.ticket.id)
                    {
                        TicketRead::Found(ticket) => {
                            ticket_pull_request_in_repo(&ticket.pull_request_urls, repo.as_deref())
                        }
                        TicketRead::NotFound | TicketRead::Unread => None,
                    }
                })
            }),
    };
    WorkTargets {
        cwd,
        branch,
        linear_key,
        tickets,
        pull_request,
        branch_pull_request,
        linear_project: manual.linear_project,
        github_project: manual.github_project,
    }
}

/// The ticket's PR a session without its own shows: the first one Linear attached in the session's
/// repo (a Linear project can span repos), or the first one while the repo is not known yet.
///
/// CDXC:WorkMode 2026-10-12 WHY:
/// A session linked to DE-40 showed no PR chip while the ticket's Work details already showed PR #6647: the card waited for the branch probe's 5-minute PR TTL and the issue's 2-minute TTL. Issues awaiting a PR are re-read every pass, and the details hand what they read to these caches (`share_work_item_with_sidebar`).
fn ticket_pull_request_in_repo(urls: &[String], repo: Option<&str>) -> Option<String> {
    match repo {
        Some(repo) => urls
            .iter()
            .find(|url| {
                pull_request_url_parts(url).is_some_and(|(owner_repo, _)| owner_repo == repo)
            })
            .cloned(),
        None => urls.first().cloned(),
    }
}

/// `PresentationSession.work` (packages/gx-protocol/src/presentation.rs); `None` outside work mode.
pub(crate) fn presentation_session_work(project: &Value, session: &Value) -> Option<Value> {
    if !project_work_mode(project) {
        return None;
    }
    let targets = work_targets(project, session);
    let mut output = Map::new();
    if let Some(branch) = &targets.branch {
        output.insert("branch".to_string(), json!(branch));
    }
    if let Some(pull_request) = presentation_pull_request(&targets) {
        if pull_request.get("state").and_then(Value::as_str) == Some("merged") {
            let key = work_cleanup_pull_request_key(
                pull_request
                    .get("number")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                pull_request.get("url").and_then(Value::as_str),
            );
            if issues_workflow_settings().offer_cleanup_when_pr_merges
                && offers_work_cleanup(session, &key)
            {
                output.insert("offerCleanup".to_string(), json!(true));
            }
        }
        output.insert("pullRequest".to_string(), pull_request);
    }
    // The menu's "Back to automatic" only means something when a link was set by hand.
    let manual = manual_work_links(session);
    if manual != ManualWorkLinks::default() {
        output.insert("handSet".to_string(), json!(true));
    }

    let linear_identifiers = targets.ids_of(WorkSourceId::Linear);
    let issues: Vec<Option<LinearIssueInfo>> = linear_identifiers
        .iter()
        .map(|identifier| {
            match targets
                .linear_key
                .map(|key| cached_linear_issue_read(key, identifier))
            {
                Some(LinearIssueRead::Found(info)) => Some(info),
                Some(LinearIssueRead::NotFound) => Some(LinearIssueInfo::not_found(identifier)),
                Some(LinearIssueRead::Unread) | None => None,
            }
        })
        .collect();
    let linear_issues: Vec<Value> = linear_identifiers
        .iter()
        .zip(&issues)
        .map(|(identifier, info)| {
            let mut issue = Map::new();
            issue.insert("identifier".to_string(), json!(identifier));
            if let Some(info) = info {
                insert_text(&mut issue, "title", info.title.as_deref());
                insert_text(&mut issue, "stateType", info.state_type.as_deref());
                insert_text(&mut issue, "stateName", info.state_name.as_deref());
                insert_text(&mut issue, "url", info.url.as_deref());
            }
            Value::Object(issue)
        })
        .collect();
    if !linear_issues.is_empty() {
        output.insert("linearIssues".to_string(), Value::Array(linear_issues));
    }

    let github_issues: Vec<Value> = targets
        .github_issue_numbers()
        .iter()
        .map(|number| {
            let mut issue = Map::new();
            issue.insert("number".to_string(), json!(number));
            if let Some(info) = targets
                .cwd
                .as_deref()
                .and_then(|cwd| cached_work_github_issue(cwd, *number))
            {
                insert_text(&mut issue, "title", info.title.as_deref());
                insert_text(&mut issue, "state", info.state.as_deref());
                insert_text(&mut issue, "url", info.url.as_deref());
            }
            Value::Object(issue)
        })
        .collect();
    if !github_issues.is_empty() {
        output.insert("githubIssues".to_string(), Value::Array(github_issues));
    }

    let scope = TicketScope {
        project_id: project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        cwd: targets.cwd.as_deref(),
        linear_api_key: None,
    };
    let repo = targets.cwd.as_deref().and_then(cached_work_repo);
    let tickets: Vec<Value> = targets
        .tickets
        .iter()
        .map(|LinkedTicket { ticket, by_hand }| {
            let read = work_source(ticket.source).cached_ticket(&scope, &ticket.id);
            let mut link =
                ticket_link_value(&ticket, repo.as_deref().unwrap_or(scope.project_id), &read);
            link["source"] = json!(if *by_hand { "user" } else { "branch" });
            link["sourceLabel"] = json!(if *by_hand { "You added" } else { "From branch" });
            link
        })
        .collect();
    if !tickets.is_empty() {
        output.insert("tickets".to_string(), Value::Array(tickets));
    }

    // CDXC:WorkMode 2026-10-09 DECISION:
    // User: the card shows the project kind of the workspace's primary tracker: a Linear project for Linear, a GitHub Project for GitHub (crate::work_mode::tracker).
    let tracker = cached_project_work_tracker(
        project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let linear_project = match &targets.linear_project {
        _ if tracker != WorkSourceId::Linear => None,
        Some(Some(name)) => Some(json!({ "name": name })),
        Some(None) => None,
        None => issues.iter().flatten().find_map(|issue| {
            issue.project_name.as_ref().map(|name| {
                let mut project = Map::new();
                project.insert("name".to_string(), json!(name));
                insert_text(&mut project, "url", issue.project_url.as_deref());
                Value::Object(project)
            })
        }),
    };
    if let Some(linear_project) = linear_project {
        output.insert("linearProject".to_string(), linear_project);
    }
    if tracker == WorkSourceId::Github {
        if let Some(github_project) = session_github_project(&targets) {
            output.insert(
                "githubProject".to_string(),
                presentation_github_project(&github_project),
            );
        }
    }
    Some(Value::Object(output))
}

/// The GitHub Project a session links to: the hand-set one, else the first project item of its
/// GitHub issues, then of its PR.
pub(crate) fn session_github_project(targets: &WorkTargets) -> Option<GithubProjectInfo> {
    match &targets.github_project {
        Some(Some(reference)) => {
            let (owner, number) = parse_github_project_reference(reference)?;
            Some(
                cached_github_project(&owner, number).unwrap_or(GithubProjectInfo {
                    owner,
                    number,
                    title: None,
                    url: None,
                    status: None,
                }),
            )
        }
        Some(None) => None,
        None => {
            let cwd = targets.cwd.as_deref()?;
            targets
                .github_issue_numbers()
                .into_iter()
                .chain(
                    targets
                        .pull_request
                        .as_deref()
                        .and_then(pull_request_number_of),
                )
                .find_map(|number| cached_github_project_items(cwd, number).into_iter().next())
        }
    }
}

/// `123` or `https://github.com/o/r/pull/123` → 123.
pub(crate) fn pull_request_number_of(selector: &str) -> Option<u64> {
    let selector = selector.trim().trim_end_matches('/');
    selector
        .parse()
        .ok()
        .or_else(|| selector.rsplit_once("/pull/")?.1.parse().ok())
}

fn presentation_github_project(project: &GithubProjectInfo) -> Value {
    let mut output = Map::new();
    output.insert("owner".to_string(), json!(project.owner));
    output.insert("number".to_string(), json!(project.number));
    insert_text(&mut output, "title", project.title.as_deref());
    insert_text(&mut output, "url", project.url.as_deref());
    insert_text(&mut output, "status", project.status.as_deref());
    Value::Object(output)
}

fn presentation_pull_request(targets: &WorkTargets) -> Option<Value> {
    let selector = targets.pull_request.as_deref()?;
    let cwd = targets.cwd.as_deref().unwrap_or_default();
    if let Some(info) = cached_work_pull_request(cwd, selector) {
        let mut pull_request = Map::new();
        pull_request.insert("number".to_string(), json!(info.number));
        pull_request.insert("state".to_string(), json!(info.state.as_wire()));
        insert_text(&mut pull_request, "url", info.url.as_deref());
        insert_text(&mut pull_request, "checks", info.checks);
        return Some(Value::Object(pull_request));
    }
    // Not read yet: the branch's own PR still has its state from the git probe.
    let number: u64 = selector.parse().ok()?;
    if targets.branch_pull_request != Some(number) {
        return None;
    }
    let git = targets.cwd.as_deref().and_then(cached_session_git_status)?;
    let branch_pull_request = git.pull_request?;
    let mut pull_request = Map::new();
    pull_request.insert("number".to_string(), json!(number));
    pull_request.insert(
        "state".to_string(),
        json!(branch_pull_request.state.as_wire()),
    );
    insert_text(&mut pull_request, "url", branch_pull_request.url.as_deref());
    Some(Value::Object(pull_request))
}

/// The title a session shows while **Title sessions by their branch** is on: its branch title,
/// unless the person renamed it. Any project: a branch title needs only git.
pub(crate) fn work_display_title(
    project: &Value,
    session: &Value,
    title_source: Option<&str>,
) -> Option<String> {
    if !issues_workflow_settings().title_sessions_by_branch || title_source == Some("user") {
        return None;
    }
    let cwd = effective_session_git_cwd(session, Some(project))?;
    let git = cached_session_git_status(&cwd)?;
    work_branch(git.branch.as_deref()).and_then(branch_title)
}

fn insert_text(output: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        output.insert(key.to_string(), json!(value));
    }
}
