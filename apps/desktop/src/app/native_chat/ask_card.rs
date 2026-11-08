//! The card every question the chat shows is drawn in: an agent's own question tool (question.rs)
//! and its answered record (question_exchange.rs), a ```questions form (question_form.rs) and the
//! answers sent from a form or an HTML page (page_answers.rs). A tinted tile and a two-line title
//! lead it, the options are radio or checkbox rows that light up in the asking colour, and the
//! answers come back as plain text under their question.
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User picked "Clear ask, clear answer" for the agent's question card, then "please same style for the html modal also and the inline questions": all three places a question appears share this look. Amber marks a question waiting for the reader (its tile, progress bars and the picked option), green an answered one, muted one that got no answer. The amber is a light tint, not a strong one ("please make the yellow more subtle" on the page cards, then "yellow + dark all" for the chat): the tile fills 9% with a 14% ring and an 80% icon, a picked option takes a 32% border over a 4% wash with an 85% mark.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The phone's copy is apps/mobile/app/src/chat/native/cards/askCard.tsx; the HTML question pages draw it in server/src/visual_pages/kit.rs. Change them together.

use super::cards::SINGLE_LINE_CARD;
use super::disclosure_motion::{FoldFrame, motion_clip};
use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px, rgb,
    svg,
};
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::Value;

/// Whether an answered exchange got an answer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AskTone {
    /// Answered.
    Answered,
    /// Closed without an answer, or not a question at all any more.
    Quiet,
}

/// The colour a waiting question and the option the reader picked share.
pub(super) fn ask_color(p: &ChatAppearance) -> Hsla {
    rgb(if p.light { 0xb45309 } else { 0xfbbf24 }).into()
}

/// The question bubble the answered question cards' tile shows.
pub(super) const QUESTION_GLYPH: &str = "titlebar/chat-card-question.svg";
/// The tile glyph of a question still waiting on the reader: the live question card and an open
/// questions form (the SessionChat 2026-10-18 DECISION on `SINGLE_LINE_CARD`).
pub(super) const CHECKLIST_GLYPH: &str = "titlebar/chat-card-checklist.svg";

/// What the question tile's badge says: nothing, a question waiting on the reader, or answered.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QuestionBadge {
    None,
    Asking,
    Answered,
}

/// The one-line cards' question tile on a question card's panel, the badge's ring in that panel.
fn question_tile(glyph: &'static str, badge: QuestionBadge, p: &ChatAppearance) -> AnyElement {
    let shape = &*SINGLE_LINE_CARD;
    let ring = p.background.blend(p.card_panel);
    let tile = shape.tile(glyph, p);
    match badge {
        QuestionBadge::None => tile,
        QuestionBadge::Asking => tile.child(shape.ask_badge(p).border_color(ring)),
        QuestionBadge::Answered => tile.child(shape.check_badge(p).border_color(ring)),
    }
    .into_any_element()
}

/// The header of a question card that is still open (the live question card, an open questions
/// form): `ask_head` led by the question tile.
pub(super) fn question_ask_head(
    glyph: &'static str,
    badge: QuestionBadge,
    title: String,
    detail: String,
    trailing: Vec<AnyElement>,
    p: &ChatAppearance,
) -> Div {
    ask_head_led(
        question_tile(glyph, badge, p),
        title,
        detail,
        trailing,
        14.0,
        FontWeight::SEMIBOLD,
        p,
    )
}

/// The header of an answered card that never folds (the page answers): the question tile with the
/// green check, its tile and title at the one-line cards' x and in their title weight
/// (`lined_ask_card` keeps the body on the same edge).
pub(super) fn answered_ask_head(title: String, p: &ChatAppearance) -> Div {
    let shape = &*SINGLE_LINE_CARD;
    ask_head_led(
        question_tile(QUESTION_GLYPH, QuestionBadge::Answered, p),
        title,
        String::new(),
        Vec::new(),
        shape.padding_x,
        FontWeight(shape.title_weight),
        p,
    )
}

/// The header row: `lead`, the title over a muted detail line, then whatever trails it (progress,
/// a pill, a chevron), `pad_x` from the card's edge. Callers that fold the card give it an id and
/// a click.
fn ask_head_led(
    lead: AnyElement,
    title: String,
    detail: String,
    trailing: Vec<AnyElement>,
    pad_x: f32,
    title_weight: FontWeight,
    p: &ChatAppearance,
) -> Div {
    let s = p.scale;
    div()
        .flex()
        .items_center()
        .gap(px(10.0 * s))
        .w_full()
        .min_w_0()
        .px(px(pad_x * s))
        .py(px(12.0 * s))
        .child(lead)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.5 * s))
                        .line_height(px(18.0 * s))
                        .font_weight(title_weight)
                        .text_color(p.foreground)
                        .child(title),
                )
                .when(!detail.is_empty(), |column| {
                    column.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0 * s))
                            .line_height(px(16.0 * s))
                            .text_color(p.muted)
                            .child(detail),
                    )
                }),
        )
        .children(trailing)
}

/// The head of an answered card that folds to one line: `ask_head` in the 44px row, led by the
/// question-bubble tile in the shared title type (`SINGLE_LINE_CARD`, cards.rs).
pub(super) fn single_line_ask_head(
    title: String,
    detail: String,
    trailing: Vec<AnyElement>,
    p: &ChatAppearance,
) -> Div {
    let s = p.scale;
    let shape = &*SINGLE_LINE_CARD;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.0 * s))
        .w_full()
        .min_w_0()
        .h(px(shape.header_height(s)))
        .px(px(shape.padding_x * s))
        .child(shape.tile("titlebar/chat-card-question.svg", p))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    shape
                        .title(div().min_w_0().truncate(), p)
                        .text_color(p.foreground)
                        .child(title),
                )
                .when(!detail.is_empty(), |column| {
                    column.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0 * s))
                            .line_height(px(16.0 * s))
                            .text_color(p.muted)
                            .child(detail),
                    )
                }),
        )
        .children(trailing)
}

/// A head the reader presses to fold the card; it lights under the pointer to the card's edges.
pub(super) fn press_head(
    head: Div,
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    open_body: bool,
    p: &ChatAppearance,
) -> gpui::Stateful<Div> {
    press_head_rounded(head, id, label, open_body, 14.0 * p.scale - 1.0, p)
}

/// The head of a one-line card (`single_line_ask_head`), rounded to that card's shell.
pub(super) fn single_line_press_head(
    head: Div,
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    open_body: bool,
    p: &ChatAppearance,
) -> gpui::Stateful<Div> {
    let radius = (SINGLE_LINE_CARD.radius - 1.0) * p.scale;
    press_head_rounded(head, id, label, open_body, radius, p)
}

fn press_head_rounded(
    head: Div,
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    open_body: bool,
    radius: f32,
    p: &ChatAppearance,
) -> gpui::Stateful<Div> {
    let radius = px(radius.max(0.0));
    let hover = super::cards::card_hover_fill(p);
    head.id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .chat_cursor_pointer()
        .map(|head| {
            if open_body {
                head.rounded_t(radius)
            } else {
                head.rounded(radius)
            }
        })
        .hover(move |style| style.bg(hover))
}

/// The fold chevron at the end of a pressable head.
///
/// CDXC:SessionChat 2026-10-23 DECISION: User, on the question card docked above the composer: "the arrow in this case is flipped on the top right (should be opposite direction for that chevron)". The card sits on the composer, so folding moves its top edge down and opening moves it up: open points down (fold down), folded points up (open up). Only the docked question card uses this chevron; transcript cards keep their own right/down disclosure. The phone's `AskHead` (apps/mobile/app/src/chat/native/cards/askCard.tsx) matches.
pub(super) fn chevron(open: bool, p: &ChatAppearance) -> AnyElement {
    svg()
        .path(if open {
            "titlebar/chevron-down.svg"
        } else {
            "titlebar/chevron-up.svg"
        })
        .size(px(14.0 * p.scale))
        .flex_shrink_0()
        .text_color(p.muted)
        .into_any_element()
}

/// One short bar per question, the asked ones lit: the progress through a multi-question ask.
pub(super) fn progress_dots(index: usize, count: usize, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let hue = ask_color(p).opacity(0.8);
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(4.0 * s))
        .children((0..count).map(|dot| {
            div()
                .w(px(18.0 * s))
                .h(px(4.0 * s))
                .rounded(px(2.0 * s))
                .bg(if dot <= index {
                    hue
                } else {
                    p.foreground.opacity(0.12)
                })
        }))
        .into_any_element()
}

/// How long the question form's footer buttons wait before their shortcut tooltip shows, well under
/// the usual 500ms.
pub(super) const QUICK_TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

/// The quiet mark beside the option the agent recommends, at the chat's scale `s`.
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User: "make the 'recommended' tag more subtle, not like a pill, and not all caps", first on the HTML pages, then the chat's own question card to match them. A recommended option shows a small muted "Recommended" in sentence case after a faint amber star, with no pill, fill or border, the same mark as the pages (server/src/visual_pages/kit.rs `.gx-rec`) and the phone (apps/mobile/app/src/chat/native/cards/askCard.tsx `RecommendedMark`). Supersedes the light-grey Recommended pill the 2026-10-11 decision kept.
fn recommended_mark(s: f32, p: &ChatAppearance) -> AnyElement {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(4.0 * s))
        .text_size(px(12.0 * s))
        .line_height(px(18.0 * s))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.muted)
        .child(
            svg()
                .path("titlebar/star-filled.svg")
                .size(px(10.0 * s))
                .flex_shrink_0()
                .text_color(ask_color(p).opacity(0.75)),
        )
        .child("Recommended")
        .into_any_element()
}

/// A muted keycap: an option's shortcut or letter, or the key a button answers to.
pub(super) fn keycap(label: String, lit: bool, on_primary: bool, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .h(px(18.0 * s))
        .min_w(px(18.0 * s))
        .px(px(5.0 * s))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0 * s))
        .border_1()
        .map(|cap| {
            if on_primary {
                cap.border_color(p.background.opacity(0.18))
                    .bg(p.background.opacity(0.06))
                    .text_color(p.background.opacity(0.7))
            } else {
                cap.border_color(p.foreground.opacity(0.12))
                    .bg(p.background.opacity(0.5))
                    .text_color(if lit { p.foreground } else { p.muted })
            }
        })
        .font_family(super::fonts::CHAT_MONO)
        .text_size(px(11.0 * s))
        .font_weight(FontWeight::MEDIUM)
        .child(label)
        .into_any_element()
}

/// How the body of a folding card is moving.
pub(super) struct AskBodyMotion<'a> {
    pub key: &'a str,
    pub frame: Option<FoldFrame>,
}

/// One option the reader can pick, or a record of one they picked.
pub(super) struct AskOption {
    pub id: SharedString,
    pub label: String,
    pub detail: String,
    /// The detail with its links (a question form's `detailLinks`, question_form.rs), drawn in
    /// place of `detail`.
    pub detail_view: Option<AnyElement>,
    pub picked: bool,
    /// Checkbox rather than radio.
    pub multiple: bool,
    /// The keycap at the row's end: a shortcut digit or the option's letter.
    pub key: Option<String>,
    pub recommended: bool,
    /// "Other / none of these": its label stays muted until it is picked.
    pub other: bool,
    pub enabled: bool,
    /// The question was answered and sent: a pick reads green, like the answers card.
    pub answered: bool,
    pub action: Value,
}

impl NativeChatView {
    /// The card's shell around its head, its body and its footer band.
    pub(super) fn ask_card(
        &self,
        head: AnyElement,
        body: Option<AnyElement>,
        footer: Vec<AnyElement>,
        motion: Option<AskBodyMotion>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let shell = div()
            .rounded(px(14.0 * p.scale))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel);
        self.ask_card_in(shell, 14.0, head, body, footer, motion, None, p)
    }

    /// The asking card no taller than `max_h`: its head and footer keep their height and the body
    /// gives up the rest, so whatever inside it may shrink (`min_h_0` and a scroll) does.
    pub(super) fn capped_ask_card(
        &self,
        head: AnyElement,
        body: Option<AnyElement>,
        footer: Vec<AnyElement>,
        motion: Option<AskBodyMotion>,
        max_h: gpui::Pixels,
        p: &ChatAppearance,
    ) -> AnyElement {
        let shell = div()
            .rounded(px(14.0 * p.scale))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel);
        self.ask_card_in(shell, 14.0, head, body, footer, motion, Some(max_h), p)
    }

    /// An answered card that never folds (the page answers), its body on the one-line cards'
    /// edge so its numbered answers line up with `answered_ask_head`.
    pub(super) fn lined_ask_card(
        &self,
        head: AnyElement,
        body: Option<AnyElement>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let shell = div()
            .rounded(px(14.0 * p.scale))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel);
        self.ask_card_in(
            shell,
            SINGLE_LINE_CARD.padding_x,
            head,
            body,
            Vec::new(),
            None,
            None,
            p,
        )
    }

    /// An answered card that folds to its `single_line_ask_head`, in the shell every one-line card
    /// shares (`SINGLE_LINE_CARD`, cards.rs); opened, its body sits under that head.
    pub(super) fn single_line_ask_card(
        &self,
        head: AnyElement,
        body: Option<AnyElement>,
        motion: Option<AskBodyMotion>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let shape = &*SINGLE_LINE_CARD;
        let shell = shape.shell(div(), p);
        self.ask_card_in(
            shell,
            shape.padding_x,
            head,
            body,
            Vec::new(),
            motion,
            None,
            p,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn ask_card_in(
        &self,
        shell: Div,
        pad: f32,
        head: AnyElement,
        body: Option<AnyElement>,
        footer: Vec<AnyElement>,
        motion: Option<AskBodyMotion>,
        max_h: Option<gpui::Pixels>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let capped = max_h.is_some();
        let body = body.map(|body| {
            let padded = div()
                .w_full()
                .min_w_0()
                .when(capped, |padded| padded.flex().flex_col().min_h_0())
                .px(px(pad * s))
                .pb(px(pad * s))
                .child(body)
                .into_any_element();
            match motion {
                Some(AskBodyMotion {
                    key,
                    frame: Some(frame),
                }) => self.reveal_extent(
                    key,
                    false,
                    motion_clip(self.disclosure_height(key), frame, 0.0, 0.0, padded),
                ),
                _ => padded,
            }
        });
        shell
            .w_full()
            .min_w_0()
            .when_some(max_h, |card, max_h| card.max_h(max_h))
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_color(p.card_muted)
            .text_size(px(14.0 * s))
            .line_height(px(21.0 * s))
            .child(head)
            .when_some(body, |card, body| card.child(body))
            .when(!footer.is_empty(), |card| {
                card.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0 * s))
                        .px(px(14.0 * s))
                        .py(px(10.0 * s))
                        .rounded_b(px((14.0 * s - 1.0).max(0.0)))
                        .border_t_1()
                        .border_color(p.control_border.opacity(0.65))
                        .bg(p.card_footer)
                        .children(footer),
                )
            })
            .into_any_element()
    }

    /// An option row: radio or checkbox, label over its detail, and a keycap at the end. A picked
    /// row takes the asking colour.
    pub(super) fn ask_option(
        &self,
        option: AskOption,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        // A pick waiting to be sent is a light amber tint; once sent it reads green at full strength.
        let (hue, edge, wash, ink) = if option.answered {
            (p.success(), 0.55, 0.08, 1.0)
        } else {
            (ask_color(p), 0.32, 0.04, 0.85)
        };
        let mark_hue = hue.opacity(ink);
        let AskOption {
            id,
            label,
            detail,
            detail_view,
            picked,
            multiple,
            key,
            recommended,
            other,
            enabled,
            action,
            ..
        } = option;
        let mark = div()
            .flex_shrink_0()
            .mt(px(2.0 * s))
            .size(px(16.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .border(px(1.5 * s))
            .border_color(if picked {
                mark_hue
            } else {
                p.muted.opacity(0.7)
            })
            .map(|mark| {
                if multiple {
                    mark.rounded(px(4.0 * s))
                } else {
                    mark.rounded_full()
                }
            })
            .when(picked && multiple, |mark| {
                mark.bg(mark_hue).child(
                    svg()
                        .path("titlebar/check.svg")
                        .size(px(11.0 * s))
                        .text_color(p.background),
                )
            })
            .when(picked && !multiple, |mark| {
                mark.child(div().size(px(8.0 * s)).rounded_full().bg(mark_hue))
            });
        div()
            .id(id)
            .when(enabled, |row| row.tab_index(0))
            .role(if multiple {
                gpui::Role::CheckBox
            } else {
                gpui::Role::RadioButton
            })
            .aria_label(label.clone())
            .flex()
            .items_start()
            .gap(px(12.0 * s))
            .w_full()
            .min_w_0()
            .px(px(12.0 * s))
            .py(px(10.0 * s))
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(if picked {
                hue.opacity(edge)
            } else {
                p.control_border
            })
            .bg(if picked {
                hue.opacity(wash)
            } else if p.light {
                p.background
            } else {
                p.card_row
            })
            .when(!enabled && !picked, |row| row.opacity(0.55))
            .when(enabled, |row| {
                row.chat_cursor_pointer().when(!picked, |row| {
                    row.hover(|style| {
                        style
                            .border_color(p.foreground.opacity(0.14))
                            .bg(p.foreground.opacity(0.03))
                    })
                })
            })
            .child(mark)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.0 * s))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.0 * s))
                            .text_size(px(14.0 * s))
                            .line_height(px(20.0 * s))
                            .child(
                                div()
                                    .min_w_0()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if other && !picked {
                                        p.card_muted
                                    } else {
                                        p.foreground
                                    })
                                    .child(label.clone()),
                            )
                            .when(recommended, |line| line.child(recommended_mark(s, p))),
                    )
                    .map(|column| match detail_view {
                        Some(view) => column.child(view),
                        None if !detail.is_empty() && detail != label => column.child(
                            div()
                                .text_size(px(13.0 * s))
                                .line_height(px(19.0 * s))
                                .text_color(p.muted)
                                .child(detail),
                        ),
                        None => column,
                    }),
            )
            .when_some(key, |row, key| {
                row.child(div().mt(px(1.0 * s)).child(keycap(key, picked, false, p)))
            })
            .when(enabled, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
            })
            .into_any_element()
    }

    /// The filled button that moves the ask on (Next, Send answer, Send), with the key it answers to
    /// as a keycap (`keys`) or, for the question form's footer, in a quick tooltip (`tooltip`: the
    /// title and the chord).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ask_primary_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        keys: Option<String>,
        tooltip: Option<(String, String)>,
        action: Value,
        disabled: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let fill = p.control_primary;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .h(px(28.0 * s))
            .px(px(11.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0 * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(fill)
            .bg(fill)
            .text_color(p.background)
            .text_size(px(13.0 * s))
            .font_weight(FontWeight::MEDIUM)
            .when(disabled, |button| button.opacity(0.5))
            .child(label)
            .when_some(keys, |button, keys| {
                button.child(keycap(keys, false, true, p))
            })
            .when_some(tooltip.filter(|_| !disabled), |button, (title, chord)| {
                button.managed_tooltip_with(
                    Some(gpui_component::Placement::Top),
                    Some(QUICK_TOOLTIP_DELAY),
                    move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(title.clone())
                            .keys(chord.clone())
                            .build(window, cx)
                    },
                )
            })
            .when(!disabled, |button| {
                button
                    .chat_cursor_pointer()
                    .hover(|style| style.opacity(0.9))
                    .on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
            })
            .into_any_element()
    }

    /// The quiet button beside the primary one (the question form's Skip all): a faint fill of
    /// the foreground and the normal text colour, no border, the primary button's size, with its
    /// shortcut in a quick tooltip (`tooltip`: the title and the chord).
    ///
    /// CDXC:SessionChat 2026-10-16 DECISION: User: "skip all looks disabled cuz it's darker color"; shown a mockup of options, they picked "soft fill" for Skip all: the foreground at about 10% (hover 15%) on the dark theme and 7% (hover 11%) on the light one, in the normal text colour with no border, clearly a button but clearly second. Send answers keeps the solid fill. Neither footer button carries a keycap; each shows its shortcut in a quick tooltip (the user asked for that earlier the same day). The phone (QuestionForm.tsx, askCard.tsx `SoftButton`) and the page kit (server/src/visual_pages/kit.rs `.gx-skip-all`) match, minus the hover.
    pub(super) fn ask_soft_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        tooltip: Option<(String, String)>,
        action: Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let (rest, hover) = if p.light { (0.07, 0.11) } else { (0.10, 0.15) };
        let hover_fill = p.foreground.opacity(hover);
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .h(px(28.0 * s))
            .px(px(11.0 * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0 * s))
            .bg(p.foreground.opacity(rest))
            .text_color(p.foreground)
            .text_size(px(13.0 * s))
            .font_weight(FontWeight::MEDIUM)
            .child(label)
            .when_some(tooltip, |button, (title, chord)| {
                button.managed_tooltip_with(
                    Some(gpui_component::Placement::Top),
                    Some(QUICK_TOOLTIP_DELAY),
                    move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(title.clone())
                            .keys(chord.clone())
                            .build(window, cx)
                    },
                )
            })
            .chat_cursor_pointer()
            .hover(move |style| style.bg(hover_fill))
            .on_click(cx.listener(move |this, _, _, cx| this.invoke(action.clone(), cx)))
            .into_any_element()
    }
}

/// What a question form's footer says about the questions left blank; the footer shows it behind an
/// (i) icon (`ask_info_hint`). The phone's copy is QuestionForm.tsx `SKIPPED_HINT`, the page kit's
/// is server/src/visual_pages/kit.rs.
pub(super) const SKIPPED_HINT: &str = "Questions you leave blank are sent as skipped.";

/// A small (i) icon whose quick tooltip says `hint`, for a footer note that is not worth a line of its
/// own.
///
/// CDXC:SessionChat 2026-10-24 DECISION: User: "remove the text about skipped (add i icon there for that)". The question form's footer no longer prints "Questions you leave blank are sent as skipped." (five lines on the phone); a small (i) icon stands in its place, before Skip all and Send answers, and shows the sentence in a tooltip on hover here, on a tap on the phone (QuestionForm.tsx `InfoHint`) and on the HTML question pages (server/src/visual_pages/kit.rs `.gx-send-info`). The buttons keep their size and place. Status messages (Sending, Queued, a send error) still print as text.
pub(super) fn ask_info_hint(hint: &'static str, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .id("question-form-info")
        .role(gpui::Role::Button)
        .aria_label(hint)
        .size(px(20.0 * s))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path("titlebar/info-circle.svg")
                .size(px(16.0 * s))
                .text_color(p.muted),
        )
        .managed_tooltip_with(
            Some(gpui_component::Placement::Top),
            Some(QUICK_TOOLTIP_DELAY),
            move |window, cx| gpui_component::tooltip::Tooltip::new(hint).build(window, cx),
        )
        .into_any_element()
}

/// An answer as plain text: the card's foreground colour at medium weight, no chip, border or fill.
pub(super) fn plain_answer(words: String, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .min_w_0()
        .text_size(px(14.0 * s))
        .line_height(px(20.0 * s))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.foreground)
        .child(words)
        .into_any_element()
}

/// The line height of an answered question in a receipt row.
pub(super) const RECEIPT_QUESTION_LINE: f32 = 19.0;

/// A receipt row's question as plain muted text, or nothing when there is none.
pub(super) fn receipt_question(question: String, p: &ChatAppearance) -> Option<AnyElement> {
    (!question.is_empty()).then(|| {
        div()
            .text_size(px(13.0 * p.scale))
            .line_height(px(RECEIPT_QUESTION_LINE * p.scale))
            .text_color(p.muted)
            .child(question)
            .into_any_element()
    })
}

/// One answered question in a receipt: its number, the question (muted), and under it the plain
/// answer and whatever follows it (an option's description, a note, "Show all options").
pub(super) fn receipt_row(
    number: usize,
    question: Option<AnyElement>,
    answer: Vec<AnyElement>,
    below: Vec<AnyElement>,
    first: bool,
    p: &ChatAppearance,
) -> AnyElement {
    let s = p.scale;
    let question_lead = (SINGLE_LINE_CARD.tile_size() - RECEIPT_QUESTION_LINE) / 2.0;
    div()
        .flex()
        .items_start()
        .gap(px(10.0 * s))
        .w_full()
        .min_w_0()
        .py(px(10.0 * s))
        .when(!first, |row| {
            row.border_t_1().border_color(p.foreground.opacity(0.05))
        })
        // The card's own tile with the number in it, so the question and answer line up under
        // the title (`SINGLE_LINE_CARD.number_tile`).
        .child(SINGLE_LINE_CARD.number_tile(number, p))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                // The question's first line sits centred on the tile.
                .pt(px(question_lead * s))
                .when_some(question, |column, question| column.child(question))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0 * s))
                        .min_w_0()
                        .children(answer),
                )
                .children(below),
        )
        .into_any_element()
}

/// The muted words beside or under an answer.
pub(super) fn answer_aside(words: String, italic: bool, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .min_w_0()
        .text_size(px(12.5 * s))
        .line_height(px(18.0 * s))
        .text_color(p.muted)
        .when(italic, |aside| aside.italic())
        .child(words)
        .into_any_element()
}
