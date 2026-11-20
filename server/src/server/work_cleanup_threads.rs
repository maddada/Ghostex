//! A coordinator's merged-PR worktrees, all at once: the banner's Clean up all / Keep / Undo answer
//! (`/api/answerWorkCleanupThreads`), and the pass that cleans a merged thread up by itself.
//!
//! CDXC:WorkMode 2026-11-20 DECISION:
//! User, of a coordinator whose five threads each showed their own Clean up / Keep line (Q4286, picked B and C): one "N merged worktrees · Clean up all · Keep" banner per coordinator instead of one line per row, and finished threads fold away with the merged ones cleaned up by themselves. Strictly: only a worktree with no uncommitted or unpushed changes whose PR is merged; never a coordinator, never the user's own session (only a thread a coordinator started, not one it adopted), never a session that works or waits on the user; the session stays (asleep, under its coordinator), the banner says what was cleaned with Undo, and anything that fails a rule stays and shows in the banner instead. **Clean up merged threads by themselves** (`cleanUpMergedThreads`, on by default) turns the pass off.
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
    remove_session_worktree, resolve_project_worktree_operation_context,
    run_project_worktree_action, ProjectWorktreeOperationError,
};
use super::{schedule_presentation_session_delta, AppState};
use crate::coordinators::{
    is_sleep_inactive_session, list_coordinators, list_threads, list_threads_for, thread_owes_work,
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
            .filter(|record| {
                record.get("answer").and_then(Value::as_str) == Some(AUTO_CLEAN_UP_ANSWER)
            })
            .ok_or_else(|| "Nothing was cleaned up by itself here.".to_string())?
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

/// The cleanups the pass tried and could not do (a worktree that turned out dirty, a removal that
/// failed), so it does not try them every minute; they stay in the banner. Forgotten at restart.
fn tried() -> &'static Mutex<HashSet<(String, String, String)>> {
    static TRIED: OnceLock<Mutex<HashSet<(String, String, String)>>> = OnceLock::new();
    TRIED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// One thread the pass may clean up, once git agrees.
struct Candidate {
    project_id: String,
    session_id: String,
    pull_request_key: String,
    worktree_path: String,
    branch: Option<String>,
    sleeping: bool,
}

/// Whether a presentation session (with its coordinator fields) neither works nor waits on the
/// user: asleep, or awake and passing the orchestrator's Sleep Inactive Threads test, and no
/// approval, stuck or blocked state.
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
    quiet
        && !matches!(state, Some("working" | "waiting"))
        && !flag("awaitingApproval")
        && !flag("awaitingAnswer")
        && !present("stuckSince")
        && !present("blockedSince")
}

/// The threads that pass every rule the database can answer, in thread order.
fn auto_cleanup_candidates(state: &AppState) -> Result<Vec<Candidate>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(db_error)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let coordinators: HashSet<(String, String)> = list_coordinators(&db)?
        .into_iter()
        .map(|record| record.key())
        .collect();
    let threads = list_threads(&db)?;
    // Every live session's folder, so a worktree another session still runs in is never removed.
    let live_sessions = repository.list_sessions_excluding_stopped(None)?;
    let tried = tried()
        .lock()
        .map(|tried| tried.clone())
        .unwrap_or_default();
    let mut candidates = Vec::new();
    for thread in threads {
        let key = (thread.project_id.clone(), thread.session_id.clone());
        // Never a coordinator, nor a session that has threads of its own.
        if coordinators.contains(&key)
            || key
                == (
                    thread.coordinator_project_id.clone(),
                    thread.coordinator_session_id.clone(),
                )
            || !coordinator_thread_keys(&db, &key.0, &key.1)?.is_empty()
        {
            continue;
        }
        // Only a session its coordinator started: one linked without a task was the user's own
        // session, adopted as it was.
        if thread.task.trim().is_empty() {
            continue;
        }
        // A message its coordinator sent that its transcript does not show yet is work on its way:
        // delivering it would wake the thread in a folder that is gone.
        if thread.pending_message.is_some() {
            continue;
        }
        let (Some(project), Some(session)) = (
            repository.get_project(&key.0)?,
            repository.get_session(&key.0, &key.1)?,
        ) else {
            continue;
        };
        if !project_work_mode(&project) || text(&session, "lifecycleState") == "stopped" {
            continue;
        }
        let Some(pull_request) = presentation_session_work(&project, &session)
            .and_then(|work| work.get("pullRequest").cloned())
            .filter(|pull_request| {
                pull_request.get("state").and_then(Value::as_str) == Some("merged")
            })
        else {
            continue;
        };
        let pull_request_key = work_cleanup_pull_request_key(
            pull_request
                .get("number")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            pull_request.get("url").and_then(Value::as_str),
        );
        if !offers_work_cleanup(&session, &pull_request_key)
            || tried.contains(&(key.0.clone(), key.1.clone(), pull_request_key.clone()))
        {
            continue;
        }
        // A thread without a worktree of its own has nothing to clean up by itself.
        let Some(worktree_path) = work_cleanup_worktree_path(&session) else {
            continue;
        };
        if thread_owes_work(&key.0, &key.1) {
            continue;
        }
        let presentation = build_presentation_session_delta(&db, &repository, &key.0, &key.1)?;
        let Some(presentation) = presentation.get("session") else {
            continue;
        };
        if !is_idle_thread(presentation) {
            continue;
        }
        let inside = |cwd: &str| {
            let worktree = worktree_path.trim_end_matches('/');
            cwd == worktree || cwd.starts_with(&format!("{worktree}/"))
        };
        let shared = live_sessions.iter().any(|other| {
            (text(other, "projectId"), text(other, "sessionId")) != key
                && crate::session_git_status::effective_session_git_cwd(other, None)
                    .is_some_and(|cwd| inside(cwd.trim_end_matches('/')))
        });
        if shared {
            continue;
        }
        candidates.push(Candidate {
            branch: crate::worktree_sessions::read_worktree_session_marker(&session)
                .map(|marker| marker.branch)
                .filter(|branch| !branch.trim().is_empty()),
            sleeping: text(presentation, "lifecycleState") == "sleeping",
            project_id: key.0,
            session_id: key.1,
            pull_request_key,
            worktree_path,
        });
    }
    Ok(candidates)
}

/// What git says about a worktree before the pass removes it: its commit, when nothing is
/// uncommitted (untracked files included) and that commit is on a remote. `Err` names the rule it
/// fails.
fn worktree_is_safe_to_remove(worktree_path: &str) -> Result<String, &'static str> {
    let status = run_git_probe_command(
        worktree_path,
        &["status", "--porcelain", "--untracked-files=all"],
    )
    .ok_or("git status failed")?;
    if !status.trim().is_empty() {
        return Err("uncommitted changes");
    }
    let head = run_git_probe_command(worktree_path, &["rev-parse", "--verify", "HEAD"])
        .map(|head| head.trim().to_string())
        .filter(|head| !head.is_empty())
        .ok_or("no commit")?;
    // The commit must be on a remote branch (the PR's, or the default branch it merged into), so
    // removing the worktree loses nothing that was never pushed.
    let on_remote = run_git_probe_command(
        worktree_path,
        &[
            "for-each-ref",
            "--contains",
            &head,
            "--format=%(refname)",
            "refs/remotes",
        ],
    )
    .ok_or("git for-each-ref failed")?;
    if on_remote.trim().is_empty() {
        return Err("unpushed commits");
    }
    Ok(head)
}

fn log_auto_cleanup(
    state: &AppState,
    level: LogLevel,
    event: &str,
    candidate: &Candidate,
    error: Option<String>,
) {
    let _ = state.logger.log(GxserverLogInput {
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
    });
}

/// One pass: cleans up every merged thread that passes the rules. Runs after the work-mode pass,
/// whose caches say which PRs are merged.
pub(crate) async fn run_auto_cleanup_once(state: &Arc<AppState>) {
    let enabled = crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(CLEAN_UP_MERGED_THREADS)
                .and_then(Value::as_bool)
        })
        .unwrap_or_else(|| {
            ghostex_settings_catalog::default_value(CLEAN_UP_MERGED_THREADS)
                .is_some_and(|value| *value == ghostex_settings_catalog::J::Bool(true))
        });
    if !enabled {
        return;
    }
    let reader = state.clone();
    let Ok(Ok(candidates)) =
        tokio::task::spawn_blocking(move || auto_cleanup_candidates(&reader)).await
    else {
        return;
    };
    for candidate in candidates {
        let tried_key = (
            candidate.project_id.clone(),
            candidate.session_id.clone(),
            candidate.pull_request_key.clone(),
        );
        let path = candidate.worktree_path.clone();
        let head =
            match tokio::task::spawn_blocking(move || worktree_is_safe_to_remove(&path)).await {
                Ok(Ok(head)) => head,
                Ok(Err(rule)) => {
                    if let Ok(mut tried) = tried().lock() {
                        tried.insert(tried_key);
                    }
                    log_auto_cleanup(
                        state,
                        LogLevel::Info,
                        "workAutoCleanupSkipped",
                        &candidate,
                        Some(rule.to_string()),
                    );
                    continue;
                }
                Err(_) => continue,
            };
        // The agent holds its worktree as its working folder (Windows will not delete a folder in
        // use), so the session sleeps first, as Clean up does.
        if !candidate.sleeping {
            sleep_work_cleanup_session(state, &candidate.project_id, &candidate.session_id).await;
        }
        let mut remove = Map::new();
        remove.insert("projectId".to_string(), json!(candidate.project_id));
        remove.insert("worktreePath".to_string(), json!(candidate.worktree_path));
        // Never forced: `removeSessionWorktree` reads `git status` again and keeps a dirty worktree.
        let removed = match remove_session_worktree(state, &remove).await {
            Ok(result) => result.get("removed").and_then(Value::as_bool) == Some(true),
            Err(error) => {
                log_auto_cleanup(
                    state,
                    LogLevel::Warn,
                    "workAutoCleanupFailed",
                    &candidate,
                    Some(worktree_error_text(&error)),
                );
                false
            }
        };
        if !removed {
            if let Ok(mut tried) = tried().lock() {
                tried.insert(tried_key);
            }
            continue;
        }
        let recorded = (|| -> Result<(), DomainStateError> {
            let db = open_gxserver_database(&state.paths).map_err(db_error)?;
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            write_work_cleanup_record(
                &db,
                &candidate.project_id,
                &candidate.session_id,
                &json!({
                    "pullRequest": candidate.pull_request_key,
                    "answer": AUTO_CLEAN_UP_ANSWER,
                    "answeredAt": crate::coordinators::now_iso(),
                    "worktreePath": candidate.worktree_path,
                    "branch": candidate.branch,
                    "head": head,
                }),
            )?;
            schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                &candidate.project_id,
                &candidate.session_id,
            )
        })();
        match recorded {
            Ok(()) => {
                log_auto_cleanup(state, LogLevel::Info, "workAutoCleanedUp", &candidate, None)
            }
            Err(error) => log_auto_cleanup(
                state,
                LogLevel::Warn,
                "workAutoCleanupFailed",
                &candidate,
                Some(error.message),
            ),
        }
    }
}
