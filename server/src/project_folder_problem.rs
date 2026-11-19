//! Why a project's folder is missing, published on the project as `pathProblem`, and the watch
//! that republishes a project when its folder goes missing, comes back or changes cause.
//!
//! CDXC:Projects 2026-11-20 DECISION: User picked (Q4244) a slashed computer on the header of a
//! project whose folder is missing, "only on the project header, not on the actual sessions",
//! that opens a popover for that one project with its cause and fixes; no pop-up and no banner
//! anywhere. Every client draws the mark from the project's `pathState` and words the popover from
//! `pathProblem` (`cause` renamed, link, drive or gone; `found`, the folder Ghostex found where the
//! missing one went; `missing` and `renamedTo`, the folder along the path that was renamed and its
//! new name; `link` and `linkTarget`; `drive`), so the disk is read here once
//! (ghostex_paths::missing_folder). Use <found> calls `/api/relocateProject`, which moves the
//! project's sessions with it.
//!
//! CDXC:Projects 2026-11-20 WHY: `pathState` was only read when a project was published, so a
//! folder renamed or a drive unplugged while gxserver ran stayed "available" until something else
//! touched the project. The watch reads every listed project's folder every 15 seconds (a stat
//! each; the diagnosis only for a missing one) and publishes a project only when its state or
//! cause changed, so a quiet machine sends nothing.
//!
//! CDXC:Projects 2026-11-20 DECISION: Opening a session of such a project says so in its pane, not
//! in a pop-up: the pass puts a card on the chat of each of its sessions ("This session's folder is
//! missing", pointing at the mark) and takes it off once the folder is back. It is a watchdog card,
//! so a send does not retire it (session_chat_notice/watchdog_store.rs).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::domain::{project_path_state, DomainRepository, ProjectPathState};
use crate::server::AppState;
use crate::session_chat_notice::{
    SessionChatTerminalNotice, SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
};
use crate::storage::open_gxserver_database;
use ghostex_paths::missing_folder::{diagnose_missing_folder, MissingFolderDiagnosis};

/// The kind of the card a session of such a project shows in its chat.
pub(crate) const NOTICE_KIND: &str = "projectFolderMissing";
const CHECK_EVERY: Duration = Duration::from_secs(15);
/// How long a diagnosis is reused by publishing; the watch refreshes it every pass.
const DIAGNOSIS_TTL: Duration = Duration::from_secs(15);

fn diagnoses() -> &'static Mutex<HashMap<String, (Instant, Option<Value>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Option<Value>)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// What a project published last: its path state and problem, by project id.
fn published() -> &'static Mutex<HashMap<String, Value>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
    PUBLISHED.get_or_init(Default::default)
}

fn diagnosis_value(diagnosis: &MissingFolderDiagnosis) -> Value {
    let mut out = Map::new();
    out.insert("cause".into(), json!(diagnosis.cause.as_str()));
    for (key, value) in [
        ("found", &diagnosis.found),
        ("missing", &diagnosis.missing),
        ("renamedTo", &diagnosis.renamed_to),
        ("link", &diagnosis.link),
        ("linkTarget", &diagnosis.link_target),
        ("drive", &diagnosis.drive),
    ] {
        if let Some(value) = value {
            out.insert(key.into(), json!(value));
        }
    }
    Value::Object(out)
}

fn project_path(project: &Value) -> Option<&str> {
    project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
}

/// The diagnosis of `path`, from the disk when `fresh` or the cached one is old.
fn path_problem_for(path: &str, fresh: bool) -> Option<Value> {
    if !fresh {
        if let Ok(cache) = diagnoses().lock() {
            if let Some((at, value)) = cache.get(path) {
                if at.elapsed() < DIAGNOSIS_TTL {
                    return value.clone();
                }
            }
        }
    }
    let value = diagnose_missing_folder(path).map(|diagnosis| diagnosis_value(&diagnosis));
    if let Ok(mut cache) = diagnoses().lock() {
        cache.retain(|_, (at, _)| at.elapsed() < DIAGNOSIS_TTL * 4);
        cache.insert(path.to_string(), (Instant::now(), value.clone()));
    }
    value
}

/// The project's `pathProblem`: why its folder is missing, or `None` while it is there.
pub(crate) fn published_path_problem(project: &Value) -> Option<Value> {
    match project_path_state(project) {
        ProjectPathState::Missing | ProjectPathState::NotDirectory => {
            path_problem_for(project_path(project)?, false)
        }
        _ => None,
    }
}

/// Starts the watch that republishes projects whose folder changed state or cause.
pub(crate) fn start_project_folder_watch(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(CHECK_EVERY);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let checking = state.clone();
            let _ = tokio::task::spawn_blocking(move || check(&checking)).await;
        }
    });
}

/// One pass: reads every listed project's folder and publishes the ones that changed. BLOCKING.
fn check(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(projects) = repository.list_projects() else {
        return;
    };
    let mut seen = HashMap::new();
    let mut missing: HashMap<String, String> = HashMap::new();
    for project in &projects {
        if !crate::presentation::should_include_presentation_project(project) {
            continue;
        }
        let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        let path_state = project_path_state(project);
        let problem = match path_state {
            ProjectPathState::Missing | ProjectPathState::NotDirectory => {
                project_path(project).and_then(|path| path_problem_for(path, true))
            }
            _ => None,
        };
        if matches!(
            path_state,
            ProjectPathState::Missing | ProjectPathState::NotDirectory
        ) {
            let title = project
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            missing.insert(project_id.to_string(), title);
        }
        seen.insert(
            project_id.to_string(),
            json!({ "state": path_state.as_str(), "problem": problem }),
        );
    }
    let changed: Vec<String> = {
        let Ok(mut last) = published().lock() else {
            return;
        };
        // The first pass after a start only learns what clients were just sent.
        let first = last.is_empty();
        let changed = seen
            .iter()
            .filter(|(project_id, now)| {
                last.get(*project_id)
                    .map_or(!first && now["state"] != json!("available"), |was| {
                        was != *now
                    })
            })
            .map(|(project_id, _)| project_id.clone())
            .collect();
        *last = seen;
        changed
    };
    for project_id in changed {
        let _ = crate::server::presentation_delta::schedule_presentation_project_delta(
            state,
            &db,
            &repository,
            &project_id,
            "projectUpdated",
        );
    }
    mark_sessions(state, &repository, &missing);
}

/// The card on the chat of every session of a project whose folder is missing: the session can't
/// start, and the project's header has the fix. Taken off once the folder is back.
fn session_notice(project_title: &str) -> SessionChatTerminalNotice {
    let detail = if project_title.trim().is_empty() {
        "This session can't start until its project has its folder back. Click the slashed computer next to the project's name to fix it.".to_string()
    } else {
        format!(
            "This session can't start until {} has its folder back. Click the slashed computer next to {}'s name to fix it.",
            project_title.trim(),
            project_title.trim()
        )
    };
    SessionChatTerminalNotice::new(
        NOTICE_KIND,
        SessionChatTerminalNoticeSeverity::Warning,
        SessionChatTerminalNoticeSource::Watchdog,
        "This session's folder is missing",
    )
    .with_detail(&detail)
    .with_input_blocking(false)
}

/// The sessions this gxserver put the card on, as (project, session).
fn carded() -> &'static Mutex<HashSet<(String, String)>> {
    static CARDED: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    CARDED.get_or_init(Default::default)
}

/// Puts the card on the sessions of `missing` projects (id to name) and takes it off the others.
fn mark_sessions(
    state: &AppState,
    repository: &DomainRepository<'_>,
    missing: &HashMap<String, String>,
) {
    let mut now = HashSet::new();
    let mut changed = Vec::new();
    for (project_id, title) in missing {
        let Ok(sessions) = repository.list_sessions(Some(project_id)) else {
            continue;
        };
        for session in sessions {
            let Some(session_id) = session.get("sessionId").and_then(Value::as_str) else {
                continue;
            };
            let showing = crate::session_chat_notice::session_chat_watchdog_notice(
                project_id, session_id,
            )
            .is_some_and(|notice| notice.kind == NOTICE_KIND);
            if !showing {
                crate::session_chat_notice::set_session_chat_watchdog_notice(
                    project_id,
                    session_id,
                    session_notice(title),
                );
                changed.push((project_id.clone(), session_id.to_string()));
            }
            now.insert((project_id.clone(), session_id.to_string()));
        }
    }
    let Ok(mut carded) = carded().lock() else {
        return;
    };
    for (project_id, session_id) in carded.difference(&now) {
        if crate::session_chat_notice::session_chat_watchdog_notice(project_id, session_id)
            .is_some_and(|notice| notice.kind == NOTICE_KIND)
        {
            crate::session_chat_notice::clear_session_chat_watchdog_notice(project_id, session_id);
            changed.push((project_id.clone(), session_id.clone()));
        }
    }
    *carded = now;
    drop(carded);
    for (project_id, session_id) in changed {
        crate::session_chat_options::session_chat_terminal_notice_publisher(
            state,
            &project_id,
            &session_id,
        )();
    }
}
