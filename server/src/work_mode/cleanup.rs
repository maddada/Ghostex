//! The offer a work-mode session gets once its linked PR is merged: remove its worktree and park
//! it, or keep it. The answer is stored on the session as `runtimeSettings.workCleanup`, keyed on
//! the PR, so the offer never comes back for that PR on any client.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: when a linked PR is merged, Ghostex offers to remove the worktree and park the session, not just put it to sleep. The offer is shown once per PR: after "Clean up" or "Keep" it does not come back for the same PR. Since 2026-10-24 the **Offer Clean up when a PR merges** setting (on by default) can turn it off (CDXC:WorkMode 2026-10-24 in packages/settings-catalog/src/data/defaults.rs).
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! "Already offered" lives on the session in gxserver, not in a client, so the desktop, the web
//! build and the phone agree and an answer given on one is gone from the others.
//!
//! SEE-ALSO: `/api/answerWorkCleanup` in server/src/server/route_http/work_links.rs (the answer),
//! apps/desktop/src/app/native_sidebar/work_chips.rs (the card's Clean up / Keep chips).

use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::worktree_sessions::read_worktree_session_marker;

/// What identifies a PR in the stored answer: its URL, else `#<number>`.
pub(crate) fn work_cleanup_pull_request_key(number: u64, url: Option<&str>) -> String {
    url.map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{number}"))
}

fn answered_pull_request(session: &Value) -> Option<&str> {
    session
        .pointer("/runtimeSettings/workCleanup/pullRequest")
        .and_then(Value::as_str)
}

/// The worktree folder gxserver made for this session, when it is still on disk.
pub(crate) fn work_cleanup_worktree_path(session: &Value) -> Option<String> {
    read_worktree_session_marker(session)
        .map(|marker| marker.path)
        .filter(|path| Path::new(path).is_dir())
}

/// Whether to offer the cleanup for this merged PR: not answered for it yet, and something is
/// left to do (the session is not parked yet, or its worktree is still there).
pub(crate) fn offers_work_cleanup(session: &Value, pull_request_key: &str) -> bool {
    if answered_pull_request(session) == Some(pull_request_key) {
        return false;
    }
    let parked = session.get("isParked").and_then(Value::as_bool) == Some(true);
    !parked || work_cleanup_worktree_path(session).is_some()
}

/// Records the answer for this PR, patched in place like the links (links.rs).
pub(crate) fn write_work_cleanup_answer(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    pull_request_key: &str,
    answer: &str,
    answered_at: &str,
) -> Result<(), DomainStateError> {
    write_work_cleanup_record(
        db,
        project_id,
        session_id,
        &json!({
            "pullRequest": pull_request_key,
            "answer": answer,
            "answeredAt": answered_at,
        }),
    )
}

/// Replaces the session's whole `runtimeSettings.workCleanup` record.
pub(crate) fn write_work_cleanup_record(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    record: &Value,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.workCleanup', json(?1)) \
         WHERE projectId = ?2 AND sessionId = ?3",
        params![record.to_string(), project_id, session_id],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite work cleanup error: {error}"),
    })?;
    Ok(())
}

/// The answer gxserver records when it cleaned a merged thread up by itself
/// (server/src/server/work_cleanup_threads.rs), with what Undo needs to put the worktree back.
pub(crate) const AUTO_CLEAN_UP_ANSWER: &str = "autoCleanUp";

/// How long a coordinator's banner says what was cleaned up by itself, unless dismissed.
const AUTO_CLEANED_SHOWN_FOR_MS: i64 = 24 * 60 * 60 * 1000;

/// The session's stored cleanup answer, when it has one.
pub(crate) fn work_cleanup_record(session: &Value) -> Option<&Map<String, Value>> {
    session
        .pointer("/runtimeSettings/workCleanup")
        .and_then(Value::as_object)
}

/// A cleanup gxserver did by itself that the coordinator's banner still reports: not dismissed,
/// and less than a day old. `{ "at", "undo" }`, `undo` when the worktree can be put back (its
/// branch or its last commit is known).
///
/// A coordinator's thread the user cleaned up (Clean up, Clean up all) is reported the same way, with
/// Undo, since it keeps its place and can't run until its worktree is back (`cleaned_thread_holds_place`).
/// Only while the worktree folder is really gone.
pub(crate) fn presentation_auto_cleaned(session: &Value, now_ms: i64) -> Option<Value> {
    let record = work_cleanup_record(session)?;
    if !reports_cleanup(record) || record.get("dismissed").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let folder = record.get("worktreePath").and_then(Value::as_str)?;
    if Path::new(folder).exists() {
        return None;
    }
    let at = record.get("answeredAt").and_then(Value::as_str)?;
    let at_ms = chrono::DateTime::parse_from_rfc3339(at)
        .ok()?
        .timestamp_millis();
    if now_ms.saturating_sub(at_ms) > AUTO_CLEANED_SHOWN_FOR_MS {
        return None;
    }
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    Some(json!({
        "at": at,
        "undo": text("worktreePath") && (text("branch") || text("head")),
    }))
}

/// A cleanup the banner reports and Undo can reverse: one the pass did by itself, or the user's
/// Clean up of a coordinator's thread.
pub(crate) fn reports_cleanup(record: &Map<String, Value>) -> bool {
    match record.get("answer").and_then(Value::as_str) {
        Some(AUTO_CLEAN_UP_ANSWER) => true,
        Some("cleanUp") => record.get("thread").and_then(Value::as_bool) == Some(true),
        _ => false,
    }
}

/// Whether the session's worktree was cleaned up for its merged PR, by the user or by itself.
pub(crate) fn work_cleaned_up(session: &Value) -> bool {
    work_cleanup_record(session)
        .and_then(|record| record.get("answer"))
        .and_then(Value::as_str)
        .is_some_and(|answer| answer == "cleanUp" || answer == AUTO_CLEAN_UP_ANSWER)
}

/// A coordinator's thread cleaned up for its merged PR, by the user or by itself: its record says
/// `thread`. Its session stays where it was (it never moves into the project's folder, where it
/// would run on the user's own checkout) until Undo puts the worktree back.
pub(crate) fn cleaned_thread_holds_place(session: &Value) -> bool {
    work_cleanup_record(session).is_some_and(|record| {
        record.get("thread").and_then(Value::as_bool) == Some(true)
            && matches!(
                record.get("answer").and_then(Value::as_str),
                Some("cleanUp" | AUTO_CLEAN_UP_ANSWER)
            )
    })
}

/// CDXC:WorkMode 2026-11-20 WHY: once a coordinator's thread is cleaned up its worktree is gone, and
/// waking it (a follow-up from its coordinator, a click) used to move it into the project's folder,
/// where it resumed on the user's main checkout believing it was still on its branch (Opus review
/// of the merged-thread cleanup). It is refused instead, with what to do; Undo puts the worktree
/// back first. `Some(message)` while the thread's worktree folder is gone.
pub(crate) fn cleaned_thread_wake_refusal(session: &Value) -> Option<String> {
    if !cleaned_thread_holds_place(session) {
        return None;
    }
    let marker = read_worktree_session_marker(session)?;
    if Path::new(&marker.path).exists() {
        return None;
    }
    Some(
        "This thread's worktree was cleaned up after its PR merged, so it can't run. Use Undo in its coordinator's banner, or start a new thread."
            .to_string(),
    )
}

/// Why the pass that cleans up by itself kept a merged thread's worktree, as its banner says it:
/// `{ pullRequest, reason, detail, at }` under `runtimeSettings.workCleanupKept`.
pub(crate) const WORK_CLEANUP_KEPT_KEY: &str = "workCleanupKept";

/// The kept reason for this PR: `(code, detail)`.
pub(crate) fn work_cleanup_kept(
    session: &Value,
    pull_request_key: &str,
) -> Option<(String, String)> {
    let record = session
        .pointer(&format!("/runtimeSettings/{WORK_CLEANUP_KEPT_KEY}"))
        .and_then(Value::as_object)?;
    if record.get("pullRequest").and_then(Value::as_str) != Some(pull_request_key) {
        return None;
    }
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some((text("reason"), text("detail")))
}

/// Writes (or with `None`, clears) the kept reason.
pub(crate) fn write_work_cleanup_kept(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    kept: Option<&Value>,
) -> Result<(), DomainStateError> {
    let result = match kept {
        Some(record) => db.execute(
            &format!("UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{{}}'), '$.{WORK_CLEANUP_KEPT_KEY}', json(?1)) WHERE projectId = ?2 AND sessionId = ?3"),
            params![record.to_string(), project_id, session_id],
        ),
        None => db.execute(
            &format!("UPDATE sessions SET runtimeSettingsJson = json_remove(runtimeSettingsJson, '$.{WORK_CLEANUP_KEPT_KEY}') WHERE projectId = ?1 AND sessionId = ?2"),
            params![project_id, session_id],
        ),
    };
    result.map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite work cleanup error: {error}"),
    })?;
    Ok(())
}
