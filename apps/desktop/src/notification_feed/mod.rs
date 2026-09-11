/*
CDXC:Notifications 2026-09-11 DECISION:
User: Ghostex gets a notification feed with a bell in the titlebar immediately to
the right of the Next button, a count badge on the bell, a dropdown panel that
lists what each agent said or is waiting for, and hotkeys to open the panel and
jump through unread items.

Ownership mirrors the Back/Forward trail on purpose:
- gxserver owns the feed rows, their read state, and the jump order
  (`server/src/notification_feed`), so the desktop app, the web app, and mobile
  read one list.
- The CEF sidebar runtime talks to gxserver and activates the session a row
  points at, because it already owns session activation and attention
  acknowledgement.
- This module owns pixels and hotkey routing only. It renders from the cached
  state the sidebar pushes over the native-host bridge and sends one command
  back per click or keypress. Nothing here calls gxserver or blocks the frame.
SEE-ALSO: packages/shared/notification-feed/notification-feed-contract.ts,
apps/desktop/sidebar/gxserver-runtime (the `notificationFeedState` bridge post
and the `ghostex-gpui-sidebar-notification-feed-command` listener).
*/

use std::cell::Cell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, Bounds, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, Styled as _, Window, div,
    prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::ElementExt as _;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use crate::{
    GhostexGpuiApp, GpuiTitlebarPopupKind, NOTIFICATIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
    TITLEBAR_LEADING_BUTTON_WIDTH, TITLEBAR_LEADING_TALL_BUTTON_HEIGHT, shared_settings,
    titlebar_background, titlebar_button_hover_color, titlebar_icon_color,
    titlebar_icon_hover_color, titlebar_svg_icon, titlebar_tooltip,
};

/// Page-side event the sidebar runtime listens for. Must stay identical to
/// GPUI_SIDEBAR_NOTIFICATION_FEED_COMMAND_EVENT_NAME in the sidebar runtime.
const NOTIFICATION_FEED_COMMAND_EVENT_NAME: &str = "ghostex-gpui-sidebar-notification-feed-command";
/// Bridge message the sidebar runtime posts whenever the feed changes.
pub(crate) const NOTIFICATION_FEED_STATE_MESSAGE_TYPE: &str = "notificationFeedState";

const NOTIFICATION_BELL_ICON: &str = "titlebar/bell.svg";
const NOTIFICATION_BELL_ICON_SIZE: f32 = 15.0;
/// The app-wide attention blue (`--attention-dot` in the shared theme).
pub(crate) const NOTIFICATION_ATTENTION_BLUE: u32 = 0x95d7f6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiNotificationFeedKind {
    Finished,
    NeedsInput,
    Bell,
    Custom,
}

impl GpuiNotificationFeedKind {
    fn from_wire(kind: &str) -> Option<Self> {
        match kind {
            "finished" => Some(Self::Finished),
            "needsInput" => Some(Self::NeedsInput),
            "bell" => Some(Self::Bell),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Finished => "Finished",
            Self::NeedsInput => "Needs input",
            Self::Bell => "Bell",
            Self::Custom => "Notification",
        }
    }

    /// Card tile icon when the session has no agent logo to show.
    pub(crate) fn icon_path(self) -> &'static str {
        match self {
            Self::Finished => "titlebar/message-circle.svg",
            Self::NeedsInput => "titlebar/help-circle.svg",
            Self::Bell => "titlebar/bell.svg",
            Self::Custom => "titlebar/info-circle.svg",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiNotificationFeedItem {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) kind: GpuiNotificationFeedKind,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) body: String,
    pub(crate) agent_name: Option<String>,
    pub(crate) created_at: String,
    pub(crate) created_at_epoch_secs: Option<i64>,
    pub(crate) read: bool,
}

impl GpuiNotificationFeedItem {
    fn from_bridge_value(value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| value.get(key)?.as_str().map(str::to_string);
        let id = text("id")?;
        if !notification_feed_id_is_safe(&id) {
            return None;
        }
        let created_at = text("createdAt")?;
        Some(Self {
            created_at_epoch_secs: parse_iso_epoch_secs(&created_at),
            created_at,
            project_id: text("projectId")?,
            session_id: text("sessionId")?,
            kind: GpuiNotificationFeedKind::from_wire(value.get("kind")?.as_str()?)?,
            title: text("title").unwrap_or_default(),
            subtitle: text("subtitle").unwrap_or_default(),
            body: text("body").unwrap_or_default(),
            agent_name: text("agentName"),
            read: value.get("read")?.as_bool()?,
            id,
        })
    }
}

/// The cached feed the titlebar and the dropdown render from. Newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiNotificationFeedState {
    pub(crate) unread_count: usize,
    pub(crate) next_unread_id: Option<String>,
    pub(crate) items: Vec<GpuiNotificationFeedItem>,
}

impl GpuiNotificationFeedState {
    /// Strictly parse the sidebar's bridge payload. A malformed message (or a
    /// malformed row) leaves the previous state alone rather than blanking the
    /// bell: this is the only source of truth the titlebar has.
    pub(crate) fn from_bridge_message(message: &serde_json::Value) -> Option<Self> {
        let unread_count = message.get("unreadCount")?.as_u64()? as usize;
        let items = message
            .get("items")?
            .as_array()?
            .iter()
            .map(GpuiNotificationFeedItem::from_bridge_value)
            .collect::<Option<Vec<_>>>()?;
        let next_unread_id = match message.get("nextUnreadId") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_str()?.to_string()),
        };
        Some(Self {
            unread_count,
            next_unread_id,
            items,
        })
    }
}

/// Map the shared hotkey action ids (`packages/shared/ghostex-hotkeys.ts`) onto
/// the command the sidebar runtime executes, so a keypress and a panel click
/// enter the exact same route.
pub(crate) fn notification_feed_hotkey_command(action_id: &str) -> Option<&'static str> {
    match action_id {
        "openNotifications" => Some("open"),
        "jumpToLatestUnreadNotification" => Some("jumpToLatestUnread"),
        "deferNotificationAndJumpNext" => Some("deferAndJumpNext"),
        _ => None,
    }
}

/// Ids ride inside a JS string literal in the command script, so only the
/// characters a uuid can contain are ever interpolated.
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
    /// `{ "type": "notificationFeedState", … }` from the sidebar's native-host
    /// bridge. Repaints the bell only when the feed actually changed, and pushes
    /// the new rows into the dropdown when it is open so the list stays live.
    pub(crate) fn receive_notification_feed_state_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = GpuiNotificationFeedState::from_bridge_message(message) else {
            return;
        };
        if self.notification_feed_state == state {
            return;
        }
        self.notification_feed_state = state.clone();
        if self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Notifications)
            && let Some(handle) = self.titlebar_popup_window.clone()
        {
            let _ = handle.update(cx, |popup, window, cx| {
                popup.update_notifications_feed(state, cx);
                window.refresh();
            });
        }
        cx.notify();
    }

    /// Ask the sidebar runtime to act on the feed. Rust deliberately does not
    /// call gxserver itself: the runtime owns both the daemon conversation and
    /// the session activation that follows an `open`.
    pub(crate) fn request_notification_feed_command(
        &mut self,
        action: &'static str,
        notification_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let detail = match notification_id {
            Some(id) if notification_feed_id_is_safe(id) => {
                format!("{{ action: '{action}', notificationId: '{id}' }}")
            }
            Some(_) => return,
            None => format!("{{ action: '{action}' }}"),
        };
        let script = format!(
            "window.dispatchEvent(new CustomEvent('{NOTIFICATION_FEED_COMMAND_EVENT_NAME}', {{ detail: {detail} }})); undefined;"
        );
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn titlebar_notification_bell_visible(&self) -> bool {
        !shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(NOTIFICATIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    /// Toggles the Notifications dropdown anchored to the last painted bell
    /// bounds, for both the bell itself and the `openNotifications` hotkey.
    pub(crate) fn toggle_gpui_titlebar_notifications_popup(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_notification_bell_visible() {
            return;
        }
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Notifications);
        let trigger_bounds = self.titlebar_notification_bell_bounds.get();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Notifications,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    /// The bell, drawn in the same tall square strip as the Back/Forward arrows
    /// it follows, with the unread count overlaid at the top right.
    pub(crate) fn render_titlebar_notification_bell(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Notifications);
        let unread_count = self.notification_feed_state.unread_count;
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let button_bounds: Rc<Cell<Option<Bounds<Pixels>>>> =
            self.titlebar_notification_bell_bounds.clone();
        let shortcut = crate::gpui_configured_hotkey_label("openNotifications");
        let tooltip = match shortcut {
            Some(shortcut) if !shortcut.is_empty() => format!("Notifications ({shortcut})"),
            _ => "Notifications".to_string(),
        };

        div()
            .id("ghostex-gpui-titlebar-notifications-bell")
            .relative()
            .flex()
            .h(px(TITLEBAR_LEADING_TALL_BUTTON_HEIGHT))
            .w(px(TITLEBAR_LEADING_BUTTON_WIDTH))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .cursor_default()
            .when(open, |this| this.bg(titlebar_button_hover_color()))
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_gpui_titlebar_notifications_popup(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_gpui_titlebar_notifications_popup(window, cx);
                }),
            )
            .when(!open, |this| {
                this.managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx),
                )
            })
            .on_prepaint(move |bounds, _window, _cx| {
                button_bounds.set(Some(bounds));
            })
            .child(titlebar_svg_icon(
                NOTIFICATION_BELL_ICON,
                NOTIFICATION_BELL_ICON_SIZE,
                icon_color,
            ))
            .when(unread_count > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(1.0))
                        .right(px(1.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px(px(3.0))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        .bg(rgb(NOTIFICATION_ATTENTION_BLUE))
                        .text_size(px(9.0))
                        .line_height(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(titlebar_background())
                        .child(notification_feed_badge_label(unread_count)),
                )
            })
            .into_any_element()
    }
}
