//! A turn's prose, joined the way the renderers read it.
//!
//! Ported from `packages/shared/session-chat-presentation/prose-blocks.ts`.

use ghostex_gx_protocol::ChatBlock;

/// CDXC:SessionChat 2026-09-18 WHY:
/// A turn's text blocks are the paragraphs the agent wrote, not one run of characters. Joining them
/// with "" ran the last line of one block into the first line of the next and lost every paragraph
/// break a multi-block reply had, while React joined the same blocks with a blank line. The
/// renderers read a turn's prose through here so a block boundary is the same blank line React
/// drew.
pub fn prose_markdown(blocks: &[ChatBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Every text block run together, which is what the harness classifier reads.
///
/// Ported from `sessionChatMessageText` in `packages/core-ui/chat/session-chat-noise.ts`; it joins
/// with "" rather than a blank line because it is looking for a leading tag, not for paragraphs.
pub fn joined_text(blocks: &[ChatBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>()
}
