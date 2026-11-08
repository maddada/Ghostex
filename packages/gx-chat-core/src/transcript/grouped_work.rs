//! Agent work Reduced (`chatAgentWorkStyle` `reduced`, `transcript/chat_look.rs`): everything the
//! agent did for one message of the user's reads as one line, "Worked for 1m 16s · 9 tool calls ·
//! 2 files", and the agent's messages for that reply follow it.
//!
//! The finished turns the projection folds (`completed-work`) are merged after they are built
//! (`presentation::build_items`): each run of them with no row between, which is one prompt's turn
//! and the turns the agent's wake-ups after it opened, becomes one `grouped-work` row, and a turn
//! alone becomes one too, so the renderers draw a single style. The running turn's rows and every
//! other row stay where they are, after the line.
//!
//! CDXC:SessionChat 2026-11-07 WHY: the line is timed from its first turn's opener to the latest end of its turns, a wake-up counting only its reply (`presentation::turn_times`), the way a prompt's own label runs on through its OMP wake-ups (`turn_worked_label`, 2026-11-03). Adding up each turn's own span dropped the wait between a reply and its wake-up that the label counts on purpose, so the same reply read "1m 16s" in Default and "16s" in Reduced; adding up the labels would count the wake-ups twice. So the line never reads shorter than any of its turns.
//!
//! CDXC:SessionChat 2026-11-07 SEE-ALSO: apps/desktop/src/app/native_chat/ (desktop and web) and apps/mobile/app/src/chat/ (phone) draw `grouped-work`; `extras/transcript_search.rs` reads its fold for Find; `transcript/rows.rs` (`without_worked_time`) relabels the newest one "Previous turn".

use serde_json::Value;

use crate::document::{GroupedTurn, TranscriptItem};
use crate::transcript::chat_look::{
    asks_in_words, holds_only_answered_cards, needs_the_user, waits_on_approval,
};
use crate::transcript::file_change_rows::file_change_card;
use crate::transcript::read_images::{images_stat, push_once};
use crate::transcript::simple::{edit_label, edited_and_failed_paths, tool_count_label};
use crate::transcript::turns::worked_duration_label;
use crate::transcript::work_summary::counted_work_stats;

/// `items` with each run of `completed-work` rows folded into one `grouped-work` row. Under Agent
/// work Quiet (`hidden`, the projection's final reply ids) each reply's own calls and edits move
/// into the row's work too (`reply_calls`), and the row names the reply that carries the work
/// button (`workCarrier`).
pub fn group_work(items: Vec<TranscriptItem>, hidden: Option<&[String]>) -> Vec<TranscriptItem> {
    let mut grouped = Vec::with_capacity(items.len());
    let mut run: Vec<TranscriptItem> = Vec::new();
    for item in items {
        if matches!(item, TranscriptItem::CompletedWork { .. }) {
            run.push(item);
            continue;
        }
        if !run.is_empty() {
            grouped.push(merge(std::mem::take(&mut run), hidden));
        }
        grouped.push(item);
    }
    if !run.is_empty() {
        grouped.push(merge(run, hidden));
    }
    grouped
}

/// One `grouped-work` row for a run of `completed-work` rows, oldest first.
fn merge(run: Vec<TranscriptItem>, hidden: Option<&[String]>) -> TranscriptItem {
    let mut id = String::new();
    let mut started: Option<i64> = None;
    let mut ended: Option<i64> = None;
    let mut tool_calls = 0;
    let mut changed: Vec<String> = Vec::new();
    let mut files: Vec<Value> = Vec::new();
    let mut expandable = false;
    let mut work = Vec::new();
    let mut read_images: Vec<Value> = Vec::new();
    let mut turns = Vec::with_capacity(run.len());
    for item in run {
        let TranscriptItem::CompletedWork {
            id: turn_id,
            files: turn_files,
            expandable: turn_expandable,
            deferred,
            work: turn_work,
            questions,
            sent_messages,
            gbot_cards,
            workflows,
            artifacts,
            mut final_message,
            read_images: turn_images,
            tally,
            ..
        } = item
        else {
            continue;
        };
        if id.is_empty() {
            id = format!("grouped:{turn_id}");
            started = tally.started_ms;
            ended = tally.ended_ms;
        } else {
            ended = ended.max(tally.extends_to_ms);
        }
        tool_calls += tally.tool_calls;
        for path in tally.changed {
            if !changed.contains(&path) {
                changed.push(path);
            }
        }
        files.extend(turn_files);
        expandable |= turn_expandable;
        work.extend(turn_work);
        if hidden.is_some() {
            if let Some(calls) = final_message.as_mut().and_then(reply_calls) {
                // The work button opens these even when the reply held the turn's only calls.
                expandable = true;
                work.push(calls);
            }
        }
        for image in turn_images {
            push_once(&mut read_images, image);
        }
        turns.push(GroupedTurn {
            id: turn_id,
            deferred,
            questions,
            sent_messages,
            gbot_cards,
            workflows,
            artifacts,
            final_message,
        });
    }
    let (_, failed) = edited_and_failed_paths(files.iter().filter_map(|file| {
        let path = file.get("path").and_then(Value::as_str)?;
        Some((path, file.get("failed") == Some(&Value::Bool(true))))
    }));
    let failed_files = failed.len();
    let changed_files = changed.len();
    // The renderers draw a reply's hover row exactly for these (desktop `has_reply_actions`, phone
    // `hasReplyActions`), so the button is never named onto a reply without one.
    let work_carrier = hidden.and_then(|final_ids| {
        turns
            .iter()
            .rev()
            .filter_map(|turn| turn.final_message.as_deref())
            .filter(|reply| {
                reply["role"] == "assistant" && reply["actionContent"]["copyable"] == true
            })
            .filter_map(|reply| reply["id"].as_str())
            .find(|id| final_ids.iter().any(|known| known == id))
            .map(str::to_string)
    });
    let label = worked_duration_label(started, ended);
    let work_button = work_button(short_time(&label), tool_calls, changed_files);
    let mut stats = counted_work_stats(tool_calls, changed_files);
    // The strip's "3 images" comes first, as on a finished turn's card (`bubble_read_images`).
    if !read_images.is_empty() {
        stats.insert(0, images_stat(read_images.len()));
    }
    TranscriptItem::GroupedWork {
        id,
        label,
        stats,
        files_card: file_change_card(&files),
        files_label: format!(
            "{changed_files} {} changed",
            if changed_files == 1 { "file" } else { "files" }
        ),
        simple_files_label: edit_label(changed_files, failed_files),
        files,
        expandable,
        work,
        turns,
        read_images,
        work_button,
        work_carrier,
    }
}

/// Agent work Quiet: takes a reply's own calls and edits off it and returns them as a row of the
/// work (`None` when it has none), so no work is drawn in or after a reply. The row keeps none of
/// what the reply itself shows (its words, actions, pictures, cards).
///
/// CDXC:SessionChat 2026-11-07 WHY: the row reuses the reply's id on purpose: its calls' details (`row_detail`) and its Find and disclosure keys (`tool:<id>:<n>`) are read by it, and nothing indexes a grouped row's work by id, so the reply and this row sharing it collides with nothing.
fn reply_calls(
    reply: &mut crate::document::ProjectedMessage,
) -> Option<crate::document::ProjectedMessage> {
    let has = |key: &str| reply[key].as_array().is_some_and(|list| !list.is_empty());
    if !reply.is_object() || !(has("tools") || has("files")) {
        return None;
    }
    let mut calls = (**reply).clone();
    if let Value::Object(fields) = &mut calls {
        for key in [
            "actionContent",
            "copyText",
            "images",
            "questions",
            "sentMessages",
            "gbotCards",
            "workflows",
            "readImages",
            "markdownReferences",
            "time",
        ] {
            fields.remove(key);
        }
        fields.insert("text".into(), Value::String(String::new()));
        // As a row of an opened work fold lists them (`tool_rows.rs`).
        fields.insert("toolsFlat".into(), Value::Bool(true));
        fields.insert("toolsShowAllRows".into(), Value::Bool(true));
    }
    strip_calls(reply);
    Some(std::sync::Arc::new(calls))
}

/// `row` without its calls and edits.
fn strip_calls(row: &mut crate::document::ProjectedMessage) {
    if let Value::Object(fields) = std::sync::Arc::make_mut(row) {
        fields.insert("tools".into(), Value::Array(Vec::new()));
        fields.insert("files".into(), Value::Array(Vec::new()));
        for key in ["toolCard", "toolFold", "readImages", "fileCard"] {
            fields.remove(key);
        }
        // A running thought's line counts the calls it folds; none are left to open.
        if let Some(Value::Object(card)) = fields.get_mut("thinkingCard") {
            card.remove("calls");
        }
    }
}

/// Agent work Quiet, while the agent works: the status line says what runs, so the rows of the
/// response still running (`live`, from its start, `TranscriptProjection::live_ids`) are dropped.
/// A row that needs the user always stays (`chat_look::needs_the_user`): one with a call or edit
/// waiting on an approval draws only that ([`only_what_waits`]), any other without its calls and
/// edits, and so do the replies of the runs that finished up to `finished_before`
/// (`working::quiet_finished_before`) and a row that reports an error or a status. A row kept only
/// for an answered card draws the card alone (`chat_look::holds_only_answered_cards`): the words it
/// holds are the agent's between its tool calls. Finished rows outside grouped rows keep their
/// calls, drawn as Reduced draws them, since no button would reach them. A row is judged whole: the
/// live rows are projected at once (`presentation::turn_items`), so a stand-in only reaches here
/// for a row the cache cannot hold, and keeps its place.
pub fn hide_work(
    items: &mut Vec<TranscriptItem>,
    live: &std::collections::HashSet<&str>,
    finished_before: Option<i64>,
) {
    items.retain_mut(|item| {
        let TranscriptItem::Message { message } = item else {
            return true;
        };
        if !live.contains(message["id"].as_str().unwrap_or_default()) {
            return true;
        }
        let role = message["role"].as_str().unwrap_or_default();
        if !matches!(role, "assistant" | "reasoning") || message["pending"] == true {
            return true;
        }
        let finished_reply = role == "assistant"
            && message["text"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
            && message["timestamp"]
                .as_i64()
                .zip(finished_before)
                .is_some_and(|(at, before)| at <= before);
        if waits_on_approval(message) {
            only_what_waits(message, finished_reply);
            return true;
        }
        let has = |key: &str| message[key].as_array().is_some_and(|list| !list.is_empty());
        let reports = role != "reasoning"
            && ["apiError", "systemCard", "suppressed", "sideQuestion"]
                .iter()
                .any(|key| message[*key].is_object());
        let shows = needs_the_user(message) || finished_reply || reports;
        if shows && (has("tools") || has("files")) {
            strip_calls(message);
        }
        if !finished_reply && !reports && holds_only_answered_cards(message) {
            drop_words(message);
        }
        shows
    });
}

/// `row` showing only its calls and edits that wait on an approval: the rest of its calls keep
/// their places, since the disclosure keys and details count by place, and draw no row, as a call
/// with a card of its own does. Its words go too, unless they are a finished reply or ask a
/// question.
fn only_what_waits(row: &mut crate::document::ProjectedMessage, keep_words: bool) {
    let keep_words = keep_words || asks_in_words(row);
    let Value::Object(fields) = std::sync::Arc::make_mut(row) else {
        return;
    };
    let mut waiting = 0;
    if let Some(Value::Array(tools)) = fields.get_mut("tools") {
        for tool in tools.iter_mut() {
            if let Value::Object(tool) = tool {
                if tool.get("awaitingApproval") == Some(&Value::Bool(true)) {
                    waiting += 1;
                } else {
                    tool.insert("exchange".into(), Value::Bool(true));
                }
            }
        }
    }
    fields.insert("simpleToolLabel".into(), tool_count_label(waiting).into());
    fields.remove("fileCard");
    let waiting_files = fields
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .map(|files| {
            files.retain(|file| file["awaitingApproval"] == true);
            (!files.is_empty()).then(|| file_change_card(files))
        });
    // The edit line and its card count only the edits still drawn.
    if let Some(Some(card)) = waiting_files {
        fields.insert("simpleFileLabel".into(), card["title"].clone());
        fields.insert("fileCard".into(), card);
    }
    for key in ["toolFold", "toolsShowAllRows", "readImages"] {
        fields.remove(key);
    }
    if !keep_words {
        drop_words(row);
    }
}

/// `row` without its words and what acts on them. Find reads the same text, so it follows.
fn drop_words(row: &mut crate::document::ProjectedMessage) {
    let Value::Object(fields) = std::sync::Arc::make_mut(row) else {
        return;
    };
    fields.insert("text".into(), Value::String(String::new()));
    for key in [
        "actionContent",
        "copyText",
        "markdownReferences",
        "time",
        "thinkingCard",
        "reasoning",
    ] {
        fields.remove(key);
    }
}

/// The time in a "Worked for 1m 16s" label, `None` for a label without one ("Worked", "Previous
/// turn").
pub(crate) fn short_time(label: &str) -> Option<&str> {
    label.strip_prefix("Worked for ")
}

/// What the work button under Agent work Quiet says: the time when there is one, then "5 tools"
/// and "2 files" when there are any ("24s · 5 tools · 2 files"), or "Worked" when that leaves
/// nothing.
fn work_button(time: Option<&str>, tools: usize, files: usize) -> String {
    let mut parts: Vec<String> = time.map(str::to_string).into_iter().collect();
    if tools > 0 {
        parts.push(format!(
            "{tools} {}",
            if tools == 1 { "tool" } else { "tools" }
        ));
    }
    if files > 0 {
        parts.push(format!(
            "{files} {}",
            if files == 1 { "file" } else { "files" }
        ));
    }
    if parts.is_empty() {
        "Worked".to_string()
    } else {
        parts.join(" \u{b7} ")
    }
}

/// `button` without its leading time, for the newest reply while the working card shows that time
/// (`rows::without_worked_time`): "5 tools · 2 files", or "Previous turn" when only the time was
/// there.
pub(crate) fn without_time(button: &str, time: &str) -> String {
    match button.strip_prefix(time) {
        Some("") => "Previous turn".to_string(),
        Some(rest) => rest.strip_prefix(" \u{b7} ").unwrap_or(rest).to_string(),
        None => button.to_string(),
    }
}
