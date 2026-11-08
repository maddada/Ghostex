//! The `/api/gbots` problem ops: `problems` (the list), `problemRetry`, `problemRebrief`,
//! `problemClose`, and `flag` (a GBot raising a problem itself).
//!
//! CDXC:GBots 2026-11-09 WHY:
//! A retry restarts the chat on its own conversation (the sleep-and-wake `ghostex agents reload` runs, session_chat_agent_restart.rs) and tells it to carry on; a routine run that never opened a chat is run again instead. It is refused after two retries and re-briefs of one problem (then Chief of Staff tells the user), while the chat is working (nothing to restart), and once the chat finished a turn after the report (it moved on; read it first), so a retry never cuts off work that recovered on its own.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::store::{
    insert_problem, list_problems, live_problem_for_chat, new_problem, problem_summary,
    read_problem, save_problem, Problem, MAX_ATTEMPTS, NEEDS_YOU,
};
use crate::coordinators::now_iso;
use crate::domain::{DomainRepository, DomainStateError};
use crate::gbots::record::read_bot;
use crate::server::{queue_session_chat_prompt, AppState};
use crate::storage::open_gxserver_database;

/// A flag's text is kept to this many characters.
const FLAG_MAX_CHARS: usize = 600;

pub(crate) fn handles(op: &str) -> bool {
    matches!(op, "problems" | "problemClose" | "problemRebrief" | "flag")
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

fn conflict(message: String) -> DomainStateError {
    DomainStateError {
        code: "conflict",
        message,
    }
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn session_text(session: &Value, key: &str) -> String {
    session
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn chat_ref(state: &AppState, problem: &Problem) -> Option<String> {
    let (project, session) = problem.chat()?;
    Some(crate::ids::create_global_session_ref(
        state.metadata.server_id.as_str(),
        project,
        session,
    ))
}

fn summary(state: &AppState, db: &Connection, problem: &Problem) -> Value {
    let bot = read_bot(db, &problem.bot_id).ok().flatten();
    let mut summary = problem_summary(
        problem,
        bot.as_ref().map(|bot| bot.name.as_str()),
        chat_ref(state, problem).as_deref(),
    );
    if let Some(routine) = problem
        .automation_id
        .as_deref()
        .and_then(|id| super::report::routine_name(db, id))
    {
        summary["routineName"] = json!(routine);
    }
    summary
}

fn require_problem(db: &Connection, id: &str) -> Result<Problem, DomainStateError> {
    read_problem(db, id)?.ok_or_else(|| {
        DomainStateError::not_found(format!(
            "There is no problem {id}. `ghostex gbots problem list` shows the open ones."
        ))
    })
}

fn require_live(db: &Connection, id: &str) -> Result<Problem, DomainStateError> {
    let problem = require_problem(db, id)?;
    if !problem.is_live() {
        return Err(conflict(format!(
            "{} is closed already ({}).",
            problem.problem_id,
            problem.closed_by.as_deref().unwrap_or("closed")
        )));
    }
    Ok(problem)
}

/// The blocking ops. `may_manage`: you or Chief of Staff; `own_chat`: the caller's own chat when
/// it is a chat of a GBot (its bot id, project and session).
pub(crate) fn run(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    may_manage: bool,
    by: &str,
    own_chat: Option<(String, String, String)>,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if op == "flag" {
        return flag(state, db, own_chat, params);
    }
    if !may_manage {
        return Err(forbidden(
            "Only you and Chief of Staff look after GBot problems. A GBot raises one with `ghostex gbots flag`.",
        ));
    }
    match op {
        "problems" => {
            let all = params.get("all").and_then(Value::as_bool) == Some(true);
            let rows = list_problems(db, !all, 50)?;
            Ok(json!({
                "problems": rows.iter().map(|row| summary(state, db, row)).collect::<Vec<_>>(),
            }))
        }
        "problemClose" => {
            let mut problem = require_live(db, required(params, "problem")?)?;
            let handed = params.get("handed").and_then(Value::as_bool) == Some(true);
            if handed {
                problem.status = "handed".to_string();
                problem.closed_by = Some(format!("{by}: told the user"));
            } else {
                problem.status = "closed".to_string();
                problem.closed_by = Some(by.to_string());
            }
            save_problem(db, &mut problem)?;
            Ok(json!({ "problem": summary(state, db, &problem) }))
        }
        "problemRebrief" => rebrief(state, db, repository, by, params),
        other => Err(DomainStateError::bad_request(format!(
            "Unknown GBots op {other}."
        ))),
    }
}

fn required<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    text(params, key).ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

fn check_attempts(problem: &Problem) -> Result<(), DomainStateError> {
    if problem.attempts >= MAX_ATTEMPTS {
        return Err(conflict(format!(
            "{} was retried or re-briefed {MAX_ATTEMPTS} times already. Tell the user in two lines what is wrong and where, then `ghostex gbots problem close {} --handed`.",
            problem.problem_id, problem.problem_id
        )));
    }
    Ok(())
}

/// The chat a problem is about, while it is open.
fn live_chat(
    repository: &DomainRepository<'_>,
    problem: &Problem,
) -> Result<Option<Value>, DomainStateError> {
    let Some((project, session)) = problem.chat() else {
        return Ok(None);
    };
    repository
        .get_session(project, session)?
        .filter(crate::presentation::session_attributes::is_active)
        .map(Some)
        .ok_or_else(|| {
            conflict(format!(
                "{}'s chat was closed. Close the problem with `ghostex gbots problem close {}`.",
                problem.problem_id, problem.problem_id
            ))
        })
}

fn rebrief(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    by: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut problem = require_live(db, required(params, "problem")?)?;
    check_attempts(&problem)?;
    let note = required(params, "text").map_err(|_| {
        DomainStateError::bad_request(
            "Say what to tell it: ghostex gbots problem rebrief <problem> \"<text>\".",
        )
    })?;
    let Some(session) = live_chat(repository, &problem)? else {
        return Err(conflict(format!(
            "{} has no chat to send to. Retry it (`ghostex gbots problem retry {}`) or close it.",
            problem.problem_id, problem.problem_id
        )));
    };
    let (project, session_id) = (
        session_text(&session, "projectId"),
        session_text(&session, "sessionId"),
    );
    let who = if by == "chief" {
        "Chief of Staff"
    } else {
        "The user"
    };
    let message = format!(
        "{who}, about the problem Ghostex reported for this chat ({}):\n\n{}",
        problem.problem_id,
        note.chars().take(16_000).collect::<String>()
    );
    queue_session_chat_prompt(state, &project, &session_id, &message, false)?;
    crate::gbots::assign::wake_if_asleep(state, &session);
    problem.attempts += 1;
    problem.status = "retrying".to_string();
    problem.retried_at = Some(now_iso());
    save_problem(db, &mut problem)?;
    Ok(json!({ "problem": summary(state, db, &problem), "action": "rebriefed" }))
}

fn flag(
    state: &AppState,
    db: &Connection,
    own_chat: Option<(String, String, String)>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let Some((bot_id, project, session_id)) = own_chat else {
        return Err(forbidden(
            "Only a GBot raises a flag, from its own chat. Tell Chief of Staff directly instead.",
        ));
    };
    let what = required(params, "text").map_err(|_| {
        DomainStateError::bad_request(
            "Say what you need: ghostex gbots flag \"<what you need and why>\".",
        )
    })?;
    if what.chars().count() > FLAG_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "A flag is limited to {FLAG_MAX_CHARS} characters: say what you need in two sentences."
        )));
    }
    let chat = format!("{project}:{session_id}");
    if let Some(live) = live_problem_for_chat(db, &chat, true)?
        .filter(|live| live.source.as_deref() == Some("flag"))
    {
        return Err(conflict(format!(
            "Your flag {} is still open. Wait for Chief of Staff's answer in this chat, then end your turn.",
            live.problem_id
        )));
    }
    let mut problem = new_problem(&bot_id, Some(&chat), NEEDS_YOU, what);
    problem.source = Some("flag".to_string());
    insert_problem(db, &mut problem)?;
    super::report::send_report(state, db, &mut problem);
    let told = match problem.reported_to.as_deref() {
        Some("user") => "the user was notified (there is no Chief of Staff to tell)",
        Some("muted") => "this chat raised too many flags this hour, so nobody was told; wait, or say it in your reply",
        Some(_) => "Chief of Staff was told",
        None => "Chief of Staff will be told in a moment",
    };
    Ok(json!({
        "problem": summary(state, db, &problem),
        "message": format!("Flag {} raised: {told}. End your turn now; the answer comes to this chat.", problem.problem_id),
    }))
}

/// What a retry does, decided before anything restarts.
enum RetryPlan {
    /// Restart the chat; tell it to carry on when it had a conversation to come back to.
    Chat {
        project: String,
        session: String,
        kind: String,
    },
    /// Run the routine again (its run never opened a chat).
    Routine { automation_id: String },
}

/// `op: problemRetry`. Runs outside the blocking pool: the restart awaits gxserver's own
/// lifecycle calls.
pub(crate) async fn retry(
    state: std::sync::Arc<AppState>,
    may_manage: bool,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if !may_manage {
        return Err(forbidden(
            "Only you and Chief of Staff retry a GBot problem.",
        ));
    }
    let planning = state.clone();
    let plan = tokio::task::spawn_blocking(move || plan_retry(&planning, &params))
        .await
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("GBots could not answer: {error}"),
        })??;
    let (problem_id, plan) = plan;
    match plan {
        RetryPlan::Routine { automation_id } => {
            let running = state.clone();
            tokio::task::spawn_blocking(move || -> Result<(), DomainStateError> {
                let db = open(&running)?;
                let repository = DomainRepository::new(&db, running.metadata.server_id.as_str());
                crate::automations::run_routine_now(
                    &running.automation_runtime,
                    &db,
                    &repository,
                    &automation_id,
                )?;
                Ok(())
            })
            .await
            .map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("GBots could not answer: {error}"),
            })??;
        }
        RetryPlan::Chat {
            project,
            session,
            kind,
        } => {
            let reading = state.clone();
            let (read_project, read_session) = (project.clone(), session.clone());
            let row = tokio::task::spawn_blocking(move || {
                open(&reading).ok().and_then(|db| {
                    DomainRepository::new(&db, reading.metadata.server_id.as_str())
                        .get_session(&read_project, &read_session)
                        .ok()
                        .flatten()
                })
            })
            .await
            .ok()
            .flatten();
            let resumable = match &row {
                Some(row) => {
                    crate::session_chat_queue_runtime::has_conversation_to_resume(row).await
                }
                None => false,
            };
            super::watch::restart_chat(&state, &project, &session)
                .await
                .map_err(|reason| {
                    conflict(format!(
                        "Ghostex could not restart the chat ({reason}), so this try was not counted. Try again in a minute, or tell the user."
                    ))
                })?;
            if resumable {
                let what = match kind.as_str() {
                    super::store::STALLED => "after it showed no new output for a long while",
                    super::store::COULD_NOT_START => "after its agent did not come up",
                    _ => "after it stopped",
                };
                // A startup send: the queue holds it until the restarted agent's input box is up
                // (queued as a plain message right after the wake, it was never sent).
                let _ = queue_session_chat_prompt(
                    &state,
                    &project,
                    &session,
                    &format!("Ghostex restarted this chat for Chief of Staff {what}, on the same conversation. Check where you were and carry on."),
                    true,
                );
            }
        }
    }
    let answering = state.clone();
    tokio::task::spawn_blocking(move || {
        let db = open(&answering)?;
        let problem = count_retry(&db, &problem_id)?;
        Ok(json!({ "problem": summary(&answering, &db, &problem), "action": "retried" }))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("GBots could not answer: {error}"),
    })?
}

fn open(state: &AppState) -> Result<crate::storage::GxserverDatabase, DomainStateError> {
    open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })
}

/// Checks a retry may run and counts it.
fn plan_retry(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(String, RetryPlan), DomainStateError> {
    let db = open(state)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let problem = require_live(&db, required(params, "problem")?)?;
    let id = problem.problem_id.clone();
    if problem.kind == NEEDS_YOU {
        return Err(conflict(format!(
            "{id} is not a failure: the GBot waits on an answer. Answer it, re-brief it (`ghostex gbots problem rebrief {id} \"<text>\"`) or tell the user."
        )));
    }
    check_attempts(&problem)?;
    let plan = match live_chat(&repository, &problem)? {
        Some(session) => {
            let now_iso = crate::presentation::now_iso();
            let working = crate::presentation::presentation_activity(&session, &now_iso) == "working"
                && crate::presentation::session_attributes::effective_lifecycle_state(&session)
                    == "running"
                && !crate::session_self_heal::daemon_lost(
                    &session_text(&session, "projectId"),
                    &session_text(&session, "sessionId"),
                );
            if working && problem.kind != super::store::STALLED {
                return Err(conflict(format!(
                    "{id}'s chat is working now, so there is nothing to restart. Ghostex closes {id} once it is back on track."
                )));
            }
            let reported = problem
                .reported_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.timestamp_millis());
            if reported.is_some_and(|at| super::watch::finished_since(&session, at)) {
                return Err(conflict(format!(
                    "{id}'s chat finished a turn since Ghostex reported it, so it moved on: read it (`ghostex read-session-chat {}`), then close {id} or re-brief it.",
                    chat_ref(state, &problem).unwrap_or_default()
                )));
            }
            RetryPlan::Chat {
                project: session_text(&session, "projectId"),
                session: session_text(&session, "sessionId"),
                kind: problem.kind.clone(),
            }
        }
        None => match problem.automation_id.clone() {
            Some(automation_id) => RetryPlan::Routine { automation_id },
            None => {
                return Err(conflict(format!(
                    "{id} has no chat or routine to retry. Close it with `ghostex gbots problem close {id}`."
                )))
            }
        },
    };
    Ok((id, plan))
}

/// Counts a retry that ran: the chat then gets a couple of minutes to come back (watch.rs).
fn count_retry(db: &Connection, problem_id: &str) -> Result<Problem, DomainStateError> {
    let mut problem = require_problem(db, problem_id)?;
    problem.attempts += 1;
    problem.status = "retrying".to_string();
    problem.retried_at = Some(now_iso());
    save_problem(db, &mut problem)?;
    Ok(problem)
}
