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
    "gbotsChangeSenders",
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
        _ if OTHER_ACTORS.contains(&entry.actor.kind.as_str()) => true,
        _ if !ended => false,
        _ if OPENING_ACTIONS.contains(&entry.action.as_str()) => true,
        // A first grant goes through a signed approval; a renewal of one already let in doesn't.
        _ if entry.action == "computers.grant" => entry.outcome == "approved",
        _ => false,
    }
}

/// The log's entries newest first and, for each, whether it notifies.
fn decisions(entries: &[StoredEntry]) -> impl Iterator<Item = (&StoredEntry, bool)> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry, notifies(entry, &entries[index + 1..])))
}

/// Takes out of the feed the security rows `notifies` would not have made, written before it
/// existed, so the bell is clean without the user clicking anything. Returns how many went.
pub(crate) fn remove_unimportant_security_rows(
    db: &rusqlite::Connection,
    entries: &[StoredEntry],
) -> rusqlite::Result<usize> {
    let mut removed = 0;
    for (entry, notifies) in decisions(entries) {
        if !notifies {
            removed += db.execute(
                "DELETE FROM notification_feed WHERE kind = ?1 AND sessionId = ?2",
                rusqlite::params![
                    NOTIFICATION_FEED_KIND_SECURITY,
                    security_notification_session_id(entry.seq)
                ],
            )?;
        }
    }
    Ok(removed)
}

fn remove_unimportant_rows(state: &AppState) {
    let entries = super::live_log().read_recent(usize::MAX);
    let removed = crate::storage::open_gxserver_database(&state.paths)
        .map_err(|error| error.to_string())
        .and_then(|db| {
            remove_unimportant_security_rows(&db, &entries).map_err(|error| error.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::types::ActorRecord;
    use ghostex_gx_protocol::approval::SensitiveAction;

    /// Who an action notifies when you did it on this computer; anyone else always notifies.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum WhenYouDidIt {
        /// It opens this computer up, adds a key or weakens a protection: always.
        Notifies,
        /// Only when a signed approval let it in (a computer's first grant, not a renewal).
        NotifiesWhenApproved,
        /// Basic: the log only.
        LogOnly,
    }
    use WhenYouDidIt::*;

    /// Every action the log records: the approval names, then the ids handlers log.
    const ACTIONS: &[(&str, WhenYouDidIt)] = &[
        ("collabEnable", Notifies),
        ("collabShareChat", Notifies),
        ("collabChangeShare", LogOnly),
        ("collabEnableSendMode", Notifies),
        ("collabApproveGuestMessage", LogOnly),
        ("collabChangeRelay", LogOnly),
        ("pairingShowCode", LogOnly),
        ("pairingApproveDevice", Notifies),
        ("easyConnectEnable", Notifies),
        ("easyConnectAllowList", Notifies),
        ("easyConnectPorts", Notifies),
        ("easyConnectRelay", LogOnly),
        ("computerRegister", Notifies),
        ("computerGrant", LogOnly),
        ("computerChangeAccess", Notifies),
        ("autopilotAnswerApprovals", Notifies),
        ("autopilotLiftLimits", Notifies),
        ("gbotsEnable", Notifies),
        ("gbotsAddBot", Notifies),
        ("gbotsChangeSenders", Notifies),
        ("settingsWeakenSecurity", Notifies),
        ("answerOtherSessionPrompt", LogOnly),
        ("typeIntoOtherSession", LogOnly),
        ("interruptOtherSession", LogOnly),
        ("addApproverKey", Notifies),
        ("computers.grant", NotifiesWhenApproved),
        ("computers.revoke", LogOnly),
        ("easyConnect.enable", Notifies),
        ("easyConnect.disable", LogOnly),
        ("easyConnect.ports", Notifies),
        ("easyConnect.allowList", Notifies),
        ("easyConnect.relay", LogOnly),
        ("pairing.pair", Notifies),
        ("pairing.remove", LogOnly),
        ("pairing.code", LogOnly),
        ("pairing.enableSsh", Notifies),
        ("ownerKey.add", Notifies),
        ("approverKey.add", Notifies),
        ("approverKey.remove", LogOnly),
        ("autopilot.widen", Notifies),
        ("autopilot.answer", LogOnly),
        ("gbots.create", Notifies),
        ("gbots.claim", LogOnly),
        ("settings.ownerOnly", LogOnly),
        ("settingsSync.instructions", LogOnly),
    ];

    /// Fails to compile when an approval kind is added, so it gets a row in `ACTIONS`.
    fn approval_kind(action: SensitiveAction) -> &'static str {
        use SensitiveAction::*;
        match action {
            CollabEnable
            | CollabShareChat
            | CollabChangeShare
            | CollabEnableSendMode
            | CollabApproveGuestMessage
            | CollabChangeRelay
            | PairingShowCode
            | PairingApproveDevice
            | EasyConnectEnable
            | EasyConnectAllowList
            | EasyConnectPorts
            | EasyConnectRelay
            | ComputerRegister
            | ComputerGrant
            | ComputerChangeAccess
            | AutopilotAnswerApprovals
            | AutopilotLiftLimits
            | GbotsEnable
            | GbotsAddBot
            | GbotsChangeSenders
            | SettingsWeakenSecurity
            | AnswerOtherSessionPrompt
            | TypeIntoOtherSession
            | InterruptOtherSession
            | AddApproverKey => "known",
            Other(_) => "other",
        }
    }

    /// Every actor kind `Actor::record` writes, and whether it is you at this computer.
    const ACTORS: &[(&str, bool)] = &[
        ("owner", true),
        ("sharedToken", true),
        ("ghostex", true),
        ("autopilot", true),
        ("settingsSync", true),
        ("session", false),
        ("pairedDevice", false),
        ("connectedComputer", false),
        ("extension", false),
        ("collabGuest", false),
    ];

    const OUTCOMES: &[&str] = &["done", "pending", "approved", "denied", "refused", "failed"];

    fn entry(seq: u64, at_ms: i64, action: &str, actor: &str, outcome: &str) -> StoredEntry {
        StoredEntry {
            v: 1,
            seq,
            at_ms,
            action: action.into(),
            summary: action.into(),
            target: None,
            actor: ActorRecord {
                kind: actor.into(),
                label: actor.into(),
                id: None,
            },
            outcome: outcome.into(),
            reason: None,
            approval: None,
            prev_hash: String::new(),
            hash: String::new(),
        }
    }

    #[test]
    fn every_approval_kind_has_a_row() {
        for name in [
            "collabEnable",
            "collabShareChat",
            "collabChangeShare",
            "collabEnableSendMode",
            "collabApproveGuestMessage",
            "collabChangeRelay",
            "pairingShowCode",
            "pairingApproveDevice",
            "easyConnectEnable",
            "easyConnectAllowList",
            "easyConnectPorts",
            "easyConnectRelay",
            "computerRegister",
            "computerGrant",
            "computerChangeAccess",
            "autopilotAnswerApprovals",
            "autopilotLiftLimits",
            "gbotsEnable",
            "gbotsAddBot",
            "gbotsChangeSenders",
            "settingsWeakenSecurity",
            "answerOtherSessionPrompt",
            "typeIntoOtherSession",
            "interruptOtherSession",
            "addApproverKey",
        ] {
            assert_eq!(
                approval_kind(SensitiveAction::from_wire(name)),
                "known",
                "{name}"
            );
            assert!(
                ACTIONS.iter().any(|(kind, _)| *kind == name),
                "{name} needs a row in ACTIONS"
            );
        }
    }

    /// The rule over every action, actor kind and outcome, each one entry on its own.
    #[test]
    fn only_important_entries_notify() {
        for &(action, when_you) in ACTIONS {
            for &(actor, is_you) in ACTORS {
                for &outcome in OUTCOMES {
                    let ended = matches!(outcome, "done" | "approved" | "denied");
                    let expected = match outcome {
                        "pending" | "refused" => false,
                        _ if !is_you => true,
                        _ if !ended => false,
                        _ => match when_you {
                            Notifies => true,
                            NotifiesWhenApproved => outcome == "approved",
                            LogOnly => false,
                        },
                    };
                    assert_eq!(
                        notifies(&entry(1, 0, action, actor, outcome), &[]),
                        expected,
                        "{action} by {actor}, {outcome}"
                    );
                }
            }
        }
    }

    /// The entries behind the user's screenshot: what they did themselves stays in the log,
    /// turning Collab on notifies once.
    #[test]
    fn the_screenshot_keeps_only_collab_on() {
        let mut approved = entry(18, 0, "collabEnable", "owner", "approved");
        approved.approval = Some(crate::audit::ApprovalRef {
            id: "apr_1".into(),
            signer: Some("k".into()),
        });
        assert!(notifies(&approved, &[]));
        assert!(!notifies(
            &entry(17, 0, "collabEnable", "owner", "pending"),
            &[]
        ));
        assert!(!notifies(
            &entry(19, 0, "computers.grant", "owner", "done"),
            &[]
        ));
        assert!(!notifies(
            &entry(15, 0, "computers.grant", "sharedToken", "done"),
            &[]
        ));
    }

    #[test]
    fn the_third_refusal_in_ten_minutes_notifies() {
        let minute = 60_000;
        let refused = |seq: u64, at: i64| {
            entry(
                seq,
                at * minute,
                "settings.ownerOnly",
                "sharedToken",
                "refused",
            )
        };
        let mut log: Vec<StoredEntry> = Vec::new();
        let mut told = Vec::new();
        for (seq, at) in [(1, 0), (2, 1), (3, 2), (4, 3), (5, 30), (6, 31)] {
            let next = refused(seq, at);
            told.push(notifies(&next, &log));
            log.insert(0, next);
        }
        assert_eq!(told, [false, false, true, false, false, false]);
        // Another action's refusals do not count toward this one.
        let other = vec![
            entry(2, minute, "pairing.pair", "ghostex", "refused"),
            entry(1, 0, "pairing.pair", "ghostex", "refused"),
        ];
        assert!(!notifies(&refused(3, 2), &other));
        // Denials count too.
        let denied = |seq: u64| entry(seq, 0, "typeIntoOtherSession", "owner", "denied");
        assert!(notifies(&denied(3), &[denied(2), denied(1)]));
    }

    #[test]
    fn existing_basic_rows_leave_the_feed_and_important_ones_stay() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::run_gxserver_migrations(&mut db).unwrap();
        let mut approved = entry(3, 0, "collabEnable", "owner", "approved");
        approved.approval = Some(crate::audit::ApprovalRef {
            id: "apr_1".into(),
            signer: None,
        });
        // Newest first, as `read_recent` returns them.
        let entries = vec![
            entry(4, 0, "computers.grant", "owner", "done"),
            approved,
            entry(2, 0, "collabEnable", "owner", "pending"),
            entry(1, 0, "computers.grant", "sharedToken", "done"),
        ];
        for entry in entries.iter().rev() {
            insert_notification_feed_row(
                &db,
                NewNotificationFeedRow {
                    project_id: SECURITY_NOTIFICATION_PROJECT_ID.into(),
                    session_id: security_notification_session_id(entry.seq),
                    kind: NOTIFICATION_FEED_KIND_SECURITY,
                    title: entry.summary.clone(),
                    subtitle: String::new(),
                    body: String::new(),
                    agent_name: None,
                    attention_event_id: None,
                },
            )
            .unwrap();
        }
        assert_eq!(remove_unimportant_security_rows(&db, &entries).unwrap(), 3);
        let left: Vec<(String, bool)> = db
            .prepare("SELECT sessionId, readAt IS NULL FROM notification_feed")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(left, [("security:3".to_string(), true)]);
        // Running again on the next start changes nothing.
        assert_eq!(remove_unimportant_security_rows(&db, &entries).unwrap(), 0);
    }
}
