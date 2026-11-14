use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Div, FontWeight, Hsla, IntoElement, ParentElement as _, Styled, div, px};
use serde::Deserialize;
use std::sync::LazyLock;

/// The fill a pressable card, or the pressable header of one, takes under the pointer.
pub(super) fn card_hover_fill(p: &ChatAppearance) -> Hsla {
    p.foreground.opacity(0.04)
}

/// `packages/gx-chat-core/visual/single-line-card.json`, in unscaled pixels.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SingleLineCard {
    /// The folded card's whole height, its border included.
    pub height: f32,
    pub radius: f32,
    pub padding_x: f32,
    pub title_size: f32,
    pub title_line_height: f32,
    pub title_weight: f32,
    border_alpha: f32,
    fill_input_alpha: f32,
    pill: SingleLinePill,
    tile: SingleLineTile,
    badge: SingleLineBadge,
}

/// The neutral tile every one-line card, the question cards and the edited files card lead with
/// (the agent mark on "Message from" sits in it too).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SingleLineTile {
    size: f32,
    radius: f32,
    glyph_size: f32,
    fill_alpha: f32,
    glyph_alpha: f32,
    /// A numbered tile's figure (an answered question's row number), in the glyph's colour.
    number_size: f32,
    number_weight: f32,
}

/// The round badge at a tile's bottom right; `size` is the dot, the ring sits outside it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SingleLineBadge {
    size: f32,
    ring: f32,
    offset: f32,
    glyph_size: f32,
    /// The check badge's green.
    fill: String,
    /// A failed status's red.
    error_fill: String,
    /// The waiting question's "?" badge and its ink.
    ask_fill: String,
    ask_ink: String,
}

fn hex_fill(hex: &str, fallback: Hsla) -> Hsla {
    u32::from_str_radix(hex.trim_start_matches('#'), 16)
        .map(|fill| Hsla::from(gpui::rgb(fill)))
        .unwrap_or(fallback)
}

/// The pill a one-line card puts its counts and its status in, the fold's "1 tool call".
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SingleLinePill {
    padding_x: f32,
    padding_y: f32,
    gap: f32,
    text_size: f32,
    line_height: f32,
    icon_size: f32,
    border_alpha: f32,
}

/// CDXC:SessionChat 2026-10-18 DECISION:
/// User: "let's please make all these single line cards look the same and also have same height (44px is good, like the Message from "Send Bug / Feedback" has", then "same font and font size too please, the one in Worked for xyz is good font and font size, use that. also the color of the bg needs to be like "Worked for 8s" for all of them". Every transcript card that folds to one line (the "Worked for 12s" / "Previous turn" fold, the Message from / Message to / subagent reply cards, the answered question form and the "You answered" receipts) is 44px tall with its border, wears the fold's fill, hairline and 10px radius, and titles in the fold's 13.5px medium type; icons shrink to fit the row rather than growing it, and an opened card keeps its body under the 44px header. The working card above the composer and the message bubbles are not part of this. Later the same day: "keep in mind no need for the chevron for all of them and i like the pills on the right side instead of this" (an amber clock and "Queued" beside a chevron): none of these cards draws a chevron (the whole card still opens and closes), and every status on a card's right (Queued, Waiting for agent, Not sent, Delivered, Answered) is a pill shaped like the fold's "1 tool call" in its own colour; a time stays plain muted text left of the pills. Then the user picked the left icons (Q288 "C", Q289 "D"): the fold leads with a clock tile wearing a green check badge where its chevron was, and the answered question form and the "You answered" receipts lead with a question-bubble tile and no badge, both sized like the agent mark on "Message from" (`tile`); and (Q291 "Yes, match the tiles") that mark takes the tile's size and badge, so all three left icons line up; then "please make this icon here look like these ones" (the bare logo beside the Background command and Previous turn tiles), so the agent's logo, a coordinator's crown or a subagent's sparkle sits in the same neutral tile at the tile's glyph size in its own colours, its direction badge where the green check sits, superseding the bare logo. And for the page answers card ("You answered …", which never folds): "can we make this match the look of other icons we just worked on please?", so it leads with the same question tile and green check badge instead of its tinted check tile. Then, picking from the old tinted squares (docs/2026-10-19/chat-old-icon-tiles/tiles.html): the live question card takes the question tile with a "?" badge, "but make it this color #D2A024" (Q294 A); the open questions form takes the question tile with its state as a pill on the right, Waiting, Sending…, Queued or Not shown (Q295 B); the edited files card takes the pencil in the neutral tile (Q296 A); the notices and approvals keep their tinted squares ("keep", Q297, the 2026-10-11 DECISION on `notice_tone`); and the "You answered" card lines up with the one-line cards, its tile and title at their x with their medium title, its numbered answers shifting with it ("Yes, line it up", Q293). Then Q299 "yes", every tile 28px (docs/2026-10-19/chat-tiles-28/preview.html): every tile is the old squares' size, 28px with an 8px radius and a 15px glyph, and its badge 16px with a 10px glyph (resized below), superseding the 22px tile; the notices and approvals keep their own tinted squares. User (2026-10-19, no question tag): "please make the icon inside the square bigger by 1px from all sides and make the circle and icon inside it on the bottom right smaller by 2px from all sides", so the glyph in the tile (an agent's logo too) is 17px, and the badge is 12px with a 6px glyph and its 2px ring, set 4px past the tile's corner (the same overhang for its size). Then, of the live question card ("Needs your answer"): "let's change the icon inside the square here to be the checklist icon instead", so its tile shows two ticks with lines, keeping the "?" badge; and Q305 "Yes, both waiting cards": an open questions form (asking, sending, queued or could not be shown) shows it too, while the answered cards (Questions · Answered, You answered, a page's answered question) keep the question bubble. And of the "You answered" receipts: "please make the 1 2 3 and the square match the look of the icon above it", so each answered question's number sits in the same tile (`number_tile`, its figure in the glyph's colour and weight from the JSON), and the question and answer line up under the card's title. The tile's and badges' sizes are only values in single-line-card.json. Later: "this card also please needs to be like the others" (a "Background command … completed" row): every status row (a background task's end, a model, effort or Fast mode change, a compaction) is a one-line card too (2026-10-25, Q550: a background task's end left these cards to be drawn as a tool call row, gx-chat-core `transcript/background_rows.rs`), its tile saying what it is about and its badge the tone (a green check, a red cross, none when neutral), the sentence on one line with the whole of it on hover (a long press on the phone) and a fact such as "exit code 1" as a pill, superseding the 2026-09-04 wrapping status pill (status_rows.rs). And "these dont look all the same too": every title is the foreground colour (a fold that cannot open no longer greys its title), Delivered and Answered are each a check with the word in a green pill, and every card takes the same hover fill. The phone reads the same values (apps/mobile/app/src/chat/native/transcript/singleLineCard.tsx).
pub(super) static SINGLE_LINE_CARD: LazyLock<SingleLineCard> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/single-line-card.json"
    ))
    .expect("shared single-line card")
});

impl SingleLineCard {
    /// The header inside a bordered shell, so the folded card is `height` tall with its border.
    pub(super) fn header_height(&self, s: f32) -> f32 {
        (self.height - 2.0) * s
    }

    /// The shell's hairline colour, for a divider inside an opened card: the Borders theme role
    /// when it is set (helpers/theme_roles.rs).
    pub(super) fn hairline(&self, p: &ChatAppearance) -> gpui::Hsla {
        crate::app::helpers::theme_role_or(
            "border",
            p.light,
            p.foreground.opacity(self.border_alpha),
        )
    }

    /// The shell's fill laid over the transcript, for an edge fade inside the card.
    pub(super) fn solid_fill(&self, p: &ChatAppearance) -> gpui::Hsla {
        p.background.blend(p.input.opacity(self.fill_input_alpha))
    }

    /// The fold's look: its fill, hairline and radius. The shell's border is one scaled pixel,
    /// which `header_height` leaves room for.
    pub(super) fn shell<E: Styled>(&self, element: E, p: &ChatAppearance) -> E {
        element
            .rounded(px(self.radius * p.scale))
            .border(px(p.scale))
            .border_color(self.hairline(p))
            .bg(p.input.opacity(self.fill_input_alpha))
    }

    /// A pill on the card's right: `tone` colours its glyph and words, its hairline stays neutral.
    /// How wide a `pill` is (unscaled), its border included, from its label's own width.
    pub(super) fn pill_width(&self, icon: bool, label_width: f32) -> f32 {
        let pill = &self.pill;
        let inner = match (icon, label_width > 0.0) {
            (true, true) => pill.icon_size + pill.gap + label_width,
            (true, false) => pill.icon_size,
            (false, _) => label_width,
        };
        inner + 2.0 * pill.padding_x + 2.0
    }

    /// A pill's text size (unscaled).
    pub(super) fn pill_text_size(&self) -> f32 {
        self.pill.text_size
    }

    pub(super) fn pill(
        &self,
        icon: Option<&'static str>,
        label: Option<String>,
        tone: Hsla,
        p: &ChatAppearance,
    ) -> Div {
        let s = p.scale;
        let pill = &self.pill;
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(pill.gap * s))
            .px(px(pill.padding_x * s))
            .py(px(pill.padding_y * s))
            .rounded_full()
            .border(px(s))
            .border_color(p.foreground.opacity(pill.border_alpha))
            .text_size(px(pill.text_size * s))
            .line_height(px(pill.line_height * s))
            .whitespace_nowrap()
            .text_color(tone)
            .when_some(icon, |this, icon| {
                this.child(
                    gpui::svg()
                        .path(icon)
                        .size(px(pill.icon_size * s))
                        .flex_shrink_0()
                        .text_color(tone),
                )
            })
            .when_some(label, |this, label| this.child(label))
    }

    /// The tile leading the row: `glyph` in a neutral rounded square. A badge (`check_badge`,
    /// `error_badge`) goes in as its child.
    pub(super) fn tile(&self, glyph: &'static str, p: &ChatAppearance) -> Div {
        let tile = &self.tile;
        self.tile_box(p).child(
            gpui::svg()
                .path(glyph)
                .size(px(tile.glyph_size * p.scale))
                .text_color(p.foreground.opacity(tile.glyph_alpha)),
        )
    }

    /// The tile without its glyph, for a mark that brings its own colours (the agent's logo on
    /// "Message from"), drawn at `glyph_size`.
    pub(super) fn tile_box(&self, p: &ChatAppearance) -> Div {
        let s = p.scale;
        let tile = &self.tile;
        div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .size(px(tile.size * s))
            .rounded(px(tile.radius * s))
            .bg(p.foreground.opacity(tile.fill_alpha))
    }

    /// The tile with a number in it, the row number of an answered question (`receipt_row`).
    pub(super) fn number_tile(&self, number: usize, p: &ChatAppearance) -> Div {
        let tile = &self.tile;
        self.tile_box(p)
            .text_size(px(tile.number_size * p.scale))
            .line_height(px(tile.size * p.scale))
            .font_weight(FontWeight(tile.number_weight))
            .text_color(p.foreground.opacity(tile.glyph_alpha))
            .child(number.to_string())
    }

    /// The side of the tile, unscaled.
    pub(super) fn tile_size(&self) -> f32 {
        self.tile.size
    }

    /// The side of the glyph inside a tile, unscaled.
    pub(super) fn glyph_size(&self) -> f32 {
        self.tile.glyph_size
    }

    /// The green check badge: the turn's work, or a status that went well.
    pub(super) fn check_badge(&self, p: &ChatAppearance) -> Div {
        self.badge(
            hex_fill(&self.badge.fill, p.success()),
            "titlebar/chat-card-check.svg",
            gpui::white(),
            p,
        )
    }

    /// The "?" badge of a question waiting on the reader.
    pub(super) fn ask_badge(&self, p: &ChatAppearance) -> Div {
        self.badge(
            hex_fill(&self.badge.ask_fill, p.foreground),
            "titlebar/chat-card-question-mark.svg",
            hex_fill(&self.badge.ask_ink, p.background),
            p,
        )
    }

    /// The red badge of a status that failed.
    pub(super) fn error_badge(&self, p: &ChatAppearance) -> Div {
        self.badge(
            hex_fill(&self.badge.error_fill, p.error()),
            "titlebar/chat-card-x.svg",
            gpui::white(),
            p,
        )
    }

    /// The round badge at the bottom right of a row's leading icon (the tile's check, the agent
    /// mark's arrow), its ring in the card's own fill made opaque so the icon's corner does not
    /// show through it.
    pub(super) fn badge(
        &self,
        fill: Hsla,
        glyph: &'static str,
        ink: Hsla,
        p: &ChatAppearance,
    ) -> Div {
        let s = p.scale;
        let badge = &self.badge;
        let ring = p.background.blend(p.input.opacity(self.fill_input_alpha));
        div()
            .absolute()
            .right(px(badge.offset * s))
            .bottom(px(badge.offset * s))
            .size(px((badge.size + 2.0 * badge.ring) * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(fill)
            .border(px(badge.ring * s))
            .border_color(ring)
            .child(
                gpui::svg()
                    .path(glyph)
                    .size(px(badge.glyph_size * s))
                    .text_color(ink),
            )
    }

    /// A status mark as a badge (a work item's state on its update card, work_cards.rs
    /// `work_turn_card`): the mark itself at the badge's size in its own colour, on a disc of the
    /// card's fill, where `check_badge` sits.
    pub(super) fn mark_badge(&self, glyph: &'static str, color: Hsla, p: &ChatAppearance) -> Div {
        let s = p.scale;
        let badge = &self.badge;
        let ring = p.background.blend(p.input.opacity(self.fill_input_alpha));
        div()
            .absolute()
            .right(px(badge.offset * s))
            .bottom(px(badge.offset * s))
            .size(px((badge.size + 2.0 * badge.ring) * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(ring)
            .child(
                gpui::svg()
                    .path(glyph)
                    .size(px(badge.size * s))
                    .text_color(color),
            )
    }

    /// The space between two one-line cards that follow each other: the transcript's row gap
    /// (Message spacing's, `ChatAppearance::row_gap`), so a stack of cards is spaced the same
    /// whether its cards are separate rows, parts of one finished turn or one stack (sent
    /// messages, answered questions, status rows).
    pub(super) fn stack_gap(&self, p: &ChatAppearance) -> f32 {
        p.row_gap()
    }

    /// The space above a part of a transcript row: `stack_gap` when it and the part above it are
    /// both one-line cards, the row's own `gap` otherwise.
    ///
    /// CDXC:SessionChat 2026-10-27 DECISION: User: "i dont get why space here is sometimes bigger and sometimes smaller ... please figure this out i want consistent spacing when 2 cards like those come in a row in the chat transcript". Two one-line cards in a row are always one row gap apart (16px; 8px when Message spacing is Compact), whatever kind each is and wherever a turn starts or ends: between transcript rows (each row's own bottom gap), between the parts of a finished turn (the "Worked for" fold and the answered-question or sent-message cards under it, which had 10px), and inside a stack of cards (sent messages and status rows had 8px, answered questions 12px with 6px around the stack). Other content keeps its own spacing.
    /// CDXC:SessionChat 2026-11-08 DECISION: User, of a tool card with a "Message to …" card right under it in the same row: "The gaps here aren't equal, should be wide like the ones between 2 message cards". A row's tool card (and its edited-files card) counts as a one-line card too, so the answered-question, sent-message and workflow cards after it are a row gap away (transcript.rs); and in an opened turn's log a row that opens on its tool card under one that closes on its own is a row gap away (completed_work_row.rs). The phone does the same (MessageRow.tsx, TranscriptItemRow.tsx).
    pub(super) fn gap_above(
        &self,
        above_is_card: bool,
        is_card: bool,
        gap: f32,
        p: &ChatAppearance,
    ) -> f32 {
        if above_is_card && is_card {
            self.stack_gap(p)
        } else {
            gap
        }
    }

    /// The title's type, the fold's "Worked for 12s".
    pub(super) fn title<E: Styled>(&self, element: E, p: &ChatAppearance) -> E {
        element
            .text_size(px(self.title_size * p.scale))
            .line_height(px(self.title_line_height * p.scale))
            .font_weight(FontWeight(self.title_weight))
    }
}

impl NativeChatView {
    pub(crate) fn status_card(
        &self,
        title: String,
        icon: &'static str,
        body: Vec<AnyElement>,
        actions: Vec<AnyElement>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let header = div()
            .flex()
            .items_start()
            .gap(px(8.0 * p.scale))
            .child(
                gpui::svg()
                    .path(icon)
                    .size(px(14.0 * p.scale))
                    .mt(px(4.0 * p.scale))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(div().flex_1().text_color(p.foreground).child(title))
            .into_any_element();
        self.status_card_with_header(header, body, actions, p)
    }

    pub(crate) fn status_card_with_header(
        &self,
        header: AnyElement,
        body: Vec<AnyElement>,
        actions: Vec<AnyElement>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let column = (!body.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0 * s))
                .pt(px(8.0 * s))
                .children(body)
                .into_any_element()
        });
        self.status_card_panel(header, column, actions, p)
    }

    /// The card's shell around its header, its body column (already spaced) and its footer.
    ///
    /// CDXC:SessionChat 2026-10-17 DECISION:
    /// User (2026-09-16): every card above the composer, and the status cards in the transcript, shares one shell so a change is made once.
    /// Two tones: a panel for the header and body, and a band under a hairline for the actions; an input border, 0.75rem radius, 1rem/0.75rem padding, 0.875rem text.
    /// Each card leads with its own icon, the title matches the transcript text at regular weight, a dismiss is a circled X, and actions sit right-aligned in the footer band.
    /// A card whose border carries a colour is one tone. User (2026-10-17, Q253 "card"): the working indicator becomes a card too, the working card at the transcript's end, with this shell's border, radius and panel tone (working_card.rs draws it, one line high). Supersedes "The working strip stays a bare line and never becomes a card."
    /// Light-mode tones: see the Theming 2026-09-22 decision in appearance.rs. The question counter moved into the header on 2026-09-21 (question.rs).
    fn status_card_panel(
        &self,
        header: AnyElement,
        body: Option<AnyElement>,
        actions: Vec<AnyElement>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let has_actions = !actions.is_empty();
        let panel_color = p.card_panel;
        let footer_color = p.card_footer;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.input_border)
            .rounded(px(12.0 * s))
            // CDXC:SessionChat 2026-09-18 WHY:
            // GPUI overflow masks are rectangular, so a square child fill leaked beyond the status card's rounded corners.
            // Paint the outer tone on the rounded shell and round only the inset panel's top corners when a footer supplies the bottom tone.
            .bg(if has_actions {
                footer_color
            } else {
                panel_color
            })
            .overflow_hidden()
            .text_color(p.card_muted)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(16.0 * s))
                    .py(px(12.0 * s))
                    .when(has_actions, |panel| {
                        panel
                            .rounded_t(px((12.0 * s - 1.0).max(0.0)))
                            .bg(panel_color)
                    })
                    .child(header)
                    .when_some(body, |panel, body| panel.child(body)),
            )
            .when(has_actions, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0 * s))
                        .px(px(16.0 * s))
                        .py(px(10.0 * s))
                        .border_t_1()
                        .border_color(p.control_border.opacity(0.65))
                        .children(actions),
                )
            })
            .into_any_element()
    }
}
