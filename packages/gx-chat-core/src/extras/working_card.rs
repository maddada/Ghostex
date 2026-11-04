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
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_rows::tool_preview;
use crate::transcript::tool_summary::tool_description;

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
    let activity = strip.presentation.clone().filter(Value::is_object);
    let worked = state
        .transcript_view
        .worked_line
        .clone()
        .filter(|_| !working);
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
    } else if let Some(word) = &strip.label {
        (Some(word.clone()), "word")
    } else {
        (worked.clone(), "worked")
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
    }
}

/// The label of the call the transcript records as still running in the current turn: its
/// description, else its name and command. The tool row read off the screen retires once the
/// transcript records the call, and the label must stay for as long as the call runs (Q254).
fn running_call_label(state: &ChatState) -> Option<String> {
    let mut answered: Vec<&str> = Vec::new();
    for message in state.messages.list.iter().rev().take(400) {
        let mut prompt = message.role == ChatRole::User;
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
                    if let Some(label) =
                        crate::transcript::tool_label::tool_label(name, input, None, false)
                    {
                        return Some(label.text);
                    }
                    let preview = tool_preview(
                        &ToolPair {
                            call: Some(block),
                            result: None,
                        },
                        None,
                    );
                    return Some(if preview.is_empty() {
                        name.clone()
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
