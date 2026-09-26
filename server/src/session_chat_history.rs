use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{json, Value};

use crate::session_chat::{
    SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatTranscriptAgent,
};
use crate::session_chat_fork_stitch::{
    decode_stitched_cursor, encode_stitched_cursor, read_session_chat_tail_page_stitched,
    FORK_BOUNDARY_MESSAGE_ID_PREFIX,
};
use crate::session_chat_tail::SessionChatTailPage;

/// CDXC:SessionChat 2026-09-15 DECISION:
/// User: older history should arrive collapsed on desktop, web and mobile; fetch the middle work only when expanded, instead of displaying pages of tools before their user prompt arrives.
/// Scan in bounded chunks and retain summaries while crossing a long turn, so omitted tool payloads do not accumulate in the history response.
pub(crate) fn read_history(
    agent: SessionChatTranscriptAgent,
    path: &Path,
    before: u64,
    limit: usize,
    detail: bool,
    preserve_newest: bool,
    project_id: &str,
    session_id: &str,
) -> std::io::Result<Value> {
    let archived =
        crate::session_chat_local_command::load_session_chat_local_commands(project_id, session_id);
    if detail {
        let page =
            read_session_chat_tail_page_stitched(agent, path, limit.clamp(1, 200), Some(before))?;
        return Ok(match page.page {
            SessionChatTailPage::NotFound => page_value(Vec::new(), false, before),
            SessionChatTailPage::Page {
                messages,
                has_more,
                before_offset,
                ..
            } => {
                let commands =
                    crate::session_chat_local_command::select_session_chat_local_commands(
                        archived, &messages, false, has_more,
                    );
                let messages = crate::session_chat_local_command::merge_session_chat_local_commands(
                    messages, &commands,
                );
                page_value(
                    messages.iter().map(|message| json!(message)).collect(),
                    has_more,
                    before_offset,
                )
            }
        });
    }
    let mut cursor = before;
    let mut examined = 0;
    let mut output = Vec::new();
    let mut turn = Turn::default();
    let mut preserve = preserve_newest;
    loop {
        let page = read_session_chat_tail_page_stitched(agent, path, 200, Some(cursor))?;
        let SessionChatTailPage::Page {
            messages,
            has_more,
            before_offset,
            ..
        } = page.page
        else {
            output.extend(unanchored_work(agent, path, before, turn, &archived)?);
            output.reverse();
            return Ok(page_value(output, false, cursor));
        };
        // Stitched pages can cross files. Each boundary separates an older hop
        // from the next newer one; keep the existing opaque cursor encoding.
        let commands = crate::session_chat_local_command::select_session_chat_local_commands(
            archived.clone(),
            &messages,
            false,
            has_more,
        );
        let messages = crate::session_chat_local_command::merge_session_chat_local_commands(
            messages, &commands,
        );
        let (mut hop, _) = decode_stitched_cursor(before_offset);
        let mut cursors = Vec::with_capacity(messages.len());
        for message in &messages {
            if message.id.starts_with("fork-boundary:") {
                hop = hop.saturating_sub(1);
            }
            cursors.push(encode_stitched_cursor(
                hop,
                message.byte_offset.unwrap_or(0),
            ));
        }
        for (row_index, (message, row_cursor)) in
            messages.into_iter().zip(cursors).enumerate().rev()
        {
            examined += 1;
            if message.id.starts_with("fork-boundary:") {
                output.extend(unanchored_work(agent, path, before, turn, &archived)?);
                output.push(json!(message));
                turn = Turn::default();
                continue;
            }
            if message.role == SessionChatRole::User && message.byte_offset.is_some() {
                output.extend(turn.finish(Some(message), before));
                turn = Turn::default();
                preserve = false;
                if examined >= limit.max(1) {
                    output.reverse();
                    return Ok(page_value(output, row_index > 0 || has_more, row_cursor));
                }
            } else {
                turn.push(message, preserve);
            }
        }
        if !has_more || before_offset == cursor {
            output.extend(unanchored_work(agent, path, before, turn, &archived)?);
            output.reverse();
            return Ok(page_value(output, false, before_offset));
        }
        cursor = before_offset;
    }
}

/// CDXC:SessionChat 2026-09-16 DECISION:
/// User: the initial chat snapshot carries only the latest turn in detail; every older completed turn in the tail window arrives collapsed like a history page and loads its work on expansion.
/// The detailed region starts at the newest genuine prompt (not queued, not a harness-injected row), the same boundary the message list uses for a streaming response, so a task notification or local command output landing mid-turn never folds live work.
/// Rows no prompt owns (a window that starts mid-turn, a turn cut by a fork boundary) stay verbatim, and each collapsed turn's `beforeOffset` is the cursor of the row after it so expansion finds the turn on its first detail page.
pub(crate) fn collapse_snapshot_messages(
    messages: &[SessionChatMessage],
    before_offset: u64,
) -> Vec<Value> {
    let is_prompt = |message: &SessionChatMessage| {
        message.role == SessionChatRole::User && message.byte_offset.is_some()
    };
    let Some(detail_start) = messages.iter().rposition(|message| {
        is_prompt(message)
            && !message.queued
            && !crate::session_chat_decode_claude::is_noise_message(message)
    }) else {
        return messages.iter().map(|message| json!(message)).collect();
    };
    let (mut hop, _) = decode_stitched_cursor(before_offset);
    let mut cursors = Vec::with_capacity(messages.len());
    for message in messages {
        if message.id.starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX) {
            hop = hop.saturating_sub(1);
        }
        cursors.push(encode_stitched_cursor(
            hop,
            message.byte_offset.unwrap_or(0),
        ));
    }
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for (index, message) in messages.iter().enumerate().take(detail_start) {
        let boundary = message.id.starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX);
        if boundary || is_prompt(message) {
            if index > start {
                segments.push((start, index));
            }
            if boundary {
                segments.push((index, index + 1));
                start = index + 1;
            } else {
                start = index;
            }
        }
    }
    if detail_start > start {
        segments.push((start, detail_start));
    }
    let mut output = Vec::with_capacity(messages.len());
    for (segment_start, segment_end) in segments {
        let first = &messages[segment_start];
        if first.id.starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX) || !is_prompt(first) {
            output.extend(
                messages[segment_start..segment_end]
                    .iter()
                    .map(|message| json!(message)),
            );
            continue;
        }
        let before = (segment_end..messages.len())
            .find(|index| {
                !messages[*index]
                    .id
                    .starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX)
            })
            .map(|index| cursors[index])
            .unwrap_or(before_offset);
        let mut turn = Turn::default();
        for message in messages[segment_start + 1..segment_end].iter().rev() {
            turn.push(message.clone(), false);
        }
        let mut rows = turn.finish(Some(first.clone()), before);
        rows.reverse();
        output.extend(rows);
    }
    output.extend(
        messages[detail_start..]
            .iter()
            .map(|message| json!(message)),
    );
    output
}

// A transcript can begin with work without a user prompt, or a fork can cut a
// turn in half. Such rows have no valid disclosure owner: return their original
// content instead of losing it behind a summary that cannot be expanded.
fn unanchored_work(
    agent: SessionChatTranscriptAgent,
    path: &Path,
    before: u64,
    turn: Turn,
    archived: &[crate::session_chat_local_command::SessionChatLocalCommand],
) -> std::io::Result<Vec<Value>> {
    if turn.deferred == 0 {
        return Ok(turn.finish(None, before));
    }
    let mut cursor = before;
    let mut found = false;
    let mut messages = Vec::new();
    loop {
        let page = read_session_chat_tail_page_stitched(agent, path, 200, Some(cursor))?;
        let SessionChatTailPage::Page {
            messages: chunk,
            before_offset,
            has_more,
            ..
        } = page.page
        else {
            break;
        };
        let commands = crate::session_chat_local_command::select_session_chat_local_commands(
            archived.to_vec(),
            &chunk,
            false,
            has_more,
        );
        let chunk =
            crate::session_chat_local_command::merge_session_chat_local_commands(chunk, &commands);
        for message in chunk.into_iter().rev() {
            if Some(&message.id) == turn.end_id.as_ref() {
                found = true;
            }
            if found {
                messages.push(json!(message));
            }
            if messages.len() == turn.count {
                return Ok(messages);
            }
        }
        if !has_more || cursor == before_offset {
            break;
        }
        cursor = before_offset;
    }
    Ok(messages)
}

fn page_value(messages: Vec<Value>, has_more: bool, before_offset: u64) -> Value {
    json!({"messages": messages, "hasMore": has_more, "hasMoreExact": true,
        "beforeOffset": before_offset, "historyMode": "turns", "status": "ready", "epoch": 0, "seq": 0})
}

#[derive(Default)]
struct Turn {
    kept: Vec<SessionChatMessage>,
    adjacent: Option<SessionChatMessage>,
    end_id: Option<String>,
    completed_at: Option<i64>,
    count: usize,
    deferred: usize,
    final_found: bool,
    files: BTreeSet<String>,
}

impl Turn {
    fn push(&mut self, message: SessionChatMessage, preserve: bool) {
        if self.end_id.is_none() {
            self.completed_at = message.timestamp;
            self.end_id = Some(message.id.clone());
        }
        self.count += 1;
        for block in &message.blocks {
            if let SessionChatBlock::ToolCall { name, input, .. } = block {
                collect_file_paths(name, input, &mut self.files);
            }
        }
        let reply = message.role == SessionChatRole::Assistant
            && message.blocks.iter().any(|block| {
                matches!(block, SessionChatBlock::Text { text } if !text.trim().is_empty())
                    || matches!(block, SessionChatBlock::ImageRef { .. })
            });
        let final_reply = reply && !self.final_found;
        self.final_found |= reply;
        let question = message.blocks.iter().any(|block|
            matches!(block, SessionChatBlock::ToolCall { name, .. } if crate::session_chat_interactive::is_ask_user_question_tool(name)));
        if question {
            if let Some(adjacent) = self.adjacent.as_ref().filter(|next| {
                next.blocks
                    .iter()
                    .any(|block| matches!(block, SessionChatBlock::ToolResult { .. }))
            }) {
                if !self.kept.iter().any(|kept| kept.id == adjacent.id) {
                    self.kept.push(adjacent.clone());
                    self.deferred = self.deferred.saturating_sub(1);
                }
            }
        }
        let visible = message
            .blocks
            .iter()
            .any(|block| matches!(block, SessionChatBlock::ImageRef { .. }));
        self.adjacent = Some(message.clone());
        if preserve
            || final_reply
            || question
            || visible
            || message
                .async_questions
                .as_ref()
                .is_some_and(|questions| !questions.is_empty())
            || matches!(
                message.role,
                SessionChatRole::System | SessionChatRole::User
            )
        {
            self.kept.push(message);
        } else {
            self.deferred += 1;
        }
    }

    fn finish(self, user: Option<SessionChatMessage>, before: u64) -> Vec<Value> {
        let mut result: Vec<Value> = self.kept.iter().map(|message| json!(message)).collect();
        if let Some(user) = user {
            let mut value = json!(user);
            if self.deferred > 0 {
                value["deferredWork"] = json!({"beforeOffset": before, "startId": user.id,
                    "endId": self.end_id, "completedAt": self.completed_at, "messageCount": self.count, "filePaths": self.files});
            }
            result.push(value);
        }
        result
    }
}

fn collect_file_paths(name: &str, input: &Value, paths: &mut BTreeSet<String>) {
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase();
    let parsed = input
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let input = parsed.as_ref().unwrap_or(input);
    if ["write", "edit", "multiedit", "str_replace"].contains(&name.as_str()) {
        for key in [
            "file_path",
            "filePath",
            "path",
            "target_file",
            "targetFile",
            "filename",
        ] {
            if let Some(path) = input
                .get(key)
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
            {
                paths.insert(path.to_string());
                break;
            }
        }
    }
    if name == "apply_patch" {
        if let Some(patch) = input
            .as_str()
            .or_else(|| input.get("patch").and_then(Value::as_str))
        {
            collect_patch_paths(patch, paths);
        }
    }
    if name == "exec" {
        if let Some(source) = input.as_str() {
            for argument in source.split("tools.apply_patch(").skip(1) {
                // Parse one JSON string argument without evaluating JavaScript.
                let mut values =
                    serde_json::Deserializer::from_str(argument.trim_start()).into_iter::<String>();
                if let Some(Ok(patch)) = values.next() {
                    collect_patch_paths(&patch, paths);
                }
            }
        }
    }
}

fn collect_patch_paths(patch: &str, paths: &mut BTreeSet<String>) {
    let mut current = None;
    for line in patch.lines() {
        if let Some(path) = ["*** Add File: ", "*** Update File: ", "*** Delete File: "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
        {
            if let Some(previous) = current.replace(path.to_string()) {
                paths.insert(previous);
            }
        } else if let Some(path) = line.strip_prefix("*** Move to: ") {
            current = Some(path.to_string());
        }
    }
    if let Some(path) = current {
        paths.insert(path);
    }
}
