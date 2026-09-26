/*
CDXC:SessionChat 2026-09-18 SEE-ALSO:
The hoisting rule for answered question cards, drawn by the GPUI transcript
(apps/desktop/src/app/native_chat/question_exchange.rs).
The user's answer never writes a user row: the whole ask/answer exchange lives in tool blocks
between two user turns, so without hoisting it would vanish into the collapsed "Worked for Xs"
section. The raw tool pairs stay as plain rows inside the expanded work log, so nothing renders
twice.
*/
//! Port of `packages/shared/session-chat-presentation/question-hoisting.ts`.

use ghostex_gx_protocol::{ChatBlock, ChatMessage};

use crate::questions::exchange_answers::{answered_question_exchange, QuestionExchange};

/// One hoisted exchange and the key the renderer identifies it by.
#[derive(Clone, Debug, PartialEq)]
pub struct HoistedExchange {
    pub exchange: QuestionExchange,
    /// `<message id>:<exchange index within that message>`.
    pub key: String,
}

/// The answered exchanges one message's tool blocks carry, in the order they were asked.
pub fn message_question_exchanges(message: &ChatMessage) -> Vec<QuestionExchange> {
    pair_tool_blocks(&message.blocks)
        .into_iter()
        .filter_map(|pair| match (pair.call, pair.result) {
            (Some((name, input)), Some((output, is_error))) => {
                answered_question_exchange(name, input, output, is_error)
            }
            _ => None,
        })
        .collect()
}

/// Every answered exchange a completed turn's work carries, lifted out of the fold.
pub fn hoisted_question_exchanges(work: &[ChatMessage]) -> Vec<HoistedExchange> {
    let mut out = Vec::new();
    for message in work {
        for (index, exchange) in message_question_exchanges(message).into_iter().enumerate() {
            out.push(HoistedExchange {
                exchange,
                key: format!("{}:{index}", message.id),
            });
        }
    }
    out
}

/// A tool call with the result that answered it, as the question hoisting reads them.
struct ToolPair<'a> {
    call: Option<(&'a str, &'a serde_json::Value)>,
    result: Option<(&'a str, bool)>,
}

/// The transcript's own pairing (`tool_fold::pair_tool_blocks`), in this module's shape.
fn pair_tool_blocks(blocks: &[ChatBlock]) -> Vec<ToolPair<'_>> {
    crate::transcript::tool_fold::pair_tool_blocks(blocks)
        .into_iter()
        .map(|pair| ToolPair {
            call: match pair.call {
                Some(ChatBlock::ToolCall { name, input, .. }) => Some((name.as_str(), input)),
                _ => None,
            },
            result: match pair.result {
                Some(ChatBlock::ToolResult {
                    output, is_error, ..
                }) => Some((output.as_str(), *is_error == Some(true))),
                _ => None,
            },
        })
        .collect()
}
