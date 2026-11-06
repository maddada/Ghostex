//! System notifications for banked reset expiry warnings: gxserver writes the red `resetExpiring`
//! row to the feed, and the lead window turns each new one into a system notification while
//! `resetExpirySystemNotifications` is on. The bell row stays either way.
//!
//! SEE-ALSO: server/src/accounts/reset_watch.rs (the warnings), app/titlebar/account_reset.rs
//! (`open_account_notification`, where a click lands).

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::helpers::*;
use crate::*;

use super::GpuiNotificationFeedState;

/// The banner's id prefix: a click on a banner with it opens the account, not a session.
pub(crate) const ACCOUNT_RESET_BANNER_PREFIX: &str = "account-reset:";
pub(crate) const RESET_EXPIRY_SYSTEM_NOTIFICATIONS_SETTINGS_KEY: &str =
    "resetExpirySystemNotifications";
/// On the first feed this process reads, only warnings this recent are new enough to announce;
/// older unread ones were already announced before a restart, or arrived while the app was closed.
const FIRST_READ_RECENT_SECS: i64 = 15 * 60;

/// Feed rows already considered, process-wide so a second window never announces them again.
/// `None` until the first feed arrives.
static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

impl GhostexGpuiApp {
    /// CDXC:Notifications 2026-10-05 DECISION:
    /// User: a banked reset expiring within 3 days or 24 hours is "a notification in the app and as a new notification for the system (can be switched off for the system)". gxserver dedupes the warnings across restarts; this announces each new row once.
    pub(crate) fn deliver_reset_expiry_banners(
        &mut self,
        state: &GpuiNotificationFeedState,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.is_lead_window() {
            return;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64);
        let fresh = {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            let first_read = seen.is_none();
            let seen = seen.get_or_insert_with(HashSet::new);
            let fresh = state
                .items
                .iter()
                // A remote computer's accounts are not this computer's to warn about.
                .filter(|item| item.machine.is_local())
                .filter(|item| seen.insert(item.id.clone()))
                .filter(|item| {
                    !item.read && item.kind == super::GpuiNotificationFeedKind::ResetExpiring
                })
                .filter(|item| {
                    !first_read
                        || item
                            .created_at_epoch_secs
                            .is_some_and(|at| now - at <= FIRST_READ_RECENT_SECS)
                })
                .filter_map(|item| {
                    Some(GpuiSessionAttentionNotificationCandidate {
                        session_id: format!(
                            "{ACCOUNT_RESET_BANNER_PREFIX}{}",
                            item.account_id.as_deref()?
                        ),
                        title: item.title.clone(),
                        body: if item.subtitle.is_empty() {
                            item.body.clone()
                        } else {
                            format!("{}: {}", item.subtitle, item.body)
                        },
                        icon_data_url: None,
                    })
                })
                .collect::<Vec<_>>();
            seen.retain(|id| state.items.iter().any(|item| &item.id == id));
            fresh
        };
        let enabled = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(RESET_EXPIRY_SYSTEM_NOTIFICATIONS_SETTINGS_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if !enabled {
            return;
        }
        for candidate in fresh {
            self.deliver_gpui_macos_session_attention_notification(candidate, cx);
        }
    }
}
