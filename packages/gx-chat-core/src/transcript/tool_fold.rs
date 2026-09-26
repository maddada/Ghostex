//! FIFO tool-call/result pairing, the fold of consecutive tool-only messages, and the prose/tool
//! split of one message's blocks.
//!
//! Ported from `packages/core-ui/chat/session-chat-tool-fold.ts`. A result pairs with the call its
//! `callId` names (Claude and Codex record one; their parallel calls finish out of order), and a
//! result without one with the oldest call still unanswered, in document order.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};

/// One call with the result that answered it; either half can be missing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolPair<'a> {
    pub call: Option<&'a ChatBlock>,
    pub result: Option<&'a ChatBlock>,
}

impl<'a> ToolPair<'a> {
    /// The call's tool name, or `None` when the pair is an orphan result.
    pub fn call_name(&self) -> Option<&'a str> {
        match self.call {
            Some(ChatBlock::ToolCall { name, .. }) => Some(name),
            _ => None,
        }
    }

    /// The call's arguments.
    pub fn call_input(&self) -> Option<&'a serde_json::Value> {
        match self.call {
            Some(ChatBlock::ToolCall { input, .. }) => Some(input),
            _ => None,
        }
    }

    /// The result's text.
    pub fn result_output(&self) -> Option<&'a str> {
        match self.result {
            Some(ChatBlock::ToolResult { output, .. }) => Some(output),
            _ => None,
        }
    }

    /// Whether the result reported a failure.
    pub fn result_is_error(&self) -> bool {
        matches!(
            self.result,
            Some(ChatBlock::ToolResult {
                is_error: Some(true),
                ..
            })
        )
    }
}

pub fn is_tool_only_message(message: &ChatMessage) -> bool {
    !message.blocks.is_empty()
        && message.blocks.iter().all(|block| {
            matches!(
                block,
                ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
            )
        })
}

/// Folds consecutive tool-only messages INTO their preceding assistant or reasoning turn.
///
/// A reasoning summary therefore owns the tool activity that immediately follows it, independent of
/// which agent emitted the transcript. `is_transparent` marks rows that render as their own thing
/// (collapsed harness markers) but must not break a fold run: a system reminder injected between an
/// assistant turn and its tool rows would otherwise strand the tools in a separate bubble.
pub fn fold_tool_messages(
    messages: &[ChatMessage],
    is_transparent: &dyn Fn(&ChatMessage) -> bool,
) -> Vec<ChatMessage> {
    let mut output: Vec<ChatMessage> = Vec::with_capacity(messages.len());
    let mut anchor_index: Option<usize> = None;
    for message in messages {
        if is_transparent(message) {
            output.push(message.clone());
            continue;
        }
        let anchor_role = anchor_index.map(|index| output[index].role.clone());
        if is_tool_only_message(message)
            && matches!(
                anchor_role,
                Some(ChatRole::Assistant) | Some(ChatRole::Reasoning)
            )
        {
            let index = anchor_index.expect("an anchor role implies an anchor index");
            output[index].blocks.extend(message.blocks.iter().cloned());
            continue;
        }
        output.push(message.clone());
        anchor_index = Some(output.len() - 1);
    }
    output
}

/// Per-message-block-list pairing: by call id when the result names one, else first in first out.
///
/// A result whose id names no call in the list stays an orphan rather than taking another call's
/// slot. Takes an iterator rather than a slice because the file-change split hands back a filtered
/// list of borrowed blocks, which is then paired a second time.
pub fn pair_tool_blocks<'a>(blocks: impl IntoIterator<Item = &'a ChatBlock>) -> Vec<ToolPair<'a>> {
    let mut pairs: Vec<ToolPair<'a>> = Vec::new();
    let mut call_slots: Vec<usize> = Vec::new();
    let mut slots_by_id: std::collections::HashMap<&'a str, usize> = Default::default();
    let mut oldest_open = 0;
    for block in blocks {
        match block {
            ChatBlock::ToolCall { call_id, .. } => {
                if let Some(id) = call_id.as_deref() {
                    slots_by_id.entry(id).or_insert(pairs.len());
                }
                call_slots.push(pairs.len());
                pairs.push(ToolPair {
                    call: Some(block),
                    result: None,
                });
            }
            ChatBlock::ToolResult { call_id, .. } => {
                let slot = match call_id.as_deref() {
                    Some(id) => slots_by_id
                        .get(id)
                        .copied()
                        .filter(|slot| pairs[*slot].result.is_none()),
                    None => {
                        while call_slots
                            .get(oldest_open)
                            .is_some_and(|slot| pairs[*slot].result.is_some())
                        {
                            oldest_open += 1;
                        }
                        call_slots.get(oldest_open).copied()
                    }
                };
                match slot {
                    Some(slot) => pairs[slot].result = Some(block),
                    None => pairs.push(ToolPair {
                        call: None,
                        result: Some(block),
                    }),
                }
            }
            _ => {}
        }
    }
    pairs
}

/// The prose blocks and the tool blocks of one message, each in their original order.
pub fn split_blocks(blocks: &[ChatBlock]) -> (Vec<&ChatBlock>, Vec<&ChatBlock>) {
    let mut prose = Vec::new();
    let mut tools = Vec::new();
    for block in blocks {
        if matches!(
            block,
            ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
        ) {
            tools.push(block);
        } else {
            prose.push(block);
        }
    }
    (prose, tools)
}
