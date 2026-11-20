//! Boosts in the chat (gx-chat-core `extras/boosts.rs` has the user's decisions and the rules): the
//! Boost pill at the right of the pill strip; its menu, one 460px list in Show, Actions, Work and
//! Custom sections with section chips and Edit… at its bottom, and a preview card beside it; the one-time callout bubble
//! above the pill; the Boost pills' hover and double click; "Try a Boost" on a new chat; and Save as
//! Boost. Editing is Settings > Boosts (window/settings_modal/tabs/boosts.rs), which the menu's
//! Edit… button, a pill's double click and Save as Boost open.
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
pub(super) const BOOST_MENU_WIDTH: f32 = 460.0;
pub(super) const BOOST_PREVIEW_WIDTH: f32 = 236.0;
/// The Boosts "Try a Boost" offers on a new chat, in order (the phone's `TRY_A_BOOST` in
/// cards/Boosts.tsx is the same list).
const TRY_A_BOOST: [&str; 4] = ["live-page", "chart", "compare", "computer-use"];
/// The preview card's content height: its title, the picture, the description (up to
/// three lines) and what a click does.
const PREVIEW_HEIGHT: f32 = 228.0;
/// The menu's parts (unscaled): a section header, a row (its padding, the name's line and the
/// one-line description), the gap between rows, and the bottom bar of section chips and Edit….
const SECTION_HEIGHT: f32 = 26.0;
const ROW_PAD_Y: f32 = 7.0;
const LABEL_LINE: f32 = 18.0;
const DETAIL_LINE: f32 = 16.0;
const ROW_GAP: f32 = 2.0;
const FOOTER_HEIGHT: f32 = 40.0;
/// The tallest the list gets before it scrolls, so the menu never stands taller than this.
const MENU_MAX_HEIGHT: f32 = 420.0;
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
impl NativeChatView {
    fn boosts(&self) -> Option<&Value> {
        let boosts = &self.snapshot["boosts"];
        boosts.is_object().then_some(boosts)
    }

    /// Whether the strip has the Boost pill.
    pub(super) fn boost_pill_shown(&self) -> bool {
        self.boosts().is_some()
    }

    /// The menu's sections in order (Show, Actions, Work, then Custom once there is one), each
    /// with its name and its Boosts.
    fn boost_sections(&self) -> Vec<(String, String, Vec<Value>)> {
        let Some(boosts) = self.boosts() else {
            return Vec::new();
        };
        let list = boosts["list"].as_array().cloned().unwrap_or_default();
        boosts["tabs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|tab| {
                let id = text(tab, "id");
                let rows: Vec<Value> = list
                    .iter()
                    .filter(|row| row["category"] == id.as_str())
                    .cloned()
                    .collect();
                (id, text(tab, "label"), rows)
            })
            .filter(|(_, _, rows)| !rows.is_empty())
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

    /// The menu's content height (unscaled): every section's header and rows, up to
    /// `MENU_MAX_HEIGHT` (the list scrolls past it), and the bottom bar.
    /// CDXC:Boosts 2026-11-20 DECISION: User (Q4304 "B"): the menu is "too long and not wide
    /// enough" and its tabs left "empty space at the bottom of the first two tabs". It is one
    /// 460px list of every Boost under section headers, one line a description (the whole line is
    /// in the preview card), no taller than its content; "the section chips at the bottom LEFT and
    /// the edit button at the bottom RIGHT on the same row, and call it 'Edit…'". Supersedes the
    /// 280px tabbed menu and its fixed tallest-tab height.
    pub(super) fn boost_menu_height(&self) -> Option<f32> {
        self.boosts()?;
        let list: f32 = self
            .boost_sections()
            .iter()
            .map(|(_, _, rows)| {
                SECTION_HEIGHT
                    + rows.len() as f32 * (ROW_PAD_Y * 2.0 + LABEL_LINE + DETAIL_LINE + ROW_GAP)
            })
            .sum();
        Some(list.min(MENU_MAX_HEIGHT) + 6.0 + FOOTER_HEIGHT)
    }

    /// The preview card's content height, while there is a Boost to preview.
    pub(super) fn boost_preview_height(&self) -> Option<f32> {
        self.boosts()?;
        Some(PREVIEW_HEIGHT)
    }

    /// The menu: one list of every Boost under its section's header (icon, name, the description
    /// on one line, a tick while in the message), then a bar with the section chips on the left (a
    /// click scrolls to that section; the one at the top of the list is lit) and Edit… on the
    /// right. A row under the pointer shows its preview, with the whole description, beside the
    /// menu; a click puts it in the message or takes it out. `window` is the menu's own.
    pub(super) fn boost_menu_body(
        &mut self,
        p: &ChatAppearance,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.boosts()?;
        let sections = self.boost_sections();
        let s = p.scale;
        let tone = accent(p.light);
        let draft = self.draft.clone();
        let preview = self.boost_preview.clone();
        let scroll = self.boost_scroll.clone();
        // Each section's header is a child of the list; its index is what a chip scrolls to.
        let mut starts = Vec::with_capacity(sections.len());
        let mut next = 0;
        for (_, _, rows) in &sections {
            starts.push(next);
            next += 1 + rows.len();
        }
        // The lit chip is the section at the top of the list, or the last one once the list is
        // scrolled to its end (a short last section never reaches the top).
        let max = scroll.max_offset().y;
        let at_end = max > px(0.0) && -scroll.offset().y >= max - px(1.0);
        let top = scroll.top_item();
        let current = if at_end {
            sections.len().saturating_sub(1)
        } else {
            starts.iter().rposition(|start| *start <= top).unwrap_or(0)
        };
        let mut children: Vec<AnyElement> = Vec::with_capacity(next);
        for (id, label, rows) in &sections {
            children.push(
                div()
                    .id(gpui::ElementId::Name(format!("boost-section-{id}").into()))
                    .role(gpui::Role::Heading)
                    .aria_label(label.clone())
                    .flex_shrink_0()
                    .h(px((SECTION_HEIGHT - ROW_GAP) * s))
                    .flex()
                    .items_end()
                    .px(px(10.0 * s))
                    .pb(px(3.0 * s))
                    .text_size(px(10.5 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.card_muted.opacity(0.8))
                    .child(label.to_uppercase())
                    .into_any_element(),
            );
            for row in rows {
                let id = text(row, "id");
                let in_draft = row["inDraft"] == true;
                let hovered = preview.as_deref() == Some(id.as_str());
                let hover_id = id.clone();
                let click_id = id.clone();
                let draft = draft.clone();
                children.push(
                    div()
                        .id(gpui::ElementId::Name(format!("boost-row-{id}").into()))
                        .role(gpui::Role::MenuItem)
                        .aria_label(format!(
                            "{}. {}",
                            text(row, "label"),
                            text(row, "description")
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
                                .path(text(row, "iconPath"))
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
                                        .truncate()
                                        .text_size(px(13.0 * s))
                                        .line_height(px(LABEL_LINE * s))
                                        .text_color(p.foreground)
                                        .child(text(row, "label")),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(12.0 * s))
                                        .line_height(px(DETAIL_LINE * s))
                                        .text_color(p.card_muted)
                                        .child(text(row, "description")),
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
                        .into_any_element(),
                );
            }
        }
        let rows = div()
            .id("boost-menu-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .gap(px(ROW_GAP * s))
            .children(children);
        let chips = div().flex().items_center().gap(px(4.0 * s)).children(
            sections.iter().enumerate().map(|(index, (id, label, _))| {
                let on = index == current;
                let start = starts[index];
                let scroll = scroll.clone();
                div()
                    .id(gpui::ElementId::Name(format!("boost-chip-{id}").into()))
                    .role(gpui::Role::Button)
                    .aria_label(format!("Go to {label}"))
                    .aria_selected(on)
                    .flex_shrink_0()
                    .px(px(9.0 * s))
                    .py(px(2.0 * s))
                    .rounded_full()
                    .border(px(s))
                    .text_size(px(12.0 * s))
                    .whitespace_nowrap()
                    .chat_cursor_pointer()
                    .map(|chip| {
                        if on {
                            chip.border_color(gpui::transparent_black())
                                .bg(p.foreground.opacity(0.1))
                                .text_color(p.foreground)
                        } else {
                            chip.border_color(p.foreground.opacity(0.1))
                                .text_color(p.card_muted)
                        }
                    })
                    .hover(|style| style.text_color(p.foreground))
                    .child(label.clone())
                    // The offset is set here rather than on the next layout, so this frame's chips
                    // already light the section scrolled to.
                    .on_click(cx.listener(move |_, _, window, _| {
                        if let Some(item) = scroll.bounds_for_item(start) {
                            let y = (scroll.bounds().top() - item.top())
                                .max(-scroll.max_offset().y)
                                .min(px(0.0));
                            scroll.set_offset(gpui::point(px(0.0), y));
                        } else {
                            scroll.scroll_to_top_of_item(start);
                        }
                        window.refresh();
                    }))
            }),
        );
        let edit = div()
            .id("boost-menu-edit")
            .role(gpui::Role::MenuItem)
            .aria_label("Edit Boosts")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0 * s))
            .px(px(8.0 * s))
            .py(px(3.0 * s))
            .rounded(px(6.0 * s))
            .text_size(px(12.5 * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|style| style.text_color(p.foreground).bg(p.foreground.opacity(0.06)))
            .child(
                svg()
                    .path("titlebar/settings.svg")
                    .size(px(14.0 * s))
                    .text_color(p.card_muted),
            )
            .child("Edit\u{2026}")
            .on_click(cx.listener(|this, _, _, cx| this.open_boost_settings(None, None, cx)));
        // The bar reaches over the menu's own 6px padding below it, so its row sits halfway
        // between the line above and the menu's bottom edge.
        let footer = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap(px(8.0 * s))
            .h(px(FOOTER_HEIGHT * s))
            .mb(px(-6.0 * s))
            .px(px(4.0 * s))
            .border_t_1()
            .border_color(p.foreground.opacity(0.08))
            .child(chips)
            .child(edit);
        Some(
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap(px(6.0 * s))
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
                // The whole description, which the menu's row cuts to one line.
                .child(
                    div()
                        .text_size(px(12.0 * s))
                        .line_height(px(16.0 * s))
                        .line_clamp(3)
                        .text_color(p.foreground.opacity(0.88))
                        .child(text(&row, "description")),
                )
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

    /// "Try a Boost" just above a new chat's chat box: a chip per featured Boost, on one row; a
    /// click puts it in the message. Only while the new chat's welcome shows its headline.
    /// CDXC:Boosts 2026-11-20 DECISION: User: the chips wrapping onto a second row under the
    /// headline "looks very ugly", then: "don't show the Try a Boost chips under the new-chat
    /// headline at all; on a new chat they belong just above the chat box", and "'Ask me in a form'
    /// is a bad one to highlight: feature 'Computer & Browser use' instead". The row never wraps:
    /// the chips that fit show, and the rest fold into a "+N" chip whose menu lists them.
    pub(super) fn render_try_a_boost(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let welcome = &self.snapshot["newSessionWelcome"];
        if !welcome.is_object() || welcome["showTitle"] == false {
            return None;
        }
        let rows = self.boosts()?["list"].as_array()?;
        let all: Vec<Value> = TRY_A_BOOST
            .iter()
            .filter_map(|id| rows.iter().find(|row| row["id"] == *id).cloned())
            .collect();
        if all.is_empty() {
            return None;
        }
        // The box's width on the last frame (the pane's, less its margins, before the first).
        let box_width = self.composer_bounds.get().size.width.as_f32();
        let room = if box_width > 0.0 {
            box_width
        } else {
            (self.bounds.get().size.width.as_f32() - 48.0 * p.scale).max(0.0)
        } - 16.0 * p.scale;
        let s = p.scale;
        let tone = accent(p.light);
        // Widths as the row lays them out, from the text's length (13px, about 0.5em a glyph as
        // measured in the web build): padding, border, icon and gap around each label, 6px between.
        let glyphs = |label: &str| label.chars().count() as f32 * 13.0 * 0.5 * s;
        let chip_width = |label: &str| glyphs(label) + (18.0 + 2.0 + 14.0 + 5.0) * s;
        let gap = 6.0 * s;
        let lead = glyphs("Try a Boost") + 18.0 * s;
        let more_width = |hidden: usize| glyphs(&format!("+{hidden}")) + 20.0 * s;
        let mut used = lead;
        let mut shown = 0;
        for (index, row) in all.iter().enumerate() {
            let width = gap + chip_width(&text(row, "label"));
            let left = all.len() - index - 1;
            let reserve = if left > 0 { gap + more_width(left) } else { 0.0 };
            if used + width + reserve > room {
                break;
            }
            used += width;
            shown += 1;
        }
        let hidden: Vec<Value> = all[shown..].to_vec();
        let list: Vec<Value> = all[..shown].to_vec();
        let more = (!hidden.is_empty()).then(|| {
            let count = hidden.len();
            div()
                .id("try-a-boost-more")
                .role(gpui::Role::Button)
                .aria_label(format!("{count} more Boosts"))
                .flex_shrink_0()
                .px(px(9.0 * s))
                .py(px(3.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(p.foreground.opacity(0.1))
                .text_color(p.muted)
                .chat_cursor_pointer()
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .child(format!("+{count}"))
                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    let rows: Vec<Value> = hidden
                        .iter()
                        .map(|row| {
                            json!({
                                "label": text(row, "label"),
                                "iconPath": text(row, "iconPath"),
                                "command": {"type": "toggleBoost", "id": text(row, "id"), "text": this.draft},
                            })
                        })
                        .collect();
                    this.show_chat_menu_at(rows, event.position(), 240.0, window, cx);
                }))
        });
        Some(
            div()
                .id("try-a-boost")
                .role(gpui::Role::Group)
                .aria_label("Try a Boost")
                .w_full()
                .max_w(px(room.max(0.0) + 16.0 * s))
                .overflow_hidden()
                .flex()
                .flex_nowrap()
                .justify_start()
                .px(px(8.0 * s))
                .pb(px(6.0 * s))
                .items_center()
                .gap(px(6.0 * s))
                .text_size(px(13.0 * s))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .whitespace_nowrap()
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
                        .flex_shrink_0()
                        .whitespace_nowrap()
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
                .children(more)
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
