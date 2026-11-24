//! The last collapsible inner group a Quiet View line holds, opened alongside the line when the
//! user expands it (`QuietStretch::open_inner`).
//!
//! CDXC:SessionChat 2026-11-24 DECISION:
//! User, of clicking "Working · 4 tool calls so far >" and getting a collapsed "4 tool calls" card: "we should actually show the tools expanded since it's the only one there <- this applies for all ... but in this case actually would be better to expand the last one whatever it is if it's because that's the working collapsed area". Expanding any Quiet View line (running or finished, and Summary's Active work quiet lines) also opens its last collapsible inner group on that expanding click only, never overriding a later manual collapse; collapsing and re-expanding the line repeats the rule. Desktop (`native_chat/quiet_view.rs`, `quiet_work.rs`) and phone (`quietView.tsx`, `state.ts`) both read `openInner`.

use serde_json::Value;

use crate::document::{ProjectedMessage, TranscriptItem};
use crate::transcript::quiet_view::{QuietLoad, QuietStretch, QuietTail, QuietTurnLook};

/// The render flags that affect which inner groups inside a Quiet View line are collapsible.
#[derive(Clone, Copy)]
pub(super) struct InnerContext {
    pub simple: bool,
    pub quiet_style: bool,
}

/// Sets `line.open_inner` to the last collapsible inner group inside `items[line.start..line.end]`,
/// and adds that turn's deferred reads to `line.loads` when opening a deferred turn card.
pub(super) fn fill_stretch(
    line: &mut QuietStretch,
    items: &[&TranscriptItem],
    turn_loads: &[(usize, Vec<QuietLoad>)],
    ctx: InnerContext,
) {
    let mut last: Option<(String, Vec<QuietLoad>)> = None;
    let end = line.end.min(items.len());
    for row in line.start..end {
        let tail = line.tail.as_ref().filter(|tail| tail.row == row);
        let look = line.turns.iter().find(|look| look.row == row);
        item_groups(items[row], row, tail, look, turn_loads, ctx, &mut last);
    }
    if let Some((key, loads)) = last {
        line.open_inner = Some(key);
        for load in loads {
            if !line.loads.iter().any(|seen| seen.work == load.work) {
                line.loads.push(load);
            }
        }
    }
}

/// Sets `line.open_inner` to the last collapsible inner group inside a Summary view Active work
/// quiet line's rows (`work[line.start..line.end]`).
pub(super) fn fill_summary_stretch(
    line: &mut QuietStretch,
    work: &[ProjectedMessage],
    ctx: InnerContext,
) {
    let mut last: Option<String> = None;
    let end = line.end.min(work.len());
    for row in &work[line.start..end] {
        for key in message_groups(row, false, false, false, ctx) {
            last = Some(key);
        }
    }
    line.open_inner = last;
}

fn push_msg(
    msg: &Value,
    boxed: bool,
    in_work_fold: bool,
    quiet_steps: bool,
    ctx: InnerContext,
    last: &mut Option<(String, Vec<QuietLoad>)>,
) {
    for key in message_groups(msg, boxed, in_work_fold, quiet_steps, ctx) {
        *last = Some((key, Vec::new()));
    }
}

fn item_groups(
    item: &TranscriptItem,
    row_index: usize,
    tail: Option<&QuietTail>,
    look: Option<&QuietTurnLook>,
    turn_loads: &[(usize, Vec<QuietLoad>)],
    ctx: InnerContext,
    last: &mut Option<(String, Vec<QuietLoad>)>,
) {
    match item {
        TranscriptItem::Message { message } => {
            push_msg(message, true, false, false, ctx, last);
        }
        TranscriptItem::CompletedWork {
            id,
            expandable,
            files,
            work,
            artifacts,
            sent_messages,
            cards_in_fold,
            ..
        } => {
            let flat = look.is_some_and(|look| look.flat);
            let turn_tail = tail.and_then(|tail| tail.turns.first());
            let is_kept_artifact = |row: &Value| {
                turn_tail.is_some_and(|tail| {
                    row["id"]
                        .as_str()
                        .is_some_and(|id| tail.artifacts.iter().any(|kept| kept == id))
                })
            };
            let is_kept_send =
                |index: usize| turn_tail.is_some_and(|tail| tail.sends.contains(&index));

            if !flat {
                if *expandable {
                    let loads = turn_loads
                        .iter()
                        .find(|(at, _)| *at == row_index)
                        .map(|(_, loads)| loads.clone())
                        .unwrap_or_default();
                    *last = Some((format!("work:{id}"), loads));
                }
                for row in artifacts {
                    if is_kept_artifact(row) || (*cards_in_fold && row["foldedIntoWork"] == true) {
                        continue;
                    }
                    push_msg(row, true, false, false, ctx, last);
                }
                if !*cards_in_fold {
                    for (idx, sent) in sent_messages.iter().enumerate() {
                        if !is_kept_send(idx) {
                            if let Some(key) = sent_card_key(sent) {
                                *last = Some((key, Vec::new()));
                            }
                        }
                    }
                }
            } else {
                if *cards_in_fold {
                    for row in artifacts {
                        if !is_kept_artifact(row) && row["foldedIntoWork"] == true {
                            push_msg(row, true, false, false, ctx, last);
                        }
                    }
                    for (idx, sent) in sent_messages.iter().enumerate() {
                        if !is_kept_send(idx) {
                            if let Some(key) = sent_card_key(sent) {
                                *last = Some((key, Vec::new()));
                            }
                        }
                    }
                }
                if !files.is_empty() {
                    *last = Some((format!("work-files:{id}"), Vec::new()));
                }
                for row in work {
                    push_msg(row, true, true, false, ctx, last);
                }
                for row in artifacts {
                    if is_kept_artifact(row) || (*cards_in_fold && row["foldedIntoWork"] == true) {
                        continue;
                    }
                    push_msg(row, true, false, false, ctx, last);
                }
                if !*cards_in_fold {
                    for (idx, sent) in sent_messages.iter().enumerate() {
                        if !is_kept_send(idx) {
                            if let Some(key) = sent_card_key(sent) {
                                *last = Some((key, Vec::new()));
                            }
                        }
                    }
                }
            }
        }
        TranscriptItem::GroupedWork {
            id,
            expandable,
            turns,
            work,
            cards_in_fold,
            ..
        } => {
            let flat = look.is_some_and(|look| look.flat);
            let turn_tail_for = |turn_idx: usize| {
                tail.and_then(|tail| tail.turns.iter().find(|entry| entry.turn == turn_idx))
            };
            if !flat {
                if *expandable {
                    let loads = turn_loads
                        .iter()
                        .find(|(at, _)| *at == row_index)
                        .map(|(_, loads)| loads.clone())
                        .unwrap_or_default();
                    *last = Some((format!("work:{id}"), loads));
                }
            } else {
                if *cards_in_fold {
                    for (turn_idx, turn) in turns.iter().enumerate() {
                        let turn_tail = turn_tail_for(turn_idx);
                        for row in &turn.artifacts {
                            let kept = turn_tail.is_some_and(|tail| {
                                row["id"]
                                    .as_str()
                                    .is_some_and(|id| tail.artifacts.iter().any(|k| k == id))
                            });
                            if !kept && row["foldedIntoWork"] == true {
                                push_msg(row, true, false, false, ctx, last);
                            }
                        }
                        for (idx, sent) in turn.sent_messages.iter().enumerate() {
                            if !turn_tail.is_some_and(|tail| tail.sends.contains(&idx)) {
                                if let Some(key) = sent_card_key(sent) {
                                    *last = Some((key, Vec::new()));
                                }
                            }
                        }
                    }
                }
                for row in work {
                    push_msg(row, true, true, true, ctx, last);
                }
            }
            for (turn_idx, turn) in turns.iter().enumerate() {
                let turn_tail = turn_tail_for(turn_idx);
                for row in &turn.artifacts {
                    let kept = turn_tail.is_some_and(|tail| {
                        row["id"]
                            .as_str()
                            .is_some_and(|id| tail.artifacts.iter().any(|k| k == id))
                    });
                    if kept || (*cards_in_fold && row["foldedIntoWork"] == true) {
                        continue;
                    }
                    push_msg(row, true, false, false, ctx, last);
                }
                if !*cards_in_fold {
                    for (idx, sent) in turn.sent_messages.iter().enumerate() {
                        if !turn_tail.is_some_and(|tail| tail.sends.contains(&idx)) {
                            if let Some(key) = sent_card_key(sent) {
                                *last = Some((key, Vec::new()));
                            }
                        }
                    }
                }
            }
        }
        TranscriptItem::Summary { .. } | TranscriptItem::Unknown(_) => {}
    }
}

/// The collapsible disclosure keys one message row draws, in visual top-to-bottom order.
fn message_groups(
    message: &Value,
    boxed: bool,
    in_work_fold: bool,
    quiet_steps: bool,
    ctx: InnerContext,
) -> Vec<String> {
    let Some(id) = message["id"].as_str().filter(|id| !id.is_empty()) else {
        return Vec::new();
    };
    if message["role"] == "user" && message["interAgentMessage"].is_object() {
        let body = message["interAgentMessage"]["body"]
            .as_str()
            .unwrap_or_default();
        return (!body.is_empty())
            .then(|| format!("inter-agent:{id}"))
            .into_iter()
            .collect();
    }
    if message["systemCard"]["kind"] == "agent-message" {
        let card = &message["systemCard"];
        let has_body = !card["markdown"].as_str().unwrap_or_default().is_empty()
            || !card["body"].as_str().unwrap_or_default().is_empty();
        return has_body
            .then(|| format!("agent-message:{id}"))
            .into_iter()
            .collect();
    }
    if message["role"] == "user"
        || message["shellCommand"] == true
        || message["sideQuestion"].is_object()
        || message["suppressed"].is_object()
        || message["apiError"].is_object()
        || message["systemCard"].is_object()
    {
        return Vec::new();
    }

    let body = message["text"].as_str().unwrap_or_default();
    let reasoning = message["role"] == "reasoning";
    let tool_card = message["toolCard"].is_object();
    let tools = list(message, "tools");
    let has_tools = message["toolsFlat"] != true && !tool_card && !tools.is_empty();
    let mut keys = Vec::new();
    let mut tools_rendered = false;

    if !body.is_empty() {
        if reasoning && (message["thinkingCard"].is_object() || (ctx.simple && !has_tools)) {
            let expandable = message["thinkingCard"].is_object()
                || message["thinkingRow"]["expandable"] != false;
            if expandable {
                keys.push(format!("thinking-card:{id}"));
            }
            if message["thinkingCard"]["calls"].is_string() {
                keys.push(format!("reasoning:{id}"));
                tools_rendered = true;
            }
        } else if reasoning && has_tools {
            keys.push(format!("reasoning:{id}"));
            tools_rendered = true;
        } else if !reasoning && has_tools {
            keys.push(format!("tools:{id}"));
            tools_rendered = true;
        }
    }

    let file_key = (!in_work_fold
        && !tool_card
        && !list(message, "files").is_empty()
        && (ctx.simple || ctx.quiet_style))
        .then(|| format!("files:{id}"));

    let tool_key = if tools_rendered {
        None
    } else if tool_card {
        Some(format!("tool-card:{id}"))
    } else if !tools.is_empty() && !quiet_steps {
        let show_all = message["toolsShowAllRows"] == true;
        let cards_as_rows = show_all || in_work_fold;
        let has_visible = tools.iter().any(|tool| {
            cards_as_rows
                || (tool["exchange"] != true
                    && tool["sentMessage"] != true
                    && !tool["workflow"].is_string())
        });
        if has_visible && !show_all {
            let hidden = message["toolFold"]["hiddenCount"].as_u64().unwrap_or(0);
            let card_header = boxed && !in_work_fold;
            if (ctx.simple || ctx.quiet_style) || (card_header && hidden == 0) {
                Some(
                    message["toolsLineKey"]
                        .as_str()
                        .map_or_else(|| format!("tools:{id}"), str::to_string),
                )
            } else if hidden > 0 {
                Some(format!("tool-run:{id}"))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if ctx.quiet_style && !tools_rendered && !tool_card {
        keys.extend(tool_key);
        keys.extend(file_key);
    } else {
        keys.extend(file_key);
        keys.extend(tool_key);
    }

    if !in_work_fold {
        for sent in list(message, "sentMessages") {
            if let Some(key) = sent_card_key(sent) {
                keys.push(key);
            }
        }
    }
    keys
}

fn sent_card_key(sent: &Value) -> Option<String> {
    if !sent["card"].is_object() {
        return None;
    }
    let has_body = !sent["markdown"].as_str().unwrap_or_default().is_empty()
        || !sent["body"].as_str().unwrap_or_default().is_empty();
    let key = sent["key"].as_str().filter(|key| !key.is_empty())?;
    has_body.then(|| format!("sent-message:{key}"))
}

fn list<'a>(row: &'a Value, key: &str) -> &'a [Value] {
    row[key].as_array().map_or(&[][..], Vec::as_slice)
}
