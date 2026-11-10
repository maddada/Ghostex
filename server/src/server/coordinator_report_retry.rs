//! A coordinator whose threads' reports keep failing: the run of failures, the give-up after
//! `GIVE_UP_AFTER_MS`, its row in Notifications, and the retry a new message asks for. The
//! supervisor that delivers the reports is server/src/server/coordinator_runtime.rs.

use super::*;

use crate::coordinators::{self, SessionKey};

use super::coordinator_runtime::{now_ms, RETRY_DELAYS_MS};

/// How long reports to one coordinator keep failing, the computer online, before Ghostex gives up.
///
/// CDXC:Coordinators 2026-11-10 DECISION:
/// Sebastian (Q29): Ghostex gives up on a coordinator's reports after 10 minutes of failed deliveries while the computer is online, and never gives up while it is offline: time offline doesn't count. An outage resets the whole window (`pause_for_outage`), failure time from before it included. Giving up means no timer and no backstop, and one red "Not sent" row in Notifications: "Coordinator stopped getting reports", "Ghostex tried for 10 minutes and stopped. Check what it is waiting on, then send it a message to try again." A new message accepted for that coordinator (`request_report_retry`) or a gxserver restart makes it try again, and the row turns read only when a delivery round to it then ends with no failure. What waits is each thread's current report, as before: there is no outbox.
/// WHY: the retries used to go on every 60 seconds forever, and the user never heard that a coordinator was not getting its threads' reports.
const GIVE_UP_AFTER_MS: i64 = 10 * 60_000;
const REPORTS_NOTICE_TITLE: &str = "Coordinator stopped getting reports";
const REPORTS_NOTICE_BODY: &str =
    "Ghostex tried for 10 minutes and stopped. Check what it is waiting on, then send it a message to try again.";

/// One coordinator's run of failed deliveries.
pub(super) struct FailureStreak {
    failures: usize,
    /// Earliest next try (ms).
    retry_at: i64,
    /// When this run began, or last made progress (a round that sent at least one report).
    failing_since: i64,
    /// Ghostex gave up: no attempt until `request_report_retry` or a restart.
    gave_up: bool,
}

/// The coordinators Ghostex gave up on, shared with the send route so a new message to one asks
/// for its reports again.
fn given_up() -> &'static Mutex<HashSet<SessionKey>> {
    static GIVEN_UP: std::sync::OnceLock<Mutex<HashSet<SessionKey>>> = std::sync::OnceLock::new();
    GIVEN_UP.get_or_init(|| Mutex::new(HashSet::new()))
}

/// A new message was accepted for this session (the `/api/sendSessionChatMessage` route, never
/// gxserver's own sends or a replay): when it is a coordinator Ghostex gave up on, its reports
/// are tried again on the next tick.
pub(crate) fn request_report_retry(key: &SessionKey) {
    if let Ok(mut given_up) = given_up().lock() {
        given_up.remove(key);
    }
}

/// Restarts the clock of every run that has not given up, on each tick the computer is offline,
/// open threads or none.
///
/// CDXC:Coordinators 2026-11-10 WHY:
/// While offline the supervisor sends nothing, so no failure is counted, but a run's `failing_since` stayed where the outage found it: the first failure after more than 10 minutes offline gave up at once and posted the notice. Restarting the clock on every offline tick makes a run count only failures since the network came back.
pub(super) fn pause_for_outage(retry: &mut HashMap<SessionKey, FailureStreak>, now: i64) {
    for streak in retry.values_mut().filter(|streak| !streak.gave_up) {
        streak.failing_since = now;
    }
}

/// Whether the supervisor holds off on this coordinator for now: its last round failed and the
/// next try is not due, or Ghostex gave up and nobody asked it to try again. An ask drops the run.
pub(super) fn waits_for_retry(
    retry: &mut HashMap<SessionKey, FailureStreak>,
    coordinator: &SessionKey,
    now: i64,
) -> bool {
    if let Some(streak) = retry.get(coordinator) {
        if streak.gave_up {
            let still_given_up = given_up()
                .lock()
                .map(|given_up| given_up.contains(coordinator))
                .unwrap_or(true);
            if still_given_up {
                return true;
            }
            // Asked to try again: a fresh run.
            retry.remove(coordinator);
        } else if now < streak.retry_at {
            return true;
        }
    }
    false
}

/// Counts a round that ended in a failure; `Some((failures, failing_ms))` when it makes Ghostex
/// give up on the coordinator.
pub(super) fn record_failed_round(
    retry: &mut HashMap<SessionKey, FailureStreak>,
    coordinator: &SessionKey,
    delivered_any: bool,
) -> Option<(usize, i64)> {
    let mut gave_up = None;
    let now = now_ms();
    let streak = retry.entry(coordinator.clone()).or_insert(FailureStreak {
        failures: 0,
        retry_at: now,
        failing_since: now,
        gave_up: false,
    });
    if delivered_any {
        streak.failing_since = now;
    }
    let delay = RETRY_DELAYS_MS[streak.failures.min(RETRY_DELAYS_MS.len() - 1)];
    streak.failures += 1;
    streak.retry_at = now + delay;
    if now - streak.failing_since >= GIVE_UP_AFTER_MS
        && crate::network_status::offline_since().is_none()
    {
        streak.gave_up = true;
        if let Ok(mut given_up) = given_up().lock() {
            given_up.insert(coordinator.clone());
        }
        gave_up = Some((streak.failures, now - streak.failing_since));
    }
    gave_up
}

/// After a round: the row and warning line when Ghostex gave up, or the row turning read when the
/// round reached the coordinator with no failure (`clean`).
pub(super) async fn announce_round(
    state: &Arc<AppState>,
    coordinator: &SessionKey,
    gave_up: Option<(usize, i64)>,
    clean: bool,
) {
    let notice_state = state.clone();
    let notice_key = coordinator.clone();
    match gave_up {
        Some((failures, failing_ms)) => {
            let _ = tokio::task::spawn_blocking(move || {
                post_reports_notice(&notice_state, &notice_key, failures, failing_ms)
            })
            .await;
        }
        None if clean => {
            let _ = tokio::task::spawn_blocking(move || {
                clear_reports_notice(&notice_state, &notice_key)
            })
            .await;
        }
        None => {}
    }
}

/// Ghostex gave up on a coordinator's reports: one row in Notifications and one warning line.
fn post_reports_notice(
    state: &AppState,
    coordinator: &SessionKey,
    failures: usize,
    failing_ms: i64,
) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "coordinatorReportsGaveUp".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "coordinatorProjectId": coordinator.0,
            "coordinatorSessionId": coordinator.1,
            "failures": failures,
            "failingMs": failing_ms,
        })),
    });
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(session) = repository
        .get_session(&coordinator.0, &coordinator.1)
        .ok()
        .flatten()
    else {
        return;
    };
    let title = coordinators::session_title_of(&session);
    let project = repository
        .get_project(&coordinator.0)
        .ok()
        .flatten()
        .and_then(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let subtitle = [title.trim(), project.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    use crate::notification_feed::store as feed;
    let inserted = feed::insert_coordinator_reports_notice(
        &db,
        feed::NewNotificationFeedRow {
            project_id: coordinator.0.clone(),
            session_id: coordinator.1.clone(),
            kind: feed::NOTIFICATION_FEED_KIND_SEND_FAILED,
            title: REPORTS_NOTICE_TITLE.to_string(),
            subtitle,
            body: feed::bounded_notification_body(REPORTS_NOTICE_BODY),
            agent_name: feed::notification_agent_icon(&session),
            attention_event_id: None,
        },
    );
    if inserted.is_ok() {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
}

/// The coordinator's reports go out again: its "stopped getting reports" row, if any, goes read.
fn clear_reports_notice(state: &AppState, coordinator: &SessionKey) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    if crate::notification_feed::store::mark_coordinator_reports_notice_read(&db, &coordinator.1)
        .is_ok_and(|changed| changed > 0)
    {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
}
