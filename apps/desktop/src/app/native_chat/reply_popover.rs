//! The transcript menu's Reply popover: a small frosted card where the menu was, with a field for
//! the reply and its two actions, Add to prompt (Enter) and Send reply (Cmd+Enter, Ctrl+Enter off
//! a Mac). The row and the draft it leaves are the core's (CDXC:SessionChat 2026-10-29 DECISION on
//! `reply_item` in `packages/gx-chat-core/src/composer/transcript_menu.rs`).
//!
//! CDXC:SessionChat 2026-10-29 WHY:
//! It is a child window like the chat's menus because GPUI blurs only behind a whole window
//! (frosted_overlay_window.rs), and floating cards take the frosted menu fill.

use super::{appearance::ChatAppearance, option_menu, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, WindowBounds,
    WindowOptions, div, px, size,
};
use gpui_component::Root;
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use std::time::Duration;

/// The card's size before the chat's zoom.
const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 132.0;
const RADIUS: f32 = 12.0;
/// How long the press on the Reply row still counts as "a press just now" once the card is open:
/// a little over the 0.3s of `GhostexGpuiPointerPressedRecently`.
pub(super) const OPENING_PRESS: Duration = Duration::from_millis(350);

/// The open popover's window, one per chat.
#[derive(Default)]
pub(in crate::app::native_chat) struct ReplyPopoverState {
    handle: Option<gpui::WindowHandle<Root>>,
}

/// What the popover's actions do with the chat box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplyAction {
    AddToPrompt,
    Send,
}

struct ReplyPopover {
    chat: WeakEntity<NativeChatView>,
    /// The selection quoted as Add to Chat quotes it (the Reply row's `quote`).
    quote: String,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    appearance: ChatAppearance,
    closed: bool,
    was_active: bool,
    /// True until the press that opened the card has aged out (`OPENING_PRESS`).
    settling: bool,
    /// How many times this card has taken key status back.
    rekeys: u32,
    _subscriptions: Vec<Subscription>,
}

impl NativeChatView {
    /// The Reply row's command: opens the popover at the point the menu opened from (`at`, in the
    /// chat window's content coordinates, added by `show_transcript_menu`).
    pub(super) fn open_reply_popover(
        &mut self,
        command: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(quote) = command["quote"].as_str().filter(|quote| !quote.is_empty()) else {
            return;
        };
        self.close_reply_popover(cx);
        let coordinate = |index: usize| command["at"][index].as_f64().unwrap_or(0.0) as f32;
        let at = Point::new(px(coordinate(0)), px(coordinate(1)));
        let appearance = ChatAppearance::current(&self.snapshot);
        let scale = appearance.scale;
        let content = super::child_window::content_bounds(window);
        let view = self.bounds.get();
        let within = if view.size.width > px(0.0) && view.size.height > px(0.0) {
            Bounds::new(content.origin + view.origin, view.size)
        } else {
            content
        };
        let bounds = popover_bounds(content.origin + at, within, scale);
        let parent = super::child_window::window_native_view(window)
            .unwrap_or(self.config.parent_native_view);
        let display_id =
            crate::app::window::popup_frame::PopupOwner::of(window, cx).display_for(bounds, cx);
        let glass = crate::app::helpers::window_glass_active();
        let corner_radius =
            crate::app::window::popup_frame::popup_card_radius(px(RADIUS * scale), glass);
        let chat = cx.weak_entity();
        let quote = quote.to_owned();
        cx.defer(move |cx| {
            let result = cx.open_window(
                WindowOptions {
                    kind: crate::app::window::popup_frame::child_window_kind(),
                    window_decorations: crate::app::window::popup_frame::child_window_decorations(),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id,
                    titlebar: None,
                    focus: true,
                    show: true,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    app_id: crate::gpui_platform_window_app_id(),
                    icon: crate::gpui_platform_window_icon(),
                    window_background: if glass {
                        gpui::WindowBackgroundAppearance::Blurred
                    } else {
                        gpui::WindowBackgroundAppearance::Transparent
                    },
                    ..Default::default()
                },
                {
                    let chat = chat.clone();
                    move |window, cx| {
                        crate::app::window::popup_frame::round_popup_window(
                            window,
                            corner_radius,
                            glass,
                        );
                        crate::app::helpers::apply_frosted_menu_blur(window);
                        crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                        crate::app::window::attach_gpui_app_modal_window_to_main_window(
                            window, parent,
                        );
                        let popover =
                            cx.new(|cx| ReplyPopover::new(chat, quote, appearance, window, cx));
                        cx.new(|cx| {
                            Root::new(popover, window, cx)
                                .bordered(false)
                                .bg(gpui::transparent_black())
                        })
                    }
                },
            );
            let _ = chat.update(cx, |chat, cx| match result {
                Ok(handle) => chat.reply_popover.handle = Some(handle),
                Err(error) => {
                    chat.error = Some(error.to_string());
                    cx.notify();
                }
            });
        });
    }

    /// The popover's window, which counts as one of the chat's own child windows.
    pub(super) fn reply_popover_window(&self) -> Option<gpui::WindowId> {
        self.reply_popover.handle.map(|handle| handle.window_id())
    }

    /// Takes the popover down without touching the chat box (the pane went away, a second Reply).
    pub(super) fn close_reply_popover(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.reply_popover.handle.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    /// The popover's action: the reply under the quote at the end of the chat box, then a send
    /// for Send reply, exactly as the Send button sends (queued while the agent works).
    fn apply_reply(
        &mut self,
        quote: &str,
        reply: &str,
        action: ReplyAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = ghostex_gx_chat_core::composer::transcript_menu::transcript_reply_draft(
            &self.draft,
            quote,
            reply,
        );
        self.insert_prompt(&draft, cx);
        if action == ReplyAction::Send {
            // The Send button's own call: the core queues it while the agent is working.
            self.send(false, window, cx);
        }
    }
}

/// Where the card opens: its top-left corner at the press, kept inside the chat with a margin, and
/// above the press when there is no room below it.
fn popover_bounds(at: Point<Pixels>, within: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
    let margin = px(12.0 * scale);
    let width = px(WIDTH * scale).min(within.size.width - margin * 2.0);
    let height = px(HEIGHT * scale);
    let y = if at.y + px(4.0 * scale) + height < within.bottom() - margin {
        at.y + px(4.0 * scale)
    } else {
        at.y - height - px(4.0 * scale)
    };
    let x =
        at.x.min(within.right() - width - margin)
            .max(within.left() + margin);
    let y = y
        .min(within.bottom() - height - margin)
        .max(within.top() + margin);
    Bounds::new(Point::new(x, y), size(width, height))
}

/// Whether losing key status closes the card. A press only counts once the one on the Reply row
/// has aged out, and a window that keeps taking key status back wins after `MAX_REKEYS`.
pub(super) fn key_loss_closes(settling: bool, user_dismissed: bool, rekeys: u32) -> bool {
    rekeys >= option_menu::MAX_REKEYS || (user_dismissed && !settling)
}

impl ReplyPopover {
    fn new(
        chat: WeakEntity<NativeChatView>,
        quote: String,
        appearance: ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 3)
                .auto_text_direction(true)
                .submit_on_enter(true)
                .placeholder("Reply…")
        });
        // Enter is the field's own key (GPUI runs key bindings before key-down listeners): with
        // `submit_on_enter` a plain Enter reports `PressEnter` instead of a new line, Cmd+Enter
        // (Ctrl+Enter off a Mac) reports it as `secondary`, and Shift+Enter stays a new line.
        let enter = cx.subscribe(&input, |popover: &mut Self, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter {
                secondary,
                shift: false,
            } = event
            {
                popover.close(
                    Some(if *secondary {
                        ReplyAction::Send
                    } else {
                        ReplyAction::AddToPrompt
                    }),
                    true,
                    cx,
                );
            }
        });
        let focus = cx.focus_handle();
        input.read(cx).focus_handle(cx).focus(window, cx);
        let activation = cx.observe_window_activation(window, |popover: &mut Self, window, cx| {
            if window.is_window_active() {
                popover.was_active = true;
            } else if popover.was_active {
                crate::app::window::popup_dismissal::dismiss_on_focus_loss(
                    popover,
                    window,
                    cx,
                    |popover, window, cx| popover.key_lost(window, cx),
                );
            }
        });
        // A card that opened while the app was still being activated never became key, and so was
        // never told it lost it (the same check as `ChatOptionMenu::settle_key`).
        cx.spawn_in(window, async move |popover, cx| {
            cx.background_executor().timer(OPENING_PRESS).await;
            let _ = popover.update_in(cx, |popover, window, _| {
                popover.settling = false;
                if !window.is_window_active() && !option_menu::key_loss_is_dismissal() {
                    popover.retake_key(window);
                }
            });
        })
        .detach();
        let press = crate::app::window::popup_dismissal::observe_main_window_press(
            window,
            cx,
            |popover: &mut Self, _, cx| popover.close(None, false, cx),
        );
        Self {
            chat,
            quote,
            input,
            focus,
            appearance,
            closed: false,
            was_active: window.is_window_active(),
            settling: true,
            rekeys: 0,
            _subscriptions: vec![activation, press, enter],
        }
    }

    /// Key status went elsewhere. The user sent it there (a press since the card opened, or another
    /// app) and the card closes, or something else took it and the card takes it back, as the
    /// chat's menus do (CDXC:SessionChat 2026-09-22 on `ChatOptionMenu::check_active`).
    fn key_lost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI reports activation from a queued task, so the report can be out of date already.
        if self.closed || window.is_window_active() {
            return;
        }
        if key_loss_closes(
            self.settling,
            option_menu::key_loss_is_dismissal(),
            self.rekeys,
        ) {
            self.close(None, false, cx);
        } else {
            self.retake_key(window);
        }
    }

    fn retake_key(&mut self, window: &mut Window) {
        self.rekeys += 1;
        window.activate_window();
    }

    /// Closes the card, then hands the reply to the chat for `action` (none for Escape or a click
    /// away). `to_chat` gives the chat's window the keyboard back, which a click away already did.
    fn close(&mut self, action: Option<ReplyAction>, to_chat: bool, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        let reply = self.input.read(cx).value().to_string();
        let quote = self.quote.clone();
        let chat = self.chat.clone();
        cx.defer(move |cx| {
            let Ok((handle, main)) = chat.update(cx, |chat, _| {
                (chat.reply_popover.handle.take(), chat.main_window)
            }) else {
                return;
            };
            if let Some(handle) = handle {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
            let Some(main) = main else {
                return;
            };
            let _ = main.update(cx, |_, window, cx| {
                if to_chat {
                    window.activate_window();
                }
                let _ = chat.update(cx, |chat, cx| {
                    if let Some(action) = action {
                        chat.apply_reply(&quote, &reply, action, window, cx);
                    }
                    cx.notify();
                });
            });
        });
    }

    /// Escape closes the card without touching the chat box (the field lets it through).
    fn key(&mut self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            window.prevent_default();
            self.close(None, true, cx);
        }
    }

    fn action_button(
        &self,
        id: &'static str,
        label: &'static str,
        action: ReplyAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = &self.appearance;
        let s = p.scale;
        let (hover, _) = menu_ink_washes(p.light);
        let shortcut = shortcut_label(action);
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(format!("{label} ({shortcut})"))
            .chat_cursor_pointer()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(26.0 * s))
            .px(px(8.0 * s))
            .rounded(px(6.0 * s))
            .hover(move |row| row.bg(hover))
            .child(div().text_color(p.foreground).child(label))
            .child(div().text_color(p.muted).child(shortcut))
            .on_click(cx.listener(move |popover, _, _, cx| popover.close(Some(action), true, cx)))
            .into_any_element()
    }
}

impl Render for ReplyPopover {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.appearance.clone();
        let s = p.scale;
        let (_, edge) = menu_ink_washes(p.light);
        div()
            .id("chat-reply-popover")
            .role(gpui::Role::Dialog)
            .aria_label("Reply")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(RADIUS * s))
            .border_1()
            .border_color(edge)
            .bg(p.menu_surface())
            .p(px(8.0 * s))
            .font_family(p.font)
            .text_size(px(13.0 * s))
            .text_color(p.foreground)
            .on_key_down(cx.listener(Self::key))
            .child(
                div().px(px(4.0 * s)).pt(px(2.0 * s)).child(
                    Textarea::new(&self.input)
                        .appearance(false)
                        .bordered(false)
                        .caret_color(p.foreground)
                        .text_size(px(13.0 * s)),
                ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(2.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.action_button(
                        "chat-reply-add",
                        "Add to prompt",
                        ReplyAction::AddToPrompt,
                        cx,
                    ))
                    .child(self.action_button(
                        "chat-reply-send",
                        "Send reply",
                        ReplyAction::Send,
                        cx,
                    )),
            )
    }
}

/// The action's key as the reference popover shows it: ⏎ and ⌘⏎ on a Mac keyboard, spelled out
/// elsewhere (Ctrl+Enter sends there).
fn shortcut_label(action: ReplyAction) -> &'static str {
    match (action, super::keyboard::chat_mac_keys()) {
        (ReplyAction::AddToPrompt, true) => "⏎",
        (ReplyAction::Send, true) => "⌘⏎",
        (ReplyAction::AddToPrompt, false) => "Enter",
        (ReplyAction::Send, false) => "Ctrl+Enter",
    }
}

/// The chat menus' hover wash and hairline (`option_menu/render.rs`), so the card reads as one of
/// them.
pub(super) fn menu_ink_washes(light: bool) -> (gpui::Hsla, gpui::Hsla) {
    let foreground: gpui::Hsla = gpui::rgb(if light { 0x292929 } else { 0xfcfcfc }).into();
    let border: gpui::Hsla = gpui::rgba(if light { 0x0000001f } else { 0xffffff1f }).into();
    (foreground.opacity(if light { 0.06 } else { 0.08 }), border)
}

#[cfg(test)]
mod tests {
    use super::{key_loss_closes, option_menu::MAX_REKEYS};

    /// The bug: the press on the Reply row is still "recent" when the main window takes key status
    /// back, and that closed the card before anything could be typed.
    #[test]
    fn the_press_that_opened_the_card_does_not_close_it() {
        assert!(!key_loss_closes(true, true, 0));
    }

    #[test]
    fn a_key_loss_nobody_caused_does_not_close_it() {
        assert!(!key_loss_closes(false, false, 0));
        assert!(!key_loss_closes(true, false, 0));
    }

    #[test]
    fn a_press_after_it_opened_closes_it() {
        assert!(key_loss_closes(false, true, 0));
    }

    #[test]
    fn a_window_that_keeps_taking_key_status_wins_eventually() {
        assert!(!key_loss_closes(false, false, MAX_REKEYS - 1));
        assert!(key_loss_closes(false, false, MAX_REKEYS));
        assert!(key_loss_closes(true, true, MAX_REKEYS));
    }
}
