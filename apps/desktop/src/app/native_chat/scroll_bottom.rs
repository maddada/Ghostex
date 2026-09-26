use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::hotkeys::{
    GPUI_DEFAULT_GHOSTEX_HOTKEYS, gpui_configured_hotkey_label, gpui_keystroke_from_shared_hotkey,
    gpui_migrated_hotkey_for_action, gpui_platform_hotkey_for_action,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, App, Context, EntityInputHandler as _, Focusable as _, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use serde::Deserialize;
use std::{collections::HashSet, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrollBottom {
    label: String,
    edge_threshold: f32,
    height: f32,
    font_size: f32,
    padding_x: f32,
    bottom: f32,
}

static SPEC: LazyLock<ScrollBottom> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/scroll-bottom.json"
    ))
    .expect("shared scroll button appearance")
});

pub(super) fn label() -> &'static str {
    &SPEC.label
}

pub(super) fn font_size() -> f32 {
    SPEC.font_size
}

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ScrollChatToBottom {
    shortcut: String,
}

#[derive(Default)]
struct RegisteredShortcuts(HashSet<String>);
impl gpui::Global for RegisteredShortcuts {}

fn shortcut() -> Option<String> {
    let action = "scrollChatToBottom";
    let (_, default) = GPUI_DEFAULT_GHOSTEX_HOTKEYS
        .iter()
        .find(|(id, _)| *id == action)?;
    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
    let key = settings
        .object()
        .get("hotkeys")
        .and_then(|hotkeys| hotkeys.get(action))
        .and_then(serde_json::Value::as_str);
    let key = gpui_migrated_hotkey_for_action(action, key.unwrap_or(default), default);
    gpui_keystroke_from_shared_hotkey(gpui_platform_hotkey_for_action(action, key))
}

pub(super) fn register(cx: &mut App) {
    let Some(shortcut) = shortcut() else { return };
    if !cx.has_global::<RegisteredShortcuts>() {
        cx.set_global(RegisteredShortcuts::default());
    }
    if cx
        .global_mut::<RegisteredShortcuts>()
        .0
        .insert(shortcut.clone())
    {
        cx.bind_keys([KeyBinding::new(
            &shortcut,
            ScrollChatToBottom {
                shortcut: shortcut.clone(),
            },
            Some("NativeChat"),
        )]);
    }
}

impl NativeChatView {
    pub(super) fn jump_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.list.set_follow_mode(gpui::FollowMode::Tail);
        self.invoke(serde_json::json!({"type":"composerExpand"}), cx);
        cx.notify();
    }

    pub(super) fn scroll_bottom_action(
        &mut self,
        action: &ScrollChatToBottom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if shortcut().as_deref() != Some(action.shortcut.as_str()) {
            cx.propagate();
            return;
        }
        for input in self
            .input
            .iter()
            .chain(self.answer_input.iter().map(|(_, input)| input))
            .chain(self.async_answer_input.iter().map(|(_, input)| input))
        {
            if input.read(cx).focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                cx.propagate();
                return;
            }
        }
        if self.composer_held_key.as_deref() != Some("scroll-to-bottom") {
            self.composer_held_key = Some("scroll-to-bottom".into());
            self.jump_to_bottom(cx);
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    pub(super) fn scroll_bottom_button(&self, cx: &Context<Self>) -> AnyElement {
        let remaining =
            self.list.max_offset_for_scrollbar().y + self.list.scroll_px_offset_for_scrollbar().y;
        let glass = crate::app::helpers::window_glass_active_for(self.main_window);
        let p = ChatAppearance::current(&self.snapshot).on_window_glass(glass);
        let shown = !self.list.is_following_tail() && remaining > px(SPEC.edge_threshold * p.scale);
        if glass {
            return self.scroll_bottom_window_placeholder(shown, &p, cx);
        }
        if !shown {
            return div().into_any_element();
        }
        let label = gpui_configured_hotkey_label("scrollChatToBottom")
            .filter(|label| !label.is_empty())
            .map_or_else(
                || SPEC.label.clone(),
                |key| format!("{} ({key})", SPEC.label),
            );
        // The pill wears the composer's own chrome: the same opaque card fill, the 1px outline and
        // the inset top highlight session-chat-composer-focus.css gives both of them.
        let outline = if p.light {
            gpui::black().opacity(0.08)
        } else {
            gpui::white().opacity(0.05)
        };
        let highlight = gpui::white().opacity(if p.light { 0.0 } else { 0.03 });
        div()
            .absolute()
            .bottom(px(SPEC.bottom * p.scale))
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("chat-scroll-bottom")
                    .tab_index(0)
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .chat_cursor_pointer()
                    .relative()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(SPEC.height * p.scale))
                    .px(px(SPEC.padding_x * p.scale))
                    .rounded_full()
                    .border_1()
                    .border_color(outline)
                    .bg(p.composer_background)
                    .text_color(p.primary)
                    .text_size(px(SPEC.font_size * p.scale))
                    .font_weight(FontWeight::MEDIUM)
                    .focus_visible(|style| style.border_color(p.ring))
                    .child(
                        // Visual only, and inside the pill's own frame: the `inset 0 1px` highlight.
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(1.0))
                            .bg(highlight),
                    )
                    .child(label)
                    .on_click(cx.listener(|chat, _, _, cx| chat.jump_to_bottom(cx))),
            )
            .into_any_element()
    }
}

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-23 DECISION:
    /// User: the scroll-to-bottom pill "should match the color and everything of the composer box", then, when the composer's see-through wash let the transcript read through it, "can't we make it's bg more frosted glass??". Under window glass the pill wears the composer's wash and border over a real blur of what is behind it. Supersedes the same day's opaque menu tone and the plain see-through wash.
    ///
    /// CDXC:SessionChat 2026-09-23 WHY:
    /// GPUI cannot blur behind an element inside a window, only behind a whole window, so the pill is drawn in a small blurred child window of its own (frosted_overlay_window.rs). This placeholder lays out an invisible pill of the same size where the in-window one would be and hands its bounds to that window. The pill stays down while something else sits over the pane (a chat popup window, the account-switch card, the subagent viewer), or while it would stick out of the pane, because a window of its own would float above all of them.
    fn scroll_bottom_window_placeholder(
        &self,
        shown: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let overlay = super::frosted_overlay_window::FrostedOverlay::ScrollBottom;
        let shown = shown && !self.frosted_overlay_covered(overlay);
        let report = self.frosted_overlay_reporter(overlay, shown, cx);
        if !shown {
            return div().absolute().size_0().child(report).into_any_element();
        }
        let label = gpui_configured_hotkey_label("scrollChatToBottom")
            .filter(|label| !label.is_empty())
            .map_or_else(
                || SPEC.label.clone(),
                |key| format!("{} ({key})", SPEC.label),
            );
        div()
            .absolute()
            .bottom(px(SPEC.bottom * p.scale))
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .h(px(SPEC.height * p.scale))
                    .px(px(SPEC.padding_x * p.scale))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .text_color(gpui::transparent_black())
                    .text_size(px(SPEC.font_size * p.scale))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .child(label)
                    .child(report),
            )
            .into_any_element()
    }
}
