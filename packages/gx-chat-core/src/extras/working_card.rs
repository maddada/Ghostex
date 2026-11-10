//! The working card: one card right above the composer that says what the agent is doing, with the
//! pills beside its label. It never opens; each pill opens its own menu.
//!
//! CDXC:SessionChat 2026-10-18 DECISION:
//! User, answering the working indicator review (docs/2026-10-17/chat-working-indicator/review.html): the pending tool card and the "✦ Considering…" working row with its pills become one card ("card", Q253), "the same size and look" wherever it shows, with a smooth change and nothing shifting while the reader scrolls. While a tool runs its label replaces the working word, which comes back between tools ("tool label", Q254). The ? question pill and an armed Delayed Send ride on the card ("on the pill", Q259); idle pills (Threads on an orchestrator, Background after a turn) stay on it ("in the lane", Q260). When the turn ends the card shows "Worked for 2m 13s" until the next message ("worked line", Q257; 2026-10-18: "Say Worked for 12s not Worked 12s. this is in all places we mention how long we worked please") and that turn's fold drops its time and reads "Previous turn" instead (2026-10-18: "Make this say 'Previous turn' on the left side after the chevron", `transcript::rows::without_worked_time`). Later on 2026-10-17: "let's not have that chevron at all, it's not needed"; "make the scroll to bottom pill always appear on the leftmost so it doesn't push other pills when it appears"; "the text for the working pill should be hoverable to see full text of it" (the phone shows it on a long press); and "shouldn't allow wrapping in the pill itself, we should truncate the text on the left to make space for the pills (... at the end)": the label stays on one line, cut with an ellipsis at its end, and the pills keep their full width (the pills have left the card since 2026-10-29, below, so the label has the whole row). Agents with no status card (no tool read off their screen) show the card with the spinner text alone. On 2026-10-18, after trying it: "The 'working' card shouldn't be showing the text of the chat behind it, it should be right above the chat composer and in that way the chat transcript never goes under it", so the card lives in its own row right above the composer, with the normal spacing (2026-10-19, "remove the extra space we have here please": the last transcript row ends with the gap every row has, 16px, with no end padding under it and no extra space above the card), and (2026-10-20, of the card's corner) "Make this have fully rounded borders please", so the card is a capsule like the pills it carries, and the transcript's visible area ends above it; at the bottom of the chat it reads as the conversation's last thing, and in a short conversation it stays right above the composer too, with the empty space above it ("why is there a gap here?" about the card sitting under the last message instead). And: "why is the threads showing as part of the expanded working card? they shouldn't, they should show as menu for each one, let's make the pill itself not expandable at all, instead it should show a menu like we used to have for 'Threads' please", so the card never opens and each pill opens its own menu (a sheet on the phone) as the Threads pill did (CDXC:Coordinators 2026-10-15 in coordinator_threads.rs); a press or tap on the card itself does nothing. That replaces the card floating over the transcript's bottom edge when scrolled up (Q255, confirmed on 2026-10-18 as Q268 "keep overlap"), opening the card to show every section inline (Q256), an open card closing with an ease at the turn's end (Q258) and the phone opening it with a tap (Q261). Still superseded: the 2026-09-16 rule that the working strip stays a bare line and never becomes a card (cards.rs), the 2026-10-11 order of the row above the composer (the compaction is the card's label with its progress beside it) and the pending tool card's own row at the transcript's tail (`session::composition`).
//!
//! CDXC:SessionChat 2026-10-29 SEE-ALSO:
//! The pills left the card for the chat box's strip, and the card is the working or worked line alone, one row (the user's words are on extras/chat_pills.rs). That supersedes the parts of the decision above that put the ? pill, the timers and idle pills on the card.
//!
//! CDXC:SessionChat 2026-10-18 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_card.rs (desktop and web) and apps/mobile/app/src/chat/native/cards/WorkingCard.tsx (phone) draw this value and must match.

use ghostex_gx_protocol::{ChatBlock, ChatRole};
use serde_json::Value;

use crate::document::{ChatPill, WorkingCard, WorkingStrip};
use crate::session::terminal::visible_terminal_tool;
use crate::state::ChatState;
use crate::transcript::foreign::terminal_tool_activity;
use crate::transcript::tool_card::mcp_call_label;
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_label::LabelTense;
use crate::transcript::tool_rows::tool_preview;
use crate::transcript::tool_summary::{tool_description, tool_display_name};

/// The card's projection from the working strip and the pills the document already holds.
pub fn working_card(state: &ChatState, strip: &WorkingStrip, pills: &[ChatPill]) -> WorkingCard {
    let working = crate::session::working::presents_working(state);
    let tool = visible_terminal_tool(state, crate::session::working::is_working(state))
        .map(terminal_tool_activity)
        .filter(|tool| {
            tool.get("label")
                .and_then(Value::as_str)
                .is_some_and(|label| !label.is_empty())
        });
    let running = (working && tool.is_none())
        .then(|| running_call_label(state))
        .flatten();
    // The transcript already says what the running call is doing (a card's or a line's live words,
    // a running row's shimmer): the card says something else rather than the same words twice.
    let (tool, running) = if working && transcript_draws_running_call(state) {
        let recorded = running_call_label(state);
        let same_call = |tool: &Value| {
            let label = tool
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default();
            recorded
                .as_deref()
                .is_some_and(|recorded| same_words(label, recorded))
        };
        (tool.filter(|tool| !same_call(tool)), None)
    } else {
        (tool, running)
    };
    let activity = strip.presentation.clone().filter(Value::is_object);
    // Status line in the chat: the finished turn's time stays on its fold, so the card never
    // shows the worked line (`transcript/chat_look.rs`); nothing changes while the agent works.
    let worked = state
        .transcript_view
        .worked_line
        .clone()
        .filter(|_| !working && !state.transcript_view.chat_look.status_in_transcript);
    let (label, label_kind) = if let Some(activity) = &activity {
        (
            activity
                .get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            "activity",
        )
    } else if let Some(tool) = &tool {
        (
            tool.get("label")
                .and_then(Value::as_str)
                .map(str::to_string),
            "tool",
        )
    } else if let Some(label) = running {
        (Some(label), "tool")
    } else if working && reads_subagent_report(state) {
        (Some("Reading the subagent's report".to_string()), "word")
    } else if let Some(word) = &strip.label {
        (Some(word.clone()), "word")
    } else if working {
        (Some("Working".to_string()), "word")
    } else {
        // A coordinator's finished turn says what it changed after its time, as one line
        // ("Worked for 15s · Pairing page started · 1 ask"; extras/work_cards.rs `turn_summary`).
        let worked =
            worked.clone().map(
                |worked| match crate::extras::work_cards::turn_summary(state) {
                    Some(summary) => format!("{worked} \u{b7} {summary}"),
                    None => worked,
                },
            );
        (worked, "worked")
    };
    // One line: the renderers cut it with an ellipsis and show the whole text on hover.
    let label = label
        .map(|label| label.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|label| !label.is_empty());
    let label_kind = label.as_ref().map(|_| label_kind.to_string());
    let pill_ids: Vec<String> = pills.iter().map(|pill| pill.id.clone()).collect();
    let phase = if working {
        "working"
    } else if label_kind.as_deref() == Some("worked") {
        "worked"
    } else {
        "idle"
    };
    let aria_label = label
        .iter()
        .cloned()
        .chain(pills.iter().map(|pill| pill.aria_label.clone()))
        .collect::<Vec<_>>()
        .join(", ");
    // The label in runs, its paths shortened (extras/working_card_label.rs).
    let tool_label = label_kind.as_deref() == Some("tool");
    let label_parts = label
        .as_deref()
        .filter(|_| tool_label)
        .map(crate::extras::working_card_label::label_parts)
        .unwrap_or_default();
    WorkingCard {
        // The bar is the working line alone; the pills ride in the chat box's strip
        // (extras/chat_pills.rs has the user's words).
        visible: label.is_some(),
        phase: phase.to_string(),
        working,
        label,
        label_kind,
        activity,
        pills: pill_ids,
        aria_label,
        label_parts,
        in_transcript: state.transcript_view.chat_look.status_in_transcript
            && !state.transcript_view.items.is_empty(),
    }
}

/// Whether the transcript draws the running call's own words where the reader sees them: a tool
/// call card's or Tool call lines' header always does, and the tool rows do when the reader's folds
/// leave the running call open ([`default_shows_running_call`]): they start open in Detailed view
/// and on the Summary view's running row, and sit shut behind their sentence in Normal view, so
/// there the card is where the words show until the reader opens the fold. Agent work Reduced
/// turns the cards off and draws the running calls as one line that starts open only in Detailed,
/// and Quiet leaves them out of the chat; both still draw a call waiting on an approval. Under
/// Reduced the line, and the Summary view's "Active work" fold around it, are read as the reader
/// left them ([`reduced_shows_running_call`]).
///
/// CDXC:SessionChat 2026-11-09 DECISION:
/// User, with Tool call cards on, of the card's live line and this card both reading "Type-check the phone and check Help on the rebased branch": "i still see the same text twice in the chat transcript man … we agreed to solve this". While the transcript shows the running call's words, this card shows the agent's own activity, else the working word and its clock, else "Working"; it names the call only when the transcript does not (a Normal-view row shut behind its sentence, or a call read off the screen that the transcript has not recorded yet). Later the same day the user approved that the check follows what the transcript actually draws under the chat look (`transcript/chat_look.rs`): under Agent work Quiet, and under Reduced while its line is shut, this card names the running call; the same words still never show twice. Supersedes this decision's earlier line that the chat look changes nothing here. Then, of Agent work Default, the user asked that it follow the same check of the folds as they really are, with nothing else in Default's look changed: a sentence's calls opened by hand in Normal view, or closed by hand in Detailed, decide the same way.
fn transcript_draws_running_call(state: &ChatState) -> bool {
    let view = &state.transcript_view;
    if view.grouped_work() {
        let waiting = crate::transcript::approval_wait::approval_wait(state).is_some();
        return waiting || (!view.hidden_work() && reduced_shows_running_call(state));
    }
    view.tool_cards || default_shows_running_call(state)
}

/// Agent work Default: whether the transcript shows the newest running call's row with the folds
/// as the renderer reported them (`searchFolds`), read through the same walk Find uses
/// ([`visible_segments`]): a sentence's calls open by default only in Detailed or while the
/// running summary turn opens them (`toolsOpen`), and the Summary view's "Active work" fold
/// around them. The defaults come from the core's own view level, as the renderers derive theirs.
fn default_shows_running_call(state: &ChatState) -> bool {
    use crate::document::TranscriptItem;
    use crate::extras::transcript_search::visible_segments;
    use crate::transcript::view_level::{view_level, DETAILED};
    let view = &state.transcript_view;
    let running = |row: &Value| -> Vec<usize> {
        row["tools"]
            .as_array()
            .map(|tools| {
                (0..tools.len())
                    .filter(|index| tools[*index]["status"] == "running")
                    .collect()
            })
            .unwrap_or_default()
    };
    // The newest row running a call, and the item that draws it.
    // Newest item first, stopping at the first one running a call (the list has no reverse iterator).
    let mut newest_first = (0..view.items.len())
        .rev()
        .filter_map(|index| view.items.get(index));
    let Some((item, row, calls)) = newest_first.find_map(|item| {
        let rows: Vec<&Value> = match item {
            TranscriptItem::Message { message } => vec![&**message],
            // Newest first below: `work` holds the rows in the order they came, `cards` the
            // copies drawn outside its fold.
            TranscriptItem::Summary { work, cards, .. } => {
                cards.iter().chain(work.iter()).map(|row| &**row).collect()
            }
            _ => Vec::new(),
        };
        rows.into_iter().rev().find_map(|row| {
            let calls = running(row);
            (!calls.is_empty()).then(|| {
                (
                    item,
                    row["id"].as_str().unwrap_or_default().to_string(),
                    calls,
                )
            })
        })
    }) else {
        return false;
    };
    let Ok(item) = serde_json::to_value(item) else {
        return false;
    };
    let detailed = view_level(view) == DETAILED;
    let mut folds = state.extras.search.folds.clone();
    folds.simple = !detailed;
    folds.work_open = detailed;
    folds.quiet_style = false;
    folds.hidden = false;
    let newest = calls.last().copied().unwrap_or_default();
    let label = format!("tool:{row}:{newest}");
    visible_segments(&item, &folds)
        .iter()
        .any(|segment| segment.label.as_deref() == Some(label.as_str()))
}

/// Agent work Reduced: whether the row running a call shows its calls with the folds as the
/// renderer reported them (`searchFolds`, `transcript_search::SearchFolds`): the row's quiet line,
/// open by default only in Detailed (the desktop's `tool_run.rs`, the phone's `ToolRows.tsx`), and
/// in the Summary view the turn's "Active work" fold around it. A row drawn in full shows them.
fn reduced_shows_running_call(state: &ChatState) -> bool {
    use crate::document::TranscriptItem;
    use crate::transcript::view_level::{view_level, DETAILED};
    let view = &state.transcript_view;
    let folds = &state.extras.search.folds;
    let detailed = view_level(view) == DETAILED;
    let runs = |row: &&crate::document::ProjectedMessage| {
        row["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["status"] == "running"))
    };
    let line_open = |row: &Value| {
        if row["toolsShowAllRows"] == true {
            return true;
        }
        let id = row["id"].as_str().unwrap_or_default();
        let key = row["toolsLineKey"]
            .as_str()
            .map_or_else(|| format!("tools:{id}"), str::to_string);
        let awaiting = row["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["awaitingApproval"] == true));
        folds.open(&key, awaiting || detailed)
    };
    // The row of the call the card names ([`running_call_label`], the newest one still out)
    // decides: the last running row in the list's order, where a summary turn's `work` holds its
    // rows in the order they came and `cards` the copies drawn outside its fold.
    view.items
        .iter()
        .filter_map(|item| match item {
            TranscriptItem::Message { message } if runs(&message) => Some(line_open(message)),
            TranscriptItem::Summary {
                id,
                work,
                work_open,
                cards,
                ..
            } => work.iter().rev().find(runs).map(|row| {
                match cards.iter().find(|card| card["id"] == row["id"]) {
                    Some(card) => line_open(card),
                    None => folds.open(&format!("active-work:{id}"), *work_open) && line_open(row),
                }
            }),
            _ => None,
        })
        .last()
        .unwrap_or(false)
}

/// Agent work Quiet hides a subagent's report as the agent's work, so while the agent has not gone
/// on from it the line says what it does (`turns::report_lands_mid_response`).
fn reads_subagent_report(state: &ChatState) -> bool {
    use crate::transcript::turns::{is_subagent_report, passed_over_before_wake};
    state.transcript_view.hidden_work()
        && state
            .messages
            .list
            .iter()
            .rev()
            .find(|row| !passed_over_before_wake(row))
            .is_some_and(is_subagent_report)
}

/// The two labels name the same call: equal once their spacing and case are set aside.
fn same_words(a: &str, b: &str) -> bool {
    let words = |text: &str| {
        text.split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    !a.trim().is_empty() && words(a) == words(b)
}

/// The label of the call the transcript records as still running in the current turn: its
/// description, else its name and command. The tool row read off the screen retires once the
/// transcript records the call, and the label must stay for as long as the call runs (Q254).
fn running_call_label(state: &ChatState) -> Option<String> {
    let mut answered: Vec<&str> = Vec::new();
    for message in state.messages.list.iter().rev().take(400) {
        // A background task's end reaches the chat as a user row (`<task-notification>`), but it
        // is the agent's work: a call it lands beside is still running.
        let mut prompt = message.role == ChatRole::User
            && !crate::transcript::background_rows::is_background_task_row(message);
        for block in message.blocks.iter().rev() {
            match block {
                ChatBlock::ToolResult {
                    call_id: Some(id), ..
                } => {
                    prompt = false;
                    answered.push(id);
                }
                ChatBlock::ToolResult { .. } => prompt = false,
                ChatBlock::ToolCall {
                    name,
                    input,
                    call_id: Some(id),
                } if !answered.contains(&id.as_str()) => {
                    let description = tool_description(input);
                    if !description.is_empty() {
                        return Some(description);
                    }
                    // An MCP call of an app the tool cards know reads as a verb on it
                    // ("Read SPX-412 in Linear").
                    if let Some(label) = mcp_call_label(name, input, LabelTense::Base) {
                        return Some(label);
                    }
                    if let Some(label) =
                        crate::transcript::tool_label::tool_label(name, input, None, false)
                    {
                        return Some(label.text);
                    }
                    // A tool nothing here knows: its name in words and, at most, the one argument
                    // that reads as a summary, never its arguments as JSON (`tool_input_gist`).
                    let preview = tool_preview(
                        &ToolPair {
                            call: Some(block),
                            result: None,
                        },
                        None,
                    );
                    let name = tool_display_name(name);
                    return Some(if preview.is_empty() {
                        name
                    } else {
                        format!("{name}: {preview}")
                    });
                }
                _ => {}
            }
        }
        // The prompt that opened the turn: nothing before it is running.
        if prompt {
            return None;
        }
    }
    None
}
