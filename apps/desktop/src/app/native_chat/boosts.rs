//! Boosts in the chat (gx-chat-core `extras/boosts.rs` has the user's decisions and the rules): the
//! Boost pill at the right of the pill strip; its menu, one 460px list in Show, Actions, Work and
//! Custom sections with section chips and Edit at its bottom, and a preview card beside it; the
//! one-time hint chip inside the composer's top-right corner on a new chat; the chat box Boost
//! pills' hover, double click (expand into the prompt's text) and right-click menu (Expand, Edit
//! Boost, Remove); and Save as Boost. Editing is Settings > Boosts
//! (window/settings_modal/tabs/boosts.rs), which the menu's Edit button, a pill's Edit Boost and
//! Save as Boost open.
//!
//! The core's `boosts` document key holds it all; `null` (an older gxserver, no answer yet, or
//! Boosts switched off) draws none of it.
//!
//! CDXC:Boosts 2026-11-20 WHY:
//! The pill rides in its own key rather than `chatPills` because the phone draws every `chatPills`
//! entry and opens row menus only for the pills it knows. The pill sits after the panel pills,
//! before Links, and never folds into "+N" (it is the one way in). The preview is a panel of the
//! menu one level down (option_menu's side panel, as the model menu's lists are), so it opens beside
//! the menu on the side with room.
//! CDXC:Boosts 2026-11-24 DECISION:
//! The one-time Boost hint sits inside the composer at its top-right corner as a small dismissible
//! chip with an x (`boost_hint_chip`), superseding the floating frosted callout window.
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
/// The preview card's content height: its title, the picture, the description (up to
/// three lines) and what a click does.
const PREVIEW_HEIGHT: f32 = 228.0;
/// The menu's parts (unscaled): a section header, a row (its padding, the name's line and the
/// one-line description), the gap between rows, and the bottom bar of section chips and Edit.
const SECTION_HEIGHT: f32 = 26.0;
const ROW_PAD_Y: f32 = 7.0;
const LABEL_LINE: f32 = 18.0;
const DETAIL_LINE: f32 = 16.0;
const ROW_GAP: f32 = 2.0;
const FOOTER_HEIGHT: f32 = 40.0;
/// The tallest the list gets before it scrolls, so the menu never stands taller than this.
const MENU_MAX_HEIGHT: f32 = 420.0;

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
/// then what a double click and a right click do (`extras::boosts::PILL_HINT`).
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
        .unwrap_or_else(|| {
            format!(
                "Your Boost\n\n{}",
                ghostex_gx_chat_core::extras::boosts::PILL_HINT
            )
        })
}

/// The Boost a chat box pill's link names, for its double click and right-click menu.
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

    /// The prompt a Boost sends from this computer (the user's edit or Ghostex's own), from the
    /// core's `boosts` list; `None` for a Boost that is gone or while Boosts are off.
    fn boost_prompt(&self, id: &str) -> Option<String> {
        self.boosts()?["list"]
            .as_array()?
            .iter()
            .find(|row| row["id"] == id)
            .map(|row| text(row, "prompt"))
            .or_else(|| {
                ghostex_gx_chat_core::extras::boosts::boost(id)
                    .map(|boost| boost.prompt.to_string())
            })
            .filter(|prompt| !prompt.trim().is_empty())
    }

    /// A chat box Boost pill's double click, or Expand in its right-click menu: the pill becomes
    /// its prompt as ordinary text (`extras::boosts::expand_pill`). It goes through the field as
    /// one replacement, so the field's own Cmd+Z (Ctrl+Z) right after puts the pill back.
    pub(super) fn expand_boost_pill(
        &mut self,
        range: std::ops::Range<usize>,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.boost_prompt(id) else {
            return;
        };
        if let Some((draft, caret)) =
            ghostex_gx_chat_core::extras::boosts::expand_pill(&self.draft, range, &prompt)
        {
            self.insert_prompt(&draft, cx);
            self.input_caret = Some(caret);
        }
    }

    /// The right-click menu on a chat box Boost pill at `range`: Expand, Edit Boost and Remove,
    /// each command carrying the pill's range so its pick acts on that pill
    /// (`boost_pill_command`).
    pub(super) fn boost_pill_menu_rows(
        &self,
        range: &std::ops::Range<usize>,
        id: &str,
    ) -> Vec<Value> {
        let can_expand = self.boost_prompt(id).is_some();
        ghostex_gx_chat_core::extras::boosts::pill_menu_rows(id, can_expand)
            .into_iter()
            .map(|mut row| {
                row["command"]["start"] = json!(range.start);
                row["command"]["end"] = json!(range.end);
                row
            })
            .collect()
    }

    /// Whether `position` is over a Boost pill in the chat box.
    pub(super) fn boost_pill_at(&self, position: gpui::Point<Pixels>, cx: &gpui::App) -> bool {
        let Some(range) = self
            .input
            .as_ref()
            .and_then(|input| input.read(cx).inline_replacement_at(position))
        else {
            return false;
        };
        self.composer_references
            .iter()
            .any(|reference| reference.range == range && reference.kind == "boost")
    }

    /// A pick in a Boost pill's right-click menu. The pill must still sit at the range the menu
    /// was opened on; an edit to the draft since then leaves the pick doing nothing.
    pub(super) fn boost_pill_command(&mut self, command: &Value, cx: &mut Context<Self>) {
        let id = command["id"].as_str().unwrap_or_default().to_string();
        let (Some(start), Some(end)) = (command["start"].as_u64(), command["end"].as_u64()) else {
            return;
        };
        let range = start as usize..end as usize;
        match command["action"].as_str().unwrap_or_default() {
            "expand" => self.expand_boost_pill(range, &id, cx),
            "edit" => self.open_boost_settings(Some(format!("boost:{id}")), None, cx),
            "remove" => {
                if let Some((draft, caret)) =
                    ghostex_gx_chat_core::extras::boosts::remove_pill(&self.draft, range)
                {
                    self.insert_prompt(&draft, cx);
                    self.input_caret = Some(caret);
                }
            }
            _ => {}
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

    /// The Boost pill: the sparkles and "Boost" in the neutral style of the other strip pills,
    /// with a subtle neutral fill while the message has a Boost or its menu is open. A click opens
    /// the menu against it.
    ///
    /// CDXC:Boosts 2026-11-24 DECISION:
    /// User: "please color this like the other pills, pill shouldn't be yellow". The strip pill
    /// uses the exact same neutral fill, border, text and icon colours as the panel pills beside
    /// it (`chat_pills.rs` `chat_pill`), across dark, light and all built-in themes. Supersedes
    /// the gold accent on the strip pill from 2026-11-20; Boost chips inside messages keep gold.
    pub(super) fn boost_strip_pill(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let boosts = self.boosts()?;
        let pill = &boosts["pill"];
        let s = p.scale;
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
                .h(px(super::chat_pills::PILL_HEIGHT * s))
                .px(px(8.0 * s))
                .py(px(2.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(p.foreground.opacity(0.1))
                .text_size(px(13.0 * s))
                .line_height(px(19.0 * s))
                .whitespace_nowrap()
                .text_color(p.card_muted)
                .chat_cursor_pointer()
                .when(active || open, |pill| pill.bg(p.foreground.opacity(0.08)))
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .when(!open, |pill| {
                    pill.tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new(
                            "Boosts (*): ask for a Live page, chart, diagram, or browser run in one click",
                        )
                        .build(window, cx)
                    })
                })
                .child(
                    svg()
                        .path("titlebar/sparkles.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.card_muted),
                )
                .child(text(pill, "label"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_boost_menu(bounds.get(), window, cx);
                }))
                .child(
                    canvas(
                        move |bounds, _, _| {
                            measured.set(bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset(px(-s)),
                )
                .into_any_element(),
        )
    }

    /// The one-time Boost suggestion chip inside the composer's top-right corner: small gold pill
    /// with sparkles, label, and an x. Clicking the chip opens Boosts and dismisses it; clicking
    /// the x dismisses it. Reuses `boosts.hint` and `dismissBoostHint`.
    pub(super) fn boost_hint_chip(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.collab_guest() || self.list.item_count() > 0 || self.pending_send {
            return None;
        }
        let boosts = self.boosts()?;
        let hint = &boosts["hint"];
        if !boost_hint_visible(hint) {
            return None;
        }
        let s = p.scale;
        let tone = accent(p.light);
        let title = text(hint, "title");
        let body = text(hint, "text");
        let label = if !title.trim().is_empty() {
            title.clone()
        } else {
            "Try Boosts".to_string()
        };
        let aria = if !body.trim().is_empty() {
            format!("{label}. {body}")
        } else {
            label.clone()
        };
        let tip = if !body.trim().is_empty() {
            body
        } else {
            "Open Boosts".to_string()
        };
        let chip_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
        let measured = chip_bounds.clone();
        let pill_bounds = self.boost_pill_bounds.clone();
        Some(
            div()
                .id("boost-hint-chip")
                .relative()
                .role(gpui::Role::Button)
                .aria_label(aria)
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(24.0 * s))
                .gap(px(5.0 * s))
                .pl(px(8.0 * s))
                .pr(px(4.0 * s))
                .rounded_full()
                .border_1()
                .border_color(tone.opacity(0.38))
                .bg(tone.opacity(0.1))
                .text_size(px(12.0 * s))
                .line_height(px(16.0 * s))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .text_color(tone)
                .chat_cursor_pointer()
                .hover(move |style| {
                    style
                        .bg(tone.opacity(0.16))
                        .border_color(tone.opacity(0.55))
                })
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .child(
                    svg()
                        .path("titlebar/sparkles.svg")
                        .size(px(13.0 * s))
                        .flex_shrink_0()
                        .text_color(tone),
                )
                .child(label)
                .child(
                    div()
                        .id("boost-hint-chip-dismiss")
                        .role(gpui::Role::Button)
                        .aria_label("Dismiss Boosts hint")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(18.0 * s))
                        .rounded_full()
                        .text_color(tone.opacity(0.75))
                        .hover(move |style| style.bg(tone.opacity(0.2)).text_color(tone))
                        .child(
                            svg()
                                .path("titlebar/x.svg")
                                .size(px(11.0 * s))
                                .text_color(tone.opacity(0.8)),
                        )
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.invoke(json!({"type": "dismissBoostHint"}), cx);
                        })),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    let pill = pill_bounds.get();
                    let anchor = if pill.size.width > px(0.0) {
                        pill
                    } else {
                        chip_bounds.get()
                    };
                    this.open_boost_menu(anchor, window, cx);
                }))
                .child(
                    canvas(
                        move |bounds, _, _| {
                            measured.set(bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element(),
        )
    }

    /// Opens (or shuts) the Boost menu against the pill, and reads the Boosts again: Settings may
    /// have changed them since this chat opened. Opening it also dismisses the first-time hint.
    pub(super) fn open_boost_menu(
        &mut self,
        pill: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .boosts()
            .is_some_and(|boosts| boost_hint_visible(&boosts["hint"]))
        {
            self.invoke(json!({"type": "dismissBoostHint"}), cx);
        }
        self.boost_preview = None;
        self.invoke(json!({"type": "refreshBoosts"}), cx);
        self.toggle_pill_menu(BOOST_MENU, pill, window, cx);
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
    /// click scrolls to that section; the one at the top of the list is lit) and Edit on the
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
                let edit_id = id.clone();
                let custom = row["custom"] == true;
                let group = format!("boost-row-group-{id}");
                let draft = draft.clone();
                children.push(
                    div()
                        .id(gpui::ElementId::Name(format!("boost-row-{id}").into()))
                        .group(group.clone())
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
                        .when(custom, |row| {
                            // CDXC:Boosts 2026-11-21 DECISION: User: hovering a custom Boost row
                            // shows a small Edit button at its right end, level with the name's
                            // line; it opens that Boost's editor in Settings > Boosts without
                            // putting the Boost in the message. Built-in Boosts have no editor.
                            row.child(
                                div()
                                    .id(gpui::ElementId::Name(
                                        format!("boost-row-edit-{edit_id}").into(),
                                    ))
                                    .role(gpui::Role::Button)
                                    .aria_label("Edit this Boost")
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(px(4.0 * s))
                                    .h(px(LABEL_LINE * s))
                                    .px(px(6.0 * s))
                                    .rounded(px(5.0 * s))
                                    .text_size(px(12.0 * s))
                                    .text_color(p.card_muted)
                                    .chat_cursor_pointer()
                                    .opacity(0.0)
                                    .group_hover(group.clone(), |button| button.opacity(1.0))
                                    .hover(|style| {
                                        style.text_color(p.foreground).bg(p.foreground.opacity(0.1))
                                    })
                                    .child(
                                        svg()
                                            .path("titlebar/pencil.svg")
                                            .size(px(12.0 * s))
                                            .flex_shrink_0()
                                            .text_color(p.card_muted),
                                    )
                                    .child("Edit")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.open_boost_settings(
                                            Some(format!("boost:{edit_id}")),
                                            None,
                                            cx,
                                        );
                                    })),
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
        let chips =
            div()
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .children(sections.iter().enumerate().map(|(index, (id, label, _))| {
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
                }));
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
            .hover(|style| {
                style
                    .text_color(p.foreground)
                    .bg(p.foreground.opacity(0.06))
            })
            .child(
                svg()
                    .path("titlebar/settings.svg")
                    .size(px(14.0 * s))
                    .text_color(p.card_muted),
            )
            .child("Edit")
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
        move |menu, window, cx| {
            let Some(chat) = chat.upgrade() else {
                return menu;
            };
            let chat = chat.read(cx);
            // A right click on a Boost pill opens the pill's own menu (Expand, Edit Boost,
            // Remove; reference_menu.rs) instead: an empty menu here leaves the field's closed.
            if chat.boost_pill_at(window.mouse_position(), cx) {
                return menu;
            }
            let has_selection = chat
                .input
                .as_ref()
                .is_some_and(|input| !input.read(cx).selected_range().is_empty());
            let boost = chat.composer_selection_for_boost(cx);
            // Paste is offered without peeking at the clipboard, as GPUI Kit's own edit menu does.
            let menu = menu
                .menu_with_disabled("Cut", !has_selection, Box::new(gpui_component::input::Cut))
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

/// Whether `hint` carries visible copy (`title` or `text`). Never draw an empty hint chip.
pub(super) fn boost_hint_visible(hint: &Value) -> bool {
    hint.is_object()
        && (!text(hint, "title").trim().is_empty() || !text(hint, "text").trim().is_empty())
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
        // A Live page with the charts on it, as Chart it asks for.
        "chart" => div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar(Some("signups.html")))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
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
        // Ship it: the commit, its branch and the pull request it opens, top to bottom.
        "ship" => div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(3.0 * s))
            .p(px(7.0 * s))
            .children(
                [
                    ("Commit", false),
                    ("boost/onboarding", false),
                    ("PR #418", true),
                ]
                .into_iter()
                .enumerate()
                .flat_map(|(n, (label, last))| {
                    let chip = div()
                        .flex_none()
                        .px(px(6.0 * s))
                        .py(px(3.0 * s))
                        .rounded(px(5.0 * s))
                        .bg(soft)
                        .border(px(s))
                        .border_color(if last { tone } else { strong })
                        .child(small(label, p.foreground))
                        .into_any_element();
                    let link = (n > 0).then(|| {
                        div()
                            .w(px(1.0))
                            .h(px(6.0 * s))
                            .flex_none()
                            .bg(strong)
                            .into_any_element()
                    });
                    link.into_iter().chain(std::iter::once(chip))
                }),
            )
            .into_any_element(),
        // Triage messages, Add to Pings, Inbox it! and Schedule it: the cards each leaves.
        "triage-messages" | "needs-you" | "inbox" | "schedule" => div()
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(5.0 * s))
            .p(px(8.0 * s))
            .children(
                match kind {
                    "triage-messages" => vec![
                        ("DM: Launch timeline", "Urgent · suggested reply", "Reply"),
                        ("@you: Review copy", "Today · unanswered ask", "Reply"),
                    ],
                    "needs-you" => vec![(
                        "Check billing on mobile",
                        "Verify totals before release",
                        "Done",
                    )],
                    "inbox" => vec![(
                        "Why does CI fail on main?",
                        "A race in the token refresh",
                        "Done",
                    )],
                    _ => vec![
                        ("Check the deploy", "Today 15:30", "Once"),
                        ("Weekly usage report", "Mondays 09:00", "Repeats"),
                    ],
                }
                .into_iter()
                .map(|(title, line_two, chip)| {
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .px(px(7.0 * s))
                        .py(px(5.0 * s))
                        .rounded(px(6.0 * s))
                        .bg(soft)
                        .border(px(s))
                        .border_color(strong)
                        .child(
                            svg()
                                .path(match kind {
                                    "triage-messages" => "titlebar/message-circle.svg",
                                    "needs-you" | "inbox" => "titlebar/circle-dot.svg",
                                    _ => "titlebar/clock.svg",
                                })
                                .flex_none()
                                .size(px(12.0 * s))
                                .text_color(if matches!(kind, "needs-you" | "inbox") {
                                    gpui::rgb(crate::app::native_sidebar::status::QUESTION_COLOR)
                                        .into()
                                } else {
                                    tone
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(small(title, p.foreground))
                                .child(small(line_two, p.card_muted)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .px(px(4.0 * s))
                                .rounded(px(3.0 * s))
                                .border(px(s))
                                .border_color(strong)
                                .child(small(chip, p.card_muted)),
                        )
                }),
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
