use super::*;

/*
CDXC:SessionTitles 2026-08-11:
Codex writes canonical thread names to session_index.jsonl, both for `/rename`
and for its automatic first-turn title generation, but does not update its OSC
terminal title or emit a provider hook. Track the index revision for each live,
identified Codex session on gxserver's clock and only reconcile when that file
changes. The sidebar then receives the ordinary authoritative presentation
delta without client-local title state.
*/
pub(crate) fn spawn_agent_metadata_title_sync_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let sync_state = state.clone();
    let revisions = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = sync_state.clone();
            let pass_revisions = revisions.clone();
            let _ = tokio::task::spawn_blocking(move || {
                run_agent_metadata_title_sync_once(&pass_state, &pass_revisions)
            })
            .await;

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(AGENT_METADATA_TITLE_SYNC_INTERVAL) => {}
            }
        }
    })
}

pub(crate) fn run_agent_metadata_title_sync_once(
    state: &Arc<AppState>,
    revisions: &Arc<Mutex<HashMap<String, String>>>,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    /*
    CDXC:StateSync 2026-09-01:
    This pass runs once a second and only ever looks at running rows, so the
    "running" filter belongs in SQL. On a registry with thousands of stopped
    rows the old full list hydrated every one of them — including six JSON
    columns each — to throw all but a dozen away.
    */
    let sessions = repository.running_sessions()?;
    let mut live_keys = HashSet::new();
    for session in sessions.iter() {
        let Some(project_id) = read_session_text(&session, "projectId") else {
            continue;
        };
        let Some(session_id) = read_session_text(&session, "sessionId") else {
            continue;
        };
        if let Some(target) = fork_initial_rename_retry_target(&session) {
            schedule_fork_initial_rename(state.as_ref().clone(), target);
        }
        let Some(revision) = agent_metadata_title_revision(&state.paths.home_dir, &session) else {
            continue;
        };
        let key = session_observer_key(&project_id, &session_id);
        live_keys.insert(key.clone());
        let already_checked = revisions
            .lock()
            .ok()
            .and_then(|checked| checked.get(&key).cloned())
            .as_deref()
            == Some(revision.as_str());
        if already_checked {
            continue;
        }
        // CDXC:SessionTitles 2026-11-11 WHY: one session's failure is logged and its revision left unrecorded so it retries next pass; returning would hold every later session until then.
        let synced = reconcile_agent_metadata_title_for_session(
            &repository,
            &project_id,
            &session_id,
            &state.paths.home_dir,
            "metadata-mismatch",
        )
        .and_then(|changed| {
            if changed {
                schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    &project_id,
                    &session_id,
                )?;
            }
            Ok(())
        });
        if let Err(error) = synced {
            log_agent_metadata_title_sync_failure(state, &project_id, &session_id, &error.message);
            continue;
        }
        if let Ok(mut checked) = revisions.lock() {
            checked.insert(key, revision);
        }
    }
    if let Ok(mut checked) = revisions.lock() {
        checked.retain(|key, _| live_keys.contains(key));
    }
    Ok(())
}

/*
CDXC:AgentScreenDetection 2026-08-24:
Follower health used to be reconciled ONLY when a presentation session delta
happened to arrive for that session. A follower that died or wedged while the
session produced no delta (which is exactly what a frozen chat looks like) then
stayed broken indefinitely. This sweep runs the same sync on gxserver's own
clock, so a dead or wedged task is aborted and respawned within one cadence.

Cost: `sync_session_chat_followers_for_all_sessions` returns before touching the
database whenever no follower entry has subscribers, so on an idle daemon this
is a map lock every ten seconds.
*/
pub(crate) fn spawn_session_chat_follower_sync_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let sync_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = sync_state.clone();
            let _ = tokio::task::spawn_blocking(move || {
                sync_session_chat_followers_for_all_sessions(&pass_state, "periodic-follower-sync")
            })
            .await;

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(SESSION_CHAT_FOLLOWER_SYNC_INTERVAL) => {}
            }
        }
    })
}

/*
CDXC:StateSync 2026-07-29-00:00:
Sidebar V2's auto-settle window and spent-snooze collection are server rules, so
they run on gxserver's own clock instead of whichever client happens to be open.
The pass is deliberately cheap (one SQLite read, at most
`SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS` narrow writes) and rides the existing
background-task shape: blocking work off the async worker, shutdown through the
shared broadcast, one presentation delta per changed session.

Precision: auto-settle boundaries are days away and snooze wakes are resolved
client-side from `snoozedUntil` to the millisecond, so a one-minute cadence adds
no user-visible latency.

Scope: the auto-settle rule only applies when the shared sidebar settings file
selects Sidebar V2 (`read_sweep_auto_settle_after_days` returns `None` for V1,
which is also the default). Spent-snooze collection and the activity reset run
for everyone, so a user who flips back to V1 with snoozed rows still gets that
state groomed.
*/
pub(crate) fn spawn_session_lifecycle_sweep_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let sweep_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = sweep_state.clone();
            let pass_result = tokio::task::spawn_blocking(move || {
                if let Err(error) = run_session_lifecycle_sweep_once(&pass_state) {
                    log_session_lifecycle_sweep_failure(&pass_state, &error.message);
                }
                /*
                CDXC:Drafts 2026-10-23 WHY:
                Draft recovery pruning (closed and deleted sessions, 20 checkpoints per draft;
                the DECISION in session_chat_draft_recovery.rs) rides this worker: the first pass
                runs at startup, and later passes find nothing to prune unless a session closed or
                a draft passed its cap, which costs two reads and no write lock. Independent
                statement, so a failure in either pass never skips the other.
                */
                if let Err(error) = run_draft_recovery_prune_once(&pass_state) {
                    log_draft_recovery_prune_failure(&pass_state, &error.message);
                }
            })
            .await;
            if pass_result.is_err() {
                log_session_lifecycle_sweep_failure(&sweep_state, "sweep task join failed");
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(SESSION_LIFECYCLE_SWEEP_INTERVAL) => {}
            }
        }
    })
}

pub(crate) fn run_session_lifecycle_sweep_once(
    state: &Arc<AppState>,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let auto_settle_after_days = session_lifecycle::read_sweep_auto_settle_after_days(&state.paths);
    let auto_settle_on_finished_pull_request =
        session_lifecycle::auto_settle_on_finished_pull_request(auto_settle_after_days);
    let options = session_lifecycle::SessionLifecycleSweepOptions {
        auto_settle_after_days,
        auto_settle_on_finished_pull_request,
        max_mutations: session_lifecycle::SESSION_LIFECYCLE_SWEEP_MAX_MUTATIONS,
        now_iso: now_iso(),
    };
    /*
    CDXC:Git 2026-07-29-00:00:
    The sweep reads the git-status cache the refresh pass below maintains; it
    never probes. A cwd that has not been probed yet resolves to "unknown", which
    settles nothing, so a cold daemon simply waits for its first refresh pass.

    CDXC:Git 2026-07-30 (effective cwd):
    The disposition is resolved through the session's project, exactly like the
    probe pass and presentation, so PR-driven auto-settle can fire for a cwd-less
    agent session instead of silently reading "unknown" forever. The lookup map is
    built once per sweep; a session whose project row is gone resolves `None` and
    keeps the old "unknown settles nothing" behaviour.
    */
    let projects = repository.list_projects()?;
    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|project_id| (project_id, project))
        })
        .collect();
    let outcome = session_lifecycle::run_session_lifecycle_sweep(
        &repository,
        &options,
        &|session: &Value| {
            let project = session
                .get("projectId")
                .and_then(Value::as_str)
                .and_then(|project_id| projects_by_id.get(project_id).copied());
            session_git_status::session_pull_request_disposition(session, project)
        },
    )?;
    for (project_id, session_id) in &outcome.changed {
        schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)?;
    }
    Ok(())
}

/*
CDXC:Git 2026-07-29-00:00:
Sidebar V2's card row (branch, +n −n, PR badge) is server-owned, so the probing
happens here on gxserver's own clock instead of in whichever client is open.
The pass shape mirrors the lifecycle sweep: blocking work off the async worker,
shutdown through the shared broadcast, one presentation delta per changed
session.

Cost control lives in `session_git_status`: probes are keyed by UNIQUE session
cwd, cached with TTLs, bounded per pass, and every subprocess is time-boxed. The
pass only emits deltas for cwds whose status MEANINGFULLY changed — a re-probe
that finds the same branch and the same counts is silent, so a quiet machine
produces no event traffic at all.
*/
pub(crate) fn spawn_session_git_status_refresh_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let refresh_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = refresh_state.clone();
            let pass_result = tokio::task::spawn_blocking(move || {
                /*
                CDXC:StateSync 2026-07-29:
                Session git status remains Sidebar V2-only (plus the projects with
                work mode on, see `run_session_git_status_refresh_once`), so read the
                current setting for that pass. Project origin URLs are now also shown by
                the classic project's Copy Remote URL menu item, so their pass
                runs for both sidebar versions below.
                */
                let sidebar_v2_selected =
                    session_lifecycle::read_sidebar_v2_selected(&pass_state.paths);
                if let Err(error) =
                    run_session_git_status_refresh_once(&pass_state, sidebar_v2_selected)
                {
                    log_session_git_status_refresh_failure(&pass_state, &error.message);
                }
                /*
                CDXC:StateSync 2026-07-29-00:00:
                The per-project `origin` remote probe rides this same blocking
                worker and this same 60s wake-up instead of adding a fifth
                background task: it is the same kind of work (time-boxed git
                spawns feeding a TTL cache that presentation only reads), and its
                own 10-minute TTL means almost every pass here is a no-op that
                spawns nothing. A failure in one pass must not skip the other, so
                they are independent statements rather than a `?` chain.
                */
                if let Err(error) = run_project_git_remote_refresh_once(&pass_state, true) {
                    log_project_git_remote_refresh_failure(&pass_state, &error.message);
                }
                /*
                CDXC:Icons 2026-07-29 (discovered icons):
                The project-icon discovery pass rides the same worker and the
                same wake-up as the two git passes above, for the same reasons:
                it is bounded filesystem work feeding a TTL cache presentation
                only reads, and with a 10/30-minute TTL almost every pass here
                finds nothing stale and touches no files at all. Project icons
                render in both sidebar versions, so this pass deliberately does
                not share the V2-only git-data gate. Independent statement, so a
                failure in one pass never skips another.
                */
                if let Err(error) = run_project_icon_refresh_once(&pass_state) {
                    log_project_icon_refresh_failure(&pass_state, &error.message);
                }
                // Work mode's Linear and `gh` status rides right after the git pass, which it reads.
                if let Err(error) = super::work_mode_sync::run_work_mode_refresh_once(&pass_state) {
                    super::work_mode_sync::log_work_mode_refresh_failure(
                        &pass_state,
                        &error.message,
                    );
                }
                // Every session's PR, issue and Linear links, after work mode's pass whose caches it shares.
                if let Err(error) =
                    super::session_links_sync::run_session_links_refresh_once(&pass_state)
                {
                    super::session_links_sync::log_session_links_refresh_failure(
                        &pass_state,
                        &error.message,
                    );
                }
                // Bot gateway dots and today's run counts: small file reads per bot and no spawn,
                // so they ride here too.
                if let Err(error) = super::bot_sync::run_bot_refresh_once(&pass_state) {
                    super::bot_sync::log_bot_warning(
                        &pass_state,
                        "botRefreshFailed",
                        &error.message,
                    );
                }
            })
            .await;
            if pass_result.is_err() {
                log_session_git_status_refresh_failure(&refresh_state, "refresh task join failed");
            }
            let abandoned_readers = session_git_status::take_abandoned_command_readers();
            if abandoned_readers > 0 {
                log_session_git_status_reader_abandoned(&refresh_state, abandoned_readers);
            }
            /*
            The `origin` probe keeps its own counter and its own event name: the
            two passes run different commands against different directories, so
            reporting one under the other's name would send anyone reading the
            log to the wrong probe.
            */
            let abandoned_remote_readers =
                session_git_status::take_abandoned_project_git_remote_readers();
            if abandoned_remote_readers > 0 {
                log_project_git_remote_reader_abandoned(&refresh_state, abandoned_remote_readers);
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(SESSION_GIT_STATUS_REFRESH_INTERVAL) => {}
            }
        }
    })
}

pub(crate) fn run_session_git_status_refresh_once(
    state: &Arc<AppState>,
    sidebar_v2_selected: bool,
) -> std::result::Result<(), DomainStateError> {
    /*
    CDXC:StateSync 2026-07-29:
    `refresh_session_git_status_cache` enforces the gate itself, so this early
    return is about the rest of the pass: on a V1 machine there is no reason to
    open SQLite and walk every session row to build a cwd set nobody will probe.
    */
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    /*
    CDXC:WorkMode 2026-10-09 WHY:
    Issues Workflow reads each session's branch and PR from this same probe, so a project with it
    on (or every project while Title sessions by their branch is on) is probed on a V1 machine
    too; every other project still waits for Sidebar V2.
    */
    let branch_titles = crate::work_mode::issues_workflow_settings().title_sessions_by_branch;
    let work_mode_projects: HashSet<String> = repository
        .list_projects()?
        .iter()
        .filter(|project| branch_titles || crate::work_mode::project_work_mode(project))
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    if !sidebar_v2_selected && work_mode_projects.is_empty() {
        return Ok(());
    }
    let probes_session = |session: &Value| {
        sidebar_v2_selected
            || session
                .get("projectId")
                .and_then(Value::as_str)
                .is_some_and(|project_id| work_mode_projects.contains(project_id))
    };
    /*
    Only LIVE sessions are probed, and many of them share a checkout, so the
    cache is fed the DEDUPLICATED cwd set: one git (and at most one `gh`) call
    answers every row pointing at that directory.

    This is narrower than what presentation publishes: pinned and stopped rows
    reach the sidebar too, and they are deliberately NOT probed. The trade-off is
    that such a row shows whatever git status its cwd last had (or none at all,
    if nothing live ever pointed there) rather than costing a git spawn a minute
    for a checkout nobody is working in.

    CDXC:Git 2026-07-30 (effective cwd):
    The set is built from each session's EFFECTIVE cwd (`session.cwd` else
    `project.path`, see `session_git_status::effective_session_git_cwd`), so the
    project ROOT of a cwd-less agent session actually gets probed. One project
    lookup map per pass keeps that resolution O(1) per session, and because the
    cache is keyed by cwd, one probe of the project path lights up every row
    pointing at it — including the stopped and pinned rows this pass never probes
    for.
    */
    let projects = repository.list_projects()?;
    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|project_id| (project_id, project))
        })
        .collect();
    let session_project = |session: &Value| -> Option<&Value> {
        session
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|project_id| projects_by_id.get(project_id).copied())
    };
    // Both loops below skip every row that is not active, and a stopped row never is
    // (`effective_lifecycle_state` refuses to promote it), so the stopped rows are left in SQL.
    let sessions = repository.list_sessions_excluding_stopped(None)?;
    let mut cwds: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for session in &sessions {
        if !crate::presentation::is_active(session) || !probes_session(session) {
            continue;
        }
        let Some(cwd) =
            session_git_status::effective_session_git_cwd(session, session_project(session))
        else {
            continue;
        };
        if seen.insert(cwd.clone()) {
            cwds.push(cwd);
        }
    }

    // The cwd set is already limited to what may be probed, so the gate is open for it.
    let changed: HashSet<String> =
        session_git_status::refresh_session_git_status_cache(&cwds, true)
            .into_iter()
            .collect();
    if changed.is_empty() {
        return Ok(());
    }
    for session in &sessions {
        if !crate::presentation::is_active(session) {
            continue;
        }
        let Some(cwd) =
            session_git_status::effective_session_git_cwd(session, session_project(session))
        else {
            continue;
        };
        if !changed.contains(&cwd) {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)?;
    }
    Ok(())
}

/*
CDXC:StateSync 2026-07-29-00:00:
Sidebar V2 groups the same repository across machines by its `origin` remote, so
gxserver resolves that remote for its own projects and ships it in presentation.

Scope: the projects presentation actually publishes — parked Recent Projects and
hidden carrier projects are skipped so they cost no git spawns. Registered
worktree projects resolve their FAMILY ROOT (see `project_git_remote_key`), so a
project and its worktrees share one probe and one answer.

Cost: with a 10-minute TTL almost every 60s pass finds nothing stale and spawns
nothing at all. A pass only emits deltas for projects whose remote actually
CHANGED, which for real repositories is approximately never.
*/
pub(crate) fn run_project_git_remote_refresh_once(
    state: &Arc<AppState>,
    enabled: bool,
) -> std::result::Result<(), DomainStateError> {
    // Keep the caller gate ahead of the database read and path-set assembly.
    if !enabled {
        return Ok(());
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let mut paths: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_git_remote::project_git_remote_key(project) else {
            continue;
        };
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    let changed: HashSet<String> =
        project_git_remote::refresh_project_git_remote_cache(&paths, enabled)
            .into_iter()
            .collect();
    if changed.is_empty() {
        return Ok(());
    }
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_git_remote::project_git_remote_key(project) else {
            continue;
        };
        if !changed.contains(&path) {
            continue;
        }
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")?;
    }
    Ok(())
}

/*
CDXC:Icons 2026-07-29 (discovered icons):
Sidebar V2 shows each project the icon its own repository ships, so gxserver
discovers that icon for its own projects and ships it in presentation.

Scope, keying, and cost mirror the `origin` pass exactly: only the projects
presentation publishes (parked and hidden ones cost nothing), keyed on the
worktree FAMILY ROOT so a project and its worktrees share one answer, and a delta
only for projects whose icon CONTENT actually changed — which for a real
repository is approximately never.

It is a separate pass rather than an extra step inside the `origin` one so that a
failure, a budget, or a TTL change on either side stays local to that probe.
*/
pub(crate) fn run_project_icon_refresh_once(
    state: &Arc<AppState>,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let mut paths: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_icon::project_icon_key(project) else {
            continue;
        };
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    }

    let changed: HashSet<String> = project_icon::refresh_project_icon_cache(&paths)
        .into_iter()
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(path) = project_icon::project_icon_key(project) else {
            continue;
        };
        if !changed.contains(&path) {
            continue;
        }
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")?;
    }
    Ok(())
}

pub(crate) fn log_project_icon_refresh_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "projectIconRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

pub(crate) fn log_project_git_remote_refresh_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "projectGitRemoteRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

pub(crate) fn log_session_git_status_refresh_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionGitStatusRefreshFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

/*
The pass itself is fine: a probe command exited, but something it left behind
still held its stdout pipe, so the thread draining that pipe had to be abandoned
instead of joined. Worth a line, because a machine that leaks these has a git or
`gh` helper surviving its process group.
*/
/// The same fact for the per-project `origin` probe, under its own event so the
/// leak is attributed to the pass that produced it.
pub(crate) fn log_project_git_remote_reader_abandoned(state: &Arc<AppState>, count: usize) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "projectGitRemoteReaderAbandoned".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({ "count": count })),
    });
}

pub(crate) fn log_session_git_status_reader_abandoned(state: &Arc<AppState>, count: usize) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionGitStatusReaderAbandoned".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({ "count": count })),
    });
}

pub(crate) fn run_draft_recovery_prune_once(
    state: &Arc<AppState>,
) -> std::result::Result<usize, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    crate::session_chat_draft_recovery::prune(&db)
}

pub(crate) fn log_draft_recovery_prune_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "draftRecoveryPruneFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

fn log_agent_metadata_title_sync_failure(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
    message: &str,
) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "agentMetadataTitleSyncFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: Some(serde_json::json!({ "projectId": project_id, "sessionId": session_id })),
    });
}

pub(crate) fn log_session_lifecycle_sweep_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionLifecycleSweepFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}

/*
CDXC:Worktrees 2026-07-29-00:00:
A worktree session starts on `ghostex/<8hex>` because nothing yet knows what the
work is. Once the session has a REAL title — from the auto-rename skill, from
first-prompt title generation, or from the user typing one — that name is the
better branch name, so gxserver renames the branch to `ghostex/<slug>`.

Why a pass instead of a hook: a title reaches a session through several
independent paths (rename RPC, agent-metadata reconciliation, the generated-title
job, terminal title events), and every one of them ends in the same durable
place. Reconciling from durable state is one code path instead of five, and it is
idempotent — `plan_worktree_branch_rename` is a pure function of the row, and the
marker it stamps makes the rename happen exactly once.

Cost: a pure read of the session table per minute. Git only runs when a rename is
actually due, and the pass touches at most
`WORKTREE_BRANCH_RENAME_MAX_PER_PASS` branches. A rename lands within one pass of
the title, and the branch label on the card refreshes with it because the git
status cache is re-probed for that cwd before the delta goes out.
*/
pub(crate) fn spawn_worktree_branch_rename_task(
    state: &Arc<AppState>,
) -> tokio::task::JoinHandle<()> {
    let sweep_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let pass_state = sweep_state.clone();
            let pass_result = tokio::task::spawn_blocking(move || {
                if let Err(error) = run_worktree_branch_rename_once(&pass_state) {
                    log_worktree_branch_rename_failure(&pass_state, &error.message);
                }
            })
            .await;
            if pass_result.is_err() {
                log_worktree_branch_rename_failure(&sweep_state, "rename task join failed");
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(WORKTREE_BRANCH_RENAME_SWEEP_INTERVAL) => {}
            }
        }
    })
}

pub(crate) fn run_worktree_branch_rename_once(
    state: &Arc<AppState>,
) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut plans = Vec::new();
    for (project_id, session_id) in worktree_branch_rename_candidates(&repository)? {
        if plans.len() >= worktree_sessions::WORKTREE_BRANCH_RENAME_MAX_PER_PASS {
            break;
        }
        let Some(session) = repository.get_session(&project_id, &session_id)? else {
            continue;
        };
        if let Some(plan) = worktree_sessions::plan_worktree_branch_rename(&session) {
            plans.push(plan);
        }
    }
    for plan in plans {
        /*
        Re-read the branch from the checkout itself: the marker says what
        gxserver created, but the user may have switched or renamed the branch
        in the meantime, and that decision wins.
        */
        if worktree_sessions::current_worktree_branch(&plan.worktree_path).as_deref()
            != Some(plan.from_branch.as_str())
        {
            continue;
        }
        let worktree_path = plan.worktree_path.clone();
        let Some(renamed) = worktree_sessions::resolve_renamed_branch_name(
            &plan.title,
            &plan.from_branch,
            &|candidate| worktree_sessions::worktree_branch_exists(&worktree_path, candidate),
        ) else {
            continue;
        };
        if !worktree_sessions::rename_worktree_branch(
            &plan.worktree_path,
            &plan.from_branch,
            &renamed,
        ) {
            continue;
        }
        let Some(session) = repository.get_session(&plan.project_id, &plan.session_id)? else {
            continue;
        };
        if let Some(runtime_settings) =
            worktree_sessions::runtime_settings_with_renamed_worktree_branch(
                &session,
                &renamed,
                &now_iso(),
            )
        {
            let mut update = Map::new();
            update.insert("projectId".to_string(), json!(plan.project_id));
            update.insert("sessionId".to_string(), json!(plan.session_id));
            update.insert(
                "runtimeSettings".to_string(),
                Value::Object(runtime_settings),
            );
            repository.update_session(&update)?;
        }
        /*
        CDXC:StateSync 2026-07-29:
        Re-probing the renamed checkout is Sidebar V2 card data, so it answers to
        the same `sidebarVersion` gate as the background passes; on a V1 machine
        the rename still lands, it just does not warm a cache nothing reads. The
        read sits here rather than at the top of the pass because a rename is a
        rare event — most passes reach this line zero times.
        */
        session_git_status::refresh_session_git_status_cache(
            &[plan.worktree_path.clone()],
            session_lifecycle::read_sidebar_v2_selected(&state.paths),
        );
        schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &plan.project_id,
            &plan.session_id,
        )?;
    }
    Ok(())
}

/// The sessions whose worktree marker could still be due a branch rename, in `list_sessions`
/// order: a marker object whose `renamedAt` is missing or blank. `plan_worktree_branch_rename`
/// still makes the whole decision on each one.
///
/// CDXC:Worktrees 2026-10-23 WHY:
/// The pass hydrated every session row each minute (over ten thousand on a long-lived registry, six JSON columns each) to find the few that carry a worktree marker. SQLite now picks the marked rows and only those are read in full. The blank test on `renamedAt` runs here with Rust's `trim`, the one `read_worktree_session_marker` applies, because SQLite's `trim` only strips spaces.
fn worktree_branch_rename_candidates(
    repository: &DomainRepository<'_>,
) -> std::result::Result<Vec<(String, String)>, DomainStateError> {
    let mut statement = repository
        .db
        .prepare_cached(
            "SELECT projectId, sessionId,
                    CASE WHEN json_type(runtimeSettingsJson, '$.worktreeSession.renamedAt') = 'text'
                         THEN json_extract(runtimeSettingsJson, '$.worktreeSession.renamedAt') END
             FROM sessions
             WHERE json_type(runtimeSettingsJson, '$.worktreeSession') = 'object'
             ORDER BY updatedAt DESC, projectId ASC, sessionId ASC",
        )
        .map_err(crate::domain::sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(crate::domain::sql_error)?;
    let mut candidates = Vec::new();
    for row in rows {
        let (project_id, session_id, renamed_at) = row.map_err(crate::domain::sql_error)?;
        if renamed_at.is_some_and(|renamed_at| !renamed_at.trim().is_empty()) {
            continue;
        }
        candidates.push((project_id, session_id));
    }
    Ok(candidates)
}

pub(crate) fn log_worktree_branch_rename_failure(state: &Arc<AppState>, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "worktreeBranchRenameFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}
