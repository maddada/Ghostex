use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::domain::{now_iso, sql_error, DomainResult, DomainStateError};

/// Newest rows the read endpoint returns. Mirrors NOTIFICATION_FEED_READ_LIMIT in the shared contract.
pub(crate) const NOTIFICATION_FEED_READ_LIMIT: usize = 200;
/// Rows kept before the oldest read rows are pruned. Mirrors NOTIFICATION_FEED_RETENTION_LIMIT in the shared contract.
pub(crate) const NOTIFICATION_FEED_RETENTION_LIMIT: usize = 500;
/// Longest stored body, in characters. Mirrors NOTIFICATION_FEED_BODY_MAX_CHARS in the shared contract.
pub(crate) const NOTIFICATION_FEED_BODY_MAX_CHARS: usize = 280;

pub(crate) const NOTIFICATION_FEED_KIND_FINISHED: &str = "finished";
pub(crate) const NOTIFICATION_FEED_KIND_NEEDS_INPUT: &str = "needsInput";
pub(crate) const NOTIFICATION_FEED_KIND_BELL: &str = "bell";
pub(crate) const NOTIFICATION_FEED_KIND_CUSTOM: &str = "custom";

pub(crate) struct NewNotificationFeedRow {
    pub project_id: String,
    pub session_id: String,
    pub kind: &'static str,
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub agent_name: Option<String>,
    pub attention_event_id: Option<String>,
}

/// Collapse whitespace and cap the body at the shared character limit with a trailing ellipsis.
pub(crate) fn bounded_notification_body(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= NOTIFICATION_FEED_BODY_MAX_CHARS {
        return collapsed;
    }
    let mut truncated: String = collapsed
        .chars()
        .take(NOTIFICATION_FEED_BODY_MAX_CHARS.saturating_sub(1))
        .collect();
    truncated.push('…');
    truncated
}

/// Insert a row, superseding the session's unread rows first, then prune old read rows.
/// One live row per session keeps the unread count equal to the number of sessions waiting, not the number of events.
pub(crate) fn insert_notification_feed_row(
    db: &Connection,
    row: NewNotificationFeedRow,
) -> DomainResult<Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = now_iso();
    db.execute(
        "DELETE FROM notification_feed WHERE sessionId = ?1 AND readAt IS NULL",
        params![row.session_id],
    )
    .map_err(sql_error)?;
    db.execute(
        r#"
        INSERT INTO notification_feed (
          id, projectId, sessionId, kind, title, subtitle, body, agentName, attentionEventId, createdAt, readAt, deferredAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL)
        "#,
        params![
            id,
            row.project_id,
            row.session_id,
            row.kind,
            row.title,
            row.subtitle,
            row.body,
            row.agent_name,
            row.attention_event_id,
            created_at,
        ],
    )
    .map_err(sql_error)?;
    prune_notification_feed(db)?;
    read_notification_feed_item(db, &id)?
        .ok_or_else(|| DomainStateError::not_found("Notification was not stored."))
}

/// Keep at most the retention limit, dropping the oldest READ rows first and never an unread row.
fn prune_notification_feed(db: &Connection) -> DomainResult<()> {
    let total: i64 = db
        .query_row("SELECT COUNT(*) FROM notification_feed", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)?;
    let excess = total - NOTIFICATION_FEED_RETENTION_LIMIT as i64;
    if excess <= 0 {
        return Ok(());
    }
    db.execute(
        r#"
        DELETE FROM notification_feed WHERE id IN (
          SELECT id FROM notification_feed WHERE readAt IS NOT NULL ORDER BY createdAt ASC LIMIT ?1
        )
        "#,
        params![excess],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Rows whose session was deleted have nothing to open; drop them before listing.
fn drop_orphaned_notification_feed_rows(db: &Connection) -> DomainResult<()> {
    db.execute(
        "DELETE FROM notification_feed WHERE sessionId NOT IN (SELECT sessionId FROM sessions)",
        [],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Mark the session's unread rows read. Deferred rows stay unread on purpose: deferring means "come back to this one last".
pub(crate) fn mark_session_notifications_read(
    db: &Connection,
    session_id: &str,
    include_deferred: bool,
) -> DomainResult<usize> {
    let sql = if include_deferred {
        "UPDATE notification_feed SET readAt = ?1, deferredAt = NULL WHERE sessionId = ?2 AND readAt IS NULL"
    } else {
        "UPDATE notification_feed SET readAt = ?1 WHERE sessionId = ?2 AND readAt IS NULL AND deferredAt IS NULL"
    };
    db.execute(sql, params![now_iso(), session_id])
        .map_err(sql_error)
}

pub(crate) fn mark_notification_read(db: &Connection, id: &str) -> DomainResult<usize> {
    db.execute(
        "UPDATE notification_feed SET readAt = ?1, deferredAt = NULL WHERE id = ?2 AND readAt IS NULL",
        params![now_iso(), id],
    )
    .map_err(sql_error)
}

pub(crate) fn mark_notification_unread(db: &Connection, id: &str) -> DomainResult<usize> {
    db.execute(
        "UPDATE notification_feed SET readAt = NULL, deferredAt = NULL WHERE id = ?1",
        params![id],
    )
    .map_err(sql_error)
}

pub(crate) fn dismiss_notification(db: &Connection, id: &str) -> DomainResult<usize> {
    db.execute("DELETE FROM notification_feed WHERE id = ?1", params![id])
        .map_err(sql_error)
}

/// The session's newest row becomes unread and moves to the back of the unread queue.
pub(crate) fn defer_session_notification(db: &Connection, session_id: &str) -> DomainResult<usize> {
    db.execute(
        r#"
        UPDATE notification_feed SET readAt = NULL, deferredAt = ?1
        WHERE id = (SELECT id FROM notification_feed WHERE sessionId = ?2 ORDER BY createdAt DESC LIMIT 1)
        "#,
        params![now_iso(), session_id],
    )
    .map_err(sql_error)
}

pub(crate) fn mark_all_notifications_read(db: &Connection) -> DomainResult<usize> {
    db.execute(
        "UPDATE notification_feed SET readAt = ?1, deferredAt = NULL WHERE readAt IS NULL",
        params![now_iso()],
    )
    .map_err(sql_error)
}

pub(crate) fn clear_notification_feed(db: &Connection) -> DomainResult<usize> {
    db.execute("DELETE FROM notification_feed", [])
        .map_err(sql_error)
}

pub(crate) fn notification_exists(db: &Connection, id: &str) -> DomainResult<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM notification_feed WHERE id = ?1)",
        params![id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(sql_error)
}

const ITEM_COLUMNS: &str =
    "id, projectId, sessionId, kind, title, subtitle, body, agentName, createdAt, readAt";

fn item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let agent_name: Option<String> = row.get(7)?;
    let read_at: Option<String> = row.get(9)?;
    let mut item = json!({
        "id": row.get::<_, String>(0)?,
        "projectId": row.get::<_, String>(1)?,
        "sessionId": row.get::<_, String>(2)?,
        "kind": row.get::<_, String>(3)?,
        "title": row.get::<_, String>(4)?,
        "subtitle": row.get::<_, String>(5)?,
        "body": row.get::<_, String>(6)?,
        "createdAt": row.get::<_, String>(8)?,
        "read": read_at.is_some(),
    });
    if let Some(agent_name) = agent_name.filter(|value| !value.trim().is_empty()) {
        item["agentName"] = Value::String(agent_name);
    }
    Ok(item)
}

pub(crate) fn read_notification_feed_item(db: &Connection, id: &str) -> DomainResult<Option<Value>> {
    db.query_row(
        &format!("SELECT {ITEM_COLUMNS} FROM notification_feed WHERE id = ?1"),
        params![id],
        item_from_row,
    )
    .optional()
    .map_err(sql_error)
}

/// The full feed state every endpoint returns: newest first, unread count, and the jump target.
pub(crate) fn read_notification_feed_state(db: &Connection) -> DomainResult<Value> {
    drop_orphaned_notification_feed_rows(db)?;
    let mut statement = db
        .prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM notification_feed ORDER BY createdAt DESC, id DESC LIMIT ?1"
        ))
        .map_err(sql_error)?;
    let items = statement
        .query_map(params![NOTIFICATION_FEED_READ_LIMIT as i64], item_from_row)
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<Value>>>()
        .map_err(sql_error)?;
    let unread_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM notification_feed WHERE readAt IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    /*
    The jump order: newest undeferred unread first; deferred rows come last, oldest deferral first, so cycling with defer walks every pending row before returning to the first one.
    */
    let next_unread_id: Option<String> = db
        .query_row(
            r#"
            SELECT id FROM notification_feed WHERE readAt IS NULL
            ORDER BY (deferredAt IS NOT NULL) ASC,
                     CASE WHEN deferredAt IS NULL THEN createdAt END DESC,
                     deferredAt ASC,
                     id DESC
            LIMIT 1
            "#,
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let mut state = json!({
        "items": items,
        "unreadCount": unread_count,
    });
    if let Some(next_unread_id) = next_unread_id {
        state["nextUnreadId"] = Value::String(next_unread_id);
    }
    Ok(state)
}
