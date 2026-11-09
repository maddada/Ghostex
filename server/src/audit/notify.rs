//! The notification for an important sensitive action: a row in the notification bell of every
//! client of this computer and a push to its paired phones. Every action stays in the log
//! (Settings > Security, `ghostex security log`); only those `notifies` picks reach the bell.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User: "i don't get why i see these notifications? please don't make them show blue dot there for these and make them only show essential stuff not basic ones here in notifications (can be still in settings logs but notification only for important security stuff not all", then "I mean the enable collab one is important i guess". So the bell and the phone hear about: anything that opens this computer to other people or devices (Collab on, sharing a chat, Send mode, pairing a phone, SSH for phones, Easy Connect on or widened, a computer let in for the first time or widened), a new owner or approver key, security weakened (a security-relaxing setting, Autopilot widened, GBots on or given a bot), even when you did it; anything an agent's session token, a phone, another computer, an extension or a Collab guest did; the same action refused or denied three times in ten minutes; and a broken log chain. They don't hear about what you did yourself on this computer otherwise, a request still waiting for your approval (its card in the Approvals pill already asks), renewing a computer's grant, Autopilot answering a prompt it was allowed to answer, or settings sync from your own computers.
//! CDXC:Security 2026-11-08 WHY: the shared token is used by this computer's own app as much as by agents, so on its own it does not make an action "someone else's"; the sensitive ones it can reach go through a signed approval, which the rule above already covers by action.

use std::sync::{Arc, OnceLock};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::notification_feed::store::{
    insert_notification_feed_row, NewNotificationFeedRow, NOTIFICATION_FEED_KIND_SECURITY,
    SECURITY_NOTIFICATION_PROJECT_ID,
};
use crate::server::AppState;

use super::chain::StoredEntry;
use super::types::Action;

static STATE: OnceLock<Arc<AppState>> = OnceLock::new();

/// Called once at gxserver start, so entries written from then on notify.
pub(crate) fn install_notifier(state: Arc<AppState>) {
    if let Err(error) = super::live_log().prime() {
        log(&state, "securityLog.readFailed", format!("{error:#}"));
    }
    remove_unimportant_rows(&state);
    alert_broken_chain(&state);
    let _ = STATE.set(state);
}

/// Actions that open this computer to other people or devices, add a key that may act as you, or
/// weaken a protection: they notify whoever did them. `kind`s are the approval names
/// (`SensitiveAction`) and the dotted ids handlers log.
const OPENING_ACTIONS: &[&str] = &[
    // Collab
    "collabEnable",
    "collabShareChat",
    "collabEnableSendMode",
    // Phones
    "pairingApproveDevice",
    "pairing.pair",
    "pairing.enableSsh",
    // Easy Connect
    "easyConnectEnable",
    "easyConnect.enable",
    "easyConnectAllowList",
    "easyConnect.allowList",
    "easyConnectPorts",
    "easyConnect.ports",
    // Connected computers (`computers.grant` only when a signed approval let it in, below)
    "computerRegister",
    "computerChangeAccess",
    // Keys
    "ownerKey.add",
    "approverKey.add",
    "addApproverKey",
    // Protections weakened
    "settingsWeakenSecurity",
    "autopilotAnswerApprovals",
    "autopilotLiftLimits",
    "autopilot.widen",
    "gbotsEnable",
    "gbotsAddBot",
    "gbots.create",
    "gbotsApiKey",
    "gbots.apiOn",
    "gbots.apiNewKey",
    "gbotsSetComputer",
    // Installing Collab: downloads a component and runs it
    "installExtensionComponent",
];

/// Actor kinds that are not you at this computer: an agent's own session token, a paired phone,
/// another computer's agents, an extension, a Collab guest.
const OTHER_ACTORS: &[&str] = &[
    "session",
    "pairedDevice",
    "connectedComputer",
    "extension",
    "collabGuest",
];

/// What Chief's head role does without an approval (`Principal::skips_approval`), logged with the
/// `allowed` outcome.
const HEAD_STEERING_ACTIONS: &[&str] = &[
    "answerOtherSessionPrompt",
    "typeIntoOtherSession",
    "interruptOtherSession",
];

/// Refused or denied this many times in `REPEAT_WINDOW_MS` (the same action), the third notifies.
const REPEATED_REFUSALS: usize = 3;
const REPEAT_WINDOW_MS: i64 = 10 * 60 * 1000;

/// Whether `entry` reaches the bell and the phones. `earlier` is the log before it, newest first
/// (only the last few minutes matter). See the CDXC decision at the top of this file.
pub(crate) fn notifies(entry: &StoredEntry, earlier: &[StoredEntry]) -> bool {
    let refusal = |entry: &StoredEntry| matches!(entry.outcome.as_str(), "refused" | "denied");
    if refusal(entry) {
        let repeats = 1 + earlier
            .iter()
            .take_while(|before| entry.at_ms - before.at_ms <= REPEAT_WINDOW_MS)
            .filter(|before| before.action == entry.action && refusal(before))
            .count();
        if repeats == REPEATED_REFUSALS {
            return true;
        }
    }
    let ended = matches!(entry.outcome.as_str(), "done" | "approved" | "denied");
    match entry.outcome.as_str() {
        // The approval card already asks; the outcome notifies once it comes.
        "pending" => false,
        // A refusal on its own is the gate working; only a repeat (above) is news.
        "refused" => false,
        // CDXC:Chief 2026-11-08 DECISION: Coordinator, on the user's behalf: Chief's approval-free steering goes into the security log with a quiet outcome and never reaches the bell or the phone, although it is done with an agent's session token (which otherwise always notifies).
        "allowed" if HEAD_STEERING_ACTIONS.contains(&entry.action.as_str()) => false,
        _ if OTHER_ACTORS.contains(&entry.actor.kind.as_str()) => true,
        _ if !ended => false,
        _ if OPENING_ACTIONS.contains(&entry.action.as_str()) => true,
        // A first grant goes through a signed approval; a renewal of one already let in doesn't.
        _ if entry.action == "computers.grant" => entry.outcome == "approved",
        _ => false,
    }
}

/// Takes out of the feed the security rows `notifies` would not have made, written before it
/// existed, so the bell is clean without the user clicking anything. Returns how many went.
///
/// It starts from the feed's security rows (a handful, the feed is pruned), not the log, which only
/// grows: `read_log(oldest)` returns the log's entries newest first, back to `oldest` and the
/// `REPEAT_WINDOW_MS` before it that the repeat rule reads. The deletes run in one transaction.
pub(crate) fn remove_unimportant_security_rows(
    db: &rusqlite::Connection,
    read_log: impl FnOnce(u64) -> Vec<StoredEntry>,
) -> rusqlite::Result<usize> {
    let rows: Vec<(String, u64)> = db
        .prepare("SELECT DISTINCT sessionId FROM notification_feed WHERE kind = ?1")?
        .query_map([NOTIFICATION_FEED_KIND_SECURITY], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|session_id| {
            let seq = session_id
                .strip_prefix(
                    crate::notification_feed::store::SECURITY_NOTIFICATION_SESSION_PREFIX,
                )?
                .parse::<u64>()
                .ok()?;
            Some((session_id, seq))
        })
        .collect();
    let Some(oldest) = rows.iter().map(|(_, seq)| *seq).min() else {
        return Ok(0);
    };
    let entries = read_log(oldest);
    let transaction = db.unchecked_transaction()?;
    let mut removed = 0;
    for (session_id, seq) in &rows {
        // A row whose entry the log no longer has is left alone.
        let Some(index) = entries.iter().position(|entry| entry.seq == *seq) else {
            continue;
        };
        if !notifies(&entries[index], &entries[index + 1..]) {
            removed += transaction.execute(
                "DELETE FROM notification_feed WHERE kind = ?1 AND sessionId = ?2",
                rusqlite::params![NOTIFICATION_FEED_KIND_SECURITY, session_id],
            )?;
        }
    }
    transaction.commit()?;
    Ok(removed)
}

fn remove_unimportant_rows(state: &AppState) {
    let removed = crate::storage::open_gxserver_database(&state.paths)
        .map_err(|error| error.to_string())
        .and_then(|db| {
            remove_unimportant_security_rows(&db, |oldest| {
                super::live_log().read_back_to(oldest, REPEAT_WINDOW_MS)
            })
            .map_err(|error| error.to_string())
        });
    match removed {
        Ok(0) => {}
        Ok(_) => crate::notification_feed::broadcast_notification_feed_changed(state),
        Err(error) => log(state, "securityLog.notifyFailed", error),
    }
}

/// The feed row's `sessionId` for a broken chain: one row however often gxserver starts.
const CHAIN_NOTIFICATION_SESSION_ID: &str = "security:chain";

/// Tells the bell and the phones once that the log was edited, cut short or damaged. A marker
/// beside the log names the break already told, so a restart does not ask again.
fn alert_broken_chain(state: &AppState) {
    let check = super::live_log().check();
    let Some(problem) = check.problem else {
        return;
    };
    let marker = super::live_log().path().with_file_name("chain-alerted");
    let key = format!("{}:{}:{:?}", problem.kind, problem.line, problem.seq);
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(key.as_str()) {
        return;
    }
    let body = format!(
        "{} Every entry is still listed in Settings > Security.",
        problem.message
    );
    if notify_feed_and_phones(
        state,
        CHAIN_NOTIFICATION_SESSION_ID.to_string(),
        "The security log was changed".to_string(),
        "Security".to_string(),
        body,
        chrono::Utc::now().timestamp_millis(),
    ) {
        let _ = std::fs::write(&marker, key);
    }
}

/// The feed row's `sessionId`: one row per entry, never superseded by the next one.
pub(crate) fn security_notification_session_id(seq: u64) -> String {
    format!(
        "{}{seq}",
        crate::notification_feed::store::SECURITY_NOTIFICATION_SESSION_PREFIX
    )
}

/// The bell row's title, subtitle and body for an entry.
pub(crate) fn notification_text(entry: &StoredEntry) -> (String, String, String) {
    let outcome = match entry.outcome.as_str() {
        "pending" => "Waiting for your approval",
        "approved" => "Approved by you",
        "denied" => "You said no",
        "refused" => "Refused",
        "failed" => "Failed",
        _ => "Done",
    };
    let mut body = format!("{outcome}. Asked by {}.", entry.actor.label);
    if let Some(reason) = &entry.reason {
        body.push(' ');
        body.push_str(reason);
    }
    let subtitle = entry
        .target
        .clone()
        .unwrap_or_else(|| "Security".to_string());
    (entry.summary.clone(), subtitle, body)
}

pub(super) fn announce(entry: &StoredEntry) {
    let Some(state) = STATE.get() else {
        return;
    };
    let earlier: Vec<StoredEntry> = super::live_log()
        .read_recent(64)
        .into_iter()
        .filter(|before| before.seq < entry.seq)
        .collect();
    if !notifies(entry, &earlier) {
        return;
    }
    let (title, subtitle, body) = notification_text(entry);
    notify_feed_and_phones(
        state,
        security_notification_session_id(entry.seq),
        title,
        subtitle,
        body,
        entry.at_ms,
    );
}

/// Writes the bell row and pushes it to the paired phones; false when the row could not be written.
fn notify_feed_and_phones(
    state: &AppState,
    session_id: String,
    title: String,
    subtitle: String,
    body: String,
    occurred_at_ms: i64,
) -> bool {
    let row = crate::storage::open_gxserver_database(&state.paths)
        .map_err(|error| error.to_string())
        .and_then(|db| {
            insert_notification_feed_row(
                &db,
                NewNotificationFeedRow {
                    project_id: SECURITY_NOTIFICATION_PROJECT_ID.into(),
                    session_id: session_id.clone(),
                    kind: NOTIFICATION_FEED_KIND_SECURITY,
                    title: title.clone(),
                    subtitle,
                    body: crate::notification_feed::store::bounded_notification_body(&body),
                    agent_name: None,
                    attention_event_id: None,
                },
            )
            .map_err(|error| error.message)
        });
    match row {
        Ok(row) => {
            crate::notification_feed::broadcast_notification_feed_changed(state);
            let id = row
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            state.push.enqueue_alert(crate::push::message::AlertEvent {
                kind: crate::push::message::AlertKind::Security,
                project_id: SECURITY_NOTIFICATION_PROJECT_ID.into(),
                session_id,
                notification_id: id,
                agent_name: "Ghostex".to_string(),
                project_name: title,
                occurred_at_ms,
            });
            true
        }
        Err(error) => {
            log(state, "securityLog.notifyFailed", error);
            false
        }
    }
}

pub(super) fn report_write_failure(action: &Action, error: &anyhow::Error) {
    if let Some(state) = STATE.get() {
        log(
            state,
            "securityLog.writeFailed",
            format!("{}: {error:#}", action.kind),
        );
    }
}

fn log(state: &AppState, event: &str, error: String) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Error,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error),
        details: None,
    });
}
