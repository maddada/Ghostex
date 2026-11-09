//! Message spacing (`chatMessageSpacing`, gx-chat-core `transcript/chat_look.rs`): at Compact the
//! transcript's rows sit 8px apart instead of 16px, a message's hover buttons sit in a 20px footer
//! row kept under it (instead of today's 24px row and its 4px gap), a quiet line sits closer to
//! the agent text above it, and the gaps inside a row shrink by the same ratio (`inner_gap`). At Default nothing here changes a size.

use super::{appearance::ChatAppearance, state::NativeChatView};
use ghostex_gx_chat_core::transcript::chat_look::MessageSpacing;
use gpui::{
    Context, InteractiveElement as _, StatefulInteractiveElement as _, Styled as _, div, px,
};

/// The space every transcript row ends with at Compact (Default: `rowGap` in
/// packages/gx-chat-core/visual/transcript-layout.json).
const COMPACT_ROW_GAP: f32 = 8.0;
/// The space between the agent's text and a quiet line drawn under it in the same message at
/// Compact (Default: the message's part gap).
const COMPACT_QUIET_TEXT_GAP: f32 = 6.0;

impl ChatAppearance {
    /// Message spacing is Compact.
    pub(crate) fn compact(&self) -> bool {
        self.chat_look.spacing == MessageSpacing::Compact
    }

    /// The space every transcript row ends with, which is also the space between two one-line
    /// cards that follow each other (`SINGLE_LINE_CARD.gap_above`).
    pub(super) fn row_gap(&self) -> f32 {
        if self.compact() {
            COMPACT_ROW_GAP
        } else {
            super::transcript_layout::LAYOUT.row_gap
        }
    }

    /// A gap inside a row (between a finished turn's parts, under a thinking, side question or
    /// status row), shrunk at Compact by the row gap's own ratio (16px to 8px): `default` at
    /// Default.
    pub(super) fn inner_gap(&self, default: f32) -> f32 {
        if self.compact() {
            default * COMPACT_ROW_GAP / super::transcript_layout::LAYOUT.row_gap
        } else {
            default
        }
    }

    /// How far a quiet line under the agent's text in the same message is pulled up from the
    /// message's part gap: nothing at Default.
    pub(super) fn quiet_line_lift(&self) -> f32 {
        if self.compact() {
            super::transcript::MESSAGE_PART_GAP - COMPACT_QUIET_TEXT_GAP
        } else {
            0.0
        }
    }
}

/// The footer row under every message at Compact, before the zoom: its space is always kept, so
/// its buttons never cover text and showing them moves nothing.
pub(super) const FOOTER_HEIGHT: f32 = 20.0;
/// A footer button's square at Compact.
pub(super) const FOOTER_BUTTON: f32 = 20.0;
/// How much of the muted colour the footer's icons, time and work button show at rest.
pub(super) const FOOTER_DIM: f32 = 0.65;

impl NativeChatView {
    /// Whether a message's footer row shows its buttons at Compact: while the pointer is on the
    /// message or it holds keyboard focus.
    pub(super) fn message_bar_shown(&self, id: &str, focused: bool) -> bool {
        focused || self.hovered_message.as_deref() == Some(id)
    }

    /// Forgets which message the pointer was on, after the transcript jumped or the pane was
    /// hidden: no footer shows its buttons on a message the pointer is not on.
    pub(super) fn clear_message_hover(&mut self) {
        self.hovered_message = None;
    }

    /// Measures every row again when the chat look moved: a row whose projected value stays the
    /// same is left out of the core's splice (`splice` in gx-chat-core `core.rs`), but its height
    /// changed with Message spacing's gaps, or with the width Band or Left aligned gives your
    /// message. Rows off screen would keep their old heights and the scroll would drift.
    pub(super) fn remeasure_on_look_change(&mut self, before: &serde_json::Value) {
        if before["chatLook"] != self.snapshot["chatLook"] {
            self.clear_message_hover();
            self.list.remeasure();
            self.subagent_list.remeasure();
        }
    }

    /// Records the pointer entering and leaving a message row that has a footer at Compact, so
    /// its buttons are built only while they show.
    pub(super) fn track_message_hover(
        &self,
        row: gpui::Stateful<gpui::Div>,
        id: &str,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = id.to_string();
        row.on_hover(cx.listener(move |chat, hovered: &bool, _, cx| {
            let current = chat.hovered_message.as_deref() == Some(id.as_str());
            if *hovered != current {
                chat.hovered_message = hovered.then(|| id.clone());
                cx.notify();
            }
        }))
    }
}

/// A message's footer row at Compact: its 20px always kept under the message, inside the
/// message's own row, holding nothing until the message is hovered or focused (or its work is
/// open).
///
/// CDXC:SessionChat 2026-11-07 WHY:
/// The buttons are built only while they show (`message_bar_shown`), so a resting footer has no hitboxes: no pointer cursor, tooltip or click over an invisible button. Keyboard users reach them through the reply or prompt itself, a tab stop at Compact (`focusable_reply`). The footer replaces the floating bar Compact first drew over the message's bottom edge (the hover footer DECISION in packages/gx-chat-core/src/transcript/chat_look.rs); do not lay the buttons over the text again.
///
/// CDXC:SessionChat 2026-11-07 SEE-ALSO:
/// packages/gx-chat-core/src/transcript/chat_look.rs (the user decision), apps/mobile/app/src/chat/native/transcript/MessageActions.tsx (the phone's footer, which always shows its time and work button).
pub(super) fn message_footer(p: &ChatAppearance) -> gpui::Div {
    let s = p.scale;
    div()
        .w_full()
        .min_w_0()
        .h(px(FOOTER_HEIGHT * s))
        .flex()
        .items_center()
        .gap(px(8.0 * s))
}

/// A reply or prompt of yours at Compact: a tab stop of its own, since its footer's buttons only
/// exist while they show, with the chat's 3px focus ring at a fifth of the ring colour.
pub(super) fn focusable_reply(
    row: gpui::Stateful<gpui::Div>,
    p: &ChatAppearance,
) -> gpui::Stateful<gpui::Div> {
    let s = p.scale;
    let ring = p.ring;
    row.tab_index(0).focus_visible(move |style| {
        let mut style = style.rounded(px(6.0 * s));
        // The 3px halo, then the 1px ring line the chat's controls draw as their border, here as
        // a shadow so focusing the message moves nothing.
        let shadow = |color: gpui::Hsla, spread: f32| gpui::BoxShadow {
            color,
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(spread * s),
            inset: false,
        };
        style.box_shadow = Some(vec![shadow(ring.opacity(0.2), 3.0), shadow(ring, 1.0)]);
        style
    })
}
