//! A coordinator's merged-PR worktrees, all at once: the banner's Clean up all / Keep / Undo answer
//! (`/api/answerWorkCleanupThreads`), and the pass that cleans a merged thread up by itself.
//!
//! CDXC:WorkMode 2026-11-20 DECISION:
//! User, of a coordinator whose five threads each showed their own Clean up / Keep line (Q4286, picked B and C): one "N merged worktrees · Clean up all · Keep" banner per coordinator instead of one line per row, and finished threads fold away with the merged ones cleaned up by themselves. Strictly: only a worktree with no uncommitted or unpushed changes whose PR is merged; never a coordinator, never the user's own session (only a thread a coordinator started, not one it adopted), never a session that works or waits on the user; the session stays (asleep, under its coordinator), the banner says what was cleaned with Undo, and anything that fails a rule stays and shows in the banner instead. Then, on the default: "Ship it as the page described", so **Clean up merged threads by themselves** (`cleanUpMergedThreads`) is off by default and the banner asks until it is turned on; and manual Clean up / Clean up all skip a thread whose coordinator's message is still on its way, as this pass does.
//!
//! SEE-ALSO: server/src/work_mode/cleanup.rs (the stored answer, `autoCleaned`), route_http/work_links.rs
//! (`/api/answerWorkCleanup`, one session), packages/gx-core/src/sidebar_view/threads.rs (the banner's
//! counts and the finished fold), apps/desktop/src/app/native_sidebar/threads.rs (the banner),
//! apps/mobile/app/src/contract/coordinatorTree.ts (the phone's).

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Map, Value};

use super::route_http::{answer_work_cleanup, sleep_work_cleanup_session};
use super::worktree_ops::{
    canonical_worktree_path_key, remove_session_worktree,
    resolve_project_worktree_operation_context, run_project_worktree_action,
    ProjectWorktreeOperationError,
};
use super::{schedule_presentation_session_delta, AppState};
use crate::coordinators::{
    is_sleep_inactive_session, list_coordinators, list_threads, list_threads_for,
    started_by_coordinator, thread_owes_work,
};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::presentation::build_presentation_session_delta;
use crate::session_git_status::{run_git_probe_command, run_git_probe_command_with_status};
use crate::storage::open_gxserver_database;
use crate::work_mode::{
    offers_work_cleanup, presentation_session_work, project_work_mode,
    work_cleanup_pull_request_key, work_cleanup_record, work_cleanup_worktree_path,
    write_work_cleanup_answer, write_work_cleanup_record, AUTO_CLEAN_UP_ANSWER,
};

/// The setting that lets the pass clean merged threads up by itself.
const CLEAN_UP_MERGED_THREADS: &str = "cleanUpMergedThreads";

fn db_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The coordinator's own threads on this computer (never the coordinator itself).
fn coordinator_thread_keys(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Vec<(String, String)>, DomainStateError> {
    Ok(list_threads_for(db, project_id, session_id)?
        .into_iter()
        .filter(|thread| !(thread.project_id == project_id && thread.session_id == session_id))
        .map(|thread| (thread.project_id, thread.session_id))
        .collect())
}

/// `{ projectId, sessionId, answer }` naming a coordinator: `cleanUp` or `keep` answers the merged-PR
/// offer of each of its threads that has one, as `/api/answerWorkCleanup` does for one session;
/// `undo` puts back the worktrees the pass cleaned up by itself (and keeps them); `dismiss` hides the
/// banner's line about them.
pub(super) async fn answer_work_cleanup_threads(
    state: &Arc<AppState>,
    body: &Value,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let params = read_domain_rpc_params(body)?;
    let answer = params
        .get("answer")
        .and_then(Value::as_str)
        .filter(|answer| matches!(*answer, "cleanUp" | "keep" | "undo" | "dismiss"))
        .ok_or_else(|| {
            DomainStateError::bad_request("answer must be cleanUp, keep, undo or dismiss.")
        })?
        .to_string();
    let required = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
    };
    let (project_id, session_id) = (required("projectId")?, required("sessionId")?);

    // Which threads the answer is for, read once up front.
    let targets: Vec<(String, String)> = {
        let db = open_gxserver_database(&state.paths).map_err(db_error)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let now_ms = chrono::Utc::now().timestamp_millis();
        coordinator_thread_keys(&db, &project_id, &session_id)?
            .into_iter()
            .filter(|(thread_project, thread_session)| {
                let Ok(Some(session)) = repository.get_session(thread_project, thread_session)
                else {
                    return false;
                };
                match answer.as_str() {
                    "cleanUp" | "keep" => {
                        let Ok(Some(project)) = repository.get_project(thread_project) else {
                            return false;
                        };
                        presentation_session_work(&project, &session)
                            .and_then(|work| work.get("offerCleanup").and_then(Value::as_bool))
                            == Some(true)
                    }
                    "undo" => crate::work_mode::presentation_auto_cleaned(&session, now_ms)
                        .is_some_and(|line| line.get("undo") == Some(&json!(true))),
                    _ => crate::work_mode::presentation_auto_cleaned(&session, now_ms).is_some(),
                }
            })
            .collect()
    };

    let mut results: Vec<Value> = Vec::new();
    for (thread_project, thread_session) in targets {
        let result = match answer.as_str() {
            "cleanUp" | "keep" => answer_work_cleanup(
                state,
                &json!({ "params": {
                    "projectId": thread_project,
                    "sessionId": thread_session,
                    "answer": answer,
                }}),
            )
            .await
            .map_err(|error| worktree_error_text(&error)),
            "undo" => undo_auto_cleanup(state, &thread_project, &thread_session).await,
            _ => dismiss_auto_cleanup(state, &thread_project, &thread_session)
                .map_err(|error| error.message),
        };
        results.push(match result {
            Ok(result) => json!({
                "projectId": thread_project,
                "sessionId": thread_session,
                "result": result,
            }),
            Err(error) => json!({
                "projectId": thread_project,
                "sessionId": thread_session,
                "error": error,
            }),
        });
    }
    Ok(json!({ "answer": answer, "threads": results }))
}

fn worktree_error_text(error: &ProjectWorktreeOperationError) -> String {
    match error {
        ProjectWorktreeOperationError::Domain(error) => error.message.clone(),
        ProjectWorktreeOperationError::Typed(error) => error.message.clone(),
        ProjectWorktreeOperationError::ProjectPath(_) => {
            "the folder could not be read.".to_string()
        }
    }
}

/// Hides the banner's line about one auto cleanup; the answer itself stays.
fn dismiss_auto_cleanup(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(db_error)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(session) = repository.get_session(project_id, session_id)? else {
        return Ok(json!({ "dismissed": false }));
    };
    let Some(mut record) = work_cleanup_record(&session).cloned() else {
        return Ok(json!({ "dismissed": false }));
    };
    record.insert("dismissed".to_string(), json!(true));
    write_work_cleanup_record(&db, project_id, session_id, &Value::Object(record))?;
    schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)?;
    Ok(json!({ "dismissed": true }))
}

/// Puts back a worktree the pass removed: on its branch when the branch is still there, else on a
/// new branch of that name at the commit it was on. The session is then kept for this PR, so the
/// pass never removes it again.
async fn undo_auto_cleanup(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, String> {
    let record = {
        let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        repository
            .get_session(project_id, session_id)
            .map_err(|error| error.message)?
            .as_ref()
            .and_then(work_cleanup_record)
            .cloned()
            .filter(|record| crate::work_mode::reports_cleanup(record))
            .ok_or_else(|| "Nothing here was cleaned up for a merged PR.".to_string())?
    };
    let field = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let worktree_path = field("worktreePath").ok_or("The worktree's folder is not known.")?;
    let mut restored = false;
    if !Path::new(&worktree_path).exists() {
        let mut context_params = Map::new();
        context_params.insert("projectId".to_string(), json!(project_id));
        let context = resolve_project_worktree_operation_context(state, &context_params)
            .map_err(|error| worktree_error_text(&error))?;
        let branch = field("branch");
        let parent = context.parent_path.clone();
        let branch_exists = match branch.clone() {
            Some(branch) => tokio::task::spawn_blocking(move || {
                run_git_probe_command_with_status(
                    &parent,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{branch}"),
                    ],
                )
                .is_some_and(|(code, _)| code == 0)
            })
            .await
            .unwrap_or(false),
            None => false,
        };
        let mut create = Map::new();
        create.insert("worktreePath".to_string(), json!(worktree_path));
        match (branch, field("head")) {
            (Some(branch), _) if branch_exists => {
                create.insert("baseRef".to_string(), json!(branch));
            }
            (Some(branch), Some(head)) => {
                create.insert("branch".to_string(), json!(branch));
                create.insert("baseRef".to_string(), json!(head));
            }
            (None, Some(head)) => {
                create.insert("baseRef".to_string(), json!(head));
            }
            _ => return Err("Neither the branch nor its last commit is left.".to_string()),
        }
        let created =
            run_project_worktree_action(&context.projects, "create", &context.parent_path, create)
                .await
                .map_err(|error| worktree_error_text(&error))?;
        if created.get("exitCode").and_then(Value::as_i64).unwrap_or(1) != 0 {
            return Err(created
                .get("stderr")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|stderr| !stderr.is_empty())
                .unwrap_or("git worktree add failed.")
                .to_string());
        }
        restored = true;
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    write_work_cleanup_answer(
        &db,
        project_id,
        session_id,
        &field("pullRequest").unwrap_or_default(),
        "keep",
        &crate::coordinators::now_iso(),
    )
    .map_err(|error| error.message)?;
    schedule_presentation_session_delta(state, &db, &repository, project_id, session_id)
        .map_err(|error| error.message)?;
    Ok(json!({ "restoredWorktree": restored }))
}

/// How long a thread must have been quiet before the pass touches it, so a thread that just
/// finished a turn, or that someone is about to type into, is left alone.
const IDLE_BEFORE_CLEANUP_MS: i64 = 20 * 60 * 1000;
/// How often the pass runs, on a task of its own (never inside the 60s git-status loop, which a
/// slow removal would hold up).
const AUTO_CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(120);

/// Removals that failed after every check passed, so the pass does not retry them every run; they
/// stay in the banner. Forgotten at restart.
fn failed() -> &'static Mutex<HashSet<(String, String, String)>> {
    static FAILED: OnceLock<Mutex<HashSet<(String, String, String)>>> = OnceLock::new();
    FAILED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// One thread that passed every rule the database can answer.
#[derive(Clone, Debug, PartialEq)]
struct Candidate {
    project_id: String,
    session_id: String,
    pull_request_key: String,
    /// What `gh pr view` takes: the PR's URL, else its number.
    pull_request_selector: String,
    pull_request_number: u64,
    worktree_path: String,
    marker_branch: Option<String>,
    sleeping: bool,
    zmx_name: Option<String>,
}

/// Whether a presentation session (with its coordinator fields) neither works nor waits on the
/// user: asleep, or awake and passing the orchestrator's Sleep Inactive Threads test, with nothing
/// queued or drafted for it, and no approval, stuck or blocked state.
fn is_idle_thread(presentation: &Value) -> bool {
    let flag = |key: &str| presentation.get(key).and_then(Value::as_bool) == Some(true);
    let present = |key: &str| presentation.get(key).is_some_and(|value| !value.is_null());
    let state = presentation
        .get("coordinatorThreadState")
        .and_then(Value::as_str);
    let lifecycle = presentation.get("lifecycleState").and_then(Value::as_str);
    let quiet = match lifecycle {
        Some("sleeping") => {
            !matches!(
                presentation.get("activity").and_then(Value::as_str),
                Some("working" | "attention")
            ) && presentation
                .get("pendingQuestionCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                == 0
        }
        Some("running") => is_sleep_inactive_session(presentation),
        _ => false,
    };
    // A queued message or a draft typed but not sent is work on its way.
    let queued = presentation
        .get("queuedPromptCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    quiet
        && queued == 0
        && !flag("hasComposerDraft")
        && !matches!(state, Some("working" | "waiting"))
        && !flag("awaitingApproval")
        && !flag("awaitingAnswer")
        && !present("stuckSince")
        && !present("blockedSince")
}

/// Whether `path` is the worktree or inside it, compared by resolved path (symlinks, Windows case
/// and separators) and by whole components.
fn is_inside(path: &str, worktree_key: &str) -> bool {
    Path::new(&canonical_worktree_path_key(path)).starts_with(Path::new(worktree_key))
}

/// Every rule the database can answer for one thread; `None` when it fails one.
fn evaluate_thread(
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    coordinators: &HashSet<(String, String)>,
    live_sessions: &[Value],
    thread: &crate::coordinators::ThreadRecord,
    now_ms: i64,
) -> Result<Option<Candidate>, DomainStateError> {
    let key = (thread.project_id.clone(), thread.session_id.clone());
    let coordinator = (
        thread.coordinator_project_id.clone(),
        thread.coordinator_session_id.clone(),
    );
    // Never a coordinator, nor a session that has threads of its own.
    if coordinators.contains(&key)
        || key == coordinator
        || !coordinator_thread_keys(db, &key.0, &key.1)?.is_empty()
    {
        return Ok(None);
    }
    // A message its coordinator sent that its transcript does not show yet is work on its way.
    if thread.pending_message.is_some() {
        return Ok(None);
    }
    let (Some(project), Some(session)) = (
        repository.get_project(&key.0)?,
        repository.get_session(&key.0, &key.1)?,
    ) else {
        return Ok(None);
    };
    // Only a session its coordinator started (`start-thread`, `agents create`), never the user's own
    // session a coordinator adopted.
    if !started_by_coordinator(&session, &coordinator) {
        return Ok(None);
    }
    if !project_work_mode(&project) || text(&session, "lifecycleState") == "stopped" {
        return Ok(None);
    }
    let Some(pull_request) = presentation_session_work(&project, &session)
        .and_then(|work| work.get("pullRequest").cloned())
        .filter(|pull_request| pull_request.get("state").and_then(Value::as_str) == Some("merged"))
    else {
        return Ok(None);
    };
    let number = pull_request
        .get("number")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    // Only the branch's own PR: a PR linked by hand or through a ticket says nothing about what this
    // branch holds.
    if crate::work_mode::work_targets(&project, &session).branch_pull_request != Some(number) {
        return Ok(None);
    }
    let url = pull_request.get("url").and_then(Value::as_str);
    let pull_request_key = work_cleanup_pull_request_key(number, url);
    if !offers_work_cleanup(&session, &pull_request_key) {
        return Ok(None);
    }
    // A thread without a worktree of its own has nothing to clean up by itself.
    let Some(worktree_path) = work_cleanup_worktree_path(&session) else {
        return Ok(None);
    };
    if thread_owes_work(&key.0, &key.1) {
        return Ok(None);
    }
    let quiet_since =
        chrono::DateTime::parse_from_rfc3339(&crate::presentation::last_active_at(&session))
            .map(|at| at.timestamp_millis())
            .ok();
    if quiet_since.is_none_or(|at| now_ms.saturating_sub(at) < IDLE_BEFORE_CLEANUP_MS) {
        return Ok(None);
    }
    let presentation = build_presentation_session_delta(db, repository, &key.0, &key.1)?;
    let Some(presentation) = presentation.get("session") else {
        return Ok(None);
    };
    if !is_idle_thread(presentation) {
        return Ok(None);
    }
    // Never a folder another live session runs in.
    let worktree_key = canonical_worktree_path_key(&worktree_path);
    let shared = live_sessions.iter().any(|other| {
        (text(other, "projectId"), text(other, "sessionId")) != key
            && crate::session_git_status::effective_session_git_cwd(other, None)
                .is_some_and(|cwd| is_inside(&cwd, &worktree_key))
    });
    if shared {
        return Ok(None);
    }
    Ok(Some(Candidate {
        pull_request_selector: url
            .map(str::to_string)
            .unwrap_or_else(|| number.to_string()),
        pull_request_number: number,
        marker_branch: crate::worktree_sessions::read_worktree_session_marker(&session)
            .map(|marker| marker.branch)
            .filter(|branch| !branch.trim().is_empty()),
        sleeping: text(presentation, "lifecycleState") == "sleeping",
        zmx_name: crate::zmx::provider_zmx_session_name(&session).ok(),
        project_id: key.0,
        session_id: key.1,
        pull_request_key,
        worktree_path,
    }))
}

/// The threads that pass every rule the database can answer, in thread order.
fn auto_cleanup_candidates(state: &AppState) -> Result<Vec<Candidate>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(db_error)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let coordinators: HashSet<(String, String)> = list_coordinators(&db)?
        .into_iter()
        .map(|record| record.key())
        .collect();
    let live_sessions = repository.list_sessions_excluding_stopped(None)?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut candidates = Vec::new();
    for thread in list_threads(&db)? {
        if let Some(candidate) = evaluate_thread(
            &db,
            &repository,
            &coordinators,
            &live_sessions,
            &thread,
            now_ms,
        )? {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

/// Every database rule again for one thread, just before its worktree goes: it still qualifies for
/// the same PR and the same folder.
fn still_qualifies(state: &AppState, candidate: &Candidate) -> bool {
    let check = || -> Result<bool, DomainStateError> {
        let db = open_gxserver_database(&state.paths).map_err(db_error)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(thread) =
            crate::coordinators::read_thread(&db, &candidate.project_id, &candidate.session_id)?
        else {
            return Ok(false);
        };
        let coordinators: HashSet<(String, String)> = list_coordinators(&db)?
            .into_iter()
            .map(|record| record.key())
            .collect();
        let live_sessions = repository.list_sessions_excluding_stopped(None)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        Ok(evaluate_thread(
            &db,
            &repository,
            &coordinators,
            &live_sessions,
            &thread,
            now_ms,
        )?
        .is_some_and(|again| {
            again.pull_request_key == candidate.pull_request_key
                && again.worktree_path == candidate.worktree_path
        }))
    };
    check().unwrap_or(false)
}

/// What git says about a worktree before the pass removes it.
#[derive(Clone, Debug, PartialEq)]
enum GitVerdict {
    /// Nothing would be lost: its commit and its branch.
    Safe {
        head: String,
        branch: Option<String>,
    },
    /// Kept, with why (`autoKept`'s code) and the files or commits.
    Kept {
        reason: &'static str,
        detail: String,
    },
    /// Git could not be asked; try again next time.
    Unknown,
}

/// CDXC:WorkMode 2026-11-20 WHY: nothing uncommitted (untracked files included), nothing git
/// ignores outside the rebuildable folders, nothing hidden by skip-worktree, and its commit inside
/// what was merged: an ancestor of its own PR's head, which GitHub keeps for good, or of the remote's
/// default branch. A commit pushed after the PR merged, or a branch reused for an issue whose earlier
/// PR merged, is not merged work, so it stays (Opus review of the merged-thread cleanup, findings 1 and 5).
fn git_verdict(
    worktree_path: &str,
    pull_request_selector: &str,
    pull_request_number: u64,
) -> GitVerdict {
    let Some(status) = run_git_probe_command(
        worktree_path,
        &["status", "--porcelain", "--untracked-files=all"],
    ) else {
        return GitVerdict::Unknown;
    };
    if !status.trim().is_empty() {
        let files: Vec<&str> = status
            .lines()
            .filter_map(|line| line.get(3..))
            .take(3)
            .collect();
        return GitVerdict::Kept {
            reason: "uncommitted",
            detail: files.join(", "),
        };
    }
    match crate::worktree_sessions::worktree_hidden_work(worktree_path) {
        None => return GitVerdict::Unknown,
        Some(Some(work)) => {
            return GitVerdict::Kept {
                reason: work.code(),
                detail: work.describe(),
            }
        }
        Some(None) => {}
    }
    let Some(head) = run_git_probe_command(worktree_path, &["rev-parse", "--verify", "HEAD"])
        .map(|head| head.trim().to_string())
        .filter(|head| !head.is_empty())
    else {
        return GitVerdict::Unknown;
    };
    let is_ancestor = |of: &str| {
        run_git_probe_command_with_status(
            worktree_path,
            &["merge-base", "--is-ancestor", &head, of],
        )
        .is_some_and(|(code, _)| code == 0)
    };
    let pull_request_head = crate::session_git_status::run_gh_command(
        Some(worktree_path),
        &[
            "pr",
            "view",
            pull_request_selector,
            "--json",
            "headRefOid,state",
        ],
    )
    .and_then(|output| serde_json::from_str::<Value>(&output).ok())
    .filter(|pull_request| pull_request.get("state").and_then(Value::as_str) == Some("MERGED"))
    .and_then(|pull_request| {
        pull_request
            .get("headRefOid")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let default_branch = run_git_probe_command_with_status(
        worktree_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "refs/remotes/origin/HEAD",
        ],
    )
    .filter(|(code, _)| *code == 0)
    .map(|_| "refs/remotes/origin/HEAD".to_string());
    let merged = pull_request_head.as_deref().is_some_and(is_ancestor)
        || default_branch.as_deref().is_some_and(is_ancestor);
    if !merged {
        return GitVerdict::Kept {
            reason: "unmerged",
            detail: format!("commits that are not in PR #{pull_request_number}"),
        };
    }
    let branch = run_git_probe_command(worktree_path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty() && branch != "HEAD");
    GitVerdict::Safe { head, branch }
}

fn log_auto_cleanup(
    state: &AppState,
    level: LogLevel,
    event: &str,
    candidate: &Candidate,
    error: Option<String>,
) {
    let entry = GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error,
        details: Some(json!({
            "projectId": candidate.project_id,
            "sessionId": candidate.session_id,
            "pullRequest": candidate.pull_request_key,
        })),
    };
    // A worktree removed by itself is kept in the log every time, so what went can always be
    // traced (`log_record`); skips carry their rule as the error and are kept as it is.
    let _ = if event == "workAutoCleanedUp" {
        state.logger.log_record(entry)
    } else {
        state.logger.log(entry)
    };
}

/// Records (or clears, with `None`) why the pass kept a thread, for its banner; writes only a
/// change.
fn record_kept(state: &AppState, candidate: &Candidate, kept: Option<(&str, &str)>) {
    let write = || -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&state.paths).map_err(db_error)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Some(session) = repository.get_session(&candidate.project_id, &candidate.session_id)?
        else {
            return Ok(());
        };
        let now = crate::work_mode::work_cleanup_kept(&session, &candidate.pull_request_key);
        let wanted = kept.map(|(reason, detail)| (reason.to_string(), detail.to_string()));
        if now == wanted {
            return Ok(());
        }
        let record = kept.map(|(reason, detail)| {
            json!({
                "pullRequest": candidate.pull_request_key,
                "reason": reason,
                "detail": detail,
                "at": crate::coordinators::now_iso(),
            })
        });
        crate::work_mode::write_work_cleanup_kept(
            &db,
            &candidate.project_id,
            &candidate.session_id,
            record.as_ref(),
        )?;
        schedule_presentation_session_delta(
            state,
            &db,
            &repository,
            &candidate.project_id,
            &candidate.session_id,
        )
    };
    let _ = write();
}

/// The setting and its catalog default.
fn auto_cleanup_enabled(state: &AppState) -> bool {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(CLEAN_UP_MERGED_THREADS)
                .and_then(Value::as_bool)
        })
        .unwrap_or_else(|| {
            ghostex_settings_catalog::default_value(CLEAN_UP_MERGED_THREADS)
                .is_some_and(|value| *value == ghostex_settings_catalog::J::Bool(true))
        })
}

/// The pass on a task of its own, every [`AUTO_CLEANUP_INTERVAL`].
pub(crate) fn spawn_work_auto_cleanup_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let pass_state = state.clone();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(AUTO_CLEANUP_INTERVAL) => {}
            }
            run_auto_cleanup_once(&pass_state).await;
        }
    })
}

/// One pass: cleans up every merged thread that passes the rules, checking each one again just
/// before its worktree goes.
pub(crate) async fn run_auto_cleanup_once(state: &Arc<AppState>) {
    // With the offer off, merged PRs ask nothing and clean up nothing.
    if !auto_cleanup_enabled(state)
        || !crate::work_mode::issues_workflow_settings().offer_cleanup_when_pr_merges
    {
        return;
    }
    let reader = state.clone();
    let Ok(Ok(candidates)) =
        tokio::task::spawn_blocking(move || auto_cleanup_candidates(&reader)).await
    else {
        return;
    };
    for candidate in candidates {
        let key = (
            candidate.project_id.clone(),
            candidate.session_id.clone(),
            candidate.pull_request_key.clone(),
        );
        if failed().lock().is_ok_and(|failed| failed.contains(&key)) {
            continue;
        }
        let verdict = {
            let checked = candidate.clone();
            tokio::task::spawn_blocking(move || {
                git_verdict(
                    &checked.worktree_path,
                    &checked.pull_request_selector,
                    checked.pull_request_number,
                )
            })
            .await
            .unwrap_or(GitVerdict::Unknown)
        };
        let (head, branch) = match verdict {
            GitVerdict::Safe { head, branch } => (head, branch),
            GitVerdict::Kept { reason, detail } => {
                record_kept(state, &candidate, Some((reason, &detail)));
                log_auto_cleanup(
                    state,
                    LogLevel::Info,
                    "workAutoCleanupSkipped",
                    &candidate,
                    Some(format!("{reason}: {detail}")),
                );
                continue;
            }
            GitVerdict::Unknown => continue,
        };
        record_kept(state, &candidate, None);
        // Never a thread someone is looking at.
        if let Some(zmx_name) = candidate.zmx_name.clone() {
            let visible = tokio::task::spawn_blocking(move || {
                crate::zmx::zmx_session_has_visible_client(&zmx_name)
            })
            .await
            .ok()
            .flatten();
            if visible == Some(true) {
                continue;
            }
        }
        // The agent holds its worktree as its working folder (Windows will not delete a folder in
        // use), so the session sleeps first, as Clean up does, and only a session that did go to
        // sleep loses its folder.
        if !candidate.sleeping {
            sleep_work_cleanup_session(state, &candidate.project_id, &candidate.session_id).await;
            let asleep = open_gxserver_database(&state.paths)
                .ok()
                .and_then(|db| {
                    DomainRepository::new(&db, state.metadata.server_id.as_str())
                        .get_session(&candidate.project_id, &candidate.session_id)
                        .ok()
                        .flatten()
                })
                .is_some_and(|session| text(&session, "lifecycleState") == "sleeping");
            if !asleep {
                log_auto_cleanup(
                    state,
                    LogLevel::Info,
                    "workAutoCleanupSkipped",
                    &candidate,
                    Some("the thread did not go to sleep".to_string()),
                );
                continue;
            }
        }
        // Everything again, right before the folder goes: the database rules, then git, with the
        // same commit.
        let rechecked = {
            let checked_state = state.clone();
            let checked = candidate.clone();
            tokio::task::spawn_blocking(move || {
                still_qualifies(&checked_state, &checked).then(|| {
                    git_verdict(
                        &checked.worktree_path,
                        &checked.pull_request_selector,
                        checked.pull_request_number,
                    )
                })
            })
            .await
            .ok()
            .flatten()
        };
        match rechecked {
            Some(GitVerdict::Safe { head: again, .. }) if again == head => {}
            _ => continue,
        }
        // The answer goes in first, saying it is a coordinator's thread, so a wake in the moment the
        // folder goes is refused rather than moved into the project's folder.
        let recorded = (|| -> Result<(), DomainStateError> {
            let db = open_gxserver_database(&state.paths).map_err(db_error)?;
            write_work_cleanup_record(
                &db,
                &candidate.project_id,
                &candidate.session_id,
                &json!({
                    "pullRequest": candidate.pull_request_key,
                    "answer": AUTO_CLEAN_UP_ANSWER,
                    "answeredAt": crate::coordinators::now_iso(),
                    "worktreePath": candidate.worktree_path,
                    "branch": branch.clone().or_else(|| candidate.marker_branch.clone()),
                    "head": head,
                    "thread": true,
                }),
            )
        })();
        if let Err(error) = recorded {
            log_auto_cleanup(
                state,
                LogLevel::Warn,
                "workAutoCleanupFailed",
                &candidate,
                Some(error.message),
            );
            continue;
        }
        let mut remove = Map::new();
        remove.insert("projectId".to_string(), json!(candidate.project_id));
        remove.insert("worktreePath".to_string(), json!(candidate.worktree_path));
        // Never forced (`removeSessionWorktree` reads git again and keeps what would be lost), the
        // branch is kept for Undo, and the thread stays where it is.
        remove.insert("keepBranch".to_string(), json!(true));
        remove.insert("keepSessions".to_string(), json!(true));
        let removed = match remove_session_worktree(state, &remove).await {
            Ok(result) if result.get("removed").and_then(Value::as_bool) == Some(true) => Ok(()),
            Ok(result) => Err(result
                .get("warnings")
                .and_then(Value::as_array)
                .and_then(|warnings| warnings.first())
                .and_then(Value::as_str)
                .unwrap_or("the worktree was kept")
                .to_string()),
            Err(error) => Err(worktree_error_text(&error)),
        };
        let finished = (|| -> Result<(), DomainStateError> {
            let db = open_gxserver_database(&state.paths).map_err(db_error)?;
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            if removed.is_err() {
                // Not removed: the offer comes back as it was.
                db.execute(
                    "UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.workCleanup') WHERE projectId = ?1 AND sessionId = ?2",
                    rusqlite::params![candidate.project_id, candidate.session_id],
                )
                .map_err(db_error)?;
            }
            schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                &candidate.project_id,
                &candidate.session_id,
            )
        })();
        match (removed, finished) {
            (Ok(()), Ok(())) => {
                log_auto_cleanup(state, LogLevel::Info, "workAutoCleanedUp", &candidate, None)
            }
            (Err(error), _) => {
                if let Ok(mut failed) = failed().lock() {
                    failed.insert(key);
                }
                log_auto_cleanup(
                    state,
                    LogLevel::Warn,
                    "workAutoCleanupFailed",
                    &candidate,
                    Some(error),
                );
            }
            (Ok(()), Err(error)) => log_auto_cleanup(
                state,
                LogLevel::Warn,
                "workAutoCleanupFailed",
                &candidate,
                Some(error.message),
            ),
        }
    }
}
