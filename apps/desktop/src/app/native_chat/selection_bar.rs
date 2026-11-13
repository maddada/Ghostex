//! The bar that floats by a transcript selection: Copy, Add to Chat and Reply, the rows the
//! transcript's right-click menu shows for a selection (`transcript_menu.rs`), drawn the way an
//! inline command's hover bar is (`run_command.rs`).
//!
//! CDXC:SessionChat 2026-11-13 DECISION:
//! User: "Add the ability to reply or add to chat any part of the text by just selecting it in the chat transcript. We need to show buttons just like the ones we show when we hover over a command." A drag, double or triple click that leaves text selected in the main transcript (replies, the user's own messages, tool output, code) shows the bar by the point the press was released. Its buttons are the core's selection rows (`transcript_menu_rows` in packages/gx-chat-core/src/composer/transcript_menu.rs), so they run exactly what the menu's rows run and are left out when the menu disables them. It goes when the selection is cleared (any press outside it), the transcript scrolls, or on Escape; it never takes Cmd+C or the drag itself.
//!
//! CDXC:SessionChat 2026-11-13 WHY:
//! It is drawn in the chat's own window, like the command bar, and not as a frosted child window: a press on it then clears the window's selection as any press does (the selection layer clears on capture), so the bar keeps the text it was shown for and acts on that, and stays up until the release, unless the press began outside it.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    Anchor, AnyElement, Context, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Point, ScrollEvent,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, canvas, deferred, div, point,
    px, svg,
};
use serde_json::{Value, json};

/// The open bar, one per chat.
#[derive(Default)]
pub(in crate::app::native_chat) struct SelectionBarState {
    /// What the bar acts on and where it sits, while it is up.
    open: Option<OpenBar>,
    /// The left press now held began in the main transcript, so its release may leave a selection.
    press_in_transcript: bool,
    /// The left press now held began on the bar: the selection it cleared keeps the bar up until
    /// the release, which may be the bar's own click.
    press_on_bar: bool,
}

struct OpenBar {
    /// The core's selection rows (Copy, Add to Chat, Reply) for the selected text.
    rows: Vec<Value>,
    /// The release point, in window coordinates.
    at: Point<Pixels>,
}

impl NativeChatView {
    /// The window-wide listeners that open and close the bar. A selection drag usually ends outside
    /// the transcript's text views, and the selection layer takes the release in the capture phase
    /// for the same reason, so these listen on the window rather than on the transcript's element.
    pub(super) fn selection_bar_listeners(&self, cx: &mut Context<Self>) -> AnyElement {
        let chat = cx.weak_entity();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let down = chat.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                    if !phase.capture() || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = down.update(cx, |chat, _| {
                        chat.selection_bar.press_in_transcript =
                            chat.in_main_transcript(event.position);
                    });
                });
                let up = chat.clone();
                // Registered after the selection layer's own release listener (it is the root's
                // first child), so the selection this release leaves is final when this runs.
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if !phase.capture() || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = up.update(cx, |chat, cx| {
                        chat.selection_bar_release(event.position, window, cx)
                    });
                });
                let scroll = chat.clone();
                window.on_mouse_event(move |event: &ScrollEvent, phase, _, cx| {
                    if !phase.capture() {
                        return;
                    }
                    let _ = scroll.update(cx, |chat, cx| {
                        if chat.selection_bar.open.is_some()
                            && chat.in_main_transcript(event.position)
                        {
                            chat.close_selection_bar(cx);
                        }
                    });
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    /// A left release: shows the bar for the selection a press in the transcript left, and
    /// drops a bar whose press began on it and ended elsewhere.
    fn selection_bar_release(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from_transcript = std::mem::take(&mut self.selection_bar.press_in_transcript);
        if std::mem::take(&mut self.selection_bar.press_on_bar) {
            // The bar's click, if the release is on it, runs after this and closes it; a render
            // without a selection closes it otherwise.
            cx.notify();
            return;
        }
        if !from_transcript || self.snapshot["subagent"].is_object() {
            return;
        }
        let selection = gpui_component::TextSelection::selected_text(window, cx)
            .trim()
            .to_owned();
        if selection.is_empty() {
            return;
        }
        // The composer hides behind a question card, which is when Add to Chat and Reply are off.
        let question_active = self.snapshot["questionCard"]["visible"] == true
            && self.snapshot["prompt"]["kind"] == "question";
        let rows = ghostex_gx_chat_core::composer::transcript_menu::transcript_menu_rows(
            None,
            &selection,
            question_active,
        );
        let rows: Vec<Value> = serde_json::to_value(rows)
            .ok()
            .and_then(|rows| rows.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row["disabled"] != true)
            .collect();
        if rows.is_empty() {
            return;
        }
        self.selection_bar.open = Some(OpenBar { rows, at });
        cx.notify();
    }

    pub(super) fn close_selection_bar(&mut self, cx: &mut Context<Self>) {
        if self.selection_bar.open.take().is_some() {
            cx.notify();
        }
    }

    /// Escape closes the bar and goes on to whatever else it does in the chat.
    pub(super) fn selection_bar_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.close_selection_bar(cx);
        }
    }

    /// The bar, below the release point (above it when the transcript has no room there), or
    /// nothing. A selection cleared some other way (Cmd+A in the chat box, a pane change) closes it
    /// at the chat's next draw.
    pub(super) fn render_selection_bar(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.selection_bar.open.as_ref()?;
        if !self.selection_bar.press_on_bar
            && !gpui_component::TextSelection::has_selection(window, cx)
        {
            self.selection_bar.open = None;
            return None;
        }
        let s = p.scale;
        let look = super::run_command::hover_bar_look(p);
        let height = look.button_size + (look.padding + look.border_width) * 2.0;
        // Clear of the line the release was on: half a line, then the bar's spacing.
        let clearance = px(12.0 * s) + look.spacing;
        let viewport = self.list.viewport_bounds();
        let at = point(
            open.at.x.clamp(viewport.left(), viewport.right()),
            open.at.y.clamp(viewport.top(), viewport.bottom()),
        );
        let below = at.y + clearance + height <= viewport.bottom();
        let position = if below {
            point(at.x, at.y + clearance)
        } else {
            point(at.x, at.y - clearance)
        };
        let buttons = open
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let label =
                    gpui::SharedString::from(row["label"].as_str().unwrap_or_default().to_owned());
                let tooltip = label.clone();
                let icon = gpui::SharedString::from(
                    row["iconPath"].as_str().unwrap_or_default().to_owned(),
                );
                let mut command = row["command"].clone();
                if command["type"] == "replyToSelection" {
                    // The Reply popover opens where the bar is, as it opens where the menu was.
                    command["at"] = json!([f32::from(position.x), f32::from(position.y)]);
                }
                div()
                    .id(("chat-selection-bar-action", index))
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(look.button_size)
                    .rounded(look.button_radius)
                    .chat_cursor_pointer()
                    .hover(move |style| style.bg(look.button_hover_background))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .child(
                        svg()
                            .path(icon)
                            .size(look.icon_size)
                            .text_color(look.icon_color)
                            .flex_shrink_0(),
                    )
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        cx.stop_propagation();
                        chat.selection_bar.open = None;
                        chat.handle_action(
                            &super::actions::NativeChatAction {
                                command: command.clone(),
                            },
                            window,
                            cx,
                        );
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        let bar = div()
            .id("chat-selection-bar")
            .role(gpui::Role::Toolbar)
            .aria_label("Selection")
            .flex()
            .items_center()
            .gap(look.gap)
            .p(look.padding)
            .rounded(look.radius)
            .border(look.border_width)
            .border_color(look.border_color)
            .bg(look.background)
            .shadow(look.shadow.clone())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|chat, _, _, cx| {
                    chat.selection_bar.press_on_bar = true;
                    // The press is the bar's: it starts no new selection in the text under it.
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down_out(cx.listener(|chat, _, _, cx| chat.close_selection_bar(cx)))
            .children(buttons);
        Some(
            deferred(
                anchored()
                    .position(position)
                    .anchor(if below {
                        Anchor::TopLeft
                    } else {
                        Anchor::BottomLeft
                    })
                    .snap_to_window_with_margin(px(8.0 * s))
                    .child(bar),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
