use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::domain::{now_iso, sql_error, DomainRepository, DomainResult, DomainStateError};

/// Newest rows the read endpoint returns. Mirrors NOTIFICATION_FEED_READ_LIMIT in the shared contract.
pub(crate) const NOTIFICATION_FEED_READ_LIMIT: usize = 40;
/// CDXC:Notifications 2026-09-30 DECISION:
/// User: "make notifications dropdown show max of 40 last notifications please and clear after that". The feed keeps 40 rows; active approval requests take a slot ahead of older ordinary rows under the 2026-11-02 decision below, so they stay visible until answered. Supersedes the 500-row retention that only pruned read rows.
/// Mirrors NOTIFICATION_FEED_RETENTION_LIMIT in the shared contract.
pub(crate) const NOTIFICATION_FEED_RETENTION_LIMIT: usize = 40;
/// Longest stored body, in characters. Mirrors NOTIFICATION_FEED_BODY_MAX_CHARS in the shared contract.
pub(crate) const NOTIFICATION_FEED_BODY_MAX_CHARS: usize = 280;

pub(crate) const NOTIFICATION_FEED_KIND_FINISHED: &str = "finished";
pub(crate) const NOTIFICATION_FEED_KIND_NEEDS_INPUT: &str = "needsInput";
pub(crate) const NOTIFICATION_FEED_KIND_BELL: &str = "bell";
pub(crate) const NOTIFICATION_FEED_KIND_CUSTOM: &str = "custom";
/// A chat message could not be sent for good (session_chat_send_failure.rs), drawn red.
pub(crate) const NOTIFICATION_FEED_KIND_SEND_FAILED: &str = "sendFailed";
/// The `attentionEventId` (a column clients never receive) that marks a coordinator's "stopped
/// getting reports" row (server/src/server/coordinator_runtime.rs): a `sendFailed` row the chat's
/// own failed sends neither replace nor clear.
///
/// CDXC:Coordinators 2026-11-10 WHY:
/// The row is `sendFailed` so the desktop and phones already in use draw it red as "Not sent" and open the coordinator, but it is not a chat message that failed: a chat-send failure on the same coordinator must not delete it or mark it read, and it must not delete theirs. Only its owner replaces it (`insert_coordinator_reports_notice`) and marks it read (`mark_coordinator_reports_notice_read`); retention and the user reading or dismissing it treat it like any row.
pub(crate) const COORDINATOR_REPORTS_NOTICE_MARK: &str = "coordinator-reports";
/// An agent's turn ended on an error Ghostex positively identified (failure.rs); clients draw it red.
pub(crate) const NOTIFICATION_FEED_KIND_FAILED: &str = "failed";
/// A banked usage reset of a saved account expires soon; clients draw it red.
pub(crate) const NOTIFICATION_FEED_KIND_RESET_EXPIRING: &str = "resetExpiring";
/// Ghostex used an expiring banked reset on its own.
pub(crate) const NOTIFICATION_FEED_KIND_RESET_REDEEMED: &str = "resetRedeemed";

/// CDXC:Notifications 2026-10-05 WHY:
/// Account rows (banked reset warnings) belong to no session, but the feed's one-live-row-per-session rule is exactly what they need per account: the 24-hour warning replaces the unread 3-day one, and an automatic redeem replaces both. Their `sessionId` is this prefix plus the saved account id, which no real session id can collide with; the orphan sweep and the read-acknowledges-the-thread rule skip them, and every row carries `accountId` so a client opens the account instead of a session.
pub(crate) const ACCOUNT_NOTIFICATION_SESSION_PREFIX: &str = "account:";
/// The `projectId` account rows are stored under.
pub(crate) const ACCOUNT_NOTIFICATION_PROJECT_ID: &str = "accounts";

/// A sensitive action recorded in the security log (server/src/audit/notify.rs): one row per log
/// entry, under this prefix plus the entry's number, so a click opens Settings > Security and the
/// orphan sweep keeps it. Clients draw it like an account row, with no session to open.
pub(crate) const NOTIFICATION_FEED_KIND_SECURITY: &str = "security";
pub(crate) const SECURITY_NOTIFICATION_SESSION_PREFIX: &str = "security:";
/// The `projectId` security rows are stored under.
pub(crate) const SECURITY_NOTIFICATION_PROJECT_ID: &str = "security";

pub(crate) fn account_notification_session_id(account_id: &str) -> String {
    format!("{ACCOUNT_NOTIFICATION_SESSION_PREFIX}{account_id}")
}

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

/// Insert a row, superseding the session's unread rows first, then prune rows past the retention limit.
/// One live row per session keeps the unread count equal to the number of sessions waiting, not the number of events.
pub(crate) fn insert_notification_feed_row(
    db: &Connection,
    row: NewNotificationFeedRow,
) -> DomainResult<Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = now_iso();
    // A finished turn says nothing about a message that was not sent, so it never replaces that row.
    // A newer row of any other kind (finished, needs input, failed) therefore sits next to an unread
    // Not sent row: a session can hold two unread rows, Not sent and the newer one.
    db.execute(
        "DELETE FROM notification_feed WHERE sessionId = ?1 AND readAt IS NULL AND (kind <> ?2 OR ?3 = ?2) AND COALESCE(attentionEventId, '') <> ?4",
        params![
            row.session_id,
            NOTIFICATION_FEED_KIND_SEND_FAILED,
            row.kind,
            COORDINATOR_REPORTS_NOTICE_MARK
        ],
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

/// Marks one row read as it is written: a row the alert rule keeps quiet (CDXC:Notifications
/// 2026-11-08 in packages/gx-core/src/alert_rule.rs) stays in the bell without lighting it.
pub(crate) fn mark_notification_feed_row_read(db: &Connection, id: &str) -> DomainResult<()> {
    db.execute(
        "UPDATE notification_feed SET readAt = ?1 WHERE id = ?2 AND readAt IS NULL",
        params![now_iso(), id],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Insert a row beside the session's unread rows instead of superseding them: only an earlier
/// unread row with the same title goes. For news about a chat that is not about its agent (a
/// Ghostex Collab guest joining, `collab_shim::http`, `notify_guest_joined`), which must not hide an
/// approval or a finished turn waiting in that chat.
pub(crate) fn insert_notification_feed_row_beside(
    db: &Connection,
    row: NewNotificationFeedRow,
) -> DomainResult<Value> {
    db.execute(
        "DELETE FROM notification_feed WHERE sessionId = ?1 AND readAt IS NULL AND kind = ?2 AND title = ?3 AND COALESCE(attentionEventId, '') <> ?4",
        params![
            row.session_id,
            row.kind,
            row.title,
            COORDINATOR_REPORTS_NOTICE_MARK
        ],
    )
    .map_err(sql_error)?;
    insert_row_superseding_nothing(db, row)
}

/// Inserts the row as it is, removing no other row, then prunes past the retention limit.
fn insert_row_superseding_nothing(
    db: &Connection,
    row: NewNotificationFeedRow,
) -> DomainResult<Value> {
    let id = uuid::Uuid::new_v4().to_string();
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
            now_iso(),
        ],
    )
    .map_err(sql_error)?;
    prune_notification_feed(db)?;
    read_notification_feed_item(db, &id)?
        .ok_or_else(|| DomainStateError::not_found("Notification was not stored."))
}

/// Keep active approvals before older ordinary rows, within the feed's 40-row limit.
fn prune_notification_feed(db: &Connection) -> DomainResult<()> {
    let mut statement = db
        .prepare("SELECT id, projectId, sessionId FROM notification_feed ORDER BY createdAt DESC, id DESC")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    if rows.len() <= NOTIFICATION_FEED_RETENTION_LIMIT {
        return Ok(());
    }
    let mut protected = std::collections::HashSet::new();
    for (id, project, session) in &rows {
        if approval_row_waits(db, id, project, session)? {
            protected.insert(id.as_str());
        }
    }
    let mut ordinary_left = NOTIFICATION_FEED_RETENTION_LIMIT.saturating_sub(protected.len());
    for (id, _, _) in &rows {
        if protected.contains(id.as_str()) {
            continue;
        }
        if ordinary_left > 0 {
            ordinary_left -= 1;
        } else {
            db.execute("DELETE FROM notification_feed WHERE id = ?1", params![id])
                .map_err(sql_error)?;
        }
    }
    Ok(())
}

/// Rows whose session was deleted have nothing to open; drop them before listing.
fn drop_orphaned_notification_feed_rows(db: &Connection) -> DomainResult<()> {
    db.execute(
        "DELETE FROM notification_feed WHERE sessionId NOT IN (SELECT sessionId FROM sessions) AND substr(sessionId, 1, length(?1)) <> ?1 AND substr(sessionId, 1, length(?2)) <> ?2",
        params![ACCOUNT_NOTIFICATION_SESSION_PREFIX, SECURITY_NOTIFICATION_SESSION_PREFIX],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Mark the session's unread rows read. Automatic activity transitions preserve deferred rows; explicit acknowledgement includes them.
pub(crate) fn mark_session_notifications_read(
    db: &Connection,
    session_id: &str,
    include_deferred: bool,
) -> DomainResult<usize> {
    let sql = if include_deferred {
        "UPDATE notification_feed SET readAt = ?1, deferredAt = NULL WHERE sessionId = ?2 AND readAt IS NULL"
    } else {
        // Attention ending on its own says nothing about a message that was not sent.
        "UPDATE notification_feed SET readAt = ?1 WHERE sessionId = ?2 AND readAt IS NULL AND deferredAt IS NULL AND kind <> 'sendFailed'"
    };
    db.execute(sql, params![now_iso(), session_id])
        .map_err(sql_error)
}

/// The failed send's row goes read once the failure is dealt with.
pub(crate) fn mark_session_send_failure_read(
    db: &Connection,
    session_id: &str,
) -> DomainResult<usize> {
    db.execute(
        "UPDATE notification_feed SET readAt = ?1 WHERE sessionId = ?2 AND readAt IS NULL AND kind = ?3 AND COALESCE(attentionEventId, '') <> ?4",
        params![
            now_iso(),
            session_id,
            NOTIFICATION_FEED_KIND_SEND_FAILED,
            COORDINATOR_REPORTS_NOTICE_MARK
        ],
    )
    .map_err(sql_error)
}

/// Writes a coordinator's "stopped getting reports" row (marked with
/// `COORDINATOR_REPORTS_NOTICE_MARK`), replacing only its own earlier unread one: the coordinator's
/// other unread rows stay, a chat-send failure with the same title included.
pub(crate) fn insert_coordinator_reports_notice(
    db: &Connection,
    mut row: NewNotificationFeedRow,
) -> DomainResult<Value> {
    row.attention_event_id = Some(COORDINATOR_REPORTS_NOTICE_MARK.to_string());
    db.execute(
        "DELETE FROM notification_feed WHERE sessionId = ?1 AND readAt IS NULL AND attentionEventId = ?2",
        params![row.session_id, COORDINATOR_REPORTS_NOTICE_MARK],
    )
    .map_err(sql_error)?;
    insert_row_superseding_nothing(db, row)
}

/// The coordinator's reports went out again: its "stopped getting reports" row goes read.
/// Returns how many rows changed.
pub(crate) fn mark_coordinator_reports_notice_read(
    db: &Connection,
    session_id: &str,
) -> DomainResult<usize> {
    db.execute(
        "UPDATE notification_feed SET readAt = ?1 WHERE sessionId = ?2 AND readAt IS NULL AND attentionEventId = ?3",
        params![now_iso(), session_id, COORDINATOR_REPORTS_NOTICE_MARK],
    )
    .map_err(sql_error)
}

/// Turns the session's unread turn-end row into a Needs input row with `body`, when its finished
/// turn left questions waiting (`session_chat_open_questions.rs`). Returns how many rows changed.
pub(crate) fn mark_unread_session_notification_needs_input(
    db: &Connection,
    session_id: &str,
    body: &str,
) -> DomainResult<usize> {
    db.execute(
        "UPDATE notification_feed SET kind = ?2, body = ?3 WHERE sessionId = ?1 AND readAt IS NULL AND kind IN (?4, ?2) AND NOT (kind = ?2 AND body = ?3)",
        params![
            session_id,
            NOTIFICATION_FEED_KIND_NEEDS_INPUT,
            body,
            NOTIFICATION_FEED_KIND_FINISHED
        ],
    )
    .map_err(sql_error)
}

/// Sessions of the unread rows, of one row when `id` is given; read before a mark-read so the caller knows which threads it read.
pub(crate) fn unread_notification_session_ids(
    db: &Connection,
    id: Option<&str>,
) -> DomainResult<Vec<String>> {
    let mut statement = db
        .prepare(
            "SELECT DISTINCT sessionId FROM notification_feed WHERE readAt IS NULL AND (?1 IS NULL OR id = ?1) AND substr(sessionId, 1, length(?2)) <> ?2 AND substr(sessionId, 1, length(?3)) <> ?3",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![
                id,
                ACCOUNT_NOTIFICATION_SESSION_PREFIX,
                SECURITY_NOTIFICATION_SESSION_PREFIX
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok(rows)
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
    let session: Option<(String, String)> = db
        .query_row(
            "SELECT projectId, sessionId FROM notification_feed WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some((project, session)) = session {
        if approval_row_waits(db, id, &project, &session)? {
            return Ok(0);
        }
    }
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
    let mut statement = db
        .prepare("SELECT id, projectId, sessionId FROM notification_feed")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut removed = 0;
    for (id, project, session) in rows {
        if !approval_row_waits(db, &id, &project, &session)? {
            removed += db
                .execute("DELETE FROM notification_feed WHERE id = ?1", params![id])
                .map_err(sql_error)?;
        }
    }
    Ok(removed)
}

/// The same live decision as the sidebar's red dot, including approvals that only the
/// coordinator's screen detector has seen.
fn approval_waits(db: &Connection, project_id: &str, session_id: &str) -> DomainResult<bool> {
    let repository = DomainRepository::new(db, "");
    Ok(repository
        .get_session(project_id, session_id)?
        .is_some_and(|session| {
            crate::session_approval::session_awaits_approval(&session, &now_iso())
        }))
}

fn approval_row_waits(
    db: &Connection,
    id: &str,
    project_id: &str,
    session_id: &str,
) -> DomainResult<bool> {
    if !approval_waits(db, project_id, session_id)? {
        return Ok(false);
    }
    let newest: Option<String> = db
        .query_row(
            "SELECT id FROM notification_feed WHERE sessionId = ?1 ORDER BY createdAt DESC, id DESC LIMIT 1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(newest.as_deref() == Some(id))
}

pub(crate) fn notification_exists(db: &Connection, id: &str) -> DomainResult<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM notification_feed WHERE id = ?1)",
        params![id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(sql_error)
}

/// The row written for this attention event of this session (the newest when several share it).
/// A read row still counts: reading a row does not end the attention it was written for.
pub(crate) fn attention_notification_id(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    attention_event_id: &str,
) -> DomainResult<Option<String>> {
    db.query_row(
        "SELECT id FROM notification_feed WHERE projectId = ?1 AND sessionId = ?2 AND attentionEventId = ?3 ORDER BY createdAt DESC, id DESC LIMIT 1",
        params![project_id, session_id, attention_event_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
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
    if let Some(account_id) = item["sessionId"]
        .as_str()
        .and_then(|id| id.strip_prefix(ACCOUNT_NOTIFICATION_SESSION_PREFIX))
    {
        item["accountId"] = Value::String(account_id.to_string());
    }
    if let Some(entry) = item["sessionId"]
        .as_str()
        .and_then(|id| id.strip_prefix(SECURITY_NOTIFICATION_SESSION_PREFIX))
        .and_then(|seq| seq.parse::<u64>().ok())
    {
        item["securityEntry"] = Value::from(entry);
    }
    Ok(item)
}

pub(crate) fn read_notification_feed_item(
    db: &Connection,
    id: &str,
) -> DomainResult<Option<Value>> {
    db.query_row(
        &format!("SELECT {ITEM_COLUMNS} FROM notification_feed WHERE id = ?1"),
        params![id],
        item_from_row,
    )
    .optional()
    .map_err(sql_error)
}

/// The full feed state every endpoint returns: newest first, unread count, and the jump target.
///
/// CDXC:Notifications 2026-10-18 WHY:
/// `nextUnreadDeferredAt` rides with the jump target when that row was pushed back: a desktop that shows remote computers in its bell merges every daemon's jump target into one order, and ranks deferred targets last the way this query does within one machine.
///
/// CDXC:Notifications 2026-11-02 DECISION:
/// User: "when we have permissions request. it has to stay shown in notifications until it's answered ... we're marking it as read even if not dealt with, this is wrong" and "show red on the notifications to indicate this pls". An open approval is unread in the feed even if focus, a screen read or Mark all read wrote readAt. Dismiss and Clear all preserve its row. When the agent answers it, the stored read state applies again.
pub(crate) fn read_notification_feed_state(db: &Connection) -> DomainResult<Value> {
    drop_orphaned_notification_feed_rows(db)?;
    prune_notification_feed(db)?;
    let mut statement = db
        .prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM notification_feed ORDER BY createdAt DESC, id DESC LIMIT ?1"
        ))
        .map_err(sql_error)?;
    let mut items = statement
        .query_map(params![NOTIFICATION_FEED_READ_LIMIT as i64], item_from_row)
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<Value>>>()
        .map_err(sql_error)?;
    overlay_live_session_agent_icons(db, &mut items)?;
    super::security_actions::overlay_removable_devices(db, &mut items);
    let mut newest_by_session = std::collections::HashMap::new();
    for item in &items {
        if let (Some(session), Some(id)) = (item["sessionId"].as_str(), item["id"].as_str()) {
            newest_by_session
                .entry(session.to_string())
                .or_insert_with(|| id.to_string());
        }
    }
    let mut statement = db
        .prepare(
            "SELECT id, projectId, sessionId, readAt, deferredAt FROM notification_feed \
             ORDER BY (deferredAt IS NOT NULL) ASC, \
             CASE WHEN deferredAt IS NULL THEN createdAt END DESC, deferredAt ASC, id DESC",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut approvals = std::collections::HashMap::new();
    let mut unread_count = 0usize;
    let mut next_unread: Option<(String, Option<String>)> = None;
    for (id, project, session, read_at, deferred_at) in rows {
        let approval = match approvals.get(&session) {
            Some(approval) => *approval,
            None => {
                let approval = approval_waits(db, &project, &session)?;
                approvals.insert(session.clone(), approval);
                approval
            }
        };
        let approval_row = approval
            && newest_by_session
                .get(&session)
                .is_some_and(|newest| newest == &id);
        if read_at.is_none() || approval_row {
            unread_count += 1;
            if next_unread.is_none() {
                next_unread = Some((id, deferred_at));
            }
        }
    }
    for item in &mut items {
        let active = item["sessionId"].as_str().is_some_and(|session| {
            approvals.get(session) == Some(&true)
                && newest_by_session
                    .get(session)
                    .and_then(|id| item["id"].as_str().map(|row| id == row))
                    == Some(true)
        });
        if active {
            item["read"] = Value::Bool(false);
            item["awaitingApproval"] = Value::Bool(true);
            item["kind"] = Value::String(NOTIFICATION_FEED_KIND_NEEDS_INPUT.to_string());
            if item["body"]
                .as_str()
                .is_none_or(|body| !body.contains("approval"))
            {
                item["body"] = Value::String("Waiting for your approval".to_string());
            }
        }
    }
    /*
    The jump order: newest undeferred unread first; deferred rows come last, oldest deferral first, so cycling with defer walks every pending row before returning to the first one.
    */
    let mut state = json!({
        "items": items,
        "unreadCount": unread_count,
    });
    if let Some((next_unread_id, deferred_at)) = next_unread {
        state["nextUnreadId"] = Value::String(next_unread_id);
        if let Some(deferred_at) = deferred_at {
            state["nextUnreadDeferredAt"] = Value::String(deferred_at);
        }
    }
    Ok(state)
}

/// The icon slug a client should draw for a session: the sidebar agent's icon from launch settings, which is what a `custom-…` agent configuration declares as its CLI family, then the transcript family, then the runtime agent name.
pub(crate) fn notification_agent_icon(session: &Value) -> Option<String> {
    session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("icon"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|icon| !icon.is_empty() && *icon != "browser")
        .map(str::to_string)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(session))
        .or_else(|| crate::presentation::read_runtime_text(session, "agentName"))
}

/// Rows written before the icon resolution existed carry the raw `custom-…` agent id, and a session's agent icon can change after the row was written, so the live session's icon wins over the stored value whenever the session still exists.
fn overlay_live_session_agent_icons(db: &Connection, items: &mut [Value]) -> DomainResult<()> {
    let mut icon_by_session: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for item in items.iter_mut() {
        let Some(session_id) = item.get("sessionId").and_then(Value::as_str) else {
            continue;
        };
        let icon = match icon_by_session.get(session_id) {
            Some(icon) => icon.clone(),
            None => {
                let icon = db
                    .query_row(
                        "SELECT launchSettingsJson, runtimeSettingsJson FROM sessions WHERE sessionId = ?1",
                        params![session_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .and_then(|(launch_settings, runtime_settings)| {
                        let session = json!({
                            "launchSettings": serde_json::from_str::<Value>(&launch_settings).unwrap_or(Value::Null),
                            "runtimeSettings": serde_json::from_str::<Value>(&runtime_settings).unwrap_or(Value::Null),
                        });
                        notification_agent_icon(&session)
                    });
                icon_by_session.insert(session_id.to_string(), icon.clone());
                icon
            }
        };
        if let Some(icon) = icon {
            item["agentName"] = Value::String(icon);
        }
    }
    Ok(())
}
