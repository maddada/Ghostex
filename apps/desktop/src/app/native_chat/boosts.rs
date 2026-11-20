//! Boosts in the chat (gx-chat-core `extras/boosts.rs` has the user's decisions and the rules): the
//! Boost pill at the right of the pill strip; its menu, a 280px list in Show, Do, Work and Custom
//! tabs with every description in full and a preview card beside it; the one-time callout bubble
//! above the pill; the Boost pills' hover and double click; "Try a Boost" on a new chat; and Save as
//! Boost. Editing is Settings > Boosts (window/settings_modal/tabs/boosts.rs), which the menu's
//! Edit Boosts row, a pill's double click and Save as Boost open.
//!
//! The core's `boosts` document key holds it all; `null` (an older gxserver, no answer yet, or
//! Boosts switched off) draws none of it.
//!
//! CDXC:Boosts 2026-11-20 WHY:
//! The pill rides in its own key rather than `chatPills` because the phone draws every `chatPills`
//! entry and opens row menus only for the pills it knows. The pill sits after the panel pills,
//! before Links, and never folds into "+N" (it is the one way in). The preview is a panel of the
//! menu one level down (option_menu's side panel, as the model menu's lists are), so it opens beside
//! the menu on the side with room; the callout is a frosted window of its own
//! (frosted_overlay_window.rs), which floats over the transcript without taking the keyboard.
//! SEE-ALSO: packages/gx-chat-core/src/extras/boosts.rs, server/src/boosts.rs.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window, canvas, div,
    px, relative, rgb, svg,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// The Boost pill's menu-toggle key (menu_toggle.rs), its menu's and preview's ids among the pill
/// menus (chat_pills.rs), and their widths (unscaled).
pub(super) const BOOST_TRIGGER: &str = "chat-pill-boost";
pub(super) const BOOST_MENU: &str = "boostMenu";
pub(super) const BOOST_PREVIEW: &str = "boostPreview";
pub(super) const BOOST_MENU_WIDTH: f32 = 280.0;
pub(super) const BOOST_PREVIEW_WIDTH: f32 = 236.0;
/// The preview card's content height.
const PREVIEW_HEIGHT: f32 = 172.0;
/// The menu's tab bar, its footer row, a row's padding and the width a description wraps in
/// (the menu less its 6px padding and border, the row's 10px sides, the 14px icon and its gap).
const TABS_HEIGHT: f32 = 30.0;
const FOOTER_HEIGHT: f32 = 40.0;
const ROW_PAD_Y: f32 = 8.0;
const LABEL_LINE: f32 = 18.0;
const DETAIL_LINE: f32 = 16.0;
const DETAIL_WIDTH: f32 = BOOST_MENU_WIDTH - 14.0 - 20.0 - 14.0 - 8.0;
/// About how wide a character of a 12px description is; a little over, so a row never clips.
const DETAIL_CHAR: f32 = 6.2;
/// The callout bubble's size (unscaled).
const CALLOUT_WIDTH: f32 = 372.0;
const CALLOUT_HEIGHT: f32 = 54.0;

/// The Boosts' accent: gold.
/// CDXC:Boosts 2026-11-20 DECISION: User: "make Boost gold please, and make the colour of a Boost
/// added to the chat gold also". On dark the Boosts' accent is the main coordinator's gold (the
/// crown, native_sidebar/threads.rs); on light it is a deeper gold, since the crown's light amber
/// (0xd97706) read orange and faint as text on white. The pills in the chat box and in sent messages
/// take `boost` in reference-visual.json, the same golds, and the phone keeps them equal.
pub(super) fn accent(light: bool) -> Hsla {
    if light {
        rgb(0xa16207).into()
    } else {
        rgb(crate::app::native_sidebar::threads::MAIN_COORDINATOR_GOLD_DARK).into()
    }
}

thread_local! {
    /// Each Boost's hover words for the chat box's pills, from the chat being drawn
    /// (`composer_references::replacements` has no view to read them from).
    static TOOLTIPS: RefCell<HashMap<String, String>> = RefCell::default();
}

/// The hover words of the Boost pill whose link points at `href` (`boost:live-page`): its prompt,
/// then "Double click to edit prompt".
pub(super) fn pill_tooltip(href: &str) -> String {
    let Some(id) = ghostex_gx_chat_core::extras::boosts::boost_id_of_href(href) else {
        return href.to_string();
    };
    TOOLTIPS
        .with(|tips| tips.borrow().get(id).cloned())
        .or_else(|| {
            ghostex_gx_chat_core::extras::boosts::boost(id)
                .map(|boost| ghostex_gx_chat_core::extras::boosts::pill_tooltip(boost.prompt))
        })
        .unwrap_or_else(|| "Your Boost\n\nDouble click to edit prompt".to_string())
}

/// The Boost a chat box pill's link names, for its double click.
pub(super) fn boost_id(href: &str) -> Option<String> {
    ghostex_gx_chat_core::extras::boosts::boost_id_of_href(href).map(str::to_string)
}

/// How many lines `text` takes in a menu row.
fn detail_lines(text: &str) -> f32 {
    (text.chars().count() as f32 * DETAIL_CHAR / DETAIL_WIDTH)
        .ceil()
        .max(1.0)
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

    /// The menu's rows: the open tab's Boosts.
    fn boost_tab_rows(&self) -> Vec<Value> {
        let Some(boosts) = self.boosts() else {
            return Vec::new();
        };
        let tab = text(boosts, "tab");
        boosts["list"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["category"] == tab.as_str())
            .cloned()
            .collect()
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

    /// Opens Settings > Boosts; `section` opens one Boost's editor (`boost:<id>`) or a new Boost
    /// (`newBoost`, holding `text`).
    pub(super) fn open_boost_settings(
        &mut self,
        section: Option<String>,
        text: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut open = json!({"type": "open", "modal": "settings", "initialTab": "boosts"});
        if let Some(section) = section {
            open["initialSection"] = json!(section);
        }
        if let Some(text) = text {
            open["initialBoostText"] = json!(text);
        }
        self.close_pill_menu(cx);
        cx.emit(super::state::NativeChatEvent::Host(open));
    }

    /// The Boost pill: the sparkles and "Boost" in the accent, a fill while the message has a
    /// Boost or its menu is open. A click opens the menu against it. Its paint also places the
    /// one-time callout above it.
    pub(super) fn boost_strip_pill(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let boosts = self.boosts()?;
        let pill = &boosts["pill"];
        let s = p.scale;
        let tone = accent(p.light);
        let open = self.chat_menu_is_open(BOOST_TRIGGER);
        let active = pill["active"] == true;
        let bounds: Rc<Cell<Bounds<Pixels>>> = self.boost_pill_bounds.clone();
        let measured = bounds.clone();
        // The callout, while it has not been seen and nothing covers the pane.
        let hint_wanted = boosts["hint"].is_object()
            && !self.frosted_overlay_covered(
                super::frosted_overlay_window::FrostedOverlay::BoostHint,
                cx,
            );
        let chat = cx.weak_entity();
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
                .when(!open, |pill| {
                    pill.tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new(
                            "Boosts: the agent answers with a page, a chart, a browser run and more",
                        )
                        .build(window, cx)
                    })
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
                    canvas(
                        move |bounds, window, cx| {
                            measured.set(bounds);
                            let frame = (hint_wanted
                                && super::child_window::bounds_inside_content_mask(window, bounds))
                            .then(|| callout_frame(bounds, s));
                            let chat = chat.clone();
                            cx.defer(move |cx| {
                                let _ = chat.update(cx, |chat, cx| {
                                    chat.place_frosted_overlay(
                                        super::frosted_overlay_window::FrostedOverlay::BoostHint,
                                        frame,
                                        cx,
                                    );
                                });
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset(px(-s)),
                )
                .into_any_element(),
        )
    }

    /// Opens (or shuts) the Boost menu against the pill, and reads the Boosts again: Settings may
    /// have changed them since this chat opened. Opening it is what the callout asked for.
    pub(super) fn open_boost_menu(
        &mut self,
        pill: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .boosts()
            .is_some_and(|boosts| boosts["hint"].is_object())
        {
            self.invoke(json!({"type": "dismissBoostHint"}), cx);
        }
        self.boost_preview = None;
        self.invoke(json!({"type": "refreshBoosts"}), cx);
        self.toggle_pill_menu(BOOST_MENU, pill, window, cx);
    }

    /// The callout's Show me, from its own window: the menu opens on the chat's next frame, which
    /// has the chat's window.
    pub(super) fn request_boost_menu(&mut self, cx: &mut Context<Self>) {
        self.invoke(json!({"type": "dismissBoostHint"}), cx);
        self.boost_menu_requested = true;
        cx.notify();
    }

    /// Opens the menu a callout asked for, now that this frame has the chat's window.
    pub(super) fn open_requested_boost_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if std::mem::take(&mut self.boost_menu_requested) {
            let pill = self.boost_pill_bounds.get();
            self.boost_preview = None;
            self.invoke(json!({"type": "refreshBoosts"}), cx);
            self.toggle_pill_menu(BOOST_MENU, pill, window, cx);
        }
    }

    /// The menu's content height (unscaled): the tabs, the open tab's rows with their full
    /// descriptions, and Edit Boosts.
    pub(super) fn boost_menu_height(&self) -> Option<f32> {
        self.boosts()?;
        let rows: f32 = self
            .boost_tab_rows()
            .iter()
            .map(|row| {
                ROW_PAD_Y * 2.0
                    + LABEL_LINE
                    + detail_lines(&text(row, "description")) * DETAIL_LINE
                    + 2.0
            })
            .sum();
        Some(TABS_HEIGHT + 6.0 + rows + FOOTER_HEIGHT)
    }

    /// The preview card's content height, while there is a Boost to preview.
    pub(super) fn boost_preview_height(&self) -> Option<f32> {
        self.boosts()?;
        Some(PREVIEW_HEIGHT)
    }

    /// The menu: the tabs, the open tab's Boosts (icon, name, the full description, a tick while
    /// in the message) and Edit Boosts. A row under the pointer shows its preview beside the menu;
    /// a click puts it in the message or takes it out. `window` is the menu's own.
    pub(super) fn boost_menu_body(
        &mut self,
        p: &ChatAppearance,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let boosts = self.boosts()?.clone();
        let s = p.scale;
        let tone = accent(p.light);
        let tab = text(&boosts, "tab");
        let tabs = div()
            .flex()
            .flex_shrink_0()
            .gap(px(2.0 * s))
            .p(px(2.0 * s))
            .h(px(TABS_HEIGHT * s))
            .rounded(px(8.0 * s))
            .bg(p.foreground.opacity(0.06))
            .children(
                boosts["tabs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|entry| {
                        let id = text(entry, "id");
                        let on = id == tab;
                        div()
                            .id(gpui::ElementId::Name(format!("boost-tab-{id}").into()))
                            .role(gpui::Role::Tab)
                            .aria_label(text(entry, "label"))
                            .aria_selected(on)
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0 * s))
                            .text_size(px(12.0 * s))
                            .text_color(if on { p.foreground } else { p.card_muted })
                            .chat_cursor_pointer()
                            .when(on, |tab| tab.bg(p.foreground.opacity(0.1)))
                            .hover(|style| style.text_color(p.foreground))
                            .child(text(entry, "label"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invoke(json!({"type": "setBoostTab", "tab": id}), cx);
                            }))
                    }),
            );
        let draft = self.draft.clone();
        let preview = self.boost_preview.clone();
        let rows = div()
            .id("boost-menu-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(2.0 * s))
            .children(self.boost_tab_rows().into_iter().map(|row| {
                let id = text(&row, "id");
                let in_draft = row["inDraft"] == true;
                let hovered = preview.as_deref() == Some(id.as_str());
                let hover_id = id.clone();
                let click_id = id.clone();
                let draft = draft.clone();
                div()
                    .id(gpui::ElementId::Name(format!("boost-row-{id}").into()))
                    .role(gpui::Role::MenuItem)
                    .aria_label(format!(
                        "{}. {}",
                        text(&row, "label"),
                        text(&row, "description")
                    ))
                    .flex()
                    .items_start()
                    .flex_shrink_0()
                    .gap(px(8.0 * s))
                    .px(px(10.0 * s))
                    .py(px(ROW_PAD_Y * s))
                    .rounded(px(6.0 * s))
                    .chat_cursor_pointer()
                    .when(hovered, |row| row.bg(p.foreground.opacity(0.07)))
                    .hover(|style| style.bg(p.foreground.opacity(0.07)))
                    .child(
                        svg()
                            .path(text(&row, "iconPath"))
                            .mt(px(2.0 * s))
                            .size(px(14.0 * s))
                            .flex_shrink_0()
                            .text_color(tone),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.0 * s))
                                    .line_height(px(LABEL_LINE * s))
                                    .text_color(p.foreground)
                                    .child(text(&row, "label")),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .text_size(px(12.0 * s))
                                    .line_height(px(DETAIL_LINE * s))
                                    .text_color(p.card_muted)
                                    .child(text(&row, "description")),
                            ),
                    )
                    .when(in_draft, |row| {
                        row.child(
                            svg()
                                .path("titlebar/check.svg")
                                .mt(px(2.0 * s))
                                .size(px(14.0 * s))
                                .flex_shrink_0()
                                .text_color(p.foreground),
                        )
                    })
                    .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                        if *hovered {
                            this.preview_boost(&hover_id, window, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.invoke(
                            json!({"type": "toggleBoost", "id": click_id, "text": draft}),
                            cx,
                        );
                        this.close_pill_menu(cx);
                    }))
            }));
        let footer = div()
            .id("boost-menu-edit")
            .role(gpui::Role::MenuItem)
            .aria_label("Edit Boosts")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0 * s))
            .mt(px(4.0 * s))
            // CDXC:Boosts 2026-11-20 WHY: User: the "Edit Boosts…" row "isn't vertically centred
            // in its footer". The menu's own 6px padding sat under the row, so the text was 6px
            // nearer the line above it than the menu's bottom edge; the row reaches down over that
            // padding (same height in the layout) so its text sits halfway between the two.
            .h(px(FOOTER_HEIGHT * s))
            .mb(px(-6.0 * s))
            .px(px(10.0 * s))
            .border_t_1()
            .border_color(p.foreground.opacity(0.08))
            .text_size(px(12.5 * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| style.text_color(p.foreground))
            .child(
                svg()
                    .path("titlebar/settings.svg")
                    .size(px(14.0 * s))
                    .text_color(p.card_muted),
            )
            .child("Edit Boosts\u{2026}")
            .on_click(cx.listener(|this, _, _, cx| this.open_boost_settings(None, None, cx)));
        Some(
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap(px(6.0 * s))
                .child(tabs)
                .child(rows)
                .child(footer)
                .into_any_element(),
        )
    }

    /// A row of the menu went under the pointer: its preview shows beside the menu, opening the
    /// side panel the first time.
    fn preview_boost(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.boost_preview.as_deref() == Some(id) {
            return;
        }
        self.boost_preview = Some(id.to_string());
        cx.notify();
        let Some(menu) = self.option_menu.clone() else {
            return;
        };
        if menu.read(cx).has_side_panel() {
            return;
        }
        let height = PREVIEW_HEIGHT;
        let anchor = window.bounds();
        menu.update(cx, |menu, cx| {
            menu.open_side_panel(
                vec![json!({"pillMenu": {"id": BOOST_PREVIEW, "height": height}})],
                anchor,
                BOOST_PREVIEW_WIDTH,
                cx,
            );
        });
    }

    /// The preview card: the Boost's name, a small picture of what the agent sends back, and what
    /// a click does.
    pub(super) fn boost_preview_body(&self, p: &ChatAppearance) -> Option<AnyElement> {
        let id = self.boost_preview.clone()?;
        let row = self.boosts()?["list"]
            .as_array()?
            .iter()
            .find(|row| row["id"] == id.as_str())?
            .clone();
        let s = p.scale;
        let tone = accent(p.light);
        let custom = row["custom"] == true;
        Some(
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap(px(8.0 * s))
                .p(px(4.0 * s))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .text_size(px(13.0 * s))
                        .child(
                            svg()
                                .path(text(&row, "iconPath"))
                                .size(px(14.0 * s))
                                .text_color(tone),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.foreground)
                                .child(text(&row, "label")),
                        )
                        .child(
                            div()
                                .text_size(px(11.0 * s))
                                .text_color(p.card_muted)
                                .child(if custom { "Your Boost" } else { "Boost" }),
                        ),
                )
                .child(preview_art(
                    &text(&row, "preview"),
                    &text(&row, "prompt"),
                    p,
                ))
                .child(
                    div()
                        .text_size(px(11.5 * s))
                        .line_height(px(15.0 * s))
                        .text_color(p.card_muted)
                        .child(if row["inDraft"] == true {
                            "In your message. Click to take it out."
                        } else {
                            "Click to add it to your message."
                        }),
                )
                .into_any_element(),
        )
    }

    /// "Try a Boost" under a new chat's headline: a chip per Show Boost; a click puts it in the
    /// message.
    pub(super) fn render_try_a_boost(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let list: Vec<Value> = self.boosts()?["list"]
            .as_array()?
            .iter()
            .filter(|row| row["category"] == "show")
            .cloned()
            .collect();
        let s = p.scale;
        let tone = accent(p.light);
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

    /// The words selected in the chat box, trimmed, while Boosts are on and some are selected.
    pub(super) fn composer_selection_for_boost(&self, cx: &gpui::App) -> Option<String> {
        self.boosts()?;
        let range = self.input.as_ref()?.read(cx).selected_range();
        self.draft
            .get(range)
            .map(str::trim)
            .filter(|selected| !selected.is_empty())
            .map(str::to_string)
    }

    /// The chat box's right-click menu: the text field's own Cut, Copy, Paste and Select All, and
    /// Save as Boost under them while words are selected.
    /// CDXC:Boosts 2026-11-20 DECISION: User: right-click on a chat-box selection showed "the
    /// default Cut/Copy/Paste menu" (with the chat's own Save as Boost menu opening behind it), and
    /// Save as Boost belongs "in the chat box's right-click menu properly". The field's menu is the
    /// one menu now, built here so it carries Save as Boost; the row runs the chat's own
    /// `saveAsBoost` command, as the transcript's row and the selection bar (selection_bar.rs) do.
    pub(super) fn composer_context_menu(
        chat: gpui::WeakEntity<Self>,
    ) -> impl Fn(
        gpui_component::native_menu::NativeMenu,
        &mut Window,
        &mut gpui::App,
    ) -> gpui_component::native_menu::NativeMenu
    + 'static {
        move |menu, _, cx| {
            let Some(chat) = chat.upgrade() else {
                return menu;
            };
            let chat = chat.read(cx);
            let has_selection = chat
                .input
                .as_ref()
                .is_some_and(|input| !input.read(cx).selected_range().is_empty());
            let boost = chat.composer_selection_for_boost(cx);
            // Paste is offered without peeking at the clipboard, as GPUI Kit's own edit menu does.
            let menu = menu
                .menu_with_disabled(
                    "Cut",
                    !has_selection,
                    Box::new(gpui_component::input::Cut),
                )
                .menu_with_disabled(
                    "Copy",
                    !has_selection,
                    Box::new(gpui_component::input::Copy),
                )
                .menu("Paste", Box::new(gpui_component::input::Paste))
                .separator()
                .menu("Select All", Box::new(gpui_component::input::SelectAll));
            match boost {
                Some(text) => menu.separator().menu(
                    "Save as Boost",
                    Box::new(super::actions::NativeChatAction {
                        command: json!({"type": "saveAsBoost", "text": text}),
                    }),
                ),
                None => menu,
            }
        }
    }
}

/// Where the callout bubble goes for the pill painted at `pill`: above it, its right edge on the
/// pill's.
fn callout_frame(pill: Bounds<Pixels>, s: f32) -> Bounds<Pixels> {
    let size = gpui::size(px(CALLOUT_WIDTH * s), px(CALLOUT_HEIGHT * s));
    let origin = gpui::point(
        (pill.right() - size.width).max(px(8.0 * s)),
        pill.top() - size.height - px(10.0 * s),
    );
    Bounds::new(origin, size)
}

/// The callout bubble, drawn in its own frosted window (frosted_overlay_window.rs): the sparkles,
/// "New: Boosts" and one line, Show me and an x.
pub(super) fn boost_callout(
    chat: gpui::WeakEntity<NativeChatView>,
    hint: &Value,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let tone = accent(p.light);
    let show = chat.clone();
    let close = chat;
    div()
        .id("boost-callout")
        .role(gpui::Role::Status)
        .aria_label(format!("{}. {}", text(hint, "title"), text(hint, "text")))
        .size_full()
        .flex()
        .items_center()
        .gap(px(10.0 * s))
        .px(px(12.0 * s))
        .rounded(px(12.0 * s))
        .border(px(s))
        .border_color(p.foreground.opacity(0.12))
        .child(
            svg()
                .path("titlebar/sparkles.svg")
                .size(px(18.0 * s))
                .flex_shrink_0()
                .text_color(tone),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.0 * s))
                        .line_height(px(17.0 * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.foreground)
                        .child(text(hint, "title")),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0 * s))
                        .line_height(px(16.0 * s))
                        .text_color(p.card_muted)
                        .child(text(hint, "text")),
                ),
        )
        .child(
            div()
                .id("boost-callout-show")
                .role(gpui::Role::Button)
                .aria_label("Show me")
                .flex_shrink_0()
                .px(px(10.0 * s))
                .py(px(4.0 * s))
                .rounded_full()
                .bg(tone.opacity(0.18))
                .text_size(px(12.5 * s))
                .text_color(tone)
                .chat_cursor_pointer()
                .hover(|style| style.bg(tone.opacity(0.26)))
                .child("Show me")
                .on_click(move |_, _, cx| {
                    let _ = show.update(cx, |chat, cx| chat.request_boost_menu(cx));
                }),
        )
        .child(
            div()
                .id("boost-callout-close")
                .role(gpui::Role::Button)
                .aria_label("Close")
                .flex_shrink_0()
                .size(px(22.0 * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.08)))
                .child(
                    svg()
                        .path("titlebar/x.svg")
                        .size(px(13.0 * s))
                        .text_color(p.card_muted),
                )
                .on_click(move |_, _, cx| {
                    let _ = close.update(cx, |chat, cx| {
                        chat.invoke(json!({"type": "dismissBoostHint"}), cx)
                    });
                }),
        )
        .into_any_element()
}

/// A small picture of what a Boost sends back, in the chat's own colours: shapes only, no words but
/// a few labels. `kind` is the Boost's `preview`; a user's Boost shows the start of its prompt.
fn preview_art(kind: &str, prompt: &str, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let tone = accent(p.light);
    let strong = p.foreground.opacity(0.22);
    let soft = p.foreground.opacity(0.1);
    let line = move |width: f32, height: f32, color: Hsla| {
        div()
            .w(relative(width))
            .h(px(height * s))
            .flex_shrink_0()
            .rounded(px(2.0 * s))
            .bg(color)
    };
    let tile = move || {
        div()
            .flex_1()
            .min_w_0()
            .rounded(px(5.0 * s))
            .bg(soft)
            .p(px(5.0 * s))
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
    };
    let small = move |label: &'static str, color: Hsla| {
        div()
            .text_size(px(8.5 * s))
            .line_height(px(11.0 * s))
            .text_color(color)
            .child(label)
    };
    let bar = move |label: Option<&'static str>| {
        div()
            .flex()
            .items_center()
            .gap(px(3.0 * s))
            .h(px(12.0 * s))
            .px(px(5.0 * s))
            .bg(soft)
            .children((0..3).map(|_| div().size(px(4.0 * s)).rounded_full().bg(strong)))
            .when_some(label, |bar, label| {
                bar.child(div().ml(px(4.0 * s)).child(small(label, p.card_muted)))
            })
    };
    let body: AnyElement = match kind {
        "page" | "app" | "browser" => div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar(Some(match kind {
                "app" => "Finder",
                "browser" => "ghostex browser",
                _ => "onboarding.html",
            })))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .gap(px(6.0 * s))
                    .p(px(7.0 * s))
                    .when(kind == "app", |body| {
                        body.child(
                            div()
                                .w(relative(0.28))
                                .flex()
                                .flex_col()
                                .gap(px(4.0 * s))
                                .children((0..4).map(|_| line(1.0, 5.0, soft))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(5.0 * s))
                            .child(line(0.45, 7.0, strong))
                            .child(line(0.7, 5.0, soft))
                            .child(div().flex().gap(px(5.0 * s)).h(px(26.0 * s)).children(
                                (0..3).map(|n| {
                                    tile().when(n == 1 && kind == "browser", |tile| {
                                        tile.border(px(s)).border_color(tone)
                                    })
                                }),
                            )),
                    ),
            )
            .into_any_element(),
        "chart" => div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .p(px(7.0 * s))
            .child(
                div()
                    .flex()
                    .gap(px(5.0 * s))
                    .child(
                        tile()
                            .child(small("Signups", p.card_muted))
                            .child(small("1,284", p.foreground)),
                    )
                    .child(
                        tile()
                            .child(small("Week", p.card_muted))
                            .child(small("+18%", p.foreground)),
                    ),
            )
            .child(
                div().flex_1().flex().items_end().gap(px(4.0 * s)).children(
                    [0.34_f32, 0.52, 0.44, 0.7, 0.62, 0.88, 0.76]
                        .into_iter()
                        .enumerate()
                        .map(|(n, height)| {
                            div()
                                .flex_1()
                                .h(relative(height))
                                .rounded_t(px(2.0 * s))
                                .bg(if n == 5 { tone } else { strong })
                        }),
                ),
            )
            .into_any_element(),
        "compare" | "triage" | "second" => div()
            .size_full()
            .flex()
            .gap(px(5.0 * s))
            .p(px(7.0 * s))
            .children((0..if kind == "second" { 2 } else { 3 }).map(|n| {
                let pick = (kind == "compare" && n == 1) || (kind == "second" && n == 1);
                tile()
                    .child(small(
                        match (kind, n) {
                            ("compare", 0) => "A",
                            ("compare", 1) => "B",
                            ("compare", _) => "C",
                            ("triage", 0) => "#412",
                            ("triage", 1) => "#415",
                            ("triage", _) => "GX-88",
                            (_, 0) => "Your agent",
                            _ => "Second agent",
                        },
                        p.foreground,
                    ))
                    .child(line(0.85, 4.0, strong))
                    .child(line(0.6, 4.0, strong))
                    .child(div().flex_1())
                    .child(
                        div()
                            .px(px(4.0 * s))
                            .rounded(px(3.0 * s))
                            .when(pick, |chip| chip.bg(p.foreground.opacity(0.85)))
                            .when(!pick, |chip| chip.border(px(s)).border_color(strong))
                            .child(small(
                                match kind {
                                    "triage" => "Merge",
                                    "second" if n == 1 => "Checked",
                                    "second" => "Answer",
                                    _ => "Pick",
                                },
                                if pick { p.background } else { p.card_muted },
                            )),
                    )
            }))
            .into_any_element(),
        "map" | "workers" | "worktree" => div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0 * s))
            .p(px(7.0 * s))
            .child(
                div()
                    .px(px(7.0 * s))
                    .py(px(3.0 * s))
                    .rounded(px(5.0 * s))
                    .bg(soft)
                    .border(px(s))
                    .border_color(if kind == "map" { strong } else { tone })
                    .child(small(
                        match kind {
                            "workers" => "Coordinator",
                            "worktree" => "main",
                            _ => "Chat box",
                        },
                        p.foreground,
                    )),
            )
            .child(div().w(px(1.0)).h(px(10.0 * s)).bg(strong))
            .child(
                div().flex().gap(px(8.0 * s)).children(
                    match kind {
                        "workers" => vec!["Worker 1", "Worker 2", "Worker 3"],
                        "worktree" => vec!["boost/onboarding"],
                        _ => vec!["gxserver", "Agent"],
                    }
                    .into_iter()
                    .map(|label| {
                        div()
                            .px(px(7.0 * s))
                            .py(px(3.0 * s))
                            .rounded(px(5.0 * s))
                            .bg(soft)
                            .border(px(s))
                            .border_color(strong)
                            .child(small(label, p.foreground))
                    }),
                ),
            )
            .into_any_element(),
        "form" | "steps" => div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(5.0 * s))
            .p(px(7.0 * s))
            .child(line(0.55, 6.0, strong))
            .children((0..3).map(|n| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .h(px(15.0 * s))
                    .px(px(5.0 * s))
                    .rounded(px(4.0 * s))
                    .bg(soft)
                    .child(
                        div()
                            .size(px(7.0 * s))
                            .rounded(px(if kind == "form" { 4.0 } else { 2.0 } * s))
                            .border(px(1.5 * s))
                            .border_color(strong)
                            .when(n == 0, |dot| dot.bg(p.foreground)),
                    )
                    .child(line(0.4, 4.0, strong))
                    .child(div().flex_1())
                    .when(kind == "steps", |row| {
                        row.child(small("Done", p.card_muted))
                    })
                    .when(kind == "form" && n == 0, |row| {
                        row.child(small("Recommended", p.card_muted))
                    })
            }))
            .into_any_element(),
        "shots" => div()
            .size_full()
            .flex()
            .gap(px(6.0 * s))
            .p(px(7.0 * s))
            .children(["Before", "After"].into_iter().map(|label| {
                tile()
                    .child(small(label, p.card_muted))
                    .child(line(0.7, 5.0, strong))
                    .child(div().flex_1().rounded(px(3.0 * s)).bg(if label == "After" {
                        tone.opacity(0.35)
                    } else {
                        strong
                    }))
            }))
            .into_any_element(),
        "docs" => div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(5.0 * s))
            .p(px(9.0 * s))
            .child(small("onboarding-plan.md", p.card_muted))
            .child(line(0.5, 7.0, strong))
            .child(line(0.9, 4.0, soft))
            .child(line(0.82, 4.0, soft))
            .child(line(0.88, 4.0, soft))
            .child(line(0.35, 6.0, strong))
            .child(line(0.75, 4.0, soft))
            .into_any_element(),
        "notify" => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(8.0 * s))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(7.0 * s))
                    .p(px(7.0 * s))
                    .rounded(px(7.0 * s))
                    .bg(soft)
                    .border(px(s))
                    .border_color(strong)
                    .child(
                        svg()
                            .path("titlebar/bell.svg")
                            .size(px(14.0 * s))
                            .text_color(tone),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(small("Onboarding plan", p.foreground))
                            .child(small("Done: 3 pages and a report", p.card_muted)),
                    ),
            )
            .into_any_element(),
        _ => div()
            .size_full()
            .p(px(9.0 * s))
            .text_size(px(11.0 * s))
            .line_height(px(15.0 * s))
            .text_color(p.card_muted)
            .overflow_hidden()
            .child(prompt.chars().take(170).collect::<String>())
            .into_any_element(),
    };
    div()
        .w_full()
        .h(px(100.0 * s))
        .flex_shrink_0()
        .rounded(px(8.0 * s))
        .border(px(s))
        .border_color(p.foreground.opacity(0.1))
        .bg(p.foreground.opacity(0.03))
        .overflow_hidden()
        .child(body)
        .into_any_element()
}
