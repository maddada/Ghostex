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
    is_tool_only_blocks(&message.blocks, |block| {
        matches!(
            block,
            ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
        )
    })
}

/// [`is_tool_only_message`] for a row in another shape: every one of `blocks`, and at least one,
/// is a tool call or result, as `is_tool` tells. gxserver reads its own rows with it.
pub fn is_tool_only_blocks<B>(blocks: &[B], is_tool: impl Fn(&B) -> bool) -> bool {
    !blocks.is_empty() && blocks.iter().all(is_tool)
}

/// What a row does to a run of tool calls in [`fold_tool_messages`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolBurstStep {
    /// The fold passes over it: the run goes on past it.
    PassesOver,
    /// A tool-only row: it joins the run, which folds into the row before it when that is an
    /// assistant or reasoning row, and otherwise starts at it.
    Joins,
    /// Any other row ends the run and may start the next one.
    Ends,
}

/// The step `message` takes in [`fold_tool_messages`]; `passes_over` is that fold's
/// `is_transparent` for it.
pub fn tool_burst_step(message: &ChatMessage, passes_over: bool) -> ToolBurstStep {
    tool_burst_step_for(passes_over, is_tool_only_message(message), &message.role)
}

/// [`tool_burst_step`] from the facts it reads: whether the fold passes over the row, whether
/// all of its blocks are tool calls and results ([`is_tool_only_blocks`]), and its role. gxserver
/// reads the same rule on the rows as it holds them, to keep a call with the results the chat
/// pairs with it (`session_chat_history.rs`).
pub fn tool_burst_step_for(passes_over: bool, tool_only: bool, role: &ChatRole) -> ToolBurstStep {
    if passes_over {
        ToolBurstStep::PassesOver
    } else if tool_only && *role != ChatRole::User {
        // A `!` shell command is the user's own turn drawn as a tool card, never the agent's work.
        ToolBurstStep::Joins
    } else {
        ToolBurstStep::Ends
    }
}

/// Whether a run of tool-only rows right after a row of `role` folds into that row; after any
/// other row the run starts at its own first row.
pub fn takes_tool_burst(role: &ChatRole) -> bool {
    matches!(role, ChatRole::Assistant | ChatRole::Reasoning)
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
        let step = tool_burst_step(message, is_transparent(message));
        if step == ToolBurstStep::PassesOver {
            output.push(message.clone());
            continue;
        }
        if step == ToolBurstStep::Joins
            && anchor_index.is_some_and(|index| takes_tool_burst(&output[index].role))
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
