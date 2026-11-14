//! A session pill's live extras and its hover card: the state dot and, for a session on another
//! computer, that computer's icon after the title, and the card hovering the pill shows. The rule
//! (which state, which computer counts as another, what the card says) is gx-chat-core's
//! `mention_card` (transcript/session_mentions.rs, where the user's decision lives); this file reads
//! it from the host's session list at draw time and gives it the chat's colours.
//!
//! SEE-ALSO: markdown_links.rs (the sent pill), composer_references.rs (the chat box pill),
//! inter_agent_message.rs (a message card's session name), coordinator_threads.rs (the Workers
//! rows), apps/mobile/app/src/chat/native/transcript/SessionPill.tsx (the phone).

use super::appearance::ChatAppearance;
use ghostex_gx_chat_core::transcript::session_mentions::{
    MENTION_CARD_HINT, MentionCard, mention_card, mention_card_meta,
};
use gpui::{AnyView, App, Hsla, Pixels, SharedString, Window, px, rgb};

/// The sidebar's working colour, as the Workers menu draws a working thread
/// (coordinator_threads.rs `WORKING_COLOR`).
const WORKING_COLOR: u32 = 0xc68a06;

/// One mark after a pill's title.
pub(super) enum PillMark {
    Dot {
        size: Pixels,
        color: Hsla,
        ring: Pixels,
    },
    Icon {
        path: SharedString,
        size: Pixels,
        color: Hsla,
    },
}

impl PillMark {
    pub(super) fn into_link_mark(self) -> gpui_component::text::InlineLinkMark {
        use gpui_component::text::InlineLinkMark;
        match self {
            Self::Dot { size, color, ring } => InlineLinkMark::Dot { size, color, ring },
            Self::Icon { path, size, color } => InlineLinkMark::Icon { path, size, color },
        }
    }

    pub(super) fn into_replacement_mark(self) -> gpui_component::input::InlineReplacementMark {
        use gpui_component::input::InlineReplacementMark;
        match self {
            Self::Dot { size, color, ring } => InlineReplacementMark::Dot { size, color, ring },
            Self::Icon { path, size, color } => InlineReplacementMark::Icon { path, size, color },
        }
    }

    pub(super) fn element(self) -> gpui::AnyElement {
        use gpui::{IntoElement as _, Styled as _};
        match self {
            Self::Dot { size, color, ring } if ring > Pixels::ZERO => gpui::div()
                .size(size)
                .flex_shrink_0()
                .rounded_full()
                .border(ring)
                .border_color(color)
                .into_any_element(),
            Self::Dot { size, color, .. } => gpui::div()
                .size(size)
                .flex_shrink_0()
                .rounded_full()
                .bg(color)
                .into_any_element(),
            Self::Icon { path, size, color } => gpui::svg()
                .path(path)
                .size(size)
                .flex_shrink_0()
                .text_color(color)
                .into_any_element(),
        }
    }
}

/// The card for the session `global_ref` names, from the host's list now; `None` when the list
/// does not have it.
pub(super) fn card(global_ref: &str) -> Option<MentionCard> {
    let (_, sessions) = crate::app::gx_chat::session_directory::current();
    mention_card(
        &sessions,
        global_ref,
        &crate::app::gx_chat::session_directory::home_server(),
    )
}

/// The card for the session a message card's pill names (`session` with `serverId`, `projectId`
/// and `sessionId`). Project and session ids are only unique on one gxserver, so the server part
/// picks the computer; a card written before it carried one matches the ids alone.
pub(super) fn card_for(server_id: &str, project_id: &str, session_id: &str) -> Option<MentionCard> {
    if !server_id.is_empty() {
        return card(&format!("{server_id}:{project_id}:{session_id}"));
    }
    let (_, sessions) = crate::app::gx_chat::session_directory::current();
    let session = sessions
        .iter()
        .find(|session| session.project_id == project_id && session.session_id == session_id)?;
    card(&session.global_ref)
}

/// The pill's marks after its title: the state dot, then the computer icon for a session on
/// another computer.
pub(super) fn marks(card: &MentionCard, p: &ChatAppearance) -> Vec<PillMark> {
    let s = p.scale;
    let dot = match card.dot.as_str() {
        "working" => PillMark::Dot {
            size: px(6.0 * s),
            color: rgb(WORKING_COLOR).into(),
            ring: Pixels::ZERO,
        },
        "attention" => PillMark::Dot {
            size: px(6.0 * s),
            color: rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
            ring: Pixels::ZERO,
        },
        _ => PillMark::Dot {
            size: px(6.0 * s),
            color: p.muted.opacity(0.8),
            ring: px(1.3 * s),
        },
    };
    let mut marks = vec![dot];
    if card.elsewhere {
        marks.push(PillMark::Icon {
            // That computer's own icon (CDXC:SessionChat 2026-11-14 in session_directory.rs).
            path: crate::app::gx_chat::session_directory::computer_icon(&card.server_id).into(),
            size: px(11.0 * s),
            color: p.muted,
        });
    }
    marks
}

/// The hover card's text: the title, then its state, agent, project and computer, then what a
/// click does.
pub(super) fn card_text(card: &MentionCard) -> String {
    format!(
        "{}\n{} \u{b7} {}\n{MENTION_CARD_HINT}",
        card.title,
        card.state,
        mention_card_meta(card)
    )
}

/// The hover card, the app's frosted card tooltip (the sidebar session tooltip's bubble).
pub(super) fn card_view(card: &MentionCard, window: &mut Window, cx: &mut App) -> AnyView {
    crate::app::native_sidebar::tooltips::sidebar_style_card_tooltip(
        card_text(card),
        0.0,
        0.0,
        window,
        cx,
    )
}

/// Answers the tooltip of a session pill in a message or in the chat box (both carry the session's
/// Global Ref as their tooltip text) with its hover card; anything else keeps its plain tooltip.
pub(crate) fn install(cx: &mut App) {
    gpui_component::tooltip::set_reference_cards(
        |text, window, cx| {
            ghostex_gx_chat_core::protocol::parse_global_session_ref(text.trim())?;
            let card = card(text)?;
            Some(card_view(&card, window, cx))
        },
        cx,
    );
}
