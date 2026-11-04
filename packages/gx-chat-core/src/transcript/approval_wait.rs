//! The tool call an approval is waiting on, opened in the transcript so its command (or, for a
//! file edit, its diff) shows beside the approval card.
//!
//! SEE-ALSO: `apps/desktop/src/app/native_chat/tool_run.rs`, `file_change_card.rs` and
//! `apps/mobile/app/src/chat/native/transcript/ToolRows.tsx`, `FileChanges.tsx`, `MessageRow.tsx`
//! read `awaitingApproval` as the row's open default, and record a reader's own open or close
//! against it.

use serde_json::Value;

use crate::document::TranscriptItem;
use crate::questions::gates::{notice_visible, prompt_key};
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::notice_dismiss_key;
use crate::state::ChatState;
use crate::transcript::file_changes::file_changes;
use crate::transcript::presentation::{message_tool_rows, ProjectionCache};
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks, ToolPair};
use crate::transcript::tool_rows::tool_fold_label;

/// The agent is blocked on a tool approval. `tool_use_id` names the call when gxserver knew it
/// (a hook approval); a screen permission dialog alone names none, and the newest call still
/// waiting for its result is the one it asks about.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalWait {
    pub tool_use_id: Option<String>,
}

/// The approval the agent is blocked on, while its card is still up.
///
/// A screen permission dialog decides when there is one: its rows are the card, and answering it
/// (here, in the terminal, or anywhere else) takes the wait away. Without one the hook approval
/// decides, until its answer goes out.
pub fn approval_wait(state: &ChatState) -> Option<ApprovalWait> {
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let approval = matches!(prompt, Some(InteractivePrompt::Approval { .. }));
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref())
        .filter(|notice| notice.kind == "permissionPrompt");
    let waiting = match &notice {
        Some(notice) => {
            notice_visible(state)
                && notice_dismiss_key(Some(notice)) != state.questions.retired_notice_key
        }
        None => approval && prompt_key(prompt.as_ref()) != state.questions.dismissed_prompt,
    };
    waiting.then(|| ApprovalWait {
        tool_use_id: prompt
            .as_ref()
            .filter(|_| approval)
            .and_then(InteractivePrompt::tool_use_id)
            .map(str::to_string),
    })
}

/// Where the waiting call is drawn: the item, the row inside a summary turn's work, and the call's
/// place in that row: its index in `tools`, and the `files` rows its writes became.
struct Target {
    item: usize,
    work: Option<usize>,
    place: Place,
}

/// The waiting call's rows inside one message. A write is drawn as file rows (one per file it
/// touches) and an `exec` wrapper as both.
struct Place {
    tool: Option<usize>,
    files: std::ops::Range<usize>,
}

/// What one projected row says about the waiting call.
enum Search {
    Found(Place),
    /// The newest row that called anything holds no waiting call: an older call is not the one.
    Stop,
    NotHere,
}

/// Where `call` is drawn among the message's tool rows and file rows, counted in the order
/// `split_file_changes` builds them.
fn place_of(
    source: &ghostex_gx_protocol::ChatMessage,
    call: &ghostex_gx_protocol::ChatBlock,
) -> Search {
    let (_, rows) = message_tool_rows(source);
    let tool = rows.iter().position(|pair| {
        pair.call
            .is_some_and(|row_call| std::ptr::eq(row_call, call))
    });
    let mut offset = 0;
    let mut files = 0..0;
    for pair in calls(source) {
        let count = match pair.call {
            Some(ghostex_gx_protocol::ChatBlock::ToolCall { name, input, .. }) => {
                file_changes(name, input).len()
            }
            _ => 0,
        };
        if pair
            .call
            .is_some_and(|pair_call| std::ptr::eq(pair_call, call))
        {
            files = offset..offset + count;
            break;
        }
        offset += count;
    }
    if tool.is_none() && files.is_empty() {
        return Search::Stop;
    }
    Search::Found(Place { tool, files })
}

fn calls(source: &ghostex_gx_protocol::ChatMessage) -> Vec<ToolPair<'_>> {
    let (_, tools) = split_blocks(&source.blocks);
    pair_tool_blocks(tools)
}

/// The call named `id` in this row.
fn by_id(source: &ghostex_gx_protocol::ChatMessage, id: &str) -> Search {
    let found = calls(source).into_iter().find_map(|pair| match pair.call {
        Some(
            call @ ghostex_gx_protocol::ChatBlock::ToolCall {
                call_id: Some(call_id),
                ..
            },
        ) if call_id == id => Some(call),
        _ => None,
    });
    match found {
        Some(call) => place_of(source, call),
        None => Search::NotHere,
    }
}

/// The newest call of this row still waiting for its result, when the row made any call at all.
fn newest_open_call(source: &ghostex_gx_protocol::ChatMessage) -> Search {
    let pairs = calls(source);
    if !pairs.iter().any(|pair| pair.call.is_some()) {
        return Search::NotHere;
    }
    match pairs
        .iter()
        .rev()
        .find(|pair| pair.call.is_some() && pair.result.is_none())
        .and_then(|pair| pair.call)
    {
        Some(call) => place_of(source, call),
        // The newest row that called anything has every answer back: the waiting call is not in
        // the transcript yet, and an older finished call is not what the approval asks about.
        None => Search::Stop,
    }
}

/// The rows the list draws, newest first, each with where it sits.
fn rows_newest_first(items: &[TranscriptItem]) -> Vec<(usize, Option<usize>, &Value)> {
    let mut rows = Vec::new();
    for (item_index, item) in items.iter().enumerate().rev() {
        match item {
            TranscriptItem::Message { message } => rows.push((item_index, None, &**message)),
            TranscriptItem::Summary { work, .. } => {
                for (work_index, message) in work.iter().enumerate().rev() {
                    rows.push((item_index, Some(work_index), &**message));
                }
            }
            // A finished turn's work holds no call an approval still waits on.
            TranscriptItem::CompletedWork { .. } | TranscriptItem::Unknown(_) => {}
        }
    }
    rows
}

/// The rows newest first, until one of them answers `find`: `Some(target or none)` once the search
/// ends, `None` when no row did. A target comes with the place in `rows` of the row it is in.
fn search(
    rows: &[(usize, Option<usize>, &Value)],
    cache: &ProjectionCache,
    find: &dyn Fn(&ghostex_gx_protocol::ChatMessage) -> Search,
) -> Option<Option<(usize, Target)>> {
    for (at, (item, work, message)) in rows.iter().enumerate() {
        let Some(entry) = message
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| cache.get(id))
        else {
            continue;
        };
        match find(&entry.source) {
            Search::Found(place) => {
                return Some(Some((
                    at,
                    Target {
                        item: *item,
                        work: *work,
                        place,
                    },
                )))
            }
            Search::Stop => return Some(None),
            Search::NotHere => {}
        }
    }
    None
}

fn locate(
    rows: &[(usize, Option<usize>, &Value)],
    cache: &ProjectionCache,
    wait: &ApprovalWait,
) -> Option<(usize, Target)> {
    if let Some(id) = &wait.tool_use_id {
        // Claude's hook names the transcript's own call id; an agent whose approval id names no
        // call (OpenCode's permission ids) is matched like a screen dialog.
        if let Some(found) = search(rows, cache, &|source| by_id(source, id)) {
            return found;
        }
    }
    search(rows, cache, &newest_open_call).flatten()
}

/// CDXC:SessionChat 2026-10-15 DECISION: User (a Ghostex customer): "when I run Claude Code in manual Mode I always have to expand the bash commands to see what ghostex is asking permission for. (in chat view) It would be better if we expand to show the last command so i know what it needs approval for. (collapse after i approve)". While an agent waits on a tool approval, the call it waits on starts open (`awaitingApproval` on the tool row, `toolsOpen` on its message, and the run fold and a summary turn's work fold keep it in view), so the command shows beside the approval card without a click; a file edit or write waiting the same way (Edit, Write and the like, drawn as file cards) opens its file rows to the diff (`awaitingApproval` on each, user 2026-10-15). Once the approval is answered, from the chat, the terminal or anywhere, the mark goes and the row folds back to how it normally draws; a row the reader opened or closed by hand while it waited keeps their choice, because the renderers record that against this default.
pub fn open_waiting_call(
    items: &mut [TranscriptItem],
    cache: &ProjectionCache,
    wait: Option<&ApprovalWait>,
) {
    let Some(wait) = wait else { return };
    if let Some((_, target)) = locate(&rows_newest_first(items), cache, wait) {
        mark_waiting_call(items, target);
    }
}

/// [`open_waiting_call`] over a list drawn as `before` (older rows, oldest first, drawn already)
/// and then `items` (`transcript/settled.rs`). Marks the waiting call when it is among `items`;
/// `false` when it is among `before`, which would have to be drawn again with the mark.
pub fn open_waiting_call_in(
    items: &mut [TranscriptItem],
    before: &[&[TranscriptItem]],
    cache: &ProjectionCache,
    wait: Option<&ApprovalWait>,
) -> bool {
    let Some(wait) = wait else { return true };
    let mut rows = rows_newest_first(items);
    let newest = rows.len();
    // The newest rows end most searches; the older rows are read only when they do not.
    let ended = match &wait.tool_use_id {
        Some(id) => search(&rows, cache, &|source| by_id(source, id)),
        None => search(&rows, cache, &newest_open_call),
    }
    .is_some();
    if !ended {
        for older in before.iter().rev() {
            rows.extend(rows_newest_first(older));
        }
    }
    match locate(&rows, cache, wait) {
        Some((at, _)) if at >= newest => false,
        Some((_, target)) => {
            drop(rows);
            mark_waiting_call(items, target);
            true
        }
        None => true,
    }
}

/// Opens the waiting call where [`locate`] found it, in the row's own copy of its model.
fn mark_waiting_call(items: &mut [TranscriptItem], target: Target) {
    let message = match (&mut items[target.item], target.work) {
        (TranscriptItem::Message { message }, None) => message,
        (
            TranscriptItem::Summary {
                work, work_open, ..
            },
            Some(index),
        ) => {
            *work_open = true;
            &mut work[index]
        }
        _ => return,
    };
    // A row with nothing to open keeps the model it shares with the projection cache.
    if !message.is_object() || (target.place.files.is_empty() && target.place.tool.is_none()) {
        return;
    }
    let Value::Object(fields) = std::sync::Arc::make_mut(message) else {
        return;
    };
    // A write waiting on its approval opens its file rows to the diff it asks to make.
    if let Some(Value::Array(files)) = fields.get_mut("files") {
        for file in files
            .iter_mut()
            .take(target.place.files.end)
            .skip(target.place.files.start)
        {
            if let Value::Object(file) = file {
                file.insert("awaitingApproval".to_string(), Value::Bool(true));
            }
        }
    }
    crate::transcript::tool_card::mark_card_waiting(
        fields,
        target.place.tool,
        target.place.files.clone(),
    );
    let Some(tool) = target.place.tool else {
        return;
    };
    fields.insert("toolsOpen".to_string(), Value::Bool(true));
    if let Some(Value::Object(row)) = fields
        .get_mut("tools")
        .and_then(|tools| tools.get_mut(tool))
    {
        row.insert("awaitingApproval".to_string(), Value::Bool(true));
    }
    // The "+N previous tool calls" fold keeps only the last call in view; a waiting call earlier in
    // the run (parallel calls) stays out of it.
    if let Some(Value::Object(fold)) = fields.get_mut("toolFold") {
        let hidden = fold
            .get("visible")
            .and_then(|visible| visible.get(tool))
            .is_some_and(|shown| shown == &Value::Bool(false));
        if hidden {
            if let Some(Value::Array(visible)) = fold.get_mut("visible") {
                visible[tool] = Value::Bool(true);
            }
            let count = fold
                .get("hiddenCount")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .saturating_sub(1);
            fold.insert("hiddenCount".to_string(), count.into());
            fold.insert(
                "collapsedLabel".to_string(),
                tool_fold_label(count as usize).into(),
            );
        }
    }
}
