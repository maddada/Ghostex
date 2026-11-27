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
    AnyElement, AppContext as _, Bounds, Context, Focusable as _, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div, px, relative, rgb, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
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
/// The command type of the chat box right-click menu's Cut, Copy, Paste and Select All rows.
pub(super) const COMPOSER_EDIT_COMMAND: &str = "composerEdit";
/// Toggle key in `menu_toggle.rs`: a second right press while the chat box's menu is up only
/// closes it.
const COMPOSER_MENU_TRIGGER: &str = "chat-composer-menu";
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
/// The filter field at the menu's top (the Files and links filter's height) and the line that
/// says nothing matches.
const FILTER_HEIGHT: f32 = 30.0;
const EMPTY_HEIGHT: f32 = 34.0;
/// The key context around the filter field. The field's own bindings (Up, Down, Enter, Escape)
/// sit as deep as these and were bound first, so these, bound later, reach the menu first; letters
/// are not bound, so they type (the model picker's search field does the same, model_menu/keys.rs).
const FILTER_KEY_CONTEXT: &str = "ChatBoostFilter";

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct BoostFilterKey {
    key: String,
}

struct BoostFilterKeysBound;
impl gpui::Global for BoostFilterKeysBound {}

/// Binds the filter field's Up, Down, Enter and Escape to the menu, once.
fn register_filter_keys(cx: &mut gpui::App) {
    if cx.has_global::<BoostFilterKeysBound>() {
        return;
    }
    cx.set_global(BoostFilterKeysBound);
    let context = format!("{FILTER_KEY_CONTEXT} > Input");
    let bindings: Vec<_> = ["up", "down", "ctrl-p", "ctrl-n", "enter", "escape"]
        .into_iter()
        .map(|key| {
            crate::app::hotkeys::gpui_runtime_key_binding(
                key,
                BoostFilterKey { key: key.into() },
                Some(&context),
                cx,
            )
        })
        .collect();
    cx.bind_keys(bindings);
}

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
        self.boost_sections_matching("")
    }

    /// The sections with only the Boosts the filter field keeps; a section with none left goes.
    fn boost_sections_matching(&self, query: &str) -> Vec<(String, String, Vec<Value>)> {
        let Some(boosts) = self.boosts() else {
            return Vec::new();
        };
        let list: Vec<Value> = boosts["list"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| {
                ghostex_gx_chat_core::extras::boosts::menu_filter_matches(
                    query,
                    &text(row, "label"),
                    &text(row, "description"),
                )
            })
            .cloned()
            .collect();
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
        self.boost_menu_closed();
        self.boost_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        self.invoke(json!({"type": "refreshBoosts"}), cx);
        self.toggle_pill_menu(BOOST_MENU, pill, window, cx);
    }

    /// The menu went away: the filter field goes with the window it was made in, and the next
    /// opening starts with every Boost.
    pub(super) fn boost_menu_closed(&mut self) {
        self.boost_preview = None;
        self.boost_filter = None;
        self.boost_filter_subscription = None;
        self.boost_filter_window = None;
        self.boost_filter_query.clear();
    }

    /// The filter field, made in the menu's own window the first time the menu draws there and
    /// focused, so typing filters straight away.
    fn sync_boost_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = window.window_handle().window_id();
        if self.boost_filter.is_some() && self.boost_filter_window == Some(here) {
            return;
        }
        register_filter_keys(cx);
        self.boost_filter_window = Some(here);
        let query = self.boost_filter_query.clone();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Filter Boosts")
                .default_value(query)
        });
        self.boost_filter_subscription = Some(cx.subscribe_in(
            &input,
            window,
            |chat, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    if query != chat.boost_filter_query {
                        chat.boost_filter_query = query;
                        chat.boost_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
                        // The first match is highlighted, so Enter adds it.
                        match chat.boost_filtered_rows().first() {
                            Some((_, id)) => {
                                let id = id.clone();
                                chat.preview_boost(&id, window, cx);
                            }
                            None => chat.hide_boost_preview(cx),
                        }
                        cx.notify();
                    }
                }
            },
        ));
        // Key presses reach the menu only through its window's native keyboard owner
        // (native_chat/focus.rs), as the model picker's search field does.
        super::focus::reclaim_keyboard_focus(window);
        input.read(cx).focus_handle(cx).focus(window, cx);
        self.boost_filter = Some(input);
    }

    /// The rows the filter keeps, in the menu's order: each one's place among the list's children
    /// (a section's header comes before its rows) and its Boost.
    fn boost_filtered_rows(&self) -> Vec<(usize, String)> {
        let mut rows = Vec::new();
        let mut child = 0;
        for (_, _, section) in self.boost_sections_matching(&self.boost_filter_query) {
            child += 1;
            for row in section {
                rows.push((child, text(&row, "id")));
                child += 1;
            }
        }
        rows
    }

    /// Up, Down, Enter and Escape in the filter field: Up and Down move the highlight through the
    /// rows the filter keeps (stopping at either end), Enter adds the highlighted Boost (or takes
    /// it out, as a click does), and Escape empties the field first, then closes the menu.
    ///
    /// CDXC:Boosts 2026-11-27 DECISION:
    /// User: "when i open the boosts menu please show a text box up top that lets me type to
    /// filter to reach a specific one". The field sits at the menu's top, focused on opening, in
    /// the Files and links filter's look; it filters every section; the highlight is the hovered
    /// row's, so the pointer and the keys share one, and the preview follows it.
    fn boost_filter_key(
        &mut self,
        action: &BoostFilterKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let rows = self.boost_filtered_rows();
        let current = self
            .boost_preview
            .as_deref()
            .and_then(|id| rows.iter().position(|(_, row)| row == id));
        match action.key.as_str() {
            "escape" => {
                if self.boost_filter_query.is_empty() {
                    self.close_pill_menu(cx);
                } else if let Some(input) = self.boost_filter.clone() {
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                    self.boost_filter_query.clear();
                    self.hide_boost_preview(cx);
                    self.boost_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
                }
            }
            "up" | "ctrl-p" | "down" | "ctrl-n" if !rows.is_empty() => {
                let up = matches!(action.key.as_str(), "up" | "ctrl-p");
                let next = match current {
                    None if up => rows.len() - 1,
                    None => 0,
                    Some(index) if up => index.saturating_sub(1),
                    Some(index) => (index + 1).min(rows.len() - 1),
                };
                let (child, id) = rows[next].clone();
                self.boost_scroll.scroll_to_item(child);
                self.preview_boost(&id, window, cx);
            }
            "enter" => {
                if let Some((_, id)) = current
                    .or((!rows.is_empty()).then_some(0))
                    .map(|index| rows[index].clone())
                {
                    let draft = self.draft.clone();
                    self.invoke(json!({"type": "toggleBoost", "id": id, "text": draft}), cx);
                    self.close_pill_menu(cx);
                }
            }
            _ => {}
        }
        cx.notify();
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
        Some(FILTER_HEIGHT + 6.0 + list.min(MENU_MAX_HEIGHT) + 6.0 + FOOTER_HEIGHT)
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.boosts()?;
        self.sync_boost_filter(window, cx);
        let sections = self.boost_sections_matching(&self.boost_filter_query);
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
        if children.is_empty() {
            children.push(
                div()
                    .id("boost-menu-no-match")
                    .flex_shrink_0()
                    .h(px(EMPTY_HEIGHT * s))
                    .flex()
                    .items_center()
                    .px(px(10.0 * s))
                    .text_size(px(13.0 * s))
                    .text_color(p.card_muted)
                    .child("No Boosts match")
                    .into_any_element(),
            );
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
        // The Files and links menu's filter field (files_links.rs), in the menu's own colours.
        let filter = self.boost_filter.clone().map(|input| {
            div()
                .key_context(FILTER_KEY_CONTEXT)
                .on_action(cx.listener(Self::boost_filter_key))
                .w_full()
                .flex_shrink_0()
                .h(px(FILTER_HEIGHT * s))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .rounded(px(8.0 * s))
                .border_1()
                .border_color(p.input_border)
                .bg(p.composer_background)
                .px(px(10.0 * s))
                .child(
                    svg()
                        .path("titlebar/search.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&input)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .p_0()
                            .text_size(px(13.0 * s))
                            .text_color(p.primary)
                            .placeholder_color(p.muted.opacity(0.6))
                            .caret_color(p.foreground),
                    ),
                )
        });
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
                .children(filter)
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

    /// No row is highlighted any more (nothing matches the filter, or it was emptied): the
    /// preview card beside the menu goes rather than standing empty.
    fn hide_boost_preview(&mut self, cx: &mut Context<Self>) {
        self.boost_preview = None;
        if let Some(menu) = self.option_menu.clone() {
            menu.update(cx, |menu, cx| menu.close_side_panel(cx));
        }
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

    /// What Save as Boost would save from words selected in another of the chat's prompt boxes
    /// (the Reply popover): the same rule as the chat box's.
    pub(super) fn selection_for_boost(&self, selected: &str) -> Option<String> {
        self.boosts()?;
        Some(selected.trim())
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
    ///
    /// CDXC:SessionChat 2026-11-26 DECISION: User, of the chat box's right-click menu drawn as the
    /// system menu: "in chat box this conext menu is bad pls gpui one like the one we use in the
    /// transcript". The field's own menu stays empty, so no system menu opens on any platform;
    /// the chat's drawn menu (`show_chat_menu_at`, the transcript's) opens at the pointer instead,
    /// with the same rows and rules: Cut and Copy only with a selection, Paste always, Select All,
    /// then Save as Boost while words are selected. Its rows run the field's own Cut, Copy, Paste
    /// and Select All actions (`composer_edit_command`), as the system menu's did.
    pub(super) fn composer_context_menu(
        chat: gpui::WeakEntity<Self>,
    ) -> impl Fn(
        gpui_component::native_menu::NativeMenu,
        &mut Window,
        &mut gpui::App,
    ) -> gpui_component::native_menu::NativeMenu
    + 'static {
        move |menu, window, cx| {
            if let Some(chat) = chat.upgrade() {
                chat.update(cx, |chat, cx| chat.show_composer_menu(window, cx));
            }
            menu
        }
    }

    /// Open the chat box's drawn right-click menu at the pointer (`composer_context_menu`).
    fn show_composer_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = window.mouse_position();
        // A right click on a Boost pill opens the pill's own menu (Expand, Edit Boost, Remove;
        // reference_menu.rs) instead.
        if self.boost_pill_at(at, cx) {
            return;
        }
        let has_selection = self
            .input
            .as_ref()
            .is_some_and(|input| !input.read(cx).selected_range().is_empty());
        let row = |label: &str, icon: &str, action: &str, disabled: bool| {
            json!({
                "label": label,
                "iconPath": icon,
                "disabled": disabled,
                "command": {"type": COMPOSER_EDIT_COMMAND, "action": action},
            })
        };
        // Paste is offered without peeking at the clipboard, as GPUI Kit's own edit menu does.
        let mut rows = vec![
            row("Cut", "titlebar/scissors.svg", "cut", !has_selection),
            row("Copy", "titlebar/copy.svg", "copy", !has_selection),
            row("Paste", "titlebar/clipboard.svg", "paste", false),
            json!({"separator": true}),
            row("Select All", "titlebar/select-all.svg", "selectAll", false),
        ];
        if let Some(text) = self.composer_selection_for_boost(cx) {
            rows.push(json!({"separator": true}));
            rows.push(json!({
                "label": "Save as Boost",
                "iconPath": "titlebar/sparkles.svg",
                "command": {"type": "saveAsBoost", "text": text},
            }));
        }
        if !self.chat_menu_toggled_shut(COMPOSER_MENU_TRIGGER, cx) {
            self.show_chat_menu_at(rows, at, 240.0, window, cx);
        }
    }

    /// A pick of Cut, Copy, Paste or Select All in the chat box's right-click menu: the field's
    /// own action, dispatched from the field as a key binding or the system menu would.
    pub(super) fn composer_edit_command(
        &mut self,
        command: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.input.clone() else {
            return;
        };
        let focus = gpui::Focusable::focus_handle(input.read(cx), cx);
        focus.focus(window, cx);
        let action: Box<dyn gpui::Action> = match command["action"].as_str() {
            Some("cut") => Box::new(gpui_component::input::Cut),
            Some("copy") => Box::new(gpui_component::input::Copy),
            Some("paste") => Box::new(gpui_component::input::Paste),
            Some("selectAll") => Box::new(gpui_component::input::SelectAll),
            _ => return,
        };
        // Deferred: this runs while the chat is being updated, and the chat's own Cut and Copy
        // handlers (edit_shortcuts.rs) and paste hook update it again on the way up.
        window.defer(cx, move |window, cx| {
            focus.dispatch_action(action.as_ref(), window, cx);
        });
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
        // `web` is Browser use: the user's own browser, with OpenCode Browse's agent tabs in it.
        "page" | "app" | "web" | "browser" => div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar(Some(match kind {
                "app" => "Finder",
                "browser" => "ghostex browser",
                "web" => "your Chrome · agent tab",
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
                                    tile()
                                        .when(n == 1 && matches!(kind, "browser" | "web"), |tile| {
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
        // Options: a Live page of options side by side, the recommended pick marked, and a second
        // question under the first, since it asks one question per kind of choice.
        "compare" => div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar(Some("options.html")))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(5.0 * s))
                    .p(px(7.0 * s))
                    .child(div().flex_1().min_h_0().flex().gap(px(5.0 * s)).children(
                        ["A", "B", "C"].into_iter().map(|label| {
                            let pick = label == "B";
                            tile()
                                .when(pick, |tile| tile.border(px(s)).border_color(tone))
                                .child(small(label, p.foreground))
                                .child(line(0.8, 4.0, strong))
                                .child(div().flex_1())
                                .when(pick, |tile| tile.child(small("Pick", tone)))
                        }),
                    ))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(4.0 * s))
                            .child(line(0.22, 4.0, strong))
                            .children([true, false].into_iter().map(|on| {
                                div()
                                    .px(px(5.0 * s))
                                    .rounded(px(3.0 * s))
                                    .when(on, |chip| chip.bg(p.foreground.opacity(0.85)))
                                    .when(!on, |chip| chip.border(px(s)).border_color(strong))
                                    .child(small(
                                        if on { "Monthly" } else { "Yearly" },
                                        if on { p.background } else { p.card_muted },
                                    ))
                            })),
                    ),
            )
            .into_any_element(),
        "triage" => div()
            .size_full()
            .flex()
            .flex_col()
            .child(bar(Some("Live page")))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .gap(px(5.0 * s))
                    .p(px(6.0 * s))
                    .children(
                        [
                            ("Requests", "PR #412", "Review"),
                            ("Messages", "Urgent · Launch DM", "Suggested reply"),
                        ]
                        .into_iter()
                        .map(|(section, item, action)| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(3.0 * s))
                                .child(small(section, p.card_muted))
                                .child(
                                    tile()
                                        .child(small(item, p.foreground))
                                        .child(line(0.72, 4.0, strong))
                                        .child(small(action, tone)),
                                )
                        }),
                    ),
            )
            .into_any_element(),
        "second" => div()
            .size_full()
            .flex()
            .gap(px(5.0 * s))
            .p(px(7.0 * s))
            .children([false, true].into_iter().map(|pick| {
                tile()
                    .child(small(
                        if pick { "Second agent" } else { "Your agent" },
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
                                if pick { "Checked" } else { "Answer" },
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
        // Add to Pings, Inbox it! and Schedule it: the cards each leaves.
        "needs-you" | "inbox" | "schedule" => div()
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(5.0 * s))
            .p(px(8.0 * s))
            .children(
                match kind {
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
