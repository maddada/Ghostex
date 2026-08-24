use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::domain::DomainStateError;
use crate::portless::read_portless_presentation_payload;

pub(crate) fn insert_workspace_groups_presentation_payload(
    snapshot: &mut Value,
    db: &Connection,
) -> Result<(), DomainStateError> {
    /*
    CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
    Mobile and CLI consumers read the GPUI-authored named-group overlay from the
    same presentation snapshot they already poll, so grouped ordering needs no
    extra round trip.
    */
    let groups = crate::workspace_groups::read_workspace_session_groups(db)?;
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert("workspaceGroups".to_string(), groups);
    }
    Ok(())
}

pub(crate) fn insert_sidebar_project_collections_presentation_payload(
    snapshot: &mut Value,
    db: &Connection,
) -> Result<(), DomainStateError> {
    /*
    CDXC:SidebarProjectCollections 2026-07-18-00:00:
    Mobile and CLI consumers read the colored project-collection overlay from
    the same presentation snapshot they already poll, so grouped project
    rendering needs no extra round trip.
    */
    let collections = crate::sidebar_project_collections::read_sidebar_project_collections(db)?;
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert("sidebarProjectCollections".to_string(), collections);
    }
    Ok(())
}

/*
CDXC:SessionChatPromptQueue 2026-08-21:
`queuedPromptCount` is the sidebar badge's whole input: how many Ghostex-owned
prompts are waiting for this session. It rides the presentation projection for
the same reason Delayed Send's countdown does — the sidebar already renders
every session from this snapshot, so the badge needs no second round trip and
no per-session chat subscription.

The count mirrors `SessionChatQueueSnapshot::queued_count` in
`session_chat_queue.rs`: EVERY row counts, `failed` included. A queue stalled
behind a failed row is the one state that needs the user to act, and hiding it
here made a dead queue indistinguishable from no queue on every surface outside
the chat view. `queuedPromptFailedCount` rides alongside so the same badge can
turn red instead of yellow, and so a consumer that means "more work is coming"
(gpui Auto Sleep) can subtract it rather than guess. Both are published as
ABSENT keys at zero, never `0`, so a session that drained its queue clears the
badge through the same whole-object session upsert every other field uses.

Cost: one grouped read of a table that is empty on almost every daemon, mirroring
`read_all_delayed_send_projections`. Never one query per session — presentation
snapshots publish many times a second on a busy sidebar.
*/
#[derive(Clone, Copy, Default)]
pub(crate) struct SessionChatQueuePresentationCounts {
    total: u64,
    failed: u64,
}

pub(crate) fn read_session_chat_queue_counts(
    db: &Connection,
) -> HashMap<(String, String), SessionChatQueuePresentationCounts> {
    let Ok(mut statement) = db.prepare(&format!(
        r#"
        SELECT
          projectId,
          sessionId,
          COUNT(*),
          SUM(CASE WHEN state = '{failed}' THEN 1 ELSE 0 END)
        FROM session_chat_queued_prompts
        GROUP BY projectId, sessionId
        "#,
        failed = crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_FAILED,
    )) else {
        return HashMap::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
            SessionChatQueuePresentationCounts {
                total: row.get::<_, i64>(2)?.max(0) as u64,
                failed: row.get::<_, i64>(3).unwrap_or(0).max(0) as u64,
            },
        ))
    }) else {
        return HashMap::new();
    };
    rows.filter_map(Result::ok)
        .filter(|(_, counts)| counts.total > 0)
        .collect()
}

pub(crate) fn insert_session_chat_queue_presentation_payload(
    snapshot: &mut Value,
    db: &Connection,
) {
    let counts = read_session_chat_queue_counts(db);
    if counts.is_empty() {
        return;
    }
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        let Some(object) = session.as_object_mut() else {
            continue;
        };
        let key = (
            object
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            object
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        if let Some(counts) = counts.get(&key) {
            object.insert("queuedPromptCount".to_string(), json!(counts.total));
            if counts.failed > 0 {
                object.insert("queuedPromptFailedCount".to_string(), json!(counts.failed));
            }
        }
    }
}

pub(crate) fn insert_session_chat_queue_session_projection(
    session: &mut Value,
    db: &Connection,
    project_id: &str,
    session_id: &str,
) {
    let snapshot = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        db, project_id, session_id,
    );
    let count = snapshot.queued_count();
    if count == 0 {
        return;
    }
    let failed_count = snapshot.failed_count();
    if let Some(object) = session.as_object_mut() {
        object.insert("queuedPromptCount".to_string(), json!(count));
        if failed_count > 0 {
            object.insert("queuedPromptFailedCount".to_string(), json!(failed_count));
        }
    }
}

pub(crate) fn insert_auto_settle_window_presentation_payload(
    snapshot: &mut Value,
    auto_settle_after_days: Option<f64>,
) {
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
    One sidebar renders rows from several daemons and each daemon reads its OWN
    `sidebarAutoSettleAfterDays`, so a client that applied the local window to
    every machine would park remote sessions the remote daemon still considers
    active (the recorded P2 minor). The key is therefore ALWAYS published — an
    explicit `null` says "this daemon settles nothing", while an ABSENT key can
    only mean a daemon too old to state its window.
    */
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert(
            "autoSettleAfterDays".to_string(),
            match auto_settle_after_days {
                Some(days) => auto_settle_window_value(days),
                None => Value::Null,
            },
        );
    }
}

/*
The window is carried as an f64 because the sweep computes with one, but the
setting users actually write is a whole number of days. Publishing `3` rather
than `3.0` round-trips their value byte for byte, which keeps the wire readable
and comparable; a fractional window (a test or a power user's `1.5`) publishes as
the float it is.
*/
pub(crate) fn auto_settle_window_value(days: f64) -> Value {
    if days.fract() == 0.0 && days.abs() < 9_007_199_254_740_992.0 {
        return json!(days as i64);
    }
    json!(days)
}

pub(crate) fn insert_portless_presentation_payload(snapshot: &mut Value, db: &Connection) {
    if let Some(snapshot) = snapshot.as_object_mut() {
        snapshot.insert(
            "portless".to_string(),
            serde_json::to_value(read_portless_presentation_payload(db))
                .expect("Portless presentation payload serializes"),
        );
    }
}
