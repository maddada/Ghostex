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

/// The account id gxserver gives the row warning that a CLIProxyAPI hub holds a Google login
/// (`GOOGLE_LOGIN_NOTICE_PREFIX` in server/src/accounts/hub/mod.rs).
const HUB_GOOGLE_LOGIN_NOTICE_PREFIX: &str = "cliproxy-google:";

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
        let (fresh_reset, fresh_pool, fresh_hub_google) = {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            let first_read = seen.is_none();
            let seen = seen.get_or_insert_with(HashSet::new);
            let mut fresh_reset = Vec::new();
            let mut fresh_pool = Vec::new();
            let mut fresh_hub_google = Vec::new();
            for item in state.items.iter() {
                // A remote computer's accounts are not this computer's to warn about.
                if !item.machine.is_local() || !seen.insert(item.id.clone()) || item.read {
                    continue;
                }
                // Q5230: a hub's Google login warning opens as a dialog once, when it arrives (or,
                // on the first feed after launch, only when it is recent).
                if item
                    .account_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(HUB_GOOGLE_LOGIN_NOTICE_PREFIX))
                {
                    if !first_read
                        || item
                            .created_at_epoch_secs
                            .is_some_and(|at| now - at <= FIRST_READ_RECENT_SECS)
                    {
                        fresh_hub_google.push((item.title.clone(), item.body.clone()));
                    }
                    continue;
                }
                let is_reset = item.kind == super::GpuiNotificationFeedKind::ResetExpiring;
                let is_pool = item.kind == super::GpuiNotificationFeedKind::Failed
                    && item
                        .account_id
                        .as_deref()
                        // `low:` is the early warning before an account runs out with no other
                        // account to move to (server/src/accounts/usage_warning.rs).
                        .is_some_and(|id| id.starts_with("pool:") || id.starts_with("low:"));
                if !is_reset && !is_pool {
                    continue;
                }
                // On the first feed snapshot after app launch, do not replay pool-limit sound or
                // OS banners for rows that were already in the feed before this process started
                // (their bell row and red sidebar dot are already visible). Banked reset warnings
                // keep the recent-first-read window.
                if first_read
                    && (is_pool
                        || !item
                            .created_at_epoch_secs
                            .is_some_and(|at| now - at <= FIRST_READ_RECENT_SECS))
                {
                    continue;
                }
                let Some(account_id) = item.account_id.as_deref() else {
                    continue;
                };
                let candidate = GpuiSessionAttentionNotificationCandidate {
                    session_id: format!("{ACCOUNT_RESET_BANNER_PREFIX}{account_id}"),
                    title: item.title.clone(),
                    body: if item.subtitle.is_empty() {
                        item.body.clone()
                    } else {
                        format!("{}: {}", item.subtitle, item.body)
                    },
                    icon_data_url: None,
                };
                if is_reset {
                    fresh_reset.push(candidate);
                } else {
                    fresh_pool.push(candidate);
                }
            }
            seen.retain(|id| state.items.iter().any(|item| &item.id == id));
            (fresh_reset, fresh_pool, fresh_hub_google)
        };
        // CDXC:AgentProviders 2026-11-27 DECISION: User (Q5230): "please warn the user with a
        // dialog saying that we don't support agy in cliproxyapi since it's not allowed by
        // google." gxserver writes the row once per hub; a toast with Open Accounts opens as the
        // centred notice modal (app/notice_modal_host.rs), and Accounts is where hubs are managed.
        for (title, body) in fresh_hub_google {
            self.app_toast_id_counter = self.app_toast_id_counter.wrapping_add(1);
            self.upsert_gpui_app_toast(
                GpuiAppToast {
                    id: format!("gpui-app-toast-{}", self.app_toast_id_counter),
                    level: GpuiAppToastLevel::Warning,
                    title,
                    description: Some(body),
                    copy_text: None,
                    action: Some(GpuiAppToastAction::OpenAccounts { provider: None }),
                    loading: false,
                    persistent: true,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    epoch: 0,
                },
                cx,
            );
        }
        // Q4472: one per-pool sound and OS banner when an agent's auto-switchable pool hits 0%
        // (affected sessions carry `blockedAlertedBy: "accountPool"` so they do not fire per chat).
        // Respects both sound settings (inside `gpui_play_not_moving_sound`) and system attention
        // notification settings (`gpui_macos_attention_notifications_enabled`).
        if !fresh_pool.is_empty() {
            crate::app::helpers::gpui_play_not_moving_sound();
            if gpui_macos_attention_notifications_enabled() {
                for candidate in fresh_pool {
                    self.deliver_gpui_macos_session_attention_notification(candidate, cx);
                }
            }
        }
        let enabled = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(RESET_EXPIRY_SYSTEM_NOTIFICATIONS_SETTINGS_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        if !enabled {
            return;
        }
        for candidate in fresh_reset {
            self.deliver_gpui_macos_session_attention_notification(candidate, cx);
        }
    }
}
