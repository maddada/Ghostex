/*
CDXC:Notifications 2026-09-20 DECISION:
User: Ghostex gets a notification feed with a bell in the sidebar's top row, a
count badge on the bell, a dropdown panel that lists what each agent said or is
waiting for, and hotkeys to open the panel and jump through unread items. This
supersedes the 2026-09-11 wording that put the bell in the titlebar immediately
to the right of the Next button.

Ownership:
- gxserver owns the feed rows, their read state, and the jump order
  (`server/src/notification_feed`), so the desktop app, the web app, and mobile
  read one list.
- The Rust store reads the feed and performs each command
  (`app/gx_store/notifications/`, the rules in gx-core `notification_feed.rs`);
  until 2026-09-25 the QuickJS sidebar runtime did.
- This module owns pixels and hotkey routing only. It renders from the cached
  state and sends one command per click or keypress. Nothing here calls
  gxserver or blocks the frame.
SEE-ALSO: packages/shared/notification-feed/notification-feed-contract.ts,
apps/desktop/src/app/gx_store/notifications/.
*/

pub(crate) mod reset_banners;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, Bounds, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use crate::app::native_sidebar::navigation::{
    SIDEBAR_TOP_ROW_BUTTON_HEIGHT, SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH, SIDEBAR_TOP_ROW_BUTTON_RADIUS,
    SIDEBAR_TOP_ROW_BUTTON_WIDTH,
};
use crate::{
    GhostexGpuiApp, GpuiTitlebarPopupKind, NOTIFICATIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
    shared_settings, titlebar_active_text_color, titlebar_background, titlebar_svg_icon,
    titlebar_tooltip_label,
};

const NOTIFICATION_BELL_ICON: &str = "titlebar/bell.svg";
const NOTIFICATION_BELL_ICON_SIZE: f32 = 15.0;
/// The app-wide attention blue (`--attention-dot` in the shared theme).
pub(crate) const NOTIFICATION_ATTENTION_BLUE: u32 = 0x95d7f6;
/// CDXC:Notifications 2026-09-16 DECISION:
/// User: make the "Project · Finished" meta line more visible in light mode.
/// The fill/border accent stays #95d7f6 (unless the theme sets its own accent, CDXC:Theming 2026-11-17 below); this darker same-hue ink is only for unread meta text on pale unread cards.
pub(crate) const NOTIFICATION_ATTENTION_BLUE_TEXT_LIGHT: u32 = 0x0d7eab;
/// CDXC:Notifications 2026-10-05 DECISION:
/// User: banked reset expiry warnings are notified "using red color". Urgent rows swap the attention blue for this red on the card, and the bell's badge turns red while one is unread.
pub(crate) const NOTIFICATION_URGENT_RED: u32 = 0xf87171;
/// The same red, darkened for unread meta text on pale light-mode cards.
pub(crate) const NOTIFICATION_URGENT_RED_TEXT_LIGHT: u32 = 0xc42b2b;

/// The attention blue of unread cards and the bell's badge, in one appearance.
///
/// CDXC:Theming 2026-11-17 DECISION: User, through the coordinator: "The notification dropdown stays blue after a theme is applied ... make it follow the applied theme (accent, surfaces, text, borders) like the rest of the app", and on 2026-11-17 (Q183) chose that a tinted preset with no Accent role moves the notifications off blue too, "using the preset's own accent". The attention blue (CDXC:Notifications 2026-09-11) takes the theme's own accent whenever the theme sets one: the Accent theme role, else the accent of a tint with a hue (`theme_own_accent_for_variant`). Only a neutral grey tint with no Accent role keeps the blue. The Danger role replaces the urgent red, laid over the work area as the accent is (`theme_role_over_work_area`); the sidebar's approval red is the sidebar's status colour and stays.
pub(crate) fn notification_attention_color(light: bool) -> u32 {
    theme_own_accent(light).unwrap_or(NOTIFICATION_ATTENTION_BLUE)
}

fn theme_own_accent(light: bool) -> Option<u32> {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    crate::app::helpers::theme_own_accent_for_variant(snapshot.object(), light)
}

/// The urgent red of unread cards and the bell's badge, in one appearance (the Danger theme role
/// when it is set, `notification_attention_color`).
pub(crate) fn notification_urgent_color(light: bool) -> u32 {
    themed_notification_color("danger", light).unwrap_or(NOTIFICATION_URGENT_RED)
}

fn themed_notification_color(role: &str, light: bool) -> Option<u32> {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    crate::app::helpers::theme_role_over_work_area(snapshot.object(), role, light)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiNotificationFeedKind {
    Finished,
    NeedsInput,
    Bell,
    Custom,
    /// A chat message could not be sent (drawn red, and the bell's badge turns red with it).
    SendFailed,
    /// An agent's run ended on an error (drawn red, like an urgent row).
    Failed,
    /// A saved account's banked usage reset expires soon (an account row, drawn red).
    ResetExpiring,
    /// Ghostex used an expiring banked reset on its own (an account row).
    ResetRedeemed,
    /// A sensitive action recorded in the security log (server/src/audit/notify.rs): opens
    /// Settings > Security.
    Security,
}

impl GpuiNotificationFeedKind {
    fn from_wire(kind: &str) -> Option<Self> {
        match kind {
            "finished" => Some(Self::Finished),
            "needsInput" => Some(Self::NeedsInput),
            "bell" => Some(Self::Bell),
            "custom" => Some(Self::Custom),
            "sendFailed" => Some(Self::SendFailed),
            "failed" => Some(Self::Failed),
            "resetExpiring" => Some(Self::ResetExpiring),
            "resetRedeemed" => Some(Self::ResetRedeemed),
            "security" => Some(Self::Security),
            _ => None,
        }
    }

    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::NeedsInput => "needsInput",
            Self::Bell => "bell",
            Self::Custom => "custom",
            Self::SendFailed => "sendFailed",
            Self::Failed => "failed",
            Self::ResetExpiring => "resetExpiring",
            Self::ResetRedeemed => "resetRedeemed",
            Self::Security => "security",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Finished => "Finished",
            Self::NeedsInput => "Needs input",
            Self::Bell => "Bell",
            Self::Custom => "Notification",
            Self::SendFailed => "Not sent",
            Self::Failed => "Failed",
            Self::ResetExpiring => "Reset expiring",
            Self::ResetRedeemed => "Reset used",
            Self::Security => "Security",
        }
    }

    /// Red rows: an expiring reset, a message not sent and a failed run (failed falls back to the
    /// alert-circle glyph when the session has no agent logo). An unread one turns the bell's badge
    /// red.
    pub(crate) fn urgent(self) -> bool {
        matches!(self, Self::ResetExpiring | Self::SendFailed | Self::Failed)
    }

    /// The card's accent in one appearance: red for urgent rows, the attention blue otherwise
    /// (each following its theme role, `notification_attention_color`).
    pub(crate) fn accent(self, light: bool) -> u32 {
        if self.urgent() {
            notification_urgent_color(light)
        } else {
            notification_attention_color(light)
        }
    }

    /// The accent as unread meta text on a pale light-mode card: the Danger role or the theme's own
    /// light accent when the theme sets one (already dark enough to read as text), else a darker
    /// ink of the red or blue.
    pub(crate) fn accent_text_light(self) -> u32 {
        if self.urgent() {
            themed_notification_color("danger", true).unwrap_or(NOTIFICATION_URGENT_RED_TEXT_LIGHT)
        } else {
            theme_own_accent(true).unwrap_or(NOTIFICATION_ATTENTION_BLUE_TEXT_LIGHT)
        }
    }

    /// Card tile icon when the session has no agent logo to show.
    pub(crate) fn icon_path(self) -> &'static str {
        match self {
            Self::Finished => "titlebar/message-circle.svg",
            Self::NeedsInput => "titlebar/help-circle.svg",
            Self::Bell => "titlebar/bell.svg",
            Self::Failed => "titlebar/alert-circle.svg",
            Self::Security => "titlebar/shield-check.svg",
            Self::Custom | Self::SendFailed | Self::ResetExpiring | Self::ResetRedeemed => {
                "titlebar/info-circle.svg"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiNotificationFeedItem {
    /// The computer whose gxserver owns the row; every command on it goes there.
    pub(crate) machine: ghostex_gx_core::MachineId,
    /// The icon the sidebar draws for the row's computer, after the row's title; `None` with only
    /// this computer. The app fills it as it hands the feed to the panel
    /// (`notification_feed_for_panel`), since the marks live in its store.
    pub(crate) computer_icon: Option<&'static str>,
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) kind: GpuiNotificationFeedKind,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) body: String,
    pub(crate) agent_name: Option<String>,
    /// Set on account rows: the saved account the row is about.
    pub(crate) account_id: Option<String>,
    /// Set on security rows: the security log entry the row announces.
    pub(crate) security_entry: Option<u64>,
    /// Set on the row for a phone gxserver added for Easy Connect without a click, while that phone
    /// is still paired: its device id, for the row's Remove (server notification_feed/security_actions.rs).
    pub(crate) remove_device: Option<String>,
    pub(crate) created_at: String,
    pub(crate) created_at_epoch_secs: Option<i64>,
    pub(crate) read: bool,
    /// Whether the bell panel has already shown this row (badge cleared), even while still unread.
    pub(crate) seen: bool,
    pub(crate) awaiting_approval: bool,
    /// The session a click opens when it is not the row's own: a coordinator's thread's approval
    /// opens the coordinator, where the card is relayed (`openProjectId`, `openSessionId`;
    /// CDXC:Notifications 2026-11-14 in server/src/worker_alerts.rs).
    pub(crate) open_on: Option<(String, String)>,
}

impl GpuiNotificationFeedItem {
    /// The project and session a click on the row opens.
    pub(crate) fn open_target(&self) -> (&str, &str) {
        match &self.open_on {
            Some((project_id, session_id)) => (project_id, session_id),
            None => (&self.project_id, &self.session_id),
        }
    }

    fn from_bridge_value(value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| value.get(key)?.as_str().map(str::to_string);
        let id = text("id")?;
        if !notification_feed_id_is_safe(&id) {
            return None;
        }
        let created_at = text("createdAt")?;
        let machine = ghostex_gx_core::notification_feed_item_machine(value);
        Some(Self {
            computer_icon: None,
            machine,
            created_at_epoch_secs: parse_iso_epoch_secs(&created_at),
            created_at,
            project_id: text("projectId")?,
            session_id: text("sessionId")?,
            kind: GpuiNotificationFeedKind::from_wire(value.get("kind")?.as_str()?)?,
            title: text("title").unwrap_or_default(),
            subtitle: text("subtitle").unwrap_or_default(),
            body: text("body").unwrap_or_default(),
            agent_name: text("agentName"),
            account_id: text("accountId"),
            security_entry: value
                .get("securityEntry")
                .and_then(serde_json::Value::as_u64),
            remove_device: value
                .get("removeDevice")
                .and_then(|device| device.get("id"))
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string),
            read: value.get("read")?.as_bool()?,
            seen: value
                .get("seen")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_else(|| value.get("read").and_then(serde_json::Value::as_bool).unwrap_or(false)),
            awaiting_approval: value
                .get("awaitingApproval")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            open_on: text("openProjectId").zip(text("openSessionId")).filter(
                |(project_id, session_id)| !project_id.is_empty() && !session_id.is_empty(),
            ),
            id,
        })
    }

    /// Unique across computers: a row id is unique only on the daemon that wrote it.
    pub(crate) fn key(&self) -> String {
        match self.machine.remote_id() {
            None => self.id.clone(),
            Some(machine_id) => format!("{machine_id}/{}", self.id),
        }
    }
}

/// The cached feed the titlebar and the dropdown render from: every online computer's rows,
/// newest first (gx-core `merged_notification_feed_message`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiNotificationFeedState {
    /// Every online computer's unread rows.
    pub(crate) unread_count: usize,
    pub(crate) local_unread_count: usize,
    pub(crate) remote_unread_count: usize,
    /// Unread rows not yet seen in the bell panel: drives the bell's badge count.
    pub(crate) unseen_count: usize,
    pub(crate) local_unseen_count: usize,
    pub(crate) remote_unseen_count: usize,
    pub(crate) next_unread_id: Option<String>,
    pub(crate) next_unread_machine: ghostex_gx_core::MachineId,
    pub(crate) items: Vec<GpuiNotificationFeedItem>,
    /// "<Computer> is offline. …" while a remote computer's rows are hidden.
    pub(crate) offline_notice: Option<String>,
}

impl Default for GpuiNotificationFeedState {
    fn default() -> Self {
        Self {
            unread_count: 0,
            local_unread_count: 0,
            remote_unread_count: 0,
            unseen_count: 0,
            local_unseen_count: 0,
            remote_unseen_count: 0,
            next_unread_id: None,
            next_unread_machine: ghostex_gx_core::MachineId::Local,
            items: Vec::new(),
            offline_notice: None,
        }
    }
}

impl GpuiNotificationFeedState {
    /// Recomputes unread/unseen totals and clears a stale jump target from `self.items`.
    pub(crate) fn recount_from_items(&mut self) {
        let count_unread = |local: bool| {
            self.items
                .iter()
                .filter(|item| item.machine.is_local() == local && !item.read)
                .count()
        };
        let count_unseen = |local: bool| {
            self.items
                .iter()
                .filter(|item| item.machine.is_local() == local && !item.read && !item.seen)
                .count()
        };
        self.local_unread_count = count_unread(true);
        self.remote_unread_count = count_unread(false);
        self.unread_count = self.local_unread_count + self.remote_unread_count;
        self.local_unseen_count = count_unseen(true);
        self.remote_unseen_count = count_unseen(false);
        self.unseen_count = self.local_unseen_count + self.remote_unseen_count;
        let next_still_unread = self.items.iter().any(|item| {
            !item.read
                && item.machine == self.next_unread_machine
                && self.next_unread_id.as_deref() == Some(item.id.as_str())
        });
        if !next_still_unread {
            self.next_unread_id = None;
        }
    }

    /// Marks every row seen optimistically so the bell badge clears on the frame the panel opens
    /// while every unread row keeps its highlight.
    pub(crate) fn mark_all_seen_optimistically(&mut self) {
        for item in &mut self.items {
            item.seen = true;
        }
        self.unseen_count = 0;
        self.local_unseen_count = 0;
        self.remote_unseen_count = 0;
    }

    /// Takes one row's unread count off the totals, as an optimistic edit does.
    pub(crate) fn note_read(&mut self, machine: &ghostex_gx_core::MachineId) {
        self.unread_count = self.unread_count.saturating_sub(1);
        if machine.is_local() {
            self.local_unread_count = self.local_unread_count.saturating_sub(1);
        } else {
            self.remote_unread_count = self.remote_unread_count.saturating_sub(1);
        }
        self.recount_from_items();
    }

    /// Puts one row's unread count back on the totals.
    pub(crate) fn note_unread(&mut self, machine: &ghostex_gx_core::MachineId) {
        self.unread_count += 1;
        if machine.is_local() {
            self.local_unread_count += 1;
        } else {
            self.remote_unread_count += 1;
        }
        self.recount_from_items();
    }
}

impl GpuiNotificationFeedState {
    /// Strictly parse the state message (gx-core `notification_feed_state_message`).
    /// A malformed message (or a malformed row) leaves the previous state alone
    /// rather than blanking the bell: this is the only source of truth the
    /// titlebar has.
    pub(crate) fn from_bridge_message(message: &serde_json::Value) -> Option<Self> {
        let unread_count = message.get("unreadCount")?.as_u64()? as usize;
        let count = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize
        };
        let items = message
            .get("items")?
            .as_array()?
            .iter()
            .map(GpuiNotificationFeedItem::from_bridge_value)
            .collect::<Option<Vec<_>>>()?;
        let unseen_count = message
            .get("unseenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| items.iter().filter(|item| !item.read && !item.seen).count());
        let local_unseen_count = message
            .get("localUnseenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| {
                items
                    .iter()
                    .filter(|item| item.machine.is_local() && !item.read && !item.seen)
                    .count()
            });
        let remote_unseen_count = message
            .get("remoteUnseenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| {
                items
                    .iter()
                    .filter(|item| !item.machine.is_local() && !item.read && !item.seen)
                    .count()
            });
        let next_unread_id = match message.get("nextUnreadId") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_str()?.to_string()),
        };
        Some(Self {
            unread_count,
            local_unread_count: count("localUnreadCount"),
            remote_unread_count: count("remoteUnreadCount"),
            unseen_count,
            local_unseen_count,
            remote_unseen_count,
            next_unread_id,
            next_unread_machine: match message
                .get("nextUnreadMachineId")
                .and_then(serde_json::Value::as_str)
            {
                Some(machine_id) => ghostex_gx_core::MachineId::Remote(machine_id.to_string()),
                None => ghostex_gx_core::MachineId::Local,
            },
            items,
            offline_notice: message
                .get("offlineNotice")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }
}

/// Map the shared hotkey action ids (`packages/shared/ghostex-hotkeys.ts` (deleted 2026-10-01)) onto
/// the feed command, so a keypress and a panel click enter the exact same route.
pub(crate) fn notification_feed_hotkey_command(action_id: &str) -> Option<&'static str> {
    match action_id {
        "openNotifications" => Some("open"),
        "jumpToLatestUnreadNotification" => Some("jumpToLatestUnread"),
        "deferNotificationAndJumpNext" => Some("deferAndJumpNext"),
        _ => None,
    }
}

/// Only the characters a uuid can contain: a row whose id has anything else is
/// refused with its whole message.
pub(crate) fn notification_feed_id_is_safe(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Parse `YYYY-MM-DDTHH:MM:SS[.fff][Z|±hh:mm]` into Unix seconds without a
/// date crate; the daemon writes ISO timestamps and nothing else.
pub(crate) fn parse_iso_epoch_secs(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let mut rest = &text[19..];
    if rest.starts_with('.') {
        let digits = rest[1..].bytes().take_while(u8::is_ascii_digit).count();
        rest = &rest[1 + digits..];
    }
    let offset_secs = match rest {
        "" | "Z" | "z" => 0,
        offset if offset.len() == 6 && (offset.starts_with('+') || offset.starts_with('-')) => {
            let sign = if offset.starts_with('-') { -1 } else { 1 };
            let hours = offset[1..3].parse::<i64>().ok()?;
            let minutes = offset[4..6].parse::<i64>().ok()?;
            sign * (hours * 3600 + minutes * 60)
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset_secs)
}

/// Howard Hinnant's civil-to-days algorithm.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// "just now", "3m", "2h", "yesterday", then a short date.
pub(crate) fn notification_feed_relative_time(created_at_epoch_secs: Option<i64>) -> String {
    let Some(created) = created_at_epoch_secs else {
        return String::new();
    };
    let elapsed = (now_epoch_secs() - created).max(0);
    if elapsed < 60 {
        return "just now".to_string();
    }
    if elapsed < 3600 {
        return format!("{}m", elapsed / 60);
    }
    if elapsed < 86_400 {
        return format!("{}h", elapsed / 3600);
    }
    if elapsed < 2 * 86_400 {
        return "yesterday".to_string();
    }
    if elapsed < 7 * 86_400 {
        return format!("{}d", elapsed / 86_400);
    }
    civil_from_days(created.div_euclid(86_400))
}

/// Inverse of `days_from_civil`, rendered as "Mon D".
fn civil_from_days(days: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    format!("{} {day}", MONTHS[(month - 1) as usize])
}

pub(crate) fn notification_feed_badge_label(unread_count: usize) -> String {
    if unread_count > 99 {
        "99+".to_string()
    } else {
        unread_count.to_string()
    }
}

impl GhostexGpuiApp {
    /// `{ "type": "notificationFeedState", … }` from the Rust store's feed reads.
    /// Repaints the bell only when the feed actually changed, and pushes the new
    /// rows into the dropdown when it is open so the list stays live.
    pub(crate) fn receive_notification_feed_state_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mut state) = GpuiNotificationFeedState::from_bridge_message(message) else {
            return;
        };
        let notifications_open =
            self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Notifications);
        let mut unseen_machines: Vec<ghostex_gx_core::MachineId> = Vec::new();
        let mut seen_rows: Vec<(ghostex_gx_core::MachineId, String, &'static str)> = Vec::new();
        if notifications_open {
            let mut marked_seen_locally = false;
            for item in &mut state.items {
                let key = item.key();
                if !item.read && !item.seen && !self.notifications_panel_kept_unread.contains(&key)
                {
                    // Keep newly arrived rows optimistically seen while the panel is open so
                    // closing the panel before `markAllSeen` returns never flashes the badge.
                    item.seen = true;
                    marked_seen_locally = true;
                    seen_rows.push((item.machine.clone(), item.id.clone(), item.kind.wire_name()));
                    if self.notifications_panel_seen_attempted.insert(key)
                        && !unseen_machines.contains(&item.machine)
                    {
                        unseen_machines.push(item.machine.clone());
                    }
                }
            }
            if marked_seen_locally {
                state.recount_from_items();
            }
        }
        for (machine, id, kind) in seen_rows {
            self.gx_store_mark_notification_item_seen(&machine, id, kind);
        }
        if self.notification_feed_state == state {
            if notifications_open && !unseen_machines.is_empty() {
                self.gx_store_mark_machines_notifications_seen(unseen_machines, cx);
            }
            return;
        }
        self.notification_feed_state = state.clone();
        self.deliver_reset_expiry_banners(&state, cx);
        if notifications_open && let Some(handle) = self.titlebar_popup_window {
            let state = self.notification_feed_for_panel(state);
            let _ = handle.update(cx, |popup, window, cx| {
                popup.update_notifications_feed(state, cx);
                window.refresh();
            });
            if !unseen_machines.is_empty() {
                self.gx_store_mark_machines_notifications_seen(unseen_machines, cx);
            }
        }
        cx.notify();
    }

    /// The feed as the panel draws it: each row carries its computer's icon, the one its sidebar
    /// row ends with (`notification_computer_icon`).
    pub(crate) fn notification_feed_for_panel(
        &self,
        mut feed: GpuiNotificationFeedState,
    ) -> GpuiNotificationFeedState {
        for item in &mut feed.items {
            item.computer_icon = self.notification_computer_icon(&item.machine);
        }
        feed
    }

    /// Acts on the feed: the Rust store performs the command against the gxserver that owns the
    /// row (`app/gx_store/notifications/commands.rs`). `row` names a row by its computer and id;
    /// `tab` is the panel tab Mark all read and Clear all act on.
    pub(crate) fn request_notification_feed_command(
        &mut self,
        action: &'static str,
        row: Option<(&ghostex_gx_core::MachineId, &str)>,
        tab: ghostex_gx_core::NotificationFeedTab,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(command) = ghostex_gx_core::NotificationFeedCommand::from_action(action) else {
            return;
        };
        if row.is_some_and(|(_, id)| !notification_feed_id_is_safe(id)) {
            return;
        }
        self.gx_store_notification_feed_command(command, row, tab, cx);
    }

    pub(crate) fn titlebar_notification_bell_visible(&self) -> bool {
        // CDXC:Extensions 2026-09-18 SEE-ALSO:
        // The bell is scoped like every other titlebar button; apps/desktop/src/app/view_scopes.rs owns the rule.
        !shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(NOTIFICATIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            && self.official_view_scope_allows("notifications")
    }

    /// Toggles the Notifications dropdown anchored to the last bell painted in the main window,
    /// for the `openNotifications` hotkey and the rail's Notifications.
    pub(crate) fn toggle_gpui_titlebar_notifications_popup(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.toggle_gpui_titlebar_notifications_popup_at(None, window, cx);
    }

    /// Toggles the Notifications dropdown hanging from `anchor` (a clicked bell's own bounds, the
    /// sidebar menu's button), or from the last bell painted in the main window without one.
    ///
    /// CDXC:Notifications 2026-11-21 WHY:
    /// User: "Not every single time, but every other time when I click on the notifications, it appears way down below where it should appear... if I see it appear very low and then I click on notifications again, it appears in the correct spot". The bell's anchor was one cell that every bell (the docked sidebar's, the hidden sidebar's header copy, the floating sidebar's in its own window), the rail's ⋯ (on every frame, for the hotkey) and the sidebar menu's Notifications row all wrote. The docked sidebar is a cached view, so on the frames where only the window's root redraws its bell does not prepaint while the rail's ⋯ does, and the cell held ⋯'s spot (beside the rail, far down) when the click came; the redraw that opening the dropdown causes painted both, so the next click was right. Now a click hangs the dropdown from the clicked bell's own bounds, only a bell in the main window updates the shared cell (the hotkey's and a reset warning's anchor), and nothing else writes it.
    pub(crate) fn toggle_gpui_titlebar_notifications_popup_at(
        &mut self,
        anchor: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_notification_bell_visible() {
            return;
        }
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Notifications);
        let trigger_bounds = anchor.or_else(|| self.titlebar_notification_bell_bounds.get());
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Notifications,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    /// CDXC:Notifications 2026-09-20 DECISION:
    /// User: the notification bell belongs in the sidebar's top row beside the sidebar's own controls, not in the titlebar after the Next button; this supersedes the 2026-09-11 placement. It keeps its unread badge, its Settings switch, and its per-view scope gate.
    /// CDXC:Notifications 2026-11-02 DECISION:
    /// User: "show red dot here instead of blue with number of unread notifications there pls", then, after seeing it red with no permission waiting: "the color of the circle above notifications is red, even tho there's no permisison notifiation now... pls fix". The count badge takes the sidebar's approval red while an unread row is a permission or approval request still waiting for an answer; otherwise it keeps its earlier colors (red for an unsent message or an expiring reset, blue for the rest).
    /// It is drawn as a sidebar row button so it matches the menu button beside it. `in_sidebar`
    /// is false for the work area header's copy, which stands in for it while the sidebar is
    /// hidden with the rail on (render/workarea_header/breadcrumb.rs): that one is not a stop of
    /// the sidebar's focus order.
    pub(crate) fn render_sidebar_notification_bell(
        &self,
        appearance: &crate::app::native_sidebar::appearance::SidebarAppearance,
        in_sidebar: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Notifications);
        let badge_count = if open {
            0
        } else {
            self.notification_feed_state.unseen_count
        };
        let unseen = || {
            self.notification_feed_state
                .items
                .iter()
                .filter(|item| !item.read && !item.seen)
        };
        let badge_color = if unseen().any(|item| item.awaiting_approval) {
            crate::app::native_sidebar::status::APPROVAL_COLOR
        } else if unseen().any(|item| item.kind.urgent()) {
            notification_urgent_color(appearance.light)
        } else {
            notification_attention_color(appearance.light)
        };
        let icon_color = if open {
            titlebar_active_text_color()
        } else {
            appearance.chrome_icon
        };
        // This bell's own bounds, which its clicks hang the dropdown from; a cached view replays
        // this render's listeners with them, so they stay this bell's (CDXC:Notifications
        // 2026-11-21 on `toggle_gpui_titlebar_notifications_popup_at`).
        let button_bounds: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
        let last_bell_bounds = self.titlebar_notification_bell_bounds.clone();
        let main_window = self.main_window_handle.map(|handle| handle.window_id());
        let tooltip = titlebar_tooltip_label("Notifications", "openNotifications");
        let hover = appearance.hover;

        div()
            .id("ghostex-gpui-sidebar-notifications-bell")
            // Windows: the sidebar's top row is a Drag region (native_sidebar/navigation.rs).
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .relative()
            .flex()
            .h(px(SIDEBAR_TOP_ROW_BUTTON_HEIGHT * scale))
            .w(px(SIDEBAR_TOP_ROW_BUTTON_WIDTH * scale))
            .rounded(px(SIDEBAR_TOP_ROW_BUTTON_RADIUS * scale))
            .flex_shrink(1.0)
            .min_w(px(SIDEBAR_TOP_ROW_BUTTON_MIN_WIDTH * scale))
            .items_center()
            .justify_center()
            .cursor_default()
            .when(open, |this| this.bg(hover))
            .hover(move |this| this.bg(hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let button_bounds = button_bounds.clone();
                    move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_gpui_titlebar_notifications_popup_at(
                            button_bounds.get(),
                            window,
                            cx,
                        );
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let button_bounds = button_bounds.clone();
                    move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_gpui_titlebar_notifications_popup_at(
                            button_bounds.get(),
                            window,
                            cx,
                        );
                    }
                }),
            )
            // The popup opens on the press; Enter and Space reach it as a click.
            .on_click(cx.listener({
                let button_bounds = button_bounds.clone();
                move |this, event: &gpui::ClickEvent, window, cx| {
                    if event.is_keyboard() {
                        cx.stop_propagation();
                        this.toggle_gpui_titlebar_notifications_popup_at(
                            button_bounds.get(),
                            window,
                            cx,
                        );
                    }
                }
            }))
            // A stop of the sidebar's focus order (it sits in the sidebar's top row).
            .map(|bell| {
                if !in_sidebar {
                    return bell;
                }
                self.sidebar_focus_stop(
                    bell,
                    "navigation:notifications",
                    crate::app::native_sidebar::focus_order::SidebarStopPlace::Fixed,
                    appearance,
                    cx,
                )
            })
            .when(!open, |this| {
                this.managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    // The sidebar's frosted bubble, like every sidebar tooltip.
                    move |window, cx| {
                        crate::app::native_sidebar::link_chips::sidebar_style_tooltip(
                            tooltip.to_string(),
                            window,
                            cx,
                        )
                    },
                )
            })
            .on_prepaint(move |bounds, window, _cx| {
                button_bounds.set(Some(bounds));
                // The floating sidebar's bell is in its own window, whose coordinates are not the
                // main window's, where the hotkey opens the dropdown.
                if main_window.is_none_or(|id| window.window_handle().window_id() == id) {
                    last_bell_bounds.set(Some(bounds));
                }
            })
            .child(titlebar_svg_icon(
                NOTIFICATION_BELL_ICON,
                NOTIFICATION_BELL_ICON_SIZE * scale,
                icon_color,
            ))
            .when(badge_count > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(1.0 * scale))
                        .right(px(1.0 * scale))
                        .flex()
                        .items_center()
                        .justify_center()
                        .min_w(px(14.0 * scale))
                        .h(px(14.0 * scale))
                        .px(px(3.0 * scale))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        .bg(rgb(badge_color))
                        .text_size(px(9.0 * scale))
                        .line_height(px(12.0 * scale))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(titlebar_background())
                        .child(notification_feed_badge_label(badge_count)),
                )
            })
            .into_any_element()
    }
}
