//! Running session links (crate::session_links): the background pass that finds the links that
//! join each open session by themselves, keeps what is known about every link current, and
//! publishes the rows whose links changed.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: a change in a linked PR tells you through the chips only ("dot-only"): failing checks, a review asked for and a merge show on the chips and in the list, and no notification is sent.
//! CDXC:SessionLinks 2026-10-24 WHY:
//! Merged and closed PRs, closed issues and done or canceled Linear tickets are final, so once read they are never read again; the `gh` budget goes to open work. Only sessions that are not closed are refreshed, like work mode's pass (CDXC:WorkMode 2026-10-23).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;

use super::{schedule_presentation_session_delta, AppState};
use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::session_git_status::{
    cached_session_git_status, effective_session_git_cwd, gh_cli_is_available,
    PullRequestDisposition,
};
use crate::session_links::{
    forget_session_link_mentions_except, needs_full_scan, of_linear_team, parse_github_url,
    presentation_session_links, remember_session_link_mentions, scan_transcript,
    session_link_entries, session_links_read_snapshot, stored_session_links, write_session_links,
    SessionLinkKind, SessionLinkRef, SessionLinkSource, StoredSessionLink, MENTIONED_BY_USER,
};
use crate::storage::open_gxserver_database;
use crate::work_mode::{
    cached_linear_team_keys, cached_work_github_issue_url, cached_work_pull_request,
    cached_work_repo, cached_work_repos, github_read_due, github_read_known, github_read_max_age,
    linear_api_key, linear_key_fingerprint, refresh_github_reads, refresh_linear_issues,
    refresh_linear_team_keys, refresh_work_repo, remember_project_linear_key, seed_github_read,
    session_is_idle, work_branch, GithubRead,
};

/// `gh` calls one pass may make (work mode's pass has its own).
const MAX_GH_CALLS_PER_PASS: usize = 20;
/// Requests one pass may send to Linear.
const MAX_LINEAR_REQUESTS_PER_PASS: usize = 3;
/// Transcripts one pass may read in full (a first read); later reads only add what is new.
const MAX_FULL_SCANS_PER_PASS: usize = 4;

/// What each open session last published as `links`, so a pass only sends the rows that changed.
fn published_links() -> &'static Mutex<HashMap<(String, String), String>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    PUBLISHED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ids(session: &Value) -> Option<(String, String)> {
    Some((
        session.get("projectId")?.as_str()?.to_string(),
        session.get("sessionId")?.as_str()?.to_string(),
    ))
}

/// One pass. Blocking; runs on the 60s background worker after work mode's pass, which fills the
/// caches both read.
pub(crate) fn run_session_links_refresh_once(
    state: &Arc<AppState>,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let projects_by_id: HashMap<String, &Value> = projects
        .iter()
        .filter_map(|project| Some((project.get("projectId")?.as_str()?.to_string(), project)))
        .collect();
    let mut sessions = Vec::new();
    for project_id in projects_by_id.keys() {
        sessions.extend(repository.list_sessions_excluding_stopped(Some(project_id))?);
    }

    // Each project's Linear key, remembered for the projection like work mode does for its own.
    let mut linear_keys: HashMap<u64, String> = HashMap::new();
    for (project_id, project) in &projects_by_id {
        let workspace_id = crate::workspaces::stored_project_workspace_id(project);
        let key = linear_api_key(&state.paths, Some(project_id), workspace_id);
        let fingerprint = key.as_deref().map(linear_key_fingerprint);
        remember_project_linear_key(project_id, fingerprint);
        if let (Some(fingerprint), Some(key)) = (fingerprint, key) {
            linear_keys.insert(fingerprint, key);
        }
    }
    let mut linear_budget = MAX_LINEAR_REQUESTS_PER_PASS;
    for key in linear_keys.values() {
        if linear_budget > 0 && refresh_linear_team_keys(key) {
            linear_budget -= 1;
        }
    }

    // How many open sessions share each checkout: an earlier branch's PR is only kept for a
    // session that has its checkout to itself (a worktree), or every session in the project's
    // folder would collect it.
    let mut sessions_per_cwd: HashMap<String, usize> = HashMap::new();
    for session in &sessions {
        let project = session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|id| projects_by_id.get(id));
        if let Some(cwd) = effective_session_git_cwd(session, project.copied()) {
            *sessions_per_cwd.entry(cwd).or_default() += 1;
        }
    }

    // The sessions used most recently are read first, so after a restart the ones in use get
    // their links within a pass or two while the rest catch up a few per pass.
    sessions.sort_by(|a, b| {
        let at = |session: &Value| {
            session
                .get("lastActiveAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        at(b).cmp(&at(a))
    });
    let mut full_scans = MAX_FULL_SCANS_PER_PASS;
    let mut open_sessions: HashSet<(String, String)> = HashSet::new();
    for session in &sessions {
        let Some((project_id, session_id)) = ids(session) else {
            continue;
        };
        open_sessions.insert((project_id.clone(), session_id.clone()));
        let Some(project) = projects_by_id.get(&project_id) else {
            continue;
        };
        let cwd = effective_session_git_cwd(session, Some(project));
        if let Some(cwd) = cwd.as_deref() {
            refresh_work_repo(cwd);
        }
        // The project's own GitHub repos (its checkout's origin and upstream): only their PRs and
        // issues join or are suggested by themselves (`SessionLinkRef::in_repos`).
        let own_repos = cwd.as_deref().map(cached_work_repos).unwrap_or_default();
        let mut stored = stored_session_links(session);
        let mut changed = false;
        // Links to another repo that joined by themselves before this rule leave again; one a
        // person added, the agent reported or anyone pinned stays.
        if !own_repos.is_empty() {
            let before = stored.items.len();
            let pinned = stored.pinned.clone();
            stored.items.retain(|item| {
                !matches!(
                    item.source,
                    SessionLinkSource::AgentOpened | SessionLinkSource::Closes
                ) || item.link.in_repos(&own_repos)
                    || pinned.contains(&item.link.key())
            });
            changed |= stored.items.len() != before;
        }
        let found = |link: SessionLinkRef, source: SessionLinkSource, via: Option<String>| {
            StoredSessionLink {
                link,
                source,
                via,
                url: None,
                added_at: Some(crate::session_links::now_iso()),
            }
        };

        // The branch's PR, kept so it stays listed after the session moves to another branch.
        if let Some(cwd) = cwd.as_deref() {
            if sessions_per_cwd.get(cwd) == Some(&1) {
                if let Some(git) = cached_session_git_status(cwd) {
                    let link = git
                        .pull_request
                        .as_ref()
                        .and_then(|pull_request| pull_request.url.as_deref())
                        .and_then(parse_github_url);
                    if let (Some(link), Some(branch)) = (link, work_branch(git.branch.as_deref())) {
                        changed |= stored.add_found(found(
                            link,
                            SessionLinkSource::Branch,
                            Some(branch.to_string()),
                        ));
                    }
                }
            }
        }

        // What the transcript says.
        if let Some((agent, path)) = session_transcript(state, &project_id, &session_id) {
            let first_read = needs_full_scan(&path);
            if !first_read || full_scans > 0 {
                if first_read {
                    full_scans -= 1;
                }
                if let Some(links) = scan_transcript(agent, &path, &project_id, &session_id) {
                    // An orchestrator's own replies only repeat its threads' reports (Q683).
                    let orchestrator =
                        crate::coordinators::read_coordinator(&db, &project_id, &session_id)
                            .ok()
                            .flatten()
                            .is_some();
                    let team_keys = crate::work_mode::project_linear_key(&project_id)
                        .and_then(cached_linear_team_keys)
                        .unwrap_or_default();
                    // A URL of another repo the chat read (research, a dependency) is not offered;
                    // a Linear ticket only an agent named is offered, never linked (Q666).
                    remember_session_link_mentions(
                        &project_id,
                        &session_id,
                        links.suggestions(&own_repos, &team_keys, orchestrator),
                    );
                    for link in links
                        .opened
                        .into_iter()
                        .filter(|link| link.in_repos(&own_repos))
                    {
                        changed |=
                            stored.add_found(found(link, SessionLinkSource::AgentOpened, None));
                    }
                    // Tickets other agents' reports or the agent's replies linked before the
                    // user's-own-words rule leave (`recheck_mentions`).
                    changed |= stored.recheck_mentions(&links.linear_mentions);
                    for identifier in links.linear_mentions {
                        if of_linear_team(&identifier, &team_keys) {
                            // Dismissed as a suggestion, then typed by the user: it links.
                            changed |= stored.relink_typed(&format!("linear:{identifier}"));
                            changed |= stored.add_found(found(
                                SessionLinkRef::Linear { identifier },
                                SessionLinkSource::Mentioned,
                                Some(MENTIONED_BY_USER.to_string()),
                            ));
                        }
                    }
                }
            }
        }

        // The issues a linked PR closes.
        let repo = cwd.as_deref().and_then(cached_work_repo);
        for entry in session_link_entries(project, session) {
            if entry.link.kind() != SessionLinkKind::PullRequest {
                continue;
            }
            let Some(url) = entry.link.github_url() else {
                continue;
            };
            let Some(info) = cached_work_pull_request("", &url) else {
                continue;
            };
            let via = entry.link.label(repo.as_deref());
            for issue in info
                .closing_issues
                .iter()
                .filter_map(|url| parse_github_url(url))
                .filter(|issue| issue.in_repos(&own_repos))
            {
                changed |=
                    stored.add_found(found(issue, SessionLinkSource::Closes, Some(via.clone())));
            }
        }

        if changed {
            write_session_links(&db, &project_id, &session_id, &stored)?;
        }
    }
    forget_session_link_mentions_except(&open_sessions);

    // Re-read: the stored links may have changed above.
    let mut sessions = Vec::new();
    for project_id in projects_by_id.keys() {
        sessions.extend(repository.list_sessions_excluding_stopped(Some(project_id))?);
    }
    refresh_link_caches(
        state,
        &projects_by_id,
        &sessions,
        &linear_keys,
        linear_budget,
    );

    let mut changed = Vec::new();
    if let Ok(mut published) = published_links().lock() {
        published.retain(|key, _| open_sessions.contains(key));
        for session in &sessions {
            let Some(key) = ids(session) else {
                continue;
            };
            let Some(project) = projects_by_id.get(&key.0) else {
                continue;
            };
            let fingerprint = presentation_session_links(project, session)
                .map(|links| links.to_string())
                .unwrap_or_default();
            if published.get(&key) != Some(&fingerprint) {
                published.insert(key.clone(), fingerprint);
                changed.push(key);
            }
        }
    }
    // What was read is kept with each session's links, so the lists show it right after the next
    // start (`entry_value` in server/src/session_links/presentation.rs).
    for session in &sessions {
        let (Some((project_id, session_id)), Some(project)) = (
            ids(session),
            session
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|id| projects_by_id.get(id)),
        ) else {
            continue;
        };
        let Some(links) = presentation_session_links(project, session) else {
            continue;
        };
        let snapshot = session_links_read_snapshot(&links);
        // Read again right before writing, so a pin or a removal made meanwhile is kept.
        let current = repository
            .get_session(&project_id, &session_id)?
            .unwrap_or_else(|| session.clone());
        let mut stored = stored_session_links(&current);
        if snapshot != stored.read {
            stored.read = snapshot;
            write_session_links(&db, &project_id, &session_id, &stored)?;
        }
    }
    for (project_id, session_id) in changed {
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
    }
    Ok(())
}

/// The session's transcript, when its agent writes one Ghostex can read.
fn session_transcript(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<(
    crate::session_chat::SessionChatTranscriptAgent,
    std::path::PathBuf,
)> {
    let resolution = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    )
    .ok()?;
    let (family, path) = resolution.transcript();
    Some((
        crate::session_chat::resolve_session_chat_transcript_agent(family)?,
        path?.to_path_buf(),
    ))
}

/// Reads what is due among every open session's links, within the pass's budgets: a link of a
/// session in use every 2 (PR) or 5 (issue) minutes, of a session idle for more than a day every
/// 30, of a session whose links list was just opened at once; a finished one is read once. GitHub
/// reads go through one GraphQL call per repo (work_mode/github_batch.rs).
fn refresh_link_caches(
    state: &AppState,
    projects_by_id: &HashMap<String, &Value>,
    sessions: &[Value],
    linear_keys: &HashMap<u64, String>,
    mut linear_budget: usize,
) {
    let now = SystemTime::now();
    let wanted = wanted_sessions();
    // The sessions used most recently first, so the ones in use get their links read first.
    let mut sessions: Vec<&Value> = sessions.iter().collect();
    sessions.sort_by(|a, b| {
        let at = |session: &Value| {
            session
                .get("lastActiveAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        at(b).cmp(&at(a))
    });
    // Every PR and issue to read, in session order, with the shortest max age any session gives
    // it; `None` for a link only suggested, which is read once.
    let mut reads: Vec<(GithubRead, Option<Duration>)> = Vec::new();
    let mut want = |read: GithubRead, max_age: Option<Duration>| match reads
        .iter_mut()
        .find(|(known, _)| known.selector == read.selector)
    {
        Some((_, known_age)) => {
            *known_age = match (*known_age, max_age) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        None => reads.push((read, max_age)),
    };
    let mut linear: HashMap<u64, Vec<String>> = HashMap::new();
    for session in &sessions {
        let Some(project) = session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|id| projects_by_id.get(id))
        else {
            continue;
        };
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let idle = session_is_idle(session, now);
        let is_wanted = wanted.contains(&(project_id.to_string(), session_id.to_string()));
        // What a restart forgot comes back from what the session kept (`sessionLinks.read`).
        let kept = stored_session_links(session).read;
        for entry in session_link_entries(project, session) {
            match &entry.link {
                SessionLinkRef::Github { kind, .. } => {
                    let pull_request = *kind == SessionLinkKind::PullRequest;
                    let Some(read) = entry
                        .link
                        .github_url()
                        .and_then(|url| GithubRead::of_url(&url, pull_request))
                    else {
                        continue;
                    };
                    if let Some(snapshot) = kept.get(&entry.link.key()).and_then(Value::as_object) {
                        seed_github_read(&read, snapshot);
                    }
                    let finished = if pull_request {
                        cached_work_pull_request("", &read.selector).is_some_and(|info| {
                            info.state.disposition() == PullRequestDisposition::Finished
                        })
                    } else {
                        cached_work_github_issue_url(&read.selector)
                            .is_some_and(|info| info.state.as_deref() == Some("closed"))
                    };
                    let max_age =
                        (!finished).then(|| github_read_max_age(pull_request, idle, is_wanted));
                    want(read, max_age);
                }
                SessionLinkRef::Linear { identifier } => {
                    if let Some(key) = crate::work_mode::project_linear_key(project_id) {
                        let finished = crate::work_mode::cached_linear_issue(key, identifier)
                            .is_some_and(|info| {
                                matches!(
                                    info.state_type.as_deref(),
                                    Some("completed") | Some("canceled")
                                )
                            });
                        let list = linear.entry(key).or_default();
                        if !finished && !list.contains(identifier) {
                            list.push(identifier.clone());
                        }
                    }
                }
                // The command's own card pass queues its reads (work_mode/command/jobs.rs).
                SessionLinkRef::Command { .. } => {}
            }
        }
        // The PRs, issues and Linear tickets this session's chat only mentioned, for the
        // Suggested rows' titles (and a Linear ticket's URL, which Link sends): read once, right
        // after the session's own links.
        for link in crate::session_links::session_link_mentions(project_id, session_id) {
            if let SessionLinkRef::Linear { identifier } = &link {
                if let Some(key) = crate::work_mode::project_linear_key(project_id) {
                    let list = linear.entry(key).or_default();
                    let unread = matches!(
                        crate::work_mode::cached_linear_issue_read(key, identifier),
                        crate::work_mode::LinearIssueRead::Unread
                    );
                    if unread && !list.contains(identifier) {
                        list.push(identifier.clone());
                    }
                }
                continue;
            }
            let pull_request = link.kind() == SessionLinkKind::PullRequest;
            if let Some(read) = link
                .github_url()
                .and_then(|url| GithubRead::of_url(&url, pull_request))
            {
                want(read, None);
            }
        }
    }
    for (fingerprint, identifiers) in &linear {
        if linear_budget == 0 {
            break;
        }
        if let Some(key) = linear_keys.get(fingerprint) {
            linear_budget = linear_budget.saturating_sub(refresh_linear_issues(
                key,
                identifiers,
                &[],
                linear_budget,
            ));
        }
    }
    // CDXC:SessionLinks 2026-10-24 WHY:
    // Every open session's PRs used to be read before any issue, so with a dozen open PRs across sessions the pass spent its whole `gh` budget on them and issues (and the Suggested rows) never got a title or a state: the user saw "#283" with nothing after it, counted as open while it was closed. Links never read come first now, issues and suggestions included, then the due open ones.
    let mut due: Vec<GithubRead> = reads
        .iter()
        .filter(|(read, _)| !github_read_known(read) && github_read_due(read, Duration::ZERO))
        .map(|(read, _)| read.clone())
        .collect();
    due.extend(
        reads
            .iter()
            .filter(|(read, max_age)| {
                github_read_known(read)
                    && max_age.is_some_and(|max_age| github_read_due(read, max_age))
            })
            .map(|(read, _)| read.clone()),
    );
    // What the earlier rules would have read this pass: every link past 2 (PR) or 5 (issue)
    // minutes, whatever its session's activity, one `gh` call each.
    let due_before = reads
        .iter()
        .filter(|(read, max_age)| {
            !github_read_known(read)
                || (max_age.is_some()
                    && github_read_due(read, github_read_max_age(read.pull_request, false, false)))
        })
        .count();
    let calls = if due.is_empty() || !gh_cli_is_available() {
        0
    } else {
        refresh_github_reads(&due, MAX_GH_CALLS_PER_PASS)
    };
    log_session_link_reads(state, reads.len(), due_before, due.len(), calls);
}

/// The sessions whose links list was opened in the last minute (`want_session_links_now`).
fn wanted_links() -> &'static Mutex<HashMap<(String, String), Instant>> {
    static WANTED: OnceLock<Mutex<HashMap<(String, String), Instant>>> = OnceLock::new();
    WANTED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn wanted_sessions() -> HashSet<(String, String)> {
    let Ok(mut wanted) = wanted_links().lock() else {
        return HashSet::new();
    };
    wanted.retain(|_, at| at.elapsed() < Duration::from_secs(60));
    wanted.keys().cloned().collect()
}

/// A session's links list (its card, the header's "+N" list, the phone's sheet) just opened: its
/// links are read on the next pass, which this starts now, whatever their age (within the pass's
/// budget and at most every 20 seconds).
pub(crate) fn want_session_links_now(state: &Arc<AppState>, project_id: &str, session_id: &str) {
    if let Ok(mut wanted) = wanted_links().lock() {
        wanted.insert(
            (project_id.to_string(), session_id.to_string()),
            Instant::now(),
        );
    }
    spawn_session_links_refresh(state);
}

/// What one pass read, for the Session links diagnostic log: the links it follows, what the
/// earlier rules would have read (one `gh` call each, at most 20), what was due under the new ones,
/// and the `gh` calls it made.
fn log_session_link_reads(
    state: &AppState,
    followed: usize,
    due_before: usize,
    due: usize,
    calls: usize,
) {
    let _ = state.logger.log_routine(
        crate::logging::DiagnosticLogScenario::SessionLinkReads,
        GxserverLogInput {
            level: LogLevel::Info,
            event: "sessionLinksReadPass".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(serde_json::json!({
                "followed": followed,
                "dueBefore": due_before,
                "callsBefore": due_before.min(MAX_GH_CALLS_PER_PASS),
                "due": due,
                "ghCalls": calls,
                // Every `gh` gxserver started since the last pass that logged (work mode's pass
                // and the branch PR probes included).
                "ghProcessesSinceLast": crate::session_git_status::take_gh_command_count(),
            })),
        },
    );
}

/// Runs a pass right away (off the request path), so a link someone just added shows its state
/// without waiting for the next minute.
pub(crate) fn spawn_session_links_refresh(state: &Arc<AppState>) {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = run_session_links_refresh_once(&state) {
            log_session_links_refresh_failure(&state, &error.message);
        }
    });
}

pub(crate) fn log_session_links_refresh_failure(state: &AppState, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionLinksRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}
