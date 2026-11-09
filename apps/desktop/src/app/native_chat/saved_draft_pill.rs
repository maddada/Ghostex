//! The Saved draft pill, pinned at the left of the chat box's pill strip (pills_strip.rs) while a
//! draft of this chat saved on another device or in the terminal is offered (`savedDraftPill`,
//! gx-chat-core extras/chat_pills.rs `saved_draft_pill`, where the user's decision lives). A click
//! opens its menu above the box like the other pills' menus: the draft's first lines, then Dismiss
//! and Use draft. It replaced the "Another saved draft is available" row above the box.
//!
//! The pill never moves: where the box is narrow it reads "Draft", and narrower still it is its
//! icon alone (`pill_label`).
//!
//! CDXC:Drafts 2026-11-04 SEE-ALSO:
//! The phone draws the same pill and a sheet in apps/mobile/app/src/chat/native/cards/SavedDraftPill.tsx.

use super::{appearance::ChatAppearance, cursor::ChatCursor as _, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};
use serde_json::{Value, json};

/// The pill's menu id and its menu-toggle key (chat_pills.rs `trigger_id`).
pub(super) const SAVED_DRAFT_MENU: &str = "savedDraft";
pub(super) const SAVED_DRAFT_TRIGGER: &str = "chat-pill-saved-draft";

/// The menu's width (unscaled).
pub(super) const SAVED_DRAFT_MENU_WIDTH: f32 = 360.0;

/// Below this chat box width (unscaled) the pill reads "Draft"; below `ICON_ONLY_BELOW`, its icon
/// alone. The phone uses the same widths (SavedDraftPill.tsx).
const SHORT_LABEL_BELOW: f32 = 480.0;
const ICON_ONLY_BELOW: f32 = 300.0;

/// The preview's line height, its most lines, and the characters a line holds at the menu's width,
/// which sizes the menu before the text is laid out (a longer guess leaves a little room, a shorter
/// one ends the preview with an ellipsis).
const PREVIEW_LINE_HEIGHT: f32 = 18.0;
const PREVIEW_MAX_LINES: usize = 6;
const PREVIEW_LINE_CHARS: usize = 50;
/// The preview box's padding above and below, and the footer row's height.
const PREVIEW_PAD_Y: f32 = 8.0;
const FOOTER_HEIGHT: f32 = 28.0;

/// How the pill reads at a chat box `width` wide (unscaled).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PillLabel {
    Full,
    Short,
    IconOnly,
}

/// The pill's label for a chat box `width` wide; an unmeasured box (0, before the first paint)
/// reads in full.
pub(super) fn pill_label(width: f32) -> PillLabel {
    if width <= 0.0 || width >= SHORT_LABEL_BELOW {
        PillLabel::Full
    } else if width >= ICON_ONLY_BELOW {
        PillLabel::Short
    } else {
        PillLabel::IconOnly
    }
}

/// The lines the preview takes at the menu's width: each paragraph's wrapped lines, at most
/// `PREVIEW_MAX_LINES`.
pub(super) fn preview_lines(preview: &str) -> usize {
    preview
        .split('\n')
        .map(|line| line.chars().count().div_ceil(PREVIEW_LINE_CHARS).max(1))
        .sum::<usize>()
        .clamp(1, PREVIEW_MAX_LINES)
}

impl NativeChatView {
    fn saved_draft(&self) -> Option<&Value> {
        Some(&self.snapshot["savedDraftPill"]).filter(|pill| pill.is_object())
    }

    /// Whether a draft is offered, which alone shows the strip.
    pub(super) fn saved_draft_offered(&self) -> bool {
        self.saved_draft().is_some()
    }

    /// The pill, in the panel pills' shape with the ? pill's amber corner dot.
    pub(super) fn saved_draft_pill_element(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let pill = self.saved_draft()?;
        let s = p.scale;
        let width = f32::from(self.pills_strip.chat_box().size.width) / s;
        let label = match pill_label(width) {
            PillLabel::Full => pill["label"].as_str(),
            PillLabel::Short => pill["shortLabel"].as_str(),
            PillLabel::IconOnly => None,
        }
        .map(str::to_owned);
        let open = self.chat_menu_is_open(SAVED_DRAFT_TRIGGER);
        let bounds = self.pill_bounds.saved_draft.clone();
        let measured = bounds.clone();
        Some(
            div()
                .id(SAVED_DRAFT_TRIGGER)
                .relative()
                .role(gpui::Role::Button)
                .aria_label(pill["ariaLabel"].as_str().unwrap_or_default().to_owned())
                .aria_expanded(open)
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(5.0 * s))
                // Icon alone, it keeps room on the right for the corner dot.
                .pl(px(if label.is_some() { 8.0 } else { 7.0 } * s))
                .pr(px(if label.is_some() { 8.0 } else { 10.0 } * s))
                .py(px(2.0 * s))
                .rounded_full()
                .border(px(s))
                .border_color(p.foreground.opacity(0.1))
                .text_size(px(13.0 * s))
                .line_height(px(19.0 * s))
                .whitespace_nowrap()
                .text_color(p.foreground)
                .chat_cursor_pointer()
                .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .child(
                    svg()
                        .path("titlebar/file-text.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.card_muted),
                )
                .children(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_pill_menu(SAVED_DRAFT_MENU, bounds.get(), window, cx);
                }))
                // Its border box, which the menu lines up with (as `chat_pill` measures).
                .child(
                    canvas(move |painted, _, _| measured.set(painted), |_, _, _, _| {})
                        .absolute()
                        .inset(px(-s)),
                )
                .into_any_element(),
        )
    }

    /// The menu's content height (unscaled), or `None` once the draft is no longer offered, which
    /// closes an open menu (pill_menu.rs `pill_menu_changed`).
    pub(super) fn saved_draft_menu_height(&self) -> Option<f32> {
        let pill = self.saved_draft()?;
        let lines = preview_lines(pill["preview"].as_str().unwrap_or_default());
        Some(
            super::chat_pills::MENU_HEADER_HEIGHT
                + super::chat_pills::MENU_SECTION_GAP
                + lines as f32 * PREVIEW_LINE_HEIGHT
                + 2.0 * PREVIEW_PAD_Y
                + super::chat_pills::MENU_SECTION_GAP
                + FOOTER_HEIGHT,
        )
    }

    /// The menu: "Saved draft" and where it came from, the draft's first lines, then the hint (when
    /// Use draft would replace typed text), Dismiss and Use draft.
    pub(super) fn saved_draft_menu_body(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pill = self.saved_draft()?.clone();
        let s = p.scale;
        let text = |key: &str| pill[key].as_str().unwrap_or_default().to_owned();
        let preview = text("preview");
        let lines = preview_lines(&preview);
        let hint = text("hint");
        let header = super::chat_pills::pill_menu_header(
            "titlebar/file-text.svg",
            "Saved draft",
            text("detail"),
            None,
            p,
        );
        let body = div()
            .flex_shrink_0()
            .px(px(10.0 * s))
            .py(px(PREVIEW_PAD_Y * s))
            .rounded(px(8.0 * s))
            .bg(p.foreground.opacity(0.05))
            .text_size(px(12.5 * s))
            .line_height(px(PREVIEW_LINE_HEIGHT * s))
            .text_color(p.foreground)
            .child(
                div()
                    .h(px(lines as f32 * PREVIEW_LINE_HEIGHT * s))
                    .overflow_hidden()
                    // GPUI only clamps when the text also asks for an ellipsis (system_cards.rs).
                    .line_clamp(lines)
                    .text_ellipsis()
                    .child(preview),
            )
            .into_any_element();
        let footer = div()
            .flex_shrink_0()
            .h(px(FOOTER_HEIGHT * s))
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .pl(px(4.0 * s))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.5 * s))
                    .text_color(p.card_muted)
                    .child(hint),
            )
            .child(self.saved_draft_button("dismiss", "Dismiss", false, p, cx))
            .child(self.saved_draft_button("use", "Use draft", true, p, cx))
            .into_any_element();
        Some(super::chat_pills::pill_menu_column(
            "chat-saved-draft-menu",
            "Saved draft",
            header,
            Some(body),
            Some(footer),
            p,
        ))
    }

    /// Dismiss (a quiet text button) or Use draft (filled); either answers the offer and closes
    /// the menu.
    fn saved_draft_button(
        &self,
        id: &'static str,
        label: &'static str,
        primary: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let command = if primary {
            json!({"type": "useIncomingDraft"})
        } else {
            json!({"type": "dismissIncomingDraft"})
        };
        div()
            .id(gpui::ElementId::Name(format!("saved-draft-{id}").into()))
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(24.0 * s))
            .px(px(10.0 * s))
            .rounded(px(7.0 * s))
            .text_size(px(12.0 * s))
            .chat_cursor_pointer()
            .map(|button| {
                if primary {
                    button
                        .bg(p.control_primary)
                        .text_color(p.background)
                        .font_weight(FontWeight::SEMIBOLD)
                        .hover(|style| style.opacity(0.9))
                } else {
                    button
                        .text_color(p.card_muted)
                        .hover(|style| style.bg(p.foreground.opacity(0.06)))
                }
            })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.close_pill_menu(cx);
                this.invoke(command.clone(), cx);
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{PillLabel, pill_label, preview_lines};

    #[test]
    fn the_pill_shortens_in_place_on_a_narrow_box() {
        assert_eq!(pill_label(0.0), PillLabel::Full, "unmeasured");
        assert_eq!(pill_label(720.0), PillLabel::Full);
        assert_eq!(pill_label(480.0), PillLabel::Full);
        assert_eq!(pill_label(479.0), PillLabel::Short);
        assert_eq!(pill_label(300.0), PillLabel::Short);
        assert_eq!(pill_label(299.0), PillLabel::IconOnly);
    }

    #[test]
    fn the_preview_takes_each_paragraph_s_wrapped_lines_up_to_six() {
        assert_eq!(preview_lines("fix it"), 1);
        assert_eq!(preview_lines(&"a".repeat(51)), 2);
        assert_eq!(preview_lines("one\n\nthree"), 3);
        assert_eq!(preview_lines(&"a".repeat(600)), 6);
    }
}
