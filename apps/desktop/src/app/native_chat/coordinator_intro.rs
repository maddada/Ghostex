//! "How to use coordinators": the panel the coordinator tips' pill opens over the chat box, a chat
//! menu window (option_menu/pill_menu.rs) like the other pill menus. Its text comes from the core
//! (`coordinatorIntro`, packages/gx-chat-core/src/extras/coordinator_intro.rs, where the user's
//! decision lives): each tip is its own card in the Settings style (an icon tile, a 14px title and a
//! 13px muted line, Settings > fields/row.rs), with a line to type that a click puts in the chat
//! box. "Don't show again" is a checkbox that applies when the panel closes: the core hides the pill
//! and tells gxserver, which drops it from every coordinator.
//!
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/CoordinatorIntro.tsx (the phone's sheet).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

/// The pill's id among the pill menus, and its menu-toggle key (chat_pills.rs).
pub(super) const INTRO_MENU: &str = ghostex_gx_chat_core::extras::coordinator_intro::INTRO_PILL;
pub(super) const INTRO_TRIGGER: &str = "chat-pill-coordinator-intro";
/// The panel's width, and its height cap (unscaled); the tips scroll inside.
pub(super) const INTRO_MENU_WIDTH: f32 = 480.0;
const INTRO_MENU_MAX: f32 = 620.0;
/// Settings' icon tile (fields/row.rs `icon_tile`): 34px, a 9px radius, 14px to the text.
const TILE: f32 = 34.0;
const TILE_GAP: f32 = 14.0;
/// A card's padding and the gap between cards.
const CARD_PAD: f32 = 14.0;
const CARD_GAP: f32 = 8.0;
/// The lede above the cards, and the footer under them (the checkbox line and its note).
const LEDE_GAP: f32 = 10.0;
const FOOTER_HEIGHT: f32 = 62.0;
/// Line heights, and the width the card texts wrap in with their average character widths.
const TITLE_LINE: f32 = 19.0;
const DETAIL_LINE: f32 = 18.0;
const EXAMPLE_HEIGHT: f32 = 28.0;
const TEXT_WIDTH: f32 = INTRO_MENU_WIDTH - 2.0 * CARD_PAD - TILE - TILE_GAP - 16.0;
const TITLE_CHAR: f32 = 7.4;
const DETAIL_CHAR: f32 = 6.6;

fn lines(text: &str, char_width: f32, width: f32) -> f32 {
    (text.chars().count() as f32 * char_width / width)
        .ceil()
        .max(1.0)
}

fn tips(intro: &Value) -> Vec<Value> {
    intro["tips"].as_array().cloned().unwrap_or_default()
}

impl NativeChatView {
    /// The panel's height (unscaled), from what each card draws; `None` once the tips are gone.
    pub(super) fn coordinator_intro_menu_height(&self) -> Option<f32> {
        let intro = &self.snapshot["coordinatorIntro"];
        if !intro.is_object() {
            return None;
        }
        let cards: f32 = tips(intro)
            .iter()
            .map(|tip| {
                2.0 * CARD_PAD
                    + TITLE_LINE * lines(&text(tip, "title"), TITLE_CHAR, TEXT_WIDTH)
                    + 2.0
                    + DETAIL_LINE * lines(&text(tip, "detail"), DETAIL_CHAR, TEXT_WIDTH)
                    + 10.0
                    + EXAMPLE_HEIGHT
                    + CARD_GAP
            })
            .sum();
        let lede = DETAIL_LINE * lines(&text(intro, "lede"), DETAIL_CHAR, INTRO_MENU_WIDTH - 16.0)
            + LEDE_GAP;
        Some(
            (super::chat_pills::MENU_HEADER_HEIGHT
                + super::chat_pills::MENU_SECTION_GAP * 2.0
                + lede
                + cards
                + FOOTER_HEIGHT)
                .min(INTRO_MENU_MAX),
        )
    }

    /// The panel: the header, the lede and the tip cards (scrolling inside), then "Don't show
    /// again".
    pub(super) fn coordinator_intro_menu_body(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let intro = self.snapshot["coordinatorIntro"].clone();
        if !intro.is_object() {
            return None;
        }
        let s = p.scale;
        let header = super::chat_pills::pill_menu_header(
            "titlebar/bulb.svg",
            ghostex_gx_chat_core::extras::coordinator_intro::TITLE,
            text(&intro, "exampleHint"),
            None,
            p,
        );
        let cards: Vec<AnyElement> = tips(&intro)
            .iter()
            .enumerate()
            .map(|(index, tip)| self.intro_tip_card(index, tip, p, cx))
            .collect();
        let list = div()
            .id("chat-coordinator-intro-list")
            .flex()
            .flex_col()
            .gap(px(CARD_GAP * s))
            .max_h(px((INTRO_MENU_MAX
                - super::chat_pills::MENU_HEADER_HEIGHT
                - super::chat_pills::MENU_SECTION_GAP * 2.0
                - FOOTER_HEIGHT)
                * s))
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(4.0 * s))
                    .pb(px((LEDE_GAP - CARD_GAP).max(0.0) * s))
                    .text_size(px(13.0 * s))
                    .line_height(px(DETAIL_LINE * s))
                    .text_color(p.card_muted)
                    .child(text(&intro, "lede")),
            )
            .children(cards)
            .into_any_element();
        let footer = self.intro_footer(&intro, p, cx);
        Some(super::chat_pills::pill_menu_column(
            "chat-coordinator-intro-menu",
            ghostex_gx_chat_core::extras::coordinator_intro::TITLE,
            header,
            Some(list),
            Some(footer),
            p,
        ))
    }

    /// One tip as a Settings-style card: its icon tile, its title and line, and the example to type.
    fn intro_tip_card(
        &self,
        index: usize,
        tip: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let example = text(tip, "example");
        let title = text(tip, "title");
        let tile = div()
            .flex_shrink_0()
            .size(px(TILE * s))
            .rounded(px(9.0 * s))
            .border(px(s))
            .border_color(p.border)
            .bg(p.foreground.opacity(0.035))
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path(format!("titlebar/{}.svg", text(tip, "icon")))
                    .size(px(17.0 * s))
                    .text_color(p.card_muted),
            );
        let use_example = {
            let example = example.clone();
            div()
                .id(("chat-coordinator-intro-example", index))
                .role(gpui::Role::Button)
                .aria_label(format!("Put in the chat box: {example}"))
                .mt(px(10.0 * s))
                .flex()
                .items_center()
                .gap(px(7.0 * s))
                .px(px(9.0 * s))
                .h(px(EXAMPLE_HEIGHT * s))
                .rounded(px(7.0 * s))
                .bg(p.foreground.opacity(0.045))
                .hover(|style| style.bg(p.foreground.opacity(0.085)))
                .chat_cursor_pointer()
                .child(
                    svg()
                        .path("titlebar/arrow-down-left.svg")
                        .size(px(12.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.5 * s))
                        .text_color(p.foreground)
                        .child(example.clone()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.use_coordinator_intro_example(&example, cx);
                }))
        };
        div()
            .id(("chat-coordinator-intro-tip", index))
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .flex()
            .items_start()
            .gap(px(TILE_GAP * s))
            .p(px(CARD_PAD * s))
            .rounded(px(12.0 * s))
            .border(px(s))
            .border_color(p.border)
            .bg(p.foreground.opacity(0.03))
            .child(tile)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0 * s))
                    .child(
                        div()
                            .text_size(px(14.0 * s))
                            .line_height(px(TITLE_LINE * s))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(13.0 * s))
                            .line_height(px(DETAIL_LINE * s))
                            .text_color(p.card_muted)
                            .child(text(tip, "detail")),
                    )
                    .child(use_example),
            )
            .into_any_element()
    }

    /// "Don't show again": a checkbox that applies when the panel closes, and what it does.
    fn intro_footer(
        &self,
        intro: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let ticked = self.coordinator_intro_dont_show;
        let label = text(intro, "dontShowAgain");
        let check = div()
            .flex_shrink_0()
            .size(px(16.0 * s))
            .rounded(px(4.0 * s))
            .border(px(s))
            .border_color(if ticked {
                p.foreground.opacity(0.7)
            } else {
                p.foreground.opacity(0.28)
            })
            .when(ticked, |check| check.bg(p.foreground.opacity(0.14)))
            .flex()
            .items_center()
            .justify_center()
            .when(ticked, |check| {
                check.child(
                    svg()
                        .path("titlebar/check.svg")
                        .size(px(12.0 * s))
                        .text_color(p.foreground),
                )
            });
        div()
            .flex_shrink_0()
            .pt(px(10.0 * s))
            .border_t(px(s))
            .border_color(p.border)
            .flex()
            .flex_col()
            .gap(px(3.0 * s))
            .px(px(4.0 * s))
            .child(
                div()
                    .id("chat-coordinator-intro-dont-show")
                    .role(gpui::Role::CheckBox)
                    .aria_label(label.clone())
                    .aria_toggled(if ticked {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .chat_cursor_pointer()
                    .child(check)
                    .child(
                        div()
                            .text_size(px(13.0 * s))
                            .text_color(p.foreground)
                            .child(label),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.coordinator_intro_dont_show = !this.coordinator_intro_dont_show;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .pl(px(24.0 * s))
                    .text_size(px(12.0 * s))
                    .line_height(px(16.0 * s))
                    .text_color(p.muted)
                    .child(text(intro, "dontShowAgainNote")),
            )
            .into_any_element()
    }

    /// Puts a tip's example in the chat box at the caret (or over the selection) and gives the box
    /// the keyboard; the panel closes out of the way. Nothing is sent.
    fn use_coordinator_intro_example(&mut self, example: &str, cx: &mut Context<Self>) {
        let selection = self
            .input
            .as_ref()
            .map(|input| input.read(cx).selected_range())
            .unwrap_or(self.draft.len()..self.draft.len());
        let start = self.draft[..selection.start.min(self.draft.len())]
            .encode_utf16()
            .count();
        let end = self.draft[..selection.end.min(self.draft.len())]
            .encode_utf16()
            .count();
        self.invoke(
            json!({"type": "useCoordinatorIntroExample", "example": example, "text": self.draft, "start": start, "end": end}),
            cx,
        );
        self.close_pill_menu(cx);
        self.focus_requested = true;
        cx.notify();
    }

    /// The panel closed: a ticked "Don't show again" turns the tips off everywhere.
    pub(super) fn coordinator_intro_menu_closed(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.coordinator_intro_dont_show) {
            self.invoke(json!({"type": "hideCoordinatorIntro"}), cx);
        }
    }
}
