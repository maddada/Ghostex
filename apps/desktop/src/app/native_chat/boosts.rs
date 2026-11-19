//! Boosts in the chat box (gx-chat-core `extras/boosts.rs` has the user's decision and the rules):
//! the Boost pill at the right of the pill strip and its menu, the Boost pill's prompt in the
//! message's hover and its double click, the prompt editor that opens where the session note does,
//! the one-time hint inside the chat box, and "Try a Boost" on a new chat's welcome.
//!
//! The core's `boosts` document key holds it all; `null` (an older gxserver, or before it
//! answered) draws none of it.
//!
//! CDXC:Boosts 2026-11-20 WHY:
//! The pill rides in its own key rather than `chatPills` because the phone draws every `chatPills`
//! entry and opens row menus only for the pills it knows: a Boost pill there would open an empty
//! sheet on the phones already out. The strip's own pills keep their order; the Boost pill sits
//! after them, before Links, and never folds into "+N" (it is the one way in).
//! SEE-ALSO: packages/gx-chat-core/src/extras/boosts.rs, server/src/boosts.rs.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Focusable as _, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window,
    canvas, div, px, rgb, svg,
};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// The Boost pill's menu-toggle key (menu_toggle.rs) and its menu's width, unscaled.
pub(super) const BOOST_TRIGGER: &str = "chat-pill-boost";
const MENU_WIDTH: f32 = 430.0;

/// The Boosts' accent: the ember of the design page, readable on the dark and the light chat.
fn accent(p: &ChatAppearance) -> Hsla {
    rgb(if p.light { 0xb75d2a } else { 0xe7a47a }).into()
}

thread_local! {
    /// Each Boost's hover words for the chat box's pills, from the chat being drawn
    /// (`composer_references::replacements` has no view to read them from).
    static TOOLTIPS: RefCell<HashMap<String, String>> = RefCell::default();
}

/// The hover words of the Boost pill whose link points at `href` (`boost:live-page`): its prompt,
/// then "Double click to edit prompt".
pub(super) fn pill_tooltip(href: &str) -> String {
    let Some(boost) = ghostex_gx_chat_core::extras::boosts::boost_of_href(href) else {
        return href.to_string();
    };
    TOOLTIPS
        .with(|tips| tips.borrow().get(boost.id).cloned())
        .unwrap_or_else(|| ghostex_gx_chat_core::extras::boosts::pill_tooltip(boost.prompt))
}

/// The Boost a chat box pill's link names, for its double click.
pub(super) fn boost_id(href: &str) -> Option<&'static str> {
    ghostex_gx_chat_core::extras::boosts::boost_of_href(href).map(|boost| boost.id)
}

impl NativeChatView {
    fn boosts(&self) -> Option<&Value> {
        let boosts = &self.snapshot["boosts"];
        boosts.is_object().then_some(boosts)
    }

    /// Whether the strip has the Boost pill.
    pub(super) fn boost_pill_shown(&self) -> bool {
        self.boosts().is_some()
    }

    /// Keeps the chat box pills' hover words current with this chat's Boosts (edited prompts).
    pub(super) fn sync_boost_tooltips(&self) {
        let tips: HashMap<String, String> = self
            .boosts()
            .and_then(|boosts| boosts["list"].as_array())
            .into_iter()
            .flatten()
            .map(|row| (text(row, "id"), text(row, "tooltip")))
            .collect();
        if !tips.is_empty() {
            TOOLTIPS.with(|current| *current.borrow_mut() = tips);
        }
    }

    /// The Boost pill: the sparkles and "Boost" in the accent, a fill while the message has a
    /// Boost. A click opens the menu of Boosts against it.
    pub(super) fn boost_strip_pill(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pill = &self.boosts()?["pill"];
        let s = p.scale;
        let tone = accent(p);
        let open = self.chat_menu_is_open(BOOST_TRIGGER);
        let active = pill["active"] == true;
        let bounds: Rc<Cell<Bounds<Pixels>>> = self.boost_pill_bounds.clone();
        let measured = bounds.clone();
        Some(
            div()
                .id(BOOST_TRIGGER)
                .relative()
                .role(gpui::Role::Button)
                .aria_label(text(pill, "ariaLabel"))
                .aria_expanded(open)
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(5.0 * s))
                .px(px(8.0 * s))
                .py(px(2.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(tone.opacity(if active { 0.6 } else { 0.4 }))
                .text_size(px(13.0 * s))
                .line_height(px(19.0 * s))
                .whitespace_nowrap()
                .text_color(tone)
                .chat_cursor_pointer()
                .when(active || open, |pill| pill.bg(tone.opacity(0.14)))
                .hover(|style| style.bg(tone.opacity(0.1)))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new(
                        "Boosts: the agent answers with a Live page, a chart or a map",
                    )
                    .build(window, cx)
                })
                .child(
                    svg()
                        .path("titlebar/sparkles.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(tone),
                )
                .child(text(pill, "label"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_boost_menu(bounds.get(), window, cx);
                }))
                .child(
                    canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .inset(px(-s)),
                )
                .into_any_element(),
        )
    }

    /// The menu of Boosts, in the sidebar menu's style: each Boost with its line, ticked while it
    /// is in the message; a pick puts it in or takes it out.
    fn open_boost_menu(
        &mut self,
        pill: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_menu_toggled_shut(BOOST_TRIGGER, cx) {
            return;
        }
        let Some(rows) = self
            .boosts()
            .and_then(|boosts| boosts["pill"]["menu"].as_array().cloned())
        else {
            return;
        };
        let (anchor, side) = self.pills_strip.menu_anchor(pill);
        self.menu_side = Some(side);
        self.show_chat_menu(rows, anchor, MENU_WIDTH, window, cx);
    }

    /// The one-time hint, a line inside the chat box (box_notes.rs) with Got it.
    pub(super) fn render_boost_hint(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let hint = self.boosts()?["hint"].as_str()?.to_string();
        let s = p.scale;
        let tone = accent(p);
        Some(
            div()
                .id("boost-hint")
                .role(gpui::Role::Status)
                .aria_label(hint.clone())
                .w_full()
                .min_w_0()
                .flex()
                .items_start()
                .gap(px(6.0 * s))
                .text_size(px(12.0 * s))
                .line_height(px(16.0 * s))
                .text_color(p.muted)
                .child(
                    svg()
                        .path("titlebar/sparkles.svg")
                        .mt(px(2.0 * s))
                        .size(px(12.0 * s))
                        .flex_shrink_0()
                        .text_color(tone),
                )
                .child(div().flex_1().min_w_0().child(hint))
                .child(
                    div()
                        .id("boost-hint-dismiss")
                        .role(gpui::Role::Button)
                        .aria_label("Got it")
                        .flex_shrink_0()
                        .text_color(tone)
                        .chat_cursor_pointer()
                        .child("Got it")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.invoke(json!({"type": "dismissBoostHint"}), cx)
                        })),
                )
                .into_any_element(),
        )
    }

    /// "Try a Boost" under a new chat's headline: a chip per Boost; a click puts it in the message.
    pub(super) fn render_try_a_boost(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let list = self.boosts()?["list"].as_array()?.clone();
        let s = p.scale;
        let tone = accent(p);
        Some(
            div()
                .id("try-a-boost")
                .role(gpui::Role::Group)
                .aria_label("Try a Boost")
                .w_full()
                .max_w(px(560.0 * s))
                .flex()
                .flex_wrap()
                .justify_center()
                .items_center()
                .gap(px(6.0 * s))
                .text_size(px(13.0 * s))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0 * s))
                        .text_color(p.muted)
                        .child(
                            svg()
                                .path("titlebar/sparkles.svg")
                                .size(px(13.0 * s))
                                .text_color(tone),
                        )
                        .child("Try a Boost"),
                )
                .children(list.into_iter().map(|row| {
                    let id = text(&row, "id");
                    let in_draft = row["inDraft"] == true;
                    let description = text(&row, "description");
                    div()
                        .id(gpui::ElementId::Name(format!("try-boost-{id}").into()))
                        .role(gpui::Role::Button)
                        .aria_label(text(&row, "label"))
                        .flex()
                        .items_center()
                        .gap(px(5.0 * s))
                        .px(px(9.0 * s))
                        .py(px(3.0 * s))
                        .rounded_full()
                        .border(px(s))
                        .border_color(if in_draft {
                            tone.opacity(0.6)
                        } else {
                            p.foreground.opacity(0.1)
                        })
                        .when(in_draft, |chip| chip.bg(tone.opacity(0.14)))
                        .text_color(p.foreground)
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.06)))
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(description.clone())
                                .build(window, cx)
                        })
                        .child(
                            svg()
                                .path(text(&row, "iconPath"))
                                .size(px(14.0 * s))
                                .text_color(tone),
                        )
                        .child(text(&row, "label"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let draft = this.draft.clone();
                            this.invoke(json!({"type": "toggleBoost", "id": id, "text": draft}), cx)
                        }))
                }))
                .into_any_element(),
        )
    }

    /// The prompt editor a double click on a Boost pill opens, where the session note opens: the
    /// Boost's prompt in a field, Reset to default when it was edited, Cancel and Save. Escape
    /// cancels, Cmd+Enter (Ctrl+Enter off a Mac) saves.
    pub(super) fn render_boost_editor(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self
            .boosts()
            .map(|boosts| boosts["editor"].clone())
            .filter(Value::is_object);
        let Some(editor) = editor else {
            self.boost_editor_input = None;
            self.boost_editor_subscription = None;
            return None;
        };
        let id = text(&editor, "id");
        let value = text(&editor, "value");
        let fresh = self
            .boost_editor_input
            .as_ref()
            .is_none_or(|(editing, _)| *editing != id);
        if fresh {
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(3, 10)
                    .auto_text_direction(true)
                    .placeholder("What the agent is told when this Boost is in a message")
                    .default_value(value.clone())
            });
            self.boost_editor_subscription = Some(cx.subscribe_in(
                &input,
                window,
                |this, input, event: &InputEvent, _, cx| {
                    if let InputEvent::Change = event {
                        this.invoke(
                            json!({"type": "editBoostPrompt", "text": input.read(cx).value().to_string()}),
                            cx,
                        );
                    }
                },
            ));
            input.read(cx).focus_handle(cx).focus(window, cx);
            self.boost_editor_input = Some((id.clone(), input));
        }
        let input = self.boost_editor_input.as_ref()?.1.clone();
        let s = p.scale;
        let tone = accent(p);
        let edited = editor["edited"] == true;
        let button = |id: &'static str, label: &'static str, command: Value, strong: bool| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .chat_cursor_pointer()
                .px(px(10.0 * s))
                .py(px(3.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(if strong {
                    tone.opacity(0.6)
                } else {
                    p.foreground.opacity(0.12)
                })
                .when(strong, |button| button.bg(tone.opacity(0.16)))
                .text_color(if strong { tone } else { p.foreground })
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
        };
        Some(
            div()
                .id("boost-editor")
                .role(gpui::Role::Group)
                .aria_label(format!("{} Boost prompt", text(&editor, "label")))
                .w_full()
                .flex()
                .flex_col()
                .gap(px(6.0 * s))
                .px(px(12.0 * s))
                .pt(px(8.0 * s))
                .pb(px(10.0 * s))
                .border_1()
                .border_color(p.border)
                .rounded(px(16.0 * s))
                .bg(p.input)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(
                            svg()
                                .path(text(&editor, "iconPath"))
                                .size(px(14.0 * s))
                                .text_color(tone),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(format!(
                                    "{} Boost prompt{}",
                                    text(&editor, "label"),
                                    if edited { " (edited)" } else { "" }
                                )),
                        )
                        .child(self.icon_command(
                            "close-boost-editor",
                            "Close without saving",
                            "titlebar/x.svg",
                            json!({"type": "closeBoostEditor"}),
                            p,
                            cx,
                        )),
                )
                .child(
                    Textarea::new(&input)
                        .appearance(false)
                        .bordered(false)
                        .caret_color(p.foreground)
                        .text_size(px(13.0 * s)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .text_size(px(12.0 * s))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(p.muted)
                                .child("Saved on this computer for every chat. Reset goes back to Ghostex's prompt."),
                        )
                        .when(edited, |row| {
                            row.child(button(
                                "reset-boost-prompt",
                                "Reset to default",
                                json!({"type": "resetBoostPrompt"}),
                                false,
                            ))
                        })
                        .child(button(
                            "save-boost-prompt",
                            "Save",
                            json!({"type": "saveBoostPrompt"}),
                            true,
                        )),
                )
                .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    let key = &event.keystroke;
                    let save =
                        key.key == "enter" && (key.modifiers.platform || key.modifiers.control);
                    if key.key == "escape" || save {
                        this.invoke(
                            json!({"type": if save { "saveBoostPrompt" } else { "closeBoostEditor" }}),
                            cx,
                        );
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                }))
                .into_any_element(),
        )
    }
}
