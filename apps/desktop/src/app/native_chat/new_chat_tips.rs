//! The tips card under a new chat's "What should we build with …?": one tip at a time (icon,
//! title, at most two lines), Previous and Next, a quiet count, Got it, and Hide tips. The tips and
//! the order they walk in are the core's (packages/gx-chat-core/src/extras/new_chat_tips.rs, where
//! the user's decision is); this file draws the card and keeps which tip each chat shows.
//!
//! CDXC:Onboarding 2026-11-11 WHY:
//! Read tips are the old Tips & Tricks panel's `gpuiTitlebarTipsReadIds`, so a tip the panel
//! marked read stays read here, and they are written straight to the settings file by the host
//! (`setNewChatTipsRead`): a broad Settings save keeps the latest copy of that key rather than its
//! own (remote_conn/settings_and_install_probe.rs). The card has a fixed height and stays out of a
//! short pane, so it never moves the chat box; Hide tips asks inside the card rather than in a
//! floating menu, so nothing dark ever sits over the glass.
//! SEE-ALSO: apps/desktop/src/app/session_chat/host_actions.rs and apps/gpui-web/src/app/chat_host.rs
//! (`setNewChatTipsRead`, `hideNewChatTips`), the Settings > Chat rows `newChatTips` and
//! `newChatTipsReset` (settings_modal/tabs/general/chat_tools.rs).

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use ghostex_gx_chat_core::extras::new_chat_tips::{
    NewChatTip, fill_new_chat_tip_keys, is_new_chat_tip_id, new_chat_tip_applies,
    new_chat_tip_order,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, Context, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// The settings key the read tips live in (the old Tips & Tricks panel's, kept so its reads count).
pub(crate) const NEW_CHAT_TIPS_READ_SETTINGS_KEY: &str = "gpuiTitlebarTipsReadIds";
/// The Settings > Chat switch; off hides the card everywhere.
pub(crate) const NEW_CHAT_TIPS_SETTINGS_KEY: &str = "newChatTips";

/// The card's height, unscaled. Fixed, so a long tip or the Hide tips question never moves the
/// title above it or the chat box below.
const CARD_HEIGHT: f32 = 124.0;
const CARD_MAX_WIDTH: f32 = 480.0;
/// Below this pane height (unscaled) the welcome keeps its title and the card steps aside.
const MIN_PANE_HEIGHT: f32 = 500.0;
const ICON_TILE: f32 = 32.0;
const TIP_FADE: Duration = Duration::from_millis(220);

/// Which tip each chat shows, and how many new chats opened the card since the app started.
#[derive(Default)]
struct TipRotation {
    by_chat: HashMap<String, String>,
    opened: usize,
    /// Where a rotation that has no `first` tips left starts, picked once per run so a restart
    /// does not open on the same tip again.
    offset: Option<usize>,
    /// The chat whose card is asking "Hide tips on new chats?".
    asking_hide: Option<String>,
}

thread_local! {
    static ROTATION: RefCell<TipRotation> = RefCell::default();
}

fn platform_name() -> &'static str {
    if cfg!(target_family = "wasm") {
        "web"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn today() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| (elapsed.as_secs() / 86_400) as i64)
        .unwrap_or_default()
}

/// The read tip ids in the settings object.
pub(crate) fn read_tip_ids(settings: &Map<String, Value>) -> HashSet<String> {
    settings
        .get(NEW_CHAT_TIPS_READ_SETTINGS_KEY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// A tip's text with this person's keys in place of its `{key:…}`s; `None` when one is unbound.
fn with_keys(text: &str) -> Option<String> {
    fill_new_chat_tip_keys(text, crate::app::hotkeys::gpui_configured_hotkey_label)
}

/// The unread tips that apply now, in today's order.
fn current_order(settings: &Map<String, Value>) -> Vec<&'static NewChatTip> {
    let read = read_tip_ids(settings);
    new_chat_tip_order(
        |tip| {
            new_chat_tip_applies(tip, platform_name(), |id| {
                ghostex_settings_catalog::built_in_extensions::enabled(settings, id)
            }) && with_keys(&tip.title).is_some()
                && with_keys(&tip.body).is_some()
        },
        |id| read.contains(id),
        today(),
    )
}

fn tips_turned_off(settings: &Map<String, Value>) -> bool {
    settings
        .get(NEW_CHAT_TIPS_SETTINGS_KEY)
        .and_then(Value::as_bool)
        == Some(false)
}

/// The tip `chat` shows: the one it already showed while it is still unread, otherwise the next
/// one of the rotation (the `first` tips in order while any is unread, then today's shuffle).
fn tip_index_for_chat(chat: &str, order: &[&'static NewChatTip]) -> usize {
    ROTATION.with(|rotation| {
        let mut rotation = rotation.borrow_mut();
        if let Some(at) = rotation
            .by_chat
            .get(chat)
            .and_then(|id| order.iter().position(|tip| &tip.id == id))
        {
            return at;
        }
        let base = if order[0].first.is_some() {
            0
        } else {
            *rotation.offset.get_or_insert_with(|| {
                web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.subsec_nanos() as usize)
                    .unwrap_or_default()
            })
        };
        let at = (base + rotation.opened) % order.len();
        rotation.opened += 1;
        rotation
            .by_chat
            .insert(chat.to_string(), order[at].id.clone());
        at
    })
}

fn show_tip(chat: &str, tip: &NewChatTip) {
    ROTATION.with(|rotation| {
        rotation
            .borrow_mut()
            .by_chat
            .insert(chat.to_string(), tip.id.clone());
    });
}

fn asking_hide(chat: &str) -> bool {
    ROTATION.with(|rotation| rotation.borrow().asking_hide.as_deref() == Some(chat))
}

fn set_asking_hide(chat: Option<&str>) {
    ROTATION.with(|rotation| rotation.borrow_mut().asking_hide = chat.map(str::to_string));
}

/// The card's soft wash and hairline: the foreground at a low alpha, so it reads as a pane of the
/// same glass on any backdrop, never a dark box and never a coloured edge.
fn wash(p: &ChatAppearance, dark: f32, light: f32) -> Hsla {
    p.foreground.opacity(if p.light { light } else { dark })
}

impl NativeChatView {
    /// The card, or `None` when tips are off, all read, the pane is too short, or this is a
    /// coordinator's welcome (which has its own intro).
    pub(super) fn render_new_chat_tips(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let height = f32::from(self.bounds.get().size.height);
        if height > 0.0 && height < MIN_PANE_HEIGHT * s {
            return None;
        }
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        let settings = snapshot.object();
        if tips_turned_off(settings) {
            return None;
        }
        let order = current_order(settings);
        if order.is_empty() {
            return None;
        }
        let chat = self.config.session_id.clone();
        let at = tip_index_for_chat(&chat, &order);
        let tip = order[at];
        let total = order.len();
        let asking = asking_hide(&chat);

        let icon_tile = div()
            .flex_shrink_0()
            .size(px(ICON_TILE * s))
            .rounded(px(10.0 * s))
            .bg(wash(p, 0.075, 0.06))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path(if asking {
                        "titlebar/eye-off.svg".to_string()
                    } else {
                        format!("titlebar/{}.svg", tip.icon)
                    })
                    .size(px(17.0 * s))
                    .text_color(p.foreground.opacity(0.82)),
            );
        let (title, body) = if asking {
            (
                "Hide tips on new chats?".to_string(),
                "You can bring them back in Settings > Chat > Tips on new chats.".to_string(),
            )
        } else {
            (
                with_keys(&tip.title).unwrap_or_default(),
                with_keys(&tip.body).unwrap_or_default(),
            )
        };
        let text_column = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(3.0 * s))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(13.5 * s))
                    .line_height(px(18.0 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .line_clamp(1)
                    .text_ellipsis()
                    .child(title),
            )
            .child(
                div()
                    .min_w_0()
                    .text_size(px(12.5 * s))
                    .line_height(px(17.5 * s))
                    .text_color(p.muted)
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(body),
            );
        let content = div()
            .flex()
            .items_start()
            .gap(px(12.0 * s))
            .min_w_0()
            .child(icon_tile)
            .child(text_column);
        let content = if cx.reduce_motion() {
            content.into_any_element()
        } else {
            content
                .with_animation(
                    gpui::ElementId::Name(
                        format!(
                            "new-chat-tip-{}-{}",
                            tip.id,
                            if asking { "ask" } else { "tip" }
                        )
                        .into(),
                    ),
                    Animation::new(TIP_FADE).with_easing(gpui::ease_out_quint()),
                    |content, delta| content.opacity(0.35 + 0.65 * delta),
                )
                .into_any_element()
        };

        let footer = if asking {
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(6.0 * s))
                .child(
                    tip_text_button("new-chat-tips-hide-cancel", "Cancel", None, false, p)
                        .on_click(cx.listener(|_, _, _, cx| {
                            set_asking_hide(None);
                            cx.notify();
                        })),
                )
                .child(
                    tip_text_button("new-chat-tips-hide", "Hide tips", None, true, p).on_click(
                        cx.listener(|this, _, _, cx| {
                            set_asking_hide(None);
                            this.host("hideNewChatTips", json!({}), cx);
                            cx.notify();
                        }),
                    ),
                )
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pl(px(2.0 * s))
                        .text_size(px(11.5 * s))
                        .text_color(p.muted.opacity(0.8))
                        .child(format!("{} of {}", at + 1, total)),
                )
                .when(total > 1, |row| {
                    row.child(
                        tip_icon_button(
                            "new-chat-tips-previous",
                            "titlebar/chevron-left.svg",
                            "Previous tip",
                            p,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.step_new_chat_tip(-1, cx))),
                    )
                    .child(
                        tip_icon_button(
                            "new-chat-tips-next",
                            "titlebar/chevron-right.svg",
                            "Next tip",
                            p,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.step_new_chat_tip(1, cx))),
                    )
                })
                .child(
                    tip_icon_button("new-chat-tips-menu", "titlebar/dots.svg", "Hide tips", p)
                        .on_click(cx.listener(|this, _, _, cx| {
                            set_asking_hide(Some(&this.config.session_id));
                            cx.notify();
                        })),
                )
                .child(div().w(px(4.0 * s)))
                .child(
                    tip_text_button(
                        "new-chat-tips-got-it",
                        "Got it",
                        Some("titlebar/check.svg"),
                        true,
                        p,
                    )
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("Don't show this tip again")
                            .build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.mark_new_chat_tip_read(cx))),
                )
        };

        Some(
            div()
                .id("new-chat-tips")
                .role(gpui::Role::Group)
                .aria_label("Tips")
                .w_full()
                .max_w(px(CARD_MAX_WIDTH * s))
                .h(px(CARD_HEIGHT * s))
                .flex_shrink_0()
                .mt(px(10.0 * s))
                .px(px(14.0 * s))
                .pt(px(14.0 * s))
                .pb(px(10.0 * s))
                .rounded(px(16.0 * s))
                .bg(wash(p, 0.045, 0.035))
                .border_1()
                .border_color(wash(p, 0.085, 0.075))
                .flex()
                .flex_col()
                .justify_between()
                .child(content)
                .child(footer)
                .into_any_element(),
        )
    }

    /// Previous (-1) or Next (1), wrapping around today's order.
    fn step_new_chat_tip(&mut self, step: isize, cx: &mut Context<Self>) {
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        let order = current_order(snapshot.object());
        if order.is_empty() {
            return;
        }
        let at = tip_index_for_chat(&self.config.session_id, &order) as isize;
        let next = (at + step).rem_euclid(order.len() as isize) as usize;
        show_tip(&self.config.session_id, order[next]);
        cx.notify();
    }

    /// Got it: the tip is read on every chat from now on, and the card moves to the one after it.
    fn mark_new_chat_tip_read(&mut self, cx: &mut Context<Self>) {
        let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
        let settings = snapshot.object();
        let order = current_order(settings);
        if order.is_empty() {
            return;
        }
        let at = tip_index_for_chat(&self.config.session_id, &order);
        if let Some(next) = order.get(at + 1).or_else(|| order.first()) {
            show_tip(&self.config.session_id, next);
        }
        let mut read = read_tip_ids(settings);
        read.retain(|id| is_new_chat_tip_id(id));
        read.insert(order[at].id.clone());
        let mut read: Vec<String> = read.into_iter().collect();
        read.sort();
        self.host("setNewChatTipsRead", json!({ "readIds": read }), cx);
        cx.notify();
    }
}

fn tip_icon_button(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .size(px(26.0 * s))
        .rounded(px(8.0 * s))
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.foreground.opacity(0.08)))
        .active(|style| style.bg(p.foreground.opacity(0.12)))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(
            svg()
                .path(icon)
                .size(px(15.0 * s))
                .text_color(p.foreground.opacity(0.62)),
        )
}

fn tip_text_button(
    id: &'static str,
    label: &'static str,
    icon: Option<&'static str>,
    emphasis: bool,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    let rest = if emphasis {
        wash(p, 0.085, 0.07)
    } else {
        p.foreground.opacity(0.0)
    };
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex()
        .items_center()
        .gap(px(5.0 * s))
        .flex_shrink_0()
        .h(px(26.0 * s))
        .px(px(10.0 * s))
        .rounded(px(8.0 * s))
        .bg(rest)
        .chat_cursor_pointer()
        .hover(|style| style.bg(p.foreground.opacity(if emphasis { 0.13 } else { 0.08 })))
        .active(|style| style.bg(p.foreground.opacity(0.16)))
        .text_size(px(12.0 * s))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.foreground.opacity(if emphasis { 0.92 } else { 0.72 }))
        .when_some(icon, |button, icon| {
            button.child(
                svg()
                    .path(icon)
                    .size(px(13.0 * s))
                    .text_color(p.foreground.opacity(0.8)),
            )
        })
        .child(label)
}
