//! The background refresh of every cache work mode reads. Blocking (Linear and `gh` are network
//! calls), so it only ever runs on a blocking worker, never on a request path.

use std::collections::HashMap;

use serde_json::Value;

use crate::paths::GxserverPaths;
use crate::session_git_status::gh_cli_is_available;

use super::*;

/// Reads which Linear key applies to every project (a local file, no network) and whether `gh` is
/// signed in, the two facts Issues Workflow being on depends on (`issues_workflow_parts`), so the
/// pass that follows knows which projects are on.
pub(crate) fn refresh_issues_workflow_setup(paths: &GxserverPaths, projects: &[Value]) {
    gh_cli_is_available();
    for project in projects {
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        let workspace_id = crate::workspaces::stored_project_workspace_id(project);
        let key = linear_api_key(paths, Some(project_id), workspace_id);
        remember_project_linear_key(project_id, key.as_deref().map(linear_key_fingerprint));
    }
}

/// Refreshes what the given sessions of work-mode projects link to, within the per-pass budgets.
pub(crate) fn refresh_work_caches(paths: &GxserverPaths, projects: &[Value], sessions: &[Value]) {
    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter(|project| project_work_mode(project))
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|id| (id, project))
        })
        .collect();
    if projects_by_id.is_empty() {
        return;
    }

    // Which tracker each project uses (a team's choice is read from Convex at most every few
    // minutes), remembered for the projection.
    let workspaces = crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| crate::workspaces::read_sidebar_workspaces(&db).ok())
        .unwrap_or(Value::Null);
    let mut team_workspaces: Vec<String> = projects_by_id
        .values()
        .map(|project| crate::workspaces::project_workspace_id(&workspaces, project, projects))
        .collect();
    team_workspaces.sort();
    team_workspaces.dedup();
    for workspace_id in &team_workspaces {
        refresh_team_work_tracker(paths, workspace_id);
    }
    let mut github_tracker_projects: Vec<&str> = Vec::new();
    for (project_id, project) in &projects_by_id {
        let tracker = project_work_tracker(paths, &workspaces, project, projects);
        remember_project_work_tracker(project_id, tracker);
        if tracker == WorkSourceId::Github {
            github_tracker_projects.push(project_id);
        }
    }

    // Which key each project uses, remembered for the projection.
    let mut keys: HashMap<u64, String> = HashMap::new();
    for (project_id, project) in &projects_by_id {
        let workspace_id = crate::workspaces::stored_project_workspace_id(project);
        let key = linear_api_key(paths, Some(project_id), workspace_id);
        let fingerprint = key.as_deref().map(linear_key_fingerprint);
        remember_project_linear_key(project_id, fingerprint);
        if let (Some(fingerprint), Some(key)) = (fingerprint, key) {
            keys.insert(fingerprint, key);
        }
    }

    let mut linear_budget = MAX_LINEAR_REQUESTS_PER_PASS;
    for key in keys.values() {
        if linear_budget == 0 {
            break;
        }
        if refresh_linear_team_keys(key) {
            linear_budget -= 1;
        }
    }

    let session_targets = |sessions: &[Value]| -> Vec<WorkTargets> {
        sessions
            .iter()
            .filter_map(|session| {
                let project_id = session.get("projectId").and_then(Value::as_str)?;
                let project = projects_by_id.get(project_id)?;
                Some(work_targets(project, session))
            })
            .collect()
    };

    let mut wanted_issues: HashMap<u64, Vec<String>> = HashMap::new();
    // Issues of sessions with no PR yet, re-read sooner so the ticket's new PR shows up quickly.
    let mut awaiting_pull_request: HashMap<u64, Vec<String>> = HashMap::new();
    for targets in session_targets(sessions) {
        // The repo picks the ticket's PR in the session's own repo (presentation.rs).
        if let Some(cwd) = targets.cwd.as_deref() {
            refresh_work_repo(cwd);
        }
        if let Some(key) = targets.linear_key {
            let identifiers = targets.ids_of(WorkSourceId::Linear);
            wanted_issues
                .entry(key)
                .or_default()
                .extend(identifiers.iter().cloned());
            if targets.pull_request.is_none() {
                awaiting_pull_request
                    .entry(key)
                    .or_default()
                    .extend(identifiers);
            }
        }
    }
    for (fingerprint, identifiers) in &wanted_issues {
        if linear_budget == 0 {
            break;
        }
        if let Some(key) = keys.get(fingerprint) {
            linear_budget = linear_budget.saturating_sub(refresh_linear_issues(
                key,
                identifiers,
                awaiting_pull_request
                    .get(fingerprint)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                linear_budget,
            ));
        }
    }

    if !gh_cli_is_available() {
        return;
    }
    // Resolved again: an issue Linear just answered for can bring its PR along. Due PRs and issues
    // are read one GraphQL call per repo, a session idle for more than a day every 30 minutes
    // (CDXC:SessionLinks 2026-10-24 in work_mode/github_batch.rs).
    let now = std::time::SystemTime::now();
    let mut due: Vec<GithubRead> = Vec::new();
    for session in sessions {
        let Some(project) = session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|project_id| projects_by_id.get(project_id))
        else {
            continue;
        };
        let targets = work_targets(project, session);
        let Some(cwd) = targets.cwd.as_deref() else {
            continue;
        };
        let idle = session_is_idle(session, now);
        let repo = cached_work_repo(cwd);
        let read_of = |selector: &str, pull_request: bool| -> Option<GithubRead> {
            let (repo, number) = match selector.parse::<u64>() {
                Ok(number) => (repo.clone()?, number),
                Err(_) => repo_and_number_of_url(selector)?,
            };
            Some(GithubRead {
                repo,
                number,
                pull_request,
                cwd: cwd.to_string(),
                selector: selector.to_string(),
            })
        };
        let mut wanted: Vec<GithubRead> = Vec::new();
        if let Some(selector) = targets.pull_request.as_deref() {
            wanted.extend(read_of(selector, true));
        }
        for number in &targets.github_issue_numbers() {
            wanted.extend(read_of(&number.to_string(), false));
        }
        for read in wanted {
            let max_age = github_read_max_age(read.pull_request, idle, false);
            if github_read_due(&read, max_age) && !due.contains(&read) {
                due.push(read);
            }
        }
    }
    let mut gh_budget =
        MAX_GH_CALLS_PER_PASS.saturating_sub(refresh_github_reads(&due, MAX_GH_CALLS_PER_PASS));

    // GitHub Projects, only for projects whose tracker is GitHub and while `gh` may read them.
    if github_tracker_projects.is_empty() {
        return;
    }
    if refresh_github_projects_access(false) == GithubProjectsAccess::MissingScope {
        return;
    }
    for session in sessions {
        if gh_budget == 0 {
            break;
        }
        let Some(project) = session
            .get("projectId")
            .and_then(Value::as_str)
            .filter(|project_id| github_tracker_projects.contains(project_id))
            .and_then(|project_id| projects_by_id.get(project_id))
        else {
            continue;
        };
        let targets = work_targets(project, session);
        match &targets.github_project {
            Some(Some(reference)) => {
                if let Some((owner, number)) = parse_github_project_reference(reference) {
                    if refresh_github_project(&owner, number) {
                        gh_budget -= 1;
                    }
                }
            }
            Some(None) => {}
            None => {
                let Some(cwd) = targets.cwd.as_deref() else {
                    continue;
                };
                let numbers = targets.github_issue_numbers().into_iter().chain(
                    targets
                        .pull_request
                        .as_deref()
                        .and_then(pull_request_number_of),
                );
                for number in numbers {
                    if gh_budget == 0 {
                        break;
                    }
                    if refresh_github_project_items(cwd, number) {
                        gh_budget -= 1;
                    }
                }
            }
        }
    }
}
