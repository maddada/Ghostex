//! Cmd+F over the transcript, ported from
//! `packages/shared/session-chat-presentation/transcript-search.ts`.
//!
//! React searched the rendered DOM because it had one; the native chat has a list of projected
//! items instead, so the same query runs over the text those items show with the renderer's folds
//! as they are ([`visible_segments`]). It is case-insensitive, counts occurrences per row, and keeps
//! the selected occurrence across a transcript refresh, as the React search did.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One occurrence of the query.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMatch {
    /// Index into the transcript item list the host ships to the renderer.
    pub item_index: usize,
    /// Identity that survives a transcript refresh: row id plus occurrence.
    pub key: String,
    /// The one-line label the occurrence is in (a tool row's name or preview, a thinking
    /// headline), by the renderer's key for it; `None` for the row's Markdown, which the
    /// renderer's text views number in reading order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The renderer's fold state, which decides what a reader can see.
///
/// The keys are the renderer's own (`work:<id>`, `tools:<id>`, `reasoning:<id>`, `tool-run:<id>`,
/// `prompt:<id>`, …), the same on desktop (`native_chat/state.rs` `expanded` / `collapsed`) and the
/// phone (`transcript/state.ts` `DisclosureStore`). A fold is open when the reader opened it, or
/// when it opens by default and the reader did not close it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchFolds {
    pub expanded: BTreeSet<String>,
    pub collapsed: BTreeSet<String>,
    /// Summarized mode: tool runs fold behind "N tool calls" and drop their command previews.
    pub simple: bool,
    /// Whether turns and tool calls open by default (verbose, or not summarized).
    pub work_open: bool,
    /// Agent work is Reduced or Quiet (`transcript/chat_look.rs`), which draw the work in the
    /// quiet style: a tool-only run folds behind its quiet line in every view, open by default in
    /// Detailed, and an opened work fold lists its calls as quiet steps. The core sets it from its
    /// own chat look, not the renderer.
    pub quiet_style: bool,
    /// Agent work is Quiet: a grouped row's opened work sits right under the reply that carries
    /// its button (`workCarrier`), after that reply, as the renderers draw it.
    pub hidden: bool,
}

impl SearchFolds {
    /// The `searchFolds` action's payload.
    pub fn from_action(action: &serde_json::Map<String, Value>) -> Self {
        let keys = |name: &str| -> BTreeSet<String> {
            action
                .get(name)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        };
        Self {
            expanded: keys("expanded"),
            collapsed: keys("collapsed"),
            simple: action.get("simple") == Some(&Value::Bool(true)),
            work_open: action.get("workOpen") == Some(&Value::Bool(true)),
            quiet_style: false,
            hidden: false,
        }
    }

    /// The renderers' `is_expanded(key, default)`.
    fn open(&self, key: &str, default: bool) -> bool {
        self.expanded.contains(key) || default && !self.collapsed.contains(key)
    }
}

/// A run of text a reader can see in one row, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub text: String,
    /// The renderer's key for a one-line label; `None` for Markdown.
    pub label: Option<String>,
}

/// CDXC:SessionChat 2026-10-20 DECISION:
/// User: "please don't search inside the collapsed areas when i do cmd + f in the gpui chat view". Find searches only what the reader can see with the folds as they are: a closed "Worked for" turn, "Agent reply" or "Active work" fold, tool-call disclosure, "+N previous tool calls" run, thinking fold, folded card or a long prompt's hidden tail ("Show more") is skipped, and the count counts only what is left. A block capped to a scroll box (a long code block, a long thought) counts whole, since its text is on screen and scrolls to the match. Card chrome (labels, file paths, "Worked for") is not searched. The renderers walk the same keys and defaults (desktop `native_chat/transcript.rs`, `tool_run.rs`, `completed_work_row.rs`; phone `transcript/MessageRow.tsx`, `ToolRows.tsx`, `TranscriptItemRow.tsx`) and report their fold state with `searchFolds`.
pub fn visible_segments(item: &Value, folds: &SearchFolds) -> Vec<Segment> {
    let mut out = Vec::new();
    match item.get("kind").and_then(Value::as_str) {
        Some("summary") => {
            let id = str_of(item, "id");
            message_segments(&item["user"], folds, false, &mut out);
            for message in array(item, "outcome") {
                message_segments(message, folds, false, &mut out);
            }
            let has_final = item["final"].is_object();
            if has_final || item["active"] == true {
                let key = if has_final {
                    format!("summary:{id}")
                } else {
                    format!("active-work:{id}")
                };
                let default = item["latestReply"] == true || item["workOpen"] == true;
                if folds.open(&key, default) {
                    if has_final {
                        for reply in array(item, "earlierReplies") {
                            message_segments(reply, folds, false, &mut out);
                        }
                        message_segments(&item["final"], folds, false, &mut out);
                    } else {
                        for message in array(item, "work") {
                            message_segments(message, folds, false, &mut out);
                        }
                    }
                }
            }
        }
        Some("completed-work") => {
            let id = str_of(item, "id");
            if folds.open(&format!("work:{id}"), folds.work_open) {
                for message in array(item, "work") {
                    message_segments(message, folds, true, &mut out);
                }
            }
            for message in array(item, "artifacts") {
                message_segments(message, folds, false, &mut out);
            }
            if item["final"].is_object() {
                message_segments(&item["final"], folds, false, &mut out);
            }
        }
        // Agent work Quiet: each turn's tail, the opened work right after the reply that carries
        // its button (`workCarrier`), or after the tails when no reply does.
        Some("grouped-work") if folds.hidden => {
            let id = str_of(item, "id");
            let open = folds.open(&format!("work:{id}"), folds.work_open);
            let carrier = item["workCarrier"].as_str();
            let steps = |out: &mut Vec<Segment>| {
                for message in array(item, "work") {
                    message_segments(message, folds, true, out);
                }
            };
            for turn in array(item, "turns") {
                for message in array(turn, "artifacts") {
                    message_segments(message, folds, false, &mut out);
                }
                if turn["final"].is_object() {
                    message_segments(&turn["final"], folds, false, &mut out);
                    if open && carrier.is_some() && turn["final"]["id"].as_str() == carrier {
                        steps(&mut out);
                    }
                }
            }
            if open && carrier.is_none() {
                steps(&mut out);
            }
        }
        // Agent work Reduced: one fold over every merged turn's work, then each turn's tail.
        Some("grouped-work") => {
            let id = str_of(item, "id");
            if folds.open(&format!("work:{id}"), folds.work_open) {
                for message in array(item, "work") {
                    message_segments(message, folds, true, &mut out);
                }
            }
            for turn in array(item, "turns") {
                for message in array(turn, "artifacts") {
                    message_segments(message, folds, false, &mut out);
                }
                if turn["final"].is_object() {
                    message_segments(&turn["final"], folds, false, &mut out);
                }
            }
        }
        _ => message_segments(&item["message"], folds, false, &mut out),
    }
    out.retain(|segment| !segment.text.is_empty());
    out
}

/// `message_row`'s visible text. `in_work_fold` is a finished turn's opened log, where answered
/// questions and sent messages stay plain tool rows.
fn message_segments(
    message: &Value,
    folds: &SearchFolds,
    in_work_fold: bool,
    out: &mut Vec<Segment>,
) {
    if !message.is_object() {
        return;
    }
    let id = str_of(message, "id");
    let body = message_body(message);
    let user = message["role"] == "user";
    let markdown = |text: String| Segment { text, label: None };
    if message["pageAnswers"].is_object() && message["pageAnswers"]["withText"] != true {
        return;
    }
    if message["shellCommand"] == true {
        let card = &message["shellCard"];
        let default = card["openByDefault"] == true;
        if card["hasBody"] == true && folds.open(&format!("shell:{id}"), default) {
            out.push(markdown(str_of(card, "output")));
        }
        return;
    }
    if user && message["suppressed"].is_null() && !message["interAgentMessage"].is_object() {
        let collapsed = str_of(message, "collapsedText");
        if !collapsed.is_empty() && !folds.expanded.contains(&format!("prompt:{id}")) {
            out.push(markdown(collapsed));
        } else {
            out.push(markdown(body));
        }
        return;
    }
    if message["suppressed"].is_object() {
        // Status and inline rows are one-line cards; the rest fold their text away.
        let kind = message["suppressed"]["kind"].as_str().unwrap_or_default();
        if kind != "status" && kind != "inline" && folds.open(&format!("suppressed:{id}"), false) {
            out.push(markdown(body));
        }
        return;
    }
    if message["apiError"].is_object() {
        return;
    }
    if message["systemCard"].is_object() {
        let visible = match message["systemCard"]["kind"].as_str().unwrap_or_default() {
            "goal" => folds.open(&format!("goal:{id}"), false),
            "command-output" => folds.open(&format!("command-output:{id}"), true),
            _ => true,
        };
        if visible {
            out.push(markdown(body));
        }
        return;
    }
    if message["interAgentMessage"].is_object() || message["sideQuestion"].is_object() {
        out.push(markdown(body));
        return;
    }
    let reasoning = message["role"] == "reasoning";
    // Inside a turn fold a row's text owns none of its calls: they follow it, always shown.
    let flat = message["toolsFlat"] == true;
    let has_tools = !flat && !array(message, "tools").is_empty();
    let tools_default = folds.work_open || message["toolsOpen"] == true;
    if body.is_empty() {
        tool_segments(message, folds, in_work_fold, out);
    } else if reasoning && has_tools && message["thinkingCard"]["calls"].is_string() {
        // A running thought folds its calls behind its line (`thinking_card.rs`); its text opens
        // on its own, so only the line and the calls it folds are searched.
        let key = format!("reasoning:{id}");
        out.push(Segment {
            text: str_of(&message["thinkingCard"], "preview"),
            label: Some(key.clone()),
        });
        if folds.open(&key, tools_default) {
            tool_segments(message, folds, in_work_fold, out);
        }
    } else if reasoning && has_tools {
        // A reasoning headline owns the tools that followed it: the headline shows, the thought's
        // tail and the tools open behind it.
        let key = format!("reasoning:{id}");
        out.push(Segment {
            text: str_of(&message["reasoning"], "headline"),
            label: Some(key.clone()),
        });
        if folds.open(&key, tools_default) {
            out.push(markdown(str_of(&message["reasoning"], "body")));
            tool_segments(message, folds, in_work_fold, out);
        }
    } else {
        out.push(markdown(body));
        if flat || (has_tools && !reasoning && folds.open(&format!("tools:{id}"), tools_default)) {
            tool_segments(message, folds, in_work_fold, out);
        }
    }
}

/// `tool_rows`: the rows a run shows with its folds as they are, each as its name and preview.
fn tool_segments(message: &Value, folds: &SearchFolds, in_work_fold: bool, out: &mut Vec<Segment>) {
    let tools = array(message, "tools");
    if tools.is_empty() {
        return;
    }
    let id = str_of(message, "id");
    let show_all = message["toolsShowAllRows"] == true;
    let cards_as_rows = show_all || in_work_fold;
    let visible: Vec<usize> = (0..tools.len())
        .filter(|index| {
            cards_as_rows
                || (tools[*index]["exchange"] != true && tools[*index]["sentMessage"] != true)
        })
        .collect();
    // An opened quiet line draws every call as one of its steps at once (the desktop's
    // `quiet_step_row`, the phone's `QuietStepRow`): no `tools:` fold stands between them.
    let shown: Vec<usize> = if folds.quiet_style && in_work_fold {
        visible
    } else if (folds.simple || folds.quiet_style) && !show_all {
        let awaiting = tools.iter().any(|tool| tool["awaitingApproval"] == true);
        if folds.open(
            &format!("tools:{id}"),
            awaiting || (folds.quiet_style && folds.work_open),
        ) {
            visible
        } else {
            Vec::new()
        }
    } else {
        let fold = &message["toolFold"];
        let hidden = fold["hiddenCount"].as_u64().unwrap_or(0);
        if show_all || hidden == 0 || folds.expanded.contains(&format!("tool-run:{id}")) {
            visible
        } else {
            visible
                .into_iter()
                .filter(|index| fold["visible"][*index] == true)
                .collect()
        }
    };
    // An opened quiet line's steps: the call's name, then its argument whatever the view, as the
    // desktop's `quiet_step_row` (quiet_work.rs) and the phone draw them.
    if folds.quiet_style && in_work_fold {
        for index in shown {
            let tool = &tools[index];
            let (name, preview) = quiet_step_text(tool);
            out.push(Segment {
                text: name,
                label: Some(format!("tool:{id}:{index}")),
            });
            out.push(Segment {
                text: preview,
                label: Some(format!("tool-preview:{id}:{index}")),
            });
        }
        return;
    }
    for index in shown {
        let tool = &tools[index];
        let described = !str_of(tool, "description").is_empty();
        let name = match str_of(tool, "label") {
            label if label.is_empty() => str_of(tool, "name"),
            label => label,
        };
        out.push(Segment {
            text: name,
            label: Some(format!("tool:{id}:{index}")),
        });
        // A subagent row shows the agent instead of the preview.
        if !str_of(&tool["subagent"], "name").is_empty() {
            continue;
        }
        let argument = if folds.simple || described {
            String::new()
        } else {
            str_of(tool, "preview")
        };
        let failure = str_of(tool, "failure");
        out.push(Segment {
            text: tool_preview_line(&argument, &failure),
            label: Some(format!("tool-preview:{id}:{index}")),
        });
    }
}

/// What a quiet step says (Agent work Reduced or Quiet): the tool's name (its label when it
/// has none), then its argument, else the subagent it ran or its description, then what failed.
pub fn quiet_step_text(tool: &Value) -> (String, String) {
    let name = match str_of(tool, "name") {
        name if name.is_empty() => str_of(tool, "label"),
        name => name,
    };
    let argument = match str_of(tool, "preview") {
        preview if preview.is_empty() => match str_of(&tool["subagent"], "name") {
            agent if agent.is_empty() => str_of(tool, "description"),
            agent => agent,
        },
        preview => preview,
    };
    (name, tool_preview_line(&argument, &str_of(tool, "failure")))
}

/// A tool row's preview line: the argument, then what failed when it failed.
pub fn tool_preview_line(argument: &str, failure: &str) -> String {
    match (argument.is_empty(), failure.is_empty()) {
        (_, true) => argument.to_string(),
        (true, false) => failure.to_string(),
        (false, false) => format!("{argument} \u{b7} {failure}"),
    }
}

/// `sessionChatTranscriptMatches`: every occurrence of `query` the reader can see, row by row, in
/// transcript order.
pub fn transcript_matches(
    items: &[Value],
    query: &str,
    folds: &SearchFolds,
) -> Vec<TranscriptMatch> {
    let needle = js_trim(query).to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let needle_utf16: Vec<u16> = needle.encode_utf16().collect();
    let mut matches = Vec::new();
    for (item_index, item) in items.iter().enumerate() {
        let segments = visible_segments(item, folds);
        if segments.is_empty() {
            continue;
        }
        let id = item_id(item);
        let mut occurrence = 0usize;
        for segment in segments {
            // The TypeScript walks the string by UTF-16 index, and a `key` only has to be stable,
            // so the occurrence counter is what the two sides must agree on.
            let units: Vec<u16> = segment.text.to_lowercase().encode_utf16().collect();
            let mut at = 0usize;
            while let Some(found) = index_of(&units, &needle_utf16, at) {
                matches.push(TranscriptMatch {
                    item_index,
                    key: format!("{id}#{occurrence}"),
                    label: segment.label.clone(),
                });
                occurrence += 1;
                at = found + needle_utf16.len();
            }
        }
    }
    matches
}

/// `sessionChatSearchCountLabel`: the terminal-style counter beside the field.
pub fn search_count_label(query: &str, total: usize, active_index: usize) -> String {
    if js_trim(query).is_empty() {
        return String::new();
    }
    if total > 0 {
        format!("{}/{total}", active_index + 1)
    } else {
        "N/A".to_string()
    }
}

/// A message's text as its Markdown shows it, without the marked-section marks.
fn message_body(message: &Value) -> String {
    message
        .get("text")
        .and_then(Value::as_str)
        .map(crate::transcript::marked_sections::without_section_marks)
        .unwrap_or_default()
}

fn str_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// `itemId`: the row kind plus whichever id the row carries.
fn item_id(item: &Value) -> String {
    let kind = item.get("kind").and_then(Value::as_str).unwrap_or_default();
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| {
            item.get("message")
                .and_then(|message| message.get("id"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    format!("{kind}:{id}")
}

/// `String.prototype.indexOf` over UTF-16 code units, from `at`.
fn index_of(haystack: &[u16], needle: &[u16], at: usize) -> Option<usize> {
    if needle.is_empty() || at + needle.len() > haystack.len() {
        return None;
    }
    (at..=haystack.len() - needle.len())
        .find(|start| &haystack[*start..*start + needle.len()] == needle)
}

/// `String.prototype.trim`, re-exported for the search's own use.
fn js_trim(value: &str) -> &str {
    crate::extras::agent_tasks::js_trim(value)
}
