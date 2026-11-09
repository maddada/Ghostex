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
use crate::transcript::folded_cards::card_stats;
use crate::transcript::read_images::{images_stat, push_once};
use crate::transcript::simple::{edit_label, edited_and_failed_paths, tool_count_label};
use crate::transcript::turns::worked_duration_label;
use crate::transcript::work_summary::counted_work_stats;

/// `items` with each run of `completed-work` rows folded into one `grouped-work` row. Each reply's
/// own calls and edits, and those of the pictures and question forms that stay in view, move into
/// the row's work too (`reply_calls`), so the line's steps list every call it counts. Under Agent
/// work Quiet (`hidden`, the projection's final reply ids) the row names the reply that carries
/// the work button (`workCarrier`).
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
    let (mut messages, mut answers, mut cards_in_fold) = (0, 0, false);
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
            mut artifacts,
            mut final_message,
            read_images: turn_images,
            tally,
            cards_in_fold: turn_folds,
            ..
        } = item
        else {
            continue;
        };
        if id.is_empty() {
            // The first turn's own id, so its fold key (`work:<id>`) is the one Default's
            // "Worked for" fold uses: a fold closed under one look stays closed under the other.
            id = turn_id.clone();
            started = tally.started_ms;
            ended = tally.ended_ms;
        } else {
            ended = ended.max(tally.extends_to_ms);
        }
        tool_calls += tally.tool_calls;
        messages += tally.messages;
        answers += tally.answers;
        cards_in_fold |= turn_folds;
        for path in tally.changed {
            if !changed.contains(&path) {
                changed.push(path);
            }
        }
        files.extend(turn_files);
        expandable |= turn_expandable;
        let turn_start = work.len();
        work.extend(turn_work);
        // The pictures the moved calls read, which their rows no longer show: the line's strip and
        // its "N images" take them over.
        let mut moved_images: Vec<Value> = Vec::new();
        let mut take_images = |row: &crate::document::ProjectedMessage| {
            for image in row["readImages"].as_array().into_iter().flatten() {
                moved_images.push(image.clone());
            }
        };
        // A picture's or a question form's calls go where the row sat among the turn's work.
        for artifact in artifacts.iter_mut() {
            let before = artifact.clone();
            if let Some(calls) = reply_calls(artifact) {
                take_images(&before);
                expandable = true;
                let at = calls["timestamp"].as_i64().map_or(work.len(), |at| {
                    work[turn_start..]
                        .iter()
                        .position(|row| row["timestamp"].as_i64().is_some_and(|row| row > at))
                        .map_or(work.len(), |place| turn_start + place)
                });
                work.insert(at, calls);
            }
        }
        let reply_before = final_message.clone();
        if let Some(calls) = final_message.as_mut().and_then(reply_calls) {
            // The line opens onto these even when the reply held the turn's only calls.
            expandable = true;
            work.push(calls);
            if let Some(reply) = &reply_before {
                take_images(reply);
            }
        }
        for image in turn_images.into_iter().chain(moved_images) {
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
    // The messages and answers folded into its turns (`folded_cards.rs`) follow the work's counts.
    stats.extend(card_stats(messages, answers));
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
        cards_in_fold,
    }
}

/// Agent work Reduced and Quiet: takes a reply's own calls and edits off it (or off a picture or
/// question form that stays in view) and returns them as a row of the work (`None` when it has
/// none), so no work is drawn in or after a reply. The row keeps none of what the reply itself
/// shows (its words, actions, pictures, cards).
///
/// CDXC:SessionChat 2026-11-09 WHY: under Reduced the reply drew its own calls as Default chevron rows under it, which the line counted (`tool_call_count` reads the reply) but its steps did not list, so "9 tool calls" opened onto 7; a picture's or a form's calls did the same under both looks.
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

/// Agent work Reduced and Quiet: a row the chat draws (the running turn's, mostly) lists its calls
/// as one quiet line after its words, as a row with no words does, instead of its words opening
/// onto them: its text owns none of them (`toolsFlat`), the run is not shown whole
/// (`toolsShowAllRows`), and a running thought no longer folds them behind its line
/// (`thinkingCard.calls`). The renderers and Find read those keys as they do in a work fold.
///
/// CDXC:SessionChat 2026-11-09 WHY: only a row with no words became a quiet line, so while the agent ran a sentence with calls drew Default's chevron over full tool rows, and a thought or a reasoning headline kept its own fold, beside the quiet lines of the rows around them, until the turn finished and folded.
pub fn calls_as_lines(items: &mut [TranscriptItem]) {
    for item in items {
        let TranscriptItem::Message { message } = item else {
            continue;
        };
        let words = message["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty());
        let calls = message["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty());
        if !words
            || !calls
            || !matches!(message["role"].as_str(), Some("assistant" | "reasoning"))
            || message["toolsFlat"] == true
            || message["toolCard"].is_object()
        {
            continue;
        }
        if let Value::Object(fields) = std::sync::Arc::make_mut(message) {
            fields.insert("toolsFlat".into(), Value::Bool(true));
            fields.insert("toolsShowAllRows".into(), Value::Bool(false));
            if let Some(Value::Object(card)) = fields.get_mut("thinkingCard") {
                card.remove("calls");
            }
        }
    }
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
    let messages: Vec<Option<&Value>> = items
        .iter()
        .map(|item| match item {
            TranscriptItem::Message { message } => Some(&**message),
            _ => None,
        })
        .collect();
    let before = before_question(&messages, live);
    let mut place = 0usize;
    items.retain_mut(|item| {
        let at = place;
        place += 1;
        match item {
            TranscriptItem::Message { message } => {
                keeps_live_row(message, live, finished_before, before.contains(&at))
            }
            // The Summary view's running turn: its "Active work" fold holds the same rows.
            TranscriptItem::Summary { work, .. } => {
                let rows: Vec<Option<&Value>> = work.iter().map(|row| Some(&**row)).collect();
                let before = before_question(&rows, live);
                let mut place = 0usize;
                work.retain_mut(|message| {
                    let at = place;
                    place += 1;
                    keeps_live_row(message, live, finished_before, before.contains(&at))
                });
                true
            }
            _ => true,
        }
    });
}

/// Agent work Quiet while the agent works but one working signal doubts it is mid-response
/// (`working::hides_running_response`, an agent such as Cursor that reports no Working): the words
/// stay in view, since a hidden reply is far worse than extra words, but the calls and edits of the
/// running response's rows (`live`) and a subagent's report do not. A call waiting on an approval
/// still draws ([`only_what_waits`]), and a row left with nothing to draw goes.
///
/// CDXC:SessionChat 2026-11-09 WHY: the contributor of the chat look (sebastianarena) chose that under Quiet the tool calls are hidden for these agents too, while hiding their words stays cautious; before, nothing of the running response was hidden for them and its calls drew in the chat.
pub fn hide_live_calls(items: &mut Vec<TranscriptItem>, live: &std::collections::HashSet<&str>) {
    let keep = |message: &mut crate::document::ProjectedMessage| -> bool {
        if !live.contains(message["id"].as_str().unwrap_or_default()) {
            return true;
        }
        let role = message["role"].as_str().unwrap_or_default();
        if role == "user" && message["interAgentMessage"]["subagent"] == true {
            return false;
        }
        if !matches!(role, "assistant" | "reasoning") || message["pending"] == true {
            return true;
        }
        if waits_on_approval(message) {
            only_what_waits(message, true);
            return true;
        }
        let has = |key: &str| message[key].as_array().is_some_and(|list| !list.is_empty());
        if !(has("tools") || has("files")) {
            return true;
        }
        strip_calls(message);
        let words = message["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty());
        words
            || needs_the_user(message)
            || ["apiError", "systemCard", "suppressed", "sideQuestion"]
                .iter()
                .any(|key| message[*key].is_object())
    };
    items.retain_mut(|item| match item {
        TranscriptItem::Message { message } => keep(message),
        TranscriptItem::Summary { work, .. } => {
            work.retain_mut(|message| keep(message));
            true
        }
        _ => true,
    });
}

/// The Summary view under Agent work Reduced and Quiet, where the running turn's "Active work"
/// starts shut: every row of it that needs the user (`chat_look::needs_the_user`: a question, an
/// approval, a picture, a card) joins the rows drawn in full under the prompt (`cards`, which the
/// fold leaves out), in the order they came, with its calls as one quiet line. Runs once the
/// waiting call is marked and Quiet has hidden the rest. Marking a waiting call opens the fold
/// around it (`approval_wait::mark_waiting_call`); once its row is drawn here the fold goes back
/// to shut, so an approval never opens the earlier work beside it.
///
/// CDXC:SessionChat 2026-11-09 WHY: the summary turn only hoisted fenced forms, pages and charts (`turns::content_rows`), which was enough while the running fold started open; shut, it also hid one-click answers in a sentence, shared pictures and approvals, which must surface at once (the 2026-11-08 decision on `chat_look::needs_the_user`).
pub fn keep_in_view_while_shut(items: &mut [TranscriptItem]) {
    for item in items {
        let TranscriptItem::Summary {
            active: true,
            work,
            work_open,
            cards,
            ..
        } = item
        else {
            continue;
        };
        if work.iter().any(|row| waits_on_approval(row)) {
            *work_open = false;
        }
        let id = |row: &crate::document::ProjectedMessage| row["id"].as_str().map(str::to_string);
        let carded: Vec<Option<String>> = cards.iter().map(id).collect();
        let mut kept: Vec<crate::document::ProjectedMessage> = work
            .iter()
            .filter(|row| carded.contains(&id(row)) || needs_the_user(row))
            .map(|row| {
                let mut row = row.clone();
                if let Value::Object(fields) = std::sync::Arc::make_mut(&mut row) {
                    fields.insert("toolsShowAllRows".into(), Value::Bool(false));
                }
                row
            })
            .collect();
        let shown: Vec<Option<String>> = kept.iter().map(id).collect();
        kept.extend(cards.drain(..).filter(|card| !shown.contains(&id(card))));
        *cards = kept;
    }
}

/// [`hide_work`] for one row: whether it stays, and what of it.
fn keeps_live_row(
    message: &mut crate::document::ProjectedMessage,
    live: &std::collections::HashSet<&str>,
    finished_before: Option<i64>,
    before_question: bool,
) -> bool {
    if !live.contains(message["id"].as_str().unwrap_or_default()) {
        return true;
    }
    let role = message["role"].as_str().unwrap_or_default();
    // A subagent's report is the agent's work, which the status line names while it runs
    // (`turns::report_lands_mid_response`).
    if role == "user" && message["interAgentMessage"]["subagent"] == true {
        return false;
    }
    if !matches!(role, "assistant" | "reasoning") || message["pending"] == true {
        return true;
    }
    // A row that ends in a call or a call's result owes more, and one a question follows
    // introduces it, so neither is a run's reply: a question's sentence stays hidden once the
    // answer starts a new run.
    let finished_reply = role == "assistant"
        && message["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
        && !ends_in_call(message)
        && !before_question
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
}

/// The places in `rows` of the live rows a question follows at once: their words introduce it,
/// they are not a run's reply. `None` stands for an item that is not a message.
fn before_question(
    rows: &[Option<&Value>],
    live: &std::collections::HashSet<&str>,
) -> std::collections::HashSet<usize> {
    rows.windows(2)
        .enumerate()
        .filter_map(|(at, pair)| {
            let (Some(row), Some(next)) = (pair[0], pair[1]) else {
                return None;
            };
            let asks = next["questions"]
                .as_array()
                .is_some_and(|list| !list.is_empty());
            (asks && live.contains(row["id"].as_str().unwrap_or_default())).then_some(at)
        })
        .collect()
}

/// Whether a row's last block is a call or a call's result.
fn ends_in_call(row: &Value) -> bool {
    row["blocks"]
        .as_array()
        .and_then(|blocks| blocks.last())
        .is_some_and(|block| matches!(block["type"].as_str(), Some("tool-call" | "tool-result")))
}

/// `row` showing only its calls and edits that wait on an approval: the rest of its calls keep
/// their places, since the disclosure keys and details count by place, and draw no row, as a call
/// with a card of its own does. A waiting edit carries its place as `disclosureIndex`, since the
/// renderers key a file row by its place in the list and this one has moved up. Its words go too,
/// unless they are a finished reply or ask a question.
///
/// CDXC:SessionChat 2026-11-08 WHY: with the edits filtered, a second edit waiting took the first one's `file:<id>:0` key, so closing the first edit's diff left the next approval's diff closed; an approval has to surface at once (`chat_look::needs_the_user`). Rows with no `disclosureIndex` stay keyed by their place, so every other list keeps its keys.
/// CDXC:SessionChat 2026-11-08 SEE-ALSO: apps/desktop/src/app/native_chat/file_change_card.rs and apps/mobile/app/src/chat/native/transcript/FileChanges.tsx read `disclosureIndex`.
fn only_what_waits(row: &mut crate::document::ProjectedMessage, keep_words: bool) {
    let keep_words = keep_words || asks_in_words(row);
    let Value::Object(fields) = std::sync::Arc::make_mut(row) else {
        return;
    };
    // Kept words head the run's own chevron (`tools:<id>`), and the quiet line of the waiting
    // call drawn under them needs a key of its own, or closing one would close the other.
    if keep_words && fields.get("role") != Some(&Value::from("reasoning")) {
        let id = fields.get("id").and_then(Value::as_str).unwrap_or_default();
        fields.insert("toolsLineKey".into(), format!("tools-line:{id}").into());
    }
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
            let mut place = 0usize;
            files.retain_mut(|file| {
                let waits = file["awaitingApproval"] == true;
                if waits {
                    file["disclosureIndex"] = Value::from(place);
                }
                place += 1;
                waits
            });
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
