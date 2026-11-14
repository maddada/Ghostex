//! How a coordinator's threads (the user calls them workers) reach the user: Settings >
//! Notifications > Workers (`workerNotifications`: Off, Silent or Sound), and the questions every
//! notification path asks about a worker.
//!
//! CDXC:Notifications 2026-11-14 DECISION:
//! User: "lets pls not show notifications on done for workers". On the worker-notifications page
//! (agent-work/Ghostex/worker-notifs/page.html) they picked, for a thread under a coordinator:
//! - Q3241: a finished turn leaves nothing, no Notifications row and no dot (stored as already
//!   seen, coordinators/quiet_turn_end.rs); its report to the coordinator is unchanged.
//! - Q3242: a message that has waited on the thread's input box goes to the coordinator only.
//! - Q3243: an approval still reaches the user (row, banner, sound, push), and opens the
//!   coordinator, where the card is relayed (`openSessionId` on the row and the push).
//! - Q3244: a question card goes to the coordinator first (it already gets a "waiting for an
//!   answer" report); the user sees the pink dot only.
//! - Q3245: not moving, an error Ghostex could not get past, and a memory-guard stop are the
//!   coordinator's to handle; the user sees the red dot only. Every account out of usage and a
//!   sign-in that stopped working still alert: only the user can fix those.
//! - Q3246: a session an ordinary agent started (not a coordinator) is not a worker; it notifies
//!   like the user's own sessions.
//! - Q3247: one Workers choice in Settings > Notifications, Off by default, brings them back:
//!   Silent puts them in the bell (unread, with their dots) without sound or system notification,
//!   Sound alerts like any session. A phone follows its own Workers choice (its former Thread
//!   reports choice, push/message.rs), for all of these, not only finished turns.
//! SEE-ALSO: packages/gx-core/src/alert_rule.rs (the rule for everything else),
//! server/src/presentation/attention_alerts.rs (the stamps), server/src/notification_feed/producer.rs.

#[cfg(not(test))]
use std::sync::Mutex;
#[cfg(not(test))]
use std::time::{Duration, Instant, SystemTime};

use rusqlite::Connection;

/// The setting's key in the shared settings file.
pub(crate) const WORKER_NOTIFICATIONS_SETTING: &str = "workerNotifications";

/// Settings > Notifications > Workers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkerAlerts {
    /// The default: workers report to their coordinator; the user sees their dots only (and
    /// approvals and account blocks, which always reach the user).
    #[default]
    Off,
    /// Their events are rows in the bell, unread, without sound or system notification.
    Silent,
    /// Their events alert like any session's.
    Sound,
}

impl WorkerAlerts {
    pub(crate) fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("silent") => Self::Silent,
            Some("sound") => Self::Sound,
            _ => Self::Off,
        }
    }

    /// Whether a worker's events get a Notifications row at all.
    pub(crate) fn rows(self) -> bool {
        self != Self::Off
    }
}

/// How often the cached choice looks at the settings file again.
#[cfg(not(test))]
const RECHECK_EVERY: Duration = Duration::from_secs(2);

#[cfg(not(test))]
struct Cached {
    checked_at: Instant,
    modified: Option<SystemTime>,
    choice: WorkerAlerts,
}

/// This computer's Workers choice. Read from the shared settings file, cached while the file is
/// unchanged, so the presentation can ask for every session it stamps.
#[cfg(not(test))]
pub(crate) fn worker_alerts() -> WorkerAlerts {
    static CACHE: Mutex<Option<Cached>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now = Instant::now();
    if let Some(cached) = cache.as_ref() {
        if now.duration_since(cached.checked_at) < RECHECK_EVERY {
            return cached.choice;
        }
    }
    let paths = crate::paths::get_gxserver_paths(None);
    let modified = std::fs::metadata(crate::shared_settings_file::shared_settings_path(&paths))
        .and_then(|metadata| metadata.modified())
        .ok();
    if let Some(cached) = cache.as_mut() {
        if cached.modified == modified {
            cached.checked_at = now;
            return cached.choice;
        }
    }
    let settings = crate::shared_settings_file::read_shared_settings(&paths);
    let choice = WorkerAlerts::from_setting(
        settings
            .as_ref()
            .and_then(|settings| settings.get(WORKER_NOTIFICATIONS_SETTING))
            .and_then(serde_json::Value::as_str),
    );
    *cache = Some(Cached {
        checked_at: now,
        modified,
        choice,
    });
    choice
}

/// Tests never read this Mac's own settings: the default.
#[cfg(test)]
pub(crate) fn worker_alerts() -> WorkerAlerts {
    WorkerAlerts::Off
}

/// The coordinator of a session that is an open thread under one, if it is.
pub(crate) fn worker_coordinator(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Option<(String, String)> {
    crate::coordinators::read_thread(db, project_id, session_id)
        .ok()
        .flatten()
        .filter(|thread| !thread.is_resolved())
        .map(|thread| (thread.coordinator_project_id, thread.coordinator_session_id))
}

/// Whether the session is an open thread under a coordinator.
pub(crate) fn is_worker(db: &Connection, project_id: &str, session_id: &str) -> bool {
    worker_coordinator(db, project_id, session_id).is_some()
}

/// Whether a worker's event that is the coordinator's to handle (Q3242, Q3244, Q3245) gets a
/// Notifications row: only when the Workers choice brings them back.
pub(crate) fn worker_event_gets_row(db: &Connection, project_id: &str, session_id: &str) -> bool {
    !is_worker(db, project_id, session_id) || worker_alerts().rows()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_reads_off_by_default() {
        assert_eq!(WorkerAlerts::from_setting(None), WorkerAlerts::Off);
        assert_eq!(WorkerAlerts::from_setting(Some("off")), WorkerAlerts::Off);
        assert_eq!(WorkerAlerts::from_setting(Some("bogus")), WorkerAlerts::Off);
        assert_eq!(
            WorkerAlerts::from_setting(Some("silent")),
            WorkerAlerts::Silent
        );
        assert_eq!(
            WorkerAlerts::from_setting(Some("sound")),
            WorkerAlerts::Sound
        );
        assert!(!WorkerAlerts::Off.rows());
        assert!(WorkerAlerts::Silent.rows());
    }
}
