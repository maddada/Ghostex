use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::{
    domain::{DomainRepository, DomainStateError},
    zmx::dispatch_zmx_session_interaction_endpoint,
};

use super::*;

pub(super) fn recover_running_automation_runs(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
) -> Result<(), DomainStateError> {
    let running = read_running_runs(db)?;
    for run in running {
        let Some(session_id) = run.session_id.as_deref() else {
            fail_run(
                db,
                &run.project_id,
                &run.id,
                "Automation run has no linked session.",
                "needs_attention",
            )?;
            continue;
        };
        let session_project_id =
            find_run_session_project_id(repository, &run).unwrap_or_else(|| run.project_id.clone());
        /*
        CDXC:Automations 2026-11-09 WHY:
        A run whose chat can't be read any more (closed while gxserver was stopped, say) ends as needing attention on its own. Its error used to end the whole scheduler tick, so one such run stopped every automation from running again.
        */
        let read = read_automation_result_from_session(
            repository,
            &session_project_id,
            session_id,
            Some(&run.created_at),
        );
        let Ok(read) = read else {
            fail_run(
                db,
                &run.project_id,
                &run.id,
                "The run's chat closed before it reported a result.",
                "needs_attention",
            )?;
            runtime.routine_run_settled(&run.project_id, &run.id);
            continue;
        };
        if let Some((status, summary)) = read {
            complete_run(db, &run.project_id, &run.id, &status, summary.as_deref())?;
            runtime.routine_run_settled(&run.project_id, &run.id);
        } else {
            /*
            A recovered run already reached the delivery step in the watcher that
            owned it, so re-adopting it must not resubmit the prompt into a
            session that is very likely already working on it.
            */
            runtime.spawn_run_watcher(
                run.project_id,
                run.id,
                session_project_id,
                session_id.to_string(),
                None,
            );
        }
    }
    Ok(())
}

/*
CDXC:Automations 2026-09-28 WHY:
The agent's own transcript is read first, then the terminal text. A full-screen TUI keeps no terminal scrollback (Claude Code, and Codex since its fullscreen view became the default in 0.157), so the terminal text is one screenful: a closing marker that scrolled out of the agent's view, or a view the user scrolled up, left the run waiting for the watcher timeout. The newest assistant message holds the marker whatever is on screen; agents without a readable transcript keep the terminal scan. Only a message written after the run was created counts, because a thread run reuses a session whose newest reply can be an earlier run's marker.
*/
pub(super) fn read_automation_result_from_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    run_created_at: Option<&str>,
) -> Result<Option<(String, Option<String>)>, DomainStateError> {
    let since_ms = run_created_at
        .and_then(|created_at| chrono::DateTime::parse_from_rfc3339(created_at).ok())
        .map(|created_at| created_at.timestamp_millis());
    let transcript_result = since_ms.and_then(|since_ms| {
        repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
            .and_then(|session| crate::notification_feed::body::last_assistant_message(&session))
            .filter(|(_, timestamp)| timestamp.is_some_and(|timestamp| timestamp >= since_ms))
            .and_then(|(text, _)| parse_automation_result(&text))
    });
    if transcript_result.is_some() {
        return Ok(transcript_result);
    }
    let mut params = Map::new();
    params.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    params.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    let text =
        dispatch_zmx_session_interaction_endpoint(repository, "/api/readSessionText", &params)
            .map_err(zmx_error)?
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default();
    Ok(parse_automation_result(&text))
}

/*
CDXC:Automations 2026-07-30:
The delivered prompt is echoed into the session scrollback, so the marker scan
reads the instruction block too. Scanning only the first occurrence let that echo
decide the run: every run resolved to the first status word named in the
instructions, seconds after delivery, with the remaining instruction lines stored
as the summary. Take the last occurrence that parses into a real status instead,
because the agent's closing marker always trails its own echoed instructions.
`build_automation_prompt` keeps the instruction text free of a literal
`AUTOMATION_RESULT: <status>` line so the echo can never parse at all.
*/
pub(super) fn parse_automation_result(text: &str) -> Option<(String, Option<String>)> {
    let haystack = text.to_ascii_uppercase();
    let mut result = None;
    let mut search_from = 0;
    while let Some(offset) = haystack[search_from..].find(AUTOMATION_RESULT_PREFIX) {
        search_from = search_from + offset + AUTOMATION_RESULT_PREFIX.len();
        if let Some(parsed) = parse_automation_result_at(text, search_from) {
            result = Some(parsed);
        }
    }
    result
}

fn parse_automation_result_at(
    text: &str,
    after_marker_index: usize,
) -> Option<(String, Option<String>)> {
    let mut lines = text.get(after_marker_index..)?.lines();
    let status = lines.next()?.trim().to_ascii_lowercase();
    let status = match status.as_str() {
        "findings" | "no_findings" | "needs_attention" => status,
        _ => return None,
    };
    let summary = lines
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    Some((status, (!summary.is_empty()).then_some(summary)))
}
