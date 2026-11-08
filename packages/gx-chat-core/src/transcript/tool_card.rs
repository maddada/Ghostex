//! The tool-call card: one rounded card per burst of tool calls between two pieces of the agent's
//! text, each call a row that reads like a person describing it, in the tense of the call
//! ("Reading notice_choices.rs", then "Read notice_choices.rs"), with a short result and its time
//! on the right, and a header that counts ("Using 3 tools", "Used 6 tools · 1 failed · 1m 4s").
//!
//! CDXC:SessionChat 2026-10-28 DECISION: User, after a video of tool calls they liked: "Let's please do this under feature flag if possible (toggle in settings) so i can test it. Keep current look as it is." With Settings > Chat > Tool call cards (`chatToolCallCards`, off by default) on, a message's tool calls and file edits are one card (Q708, all seven parts): the tools' icons stacked in its header with "Using N tools" / "Used N tools", "N failed" and the burst's time; each row in the call's tense with its result ("92 lines", "6 matches", "+18 −4", "214 passed", "exit 1") and time ("30ms", "1.2s"); a shimmer on the running row; short motion; a failed row says what failed with the reason on a second line, and no Retry; MCP calls of known apps read as a verb on the app ("Reading SPX-412 in Linear") with its logo (`tool_apps.rs`). Approvals keep their approval card; the waiting call's row says it waits (Q709). A card folds to one line as soon as its tools are done and the agent writes again, and the turn still folds into "Worked for" (Q710). With the switch off nothing here runs and every row draws as before. The renderers only draw this: desktop `native_chat/tool_card.rs`, phone `ToolCard.tsx`.
//!
//! CDXC:SessionChat 2026-11-08 DECISION: User, with Tool call cards on: "when this is enabled i see tools card area jump open then close", then picked "Folded, with the live line in the header" (Q1925) and the newest running call in it (Q1926), with "i dont get what 2 of 3 means tho, we need different text there pls". A card is one line while its calls run and after: the header says the newest running call in its own words with the shimmer and a spinner (`liveLabel`), and "+N more" for the card's other calls (`liveMore`); once they are done it says "Used N tools" as before. It never opens or folds by itself: it opens when the reader clicks it, or when a call in it waits on an approval (`waiting`, set by `mark_card_waiting`). Supersedes the part of the 2026-10-28 decision (Q710) that drew a card open while its calls ran and until the agent wrote again; every card opened, then folded a second later, because Claude writes a sentence before almost every call, and a line streamed from the terminal folded it and opened it again on top of that.

use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Map, Value};

use crate::transcript::file_changes::file_changes;
use crate::transcript::jsstr::{collapse_whitespace, js_trim};
use crate::transcript::tool_apps::{tool_app_for_server, ToolApp};
use crate::transcript::tool_fold::{is_tool_only_message, pair_tool_blocks, ToolPair};
use crate::transcript::tool_label::{tool_label_in, LabelTense};
use crate::transcript::tool_rows::is_command_tool;
use crate::transcript::tool_summary::{tool_description, truncate_tool_preview};
use crate::transcript::work_summary::{tool_duration_label, tool_time_label};

/// How many tool icons the header stacks before "+N".
pub const MAX_STACKED_ICONS: usize = 4;
/// How long a failed call's reason may run on its second line.
const REASON_LENGTH: usize = 140;
/// How long an MCP call's object (an issue key, a query) may run.
const MCP_OBJECT_LENGTH: usize = 48;
/// How far into a call's output the result readers look.
const RESULT_SCAN_LINES: usize = 4000;

/// What a call is doing, as its row draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Running,
    Done,
    Failed,
    /// The call never answered and the turn is over (interrupted, or the agent stopped).
    Stopped,
}

impl Status {
    fn of(answered: bool, failed: bool, working: bool) -> Self {
        match (answered, failed) {
            (_, true) => Self::Failed,
            (true, false) => Self::Done,
            (false, false) if working => Self::Running,
            (false, false) => Self::Stopped,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    fn tense(self) -> LabelTense {
        match self {
            Self::Running => LabelTense::Running,
            Self::Done => LabelTense::Done,
            Self::Failed => LabelTense::Failed,
            Self::Stopped => LabelTense::Base,
        }
    }
}

/// One row of a message's tool calls or file edits, in the order the calls were made.
enum Slot<'a> {
    /// A tool row, and whether the same call also wrote files (an `exec` wrapper).
    Tool(usize, ToolPair<'a>, bool),
    File(usize, ToolPair<'a>),
}

/// The message's calls in the order they were made, numbered the way its `tools` and `files`
/// rows are (`split_file_changes`): a write is one file row per file it touches, and a call that
/// wrote nothing (or an `exec` wrapper, which may have run anything) is one tool row.
fn slots<'a>(tools: &[&'a ChatBlock]) -> Vec<Slot<'a>> {
    let mut slots = Vec::new();
    let (mut tool, mut file) = (0, 0);
    for pair in pair_tool_blocks(tools.iter().copied()) {
        let files = match pair.call {
            Some(ChatBlock::ToolCall { name, input, .. }) => file_changes(name, input).len(),
            _ => 0,
        };
        let is_exec = pair
            .call_name()
            .and_then(|name| name.split('.').next_back())
            == Some("exec");
        for _ in 0..files {
            slots.push(Slot::File(file, pair));
            file += 1;
        }
        if files == 0 || is_exec {
            slots.push(Slot::Tool(tool, pair, files > 0));
            tool += 1;
        }
    }
    slots
}

/// The card for one message's calls, or `None` when it made none the card draws.
///
/// `rows` and `files` are the message's projected `tools` and `files`; `spans` when each call
/// started and ended, by call id (`work_summary::tool_spans`). `working` is whether the session
/// is working: a call that never answered after the turn ended reads as stopped, not running.
pub fn tool_card(
    tools: &[&ChatBlock],
    rows: &[Value],
    files: &[Value],
    spans: &HashMap<String, (i64, i64)>,
    working_directory: Option<&str>,
    working: bool,
) -> Option<Value> {
    let mut entries: Vec<Value> = Vec::new();
    let mut started: Option<i64> = None;
    let mut ended: Option<i64> = None;
    for slot in slots(tools) {
        let (pair, entry) = match slot {
            Slot::Tool(index, pair, wrote) => {
                let Some(row) = rows.get(index) else {
                    continue;
                };
                // An `exec` that only wrote files is already its file rows.
                if wrote && edits_only(&pair, working_directory) {
                    continue;
                }
                // Conversation and runs with cards of their own stay out of the card.
                if row["exchange"] == true
                    || row["sentMessage"] == true
                    || row.get("workflow").is_some()
                {
                    continue;
                }
                (
                    pair,
                    tool_entry(index, &pair, row, working_directory, working),
                )
            }
            Slot::File(index, pair) => {
                let Some(file) = files.get(index) else {
                    continue;
                };
                (pair, file_entry(index, &pair, file, working))
            }
        };
        let mut entry = entry;
        if let Some(ChatBlock::ToolCall {
            call_id: Some(id), ..
        }) = pair.call
        {
            if let Some((start, end)) = spans.get(id) {
                started = Some(started.map_or(*start, |at| at.min(*start)));
                ended = Some(ended.map_or(*end, |at| at.max(*end)));
                if end > start && entry["status"] != "running" {
                    entry["time"] = tool_time_label(end - start).into();
                }
            }
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        return None;
    }
    let count = entries.len();
    let running = entries.iter().any(|entry| entry["status"] == "running");
    let stopped = entries.iter().any(|entry| entry["status"] == "stopped");
    let failed = entries
        .iter()
        .filter(|entry| entry["status"] == "failed")
        .count();
    let time = match (running, started, ended) {
        (false, Some(start), Some(end)) if end > start => tool_duration_label(end - start),
        _ => String::new(),
    };
    let icons: Vec<Value> = entries
        .iter()
        .take(MAX_STACKED_ICONS)
        .map(|entry| json!({ "glyph": entry["glyph"], "app": entry["app"], "appColor": entry["appColor"] }))
        .collect();
    let failed_label = if failed > 0 {
        format!("{failed} failed")
    } else {
        String::new()
    };
    // The folded card's live line: the newest running call in its own words, and how many other
    // calls the card holds.
    let live_label = entries
        .iter()
        .rev()
        .find(|entry| entry["status"] == "running")
        .map(|entry| text(entry, "label"))
        .unwrap_or_default();
    let live_more = if live_label.is_empty() || count < 2 {
        String::new()
    } else {
        format!("+{} more", count - 1)
    };
    let used = tools_label(if running { "Using" } else { "Used" }, count);
    let folded = [used.as_str(), failed_label.as_str(), time.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    Some(json!({
        "entries": entries,
        "icons": icons,
        "moreIcons": count.saturating_sub(MAX_STACKED_ICONS),
        "count": count,
        "running": running,
        "stopped": stopped,
        "title": used,
        "liveLabel": live_label,
        "liveMore": live_more,
        "failed": failed,
        "failedLabel": failed_label,
        "time": time,
        "foldedLabel": folded,
    }))
}

/// Whether everything the call did was writing files ("Edited a.rs and b.rs").
fn edits_only(pair: &ToolPair<'_>, working_directory: Option<&str>) -> bool {
    let (Some(name), Some(input)) = (pair.call_name(), pair.call_input()) else {
        return false;
    };
    tool_label_in(name, input, working_directory, LabelTense::Done)
        .and_then(|label| label.verb)
        .is_some_and(|verb| matches!(verb, "Edited" | "Wrote" | "Created" | "Deleted"))
}

/// "Using 1 tool", "Used 6 tools".
fn tools_label(verb: &str, count: usize) -> String {
    format!("{verb} {count} tool{}", if count == 1 { "" } else { "s" })
}

fn tool_entry(
    index: usize,
    pair: &ToolPair<'_>,
    row: &Value,
    working_directory: Option<&str>,
    working: bool,
) -> Value {
    let name = pair.call_name().unwrap_or_default();
    let input = pair.call_input().unwrap_or(&Value::Null);
    let output = pair.result_output().unwrap_or_default();
    let status = if crate::transcript::background_rows::is_stopped_task(pair) {
        Status::Stopped
    } else {
        Status::of(pair.result.is_some(), pair.result_is_error(), working)
    };
    let mcp = name
        .strip_prefix("mcp__")
        .and_then(|rest| mcp_call(rest, input));
    let label_in = |tense: LabelTense| -> Option<(String, Option<&'static str>)> {
        if let Some(call) = &mcp {
            return Some((call.label(tense), None));
        }
        tool_label_in(name, input, working_directory, tense).map(|label| (label.text, label.verb))
    };
    // The agent's own words for the call stand in every tense; without them, what the call did.
    let description = tool_description(input);
    let (label, verb) = if description.is_empty() {
        label_in(status.tense()).unwrap_or_else(|| (text(row, "label"), None))
    } else {
        (
            description.clone(),
            label_in(LabelTense::Done).and_then(|(_, verb)| verb),
        )
    };
    let mut entry = Map::new();
    entry.insert("kind".to_string(), "tool".into());
    entry.insert("index".to_string(), index.into());
    entry.insert("label".to_string(), label.into());
    // What a call waiting on an approval says instead ("Run git push origin main").
    if status == Status::Running {
        let waiting = if description.is_empty() {
            label_in(LabelTense::Base).map(|(text, _)| text)
        } else {
            None
        };
        entry.insert(
            "waitingLabel".to_string(),
            waiting.unwrap_or(description).into(),
        );
    }
    entry.insert("status".to_string(), status.name().into());
    entry.insert(
        "glyph".to_string(),
        row.get("glyph").cloned().unwrap_or_else(|| "tool".into()),
    );
    let app = mcp.as_ref().and_then(|call| call.app);
    entry.insert(
        "app".to_string(),
        app.map_or(Value::Null, |app| app.key.into()),
    );
    entry.insert(
        "appColor".to_string(),
        app.map_or(Value::Null, |app| app.color.into()),
    );
    let result = match status {
        Status::Done | Status::Failed => tool_result(name, verb, output, status == Status::Failed),
        _ => String::new(),
    };
    entry.insert("result".to_string(), result.into());
    entry.insert(
        "reason".to_string(),
        if status == Status::Failed {
            failure_reason(output)
        } else {
            String::new()
        }
        .into(),
    );
    entry.insert("time".to_string(), "".into());
    // The pictures the call opened from disk, drawn under the entry like a plain row's
    // (`read_images.rs`).
    if let Some(images) = row.get("images") {
        entry.insert("images".to_string(), images.clone());
    }
    Value::Object(entry)
}

fn file_entry(index: usize, pair: &ToolPair<'_>, file: &Value, working: bool) -> Value {
    let failed = file["failed"] == true;
    let status = Status::of(pair.result.is_some(), failed, working);
    let verbs: [&str; 4] = match file["status"].as_str() {
        Some("new") => ["Create", "Creating", "Created", "create"],
        Some("deleted") => ["Delete", "Deleting", "Deleted", "delete"],
        _ => ["Edit", "Editing", "Edited", "edit"],
    };
    let filename = match text(file, "filename") {
        name if name.is_empty() => text(file, "path"),
        name => name,
    };
    let label = match status {
        Status::Running => format!("{} {filename}", verbs[1]),
        Status::Done => format!("{} {filename}", verbs[2]),
        Status::Failed => format!("Could not {} {filename}", verbs[3]),
        Status::Stopped => format!("{} {filename}", verbs[0]),
    };
    let (added, removed) = (
        file["added"].as_u64().unwrap_or(0),
        file["removed"].as_u64().unwrap_or(0),
    );
    let reason = if failed {
        failure_reason(pair.result_output().unwrap_or_default())
    } else {
        String::new()
    };
    let mut entry = json!({
        "kind": "file",
        "index": index,
        "label": label,
        "status": status.name(),
        "glyph": "edit",
        "app": null,
        "appColor": null,
        "result": "",
        "reason": reason,
        "time": "",
    });
    if status == Status::Running {
        entry["waitingLabel"] = format!("{} {filename}", verbs[0]).into();
    }
    if status == Status::Done && added + removed > 0 {
        entry["added"] = added.into();
        entry["removed"] = removed.into();
    }
    entry
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

// ---------------------------------------------------------------------------
// MCP calls
// ---------------------------------------------------------------------------

/// An MCP call read as a verb on its app.
struct McpCall {
    app: Option<&'static ToolApp>,
    /// The base, running, done and failed-object forms ("Read", "Reading", "Read", "read").
    verbs: Option<[&'static str; 4]>,
    /// What it acted on: an id, a query, or the tool's noun ("SPX-412", "issues").
    object: String,
    /// The tool's own words, when no verb fits ("spaceo click").
    words: String,
    searched: bool,
}

impl McpCall {
    fn label(&self, tense: LabelTense) -> String {
        let Some(app) = self.app else {
            // An app the card does not know keeps today's wording ("Used spaceo click").
            let verb = match tense {
                LabelTense::Base => "Use",
                LabelTense::Running => "Using",
                LabelTense::Done => "Used",
                LabelTense::Failed => "Could not use",
            };
            return format!("{verb} {}", self.words);
        };
        let Some(verbs) = self.verbs else {
            let verb = match tense {
                LabelTense::Base => "Use",
                LabelTense::Running => "Using",
                LabelTense::Done => "Used",
                LabelTense::Failed => "Could not use",
            };
            return format!("{verb} {} {}", app.name, self.words);
        };
        let verb = match tense {
            LabelTense::Base => verbs[0].to_string(),
            LabelTense::Running => verbs[1].to_string(),
            LabelTense::Done => verbs[2].to_string(),
            LabelTense::Failed => format!("Could not {}", verbs[3]),
        };
        let object = if self.searched && !self.object.is_empty() {
            format!("for {}", self.object)
        } else {
            self.object.clone()
        };
        if object.is_empty() {
            format!("{verb} {}", app.name)
        } else {
            format!("{verb} {object} in {}", app.name)
        }
    }
}

/// An MCP call's line in the given tense ("Read SPX-412 in Linear", "Use spaceo click"), for a
/// tool named `mcp__<server>__<tool>`; `None` for any other name.
pub fn mcp_call_label(name: &str, input: &Value, tense: LabelTense) -> Option<String> {
    name.strip_prefix("mcp__")
        .and_then(|rest| mcp_call(rest, input))
        .map(|call| call.label(tense))
}

/// `mcp__<server>__<tool>`, past its `mcp__`.
fn mcp_call(server_tool: &str, input: &Value) -> Option<McpCall> {
    let (server, tool) = server_tool.split_once("__").unwrap_or(("", server_tool));
    let app = tool_app_for_server(server);
    // `notion-fetch`, `linear_get_issue`: the server's own name repeated before the tool goes.
    let lowered = tool.to_ascii_lowercase();
    let mut tool_words: Vec<String> = lowered
        .split(['_', '-', '.', ' '])
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(app) = app {
        let key = app.key;
        let server = server.to_ascii_lowercase();
        while tool_words.len() > 1
            && (tool_words[0] == key
                || tool_words[0] == server
                || (tool_words[0].len() >= 4 && key.starts_with(tool_words[0].as_str())))
        {
            tool_words.remove(0);
        }
    }
    let server_words = server.replace(['_', '-'], " ").trim().to_string();
    let words = if app.is_some() {
        tool_words.join(" ")
    } else {
        let tool_words = tool
            .strip_prefix(server)
            .map(|rest| rest.trim_start_matches(['_', '-']))
            .filter(|rest| !rest.is_empty())
            .unwrap_or(tool)
            .replace(['_', '-'], " ");
        js_trim(&format!("{server_words} {tool_words}")).to_string()
    };
    if words.is_empty() {
        return None;
    }
    let verbs: Option<[&'static str; 4]> = tool_words.first().and_then(|verb| {
        Some(match verb.as_str() {
            "get" | "read" | "fetch" | "view" | "retrieve" | "show" | "describe" | "load"
            | "open" => ["Read", "Reading", "Read", "read"],
            "list" => ["List", "Listing", "Listed", "list"],
            "search" | "find" | "query" | "lookup" => ["Search", "Searching", "Searched", "search"],
            "create" | "add" | "new" | "insert" => ["Create", "Creating", "Created", "create"],
            "update" | "edit" | "set" | "modify" | "patch" | "change" | "move" | "rename" => {
                ["Update", "Updating", "Updated", "update"]
            }
            "delete" | "remove" | "archive" => ["Delete", "Deleting", "Deleted", "delete"],
            "send" | "post" | "reply" | "comment" => ["Send", "Sending", "Sent", "send"],
            _ => return None,
        })
    });
    let searched = verbs.is_some_and(|verbs| verbs[0] == "Search");
    let created = verbs.is_some_and(|verbs| verbs[0] == "Create");
    let field = |keys: &[&str]| -> Option<String> {
        keys.iter().find_map(|key| match input.get(*key) {
            Some(Value::String(text)) if !js_trim(text).is_empty() => Some(text.clone()),
            Some(Value::Number(number)) => Some(number.to_string()),
            _ => None,
        })
    };
    let noun = tool_words
        .iter()
        .skip(1)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let object = if created {
        noun
    } else if searched {
        field(&["query", "q", "search", "term", "text"])
            .map(|query| format!("\u{201c}{}\u{201d}", short(&query)))
            .unwrap_or(noun)
    } else {
        field(&[
            "identifier",
            "issueId",
            "issue_id",
            "id",
            "key",
            "number",
            "pageId",
            "page_id",
            "channel",
            "repo",
            "title",
            "name",
        ])
        .map(|id| short(&id))
        .unwrap_or(noun)
    };
    Some(McpCall {
        app,
        verbs: verbs.filter(|_| app.is_some()),
        object,
        words,
        searched,
    })
}

fn short(text: &str) -> String {
    truncate_tool_preview(js_trim(&collapse_whitespace(text)), MCP_OBJECT_LENGTH)
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// The short result a finished call's row shows on its right: "92 lines", "6 matches",
/// "12 files", "214 passed", "exit 1". Empty when the output says nothing short.
pub fn tool_result(name: &str, verb: Option<&str>, output: &str, failed: bool) -> String {
    let output = script_output(output);
    // A shell call that read or searched reports what it found; one that ran something else, or
    // failed, its tests or its exit code.
    let ran = match verb {
        Some("Ran" | "Ran a script") => true,
        Some(_) => failed && is_command_tool(name),
        None => is_command_tool(name),
    };
    if ran {
        return command_result(output, failed);
    }
    if failed {
        return String::new();
    }
    if name.starts_with("mcp__") {
        return json_count(output)
            .map(|count| count_label(count, "result", "results"))
            .unwrap_or_default();
    }
    match verb {
        Some("Read") => read_lines(output)
            .map(|count| count_label(count, "line", "lines"))
            .unwrap_or_default(),
        Some("Searched for") => search_result(output, "match", "matches"),
        Some("Found files") => search_result(output, "file", "files"),
        Some("Listed") => {
            let count = meaningful_lines(output).count();
            if count == 0 {
                String::new()
            } else {
                count_label(count, "item", "items")
            }
        }
        Some("Searched the web for") => {
            let count = output.matches("\"url\":").count();
            if count == 0 {
                String::new()
            } else {
                count_label(count, "result", "results")
            }
        }
        _ => String::new(),
    }
}

/// What a Codex code-mode script printed, past the "Script completed", "Wall time 0.1 seconds"
/// and "Output:" lines it opens with.
fn script_output(output: &str) -> &str {
    if !output.starts_with("Script completed") {
        return output;
    }
    match output.find("\nOutput:") {
        Some(at) => output[at + "\nOutput:".len()..].trim_start_matches(['\r', '\n']),
        None => output,
    }
}

fn count_label(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The lines a read returned: Claude numbers them ("    12→text"), up to the reminder it may add.
fn read_lines(output: &str) -> Option<usize> {
    let lines: Vec<&str> = output
        .lines()
        .take(RESULT_SCAN_LINES)
        .take_while(|line| !line.trim_start().starts_with("<system-reminder>"))
        .collect();
    let numbered = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim_start();
            let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
            digits > 0
                && (trimmed[digits..].starts_with('\u{2192}')
                    || trimmed[digits..].starts_with('\t'))
        })
        .count();
    let count = if numbered > 0 {
        numbered
    } else {
        let mut count = lines.len();
        while count > 0 && js_trim(lines[count - 1]).is_empty() {
            count -= 1;
        }
        count
    };
    (count > 0).then_some(count)
}

fn meaningful_lines(output: &str) -> impl Iterator<Item = &str> {
    output
        .lines()
        .take(RESULT_SCAN_LINES)
        .map(js_trim)
        .filter(|line| !line.is_empty() && !line.starts_with("<system-reminder>"))
}

/// A search's count: Claude's own "Found 6 files" line, "No matches", or one per line it printed.
fn search_result(output: &str, one: &str, many: &str) -> String {
    let first = meaningful_lines(output).next().unwrap_or_default();
    let lower = first.to_ascii_lowercase();
    if lower.starts_with("no ") && (lower.contains("found") || lower.contains("match")) {
        return format!("No {many}");
    }
    if let Some(rest) = first.strip_prefix("Found ") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(count) = digits.parse::<usize>() {
            let noun = js_trim(&rest[digits.len()..]);
            let noun = noun.strip_prefix("total ").unwrap_or(noun);
            let noun = noun.split_whitespace().next().unwrap_or(many);
            return format!("{count} {noun}");
        }
    }
    match meaningful_lines(output).count() {
        0 => String::new(),
        count => count_label(count, one, many),
    }
}

/// A JSON reply's item count: a top-level array, or the first array among its fields.
fn json_count(output: &str) -> Option<usize> {
    let value: Value = serde_json::from_str(js_trim(output)).ok()?;
    match &value {
        Value::Array(items) => Some(items.len()),
        Value::Object(fields) => [
            "issues", "items", "results", "nodes", "data", "pages", "messages",
        ]
        .iter()
        .find_map(|key| fields.get(*key).and_then(Value::as_array).map(Vec::len)),
        _ => None,
    }
}

/// A command's result: the tests it ran when it says, else its exit code when it failed.
fn command_result(output: &str, failed: bool) -> String {
    if let Some((passed, failures)) = test_counts(output) {
        return match (passed, failures) {
            (passed, 0) => format!("{passed} passed"),
            (0, failures) => format!("{failures} failed"),
            (passed, failures) => format!("{failures} failed, {passed} passed"),
        };
    }
    if failed {
        if let Some(code) = exit_code(output) {
            return format!("exit {code}");
        }
    }
    String::new()
}

/// "Exit code 1" (Claude Code, and Codex as gxserver records it), "Exit code: 1", "Process exited
/// with code 1".
fn exit_code(output: &str) -> Option<i64> {
    meaningful_lines(output).take(12).find_map(|line| {
        let rest = line
            .strip_prefix("Exit code")
            .or_else(|| line.strip_prefix("Process exited with code"))?;
        let rest = js_trim(rest.trim_start_matches(':'));
        let digits: String = rest
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '-')
            .collect();
        digits.parse().ok()
    })
}

/// The tests a test run reports, summed over every summary line: cargo's "test result: ok. 214
/// passed; 0 failed", Jest's "Tests: 1 failed, 213 passed, 214 total", Vitest's "Tests  213
/// passed (213)", pytest's "== 214 passed in 1.2s ==", bun's " 214 pass" / " 0 fail", and
/// mocha's "214 passing" / "1 failing".
fn test_counts(output: &str) -> Option<(u64, u64)> {
    let mut passed = 0u64;
    let mut failed = 0u64;
    let mut found = false;
    for line in output.lines().take(RESULT_SCAN_LINES * 4) {
        let line = js_trim(line);
        let lower = line.to_ascii_lowercase();
        let summary = lower.starts_with("test result:")
            || lower.starts_with("tests:")
            || lower.starts_with("tests ")
            || (lower.starts_with('=') && (lower.contains(" passed") || lower.contains(" failed")));
        if summary {
            let (p, f) = counts_in(&lower, &["passed"], &["failed"]);
            if p.is_some() || f.is_some() {
                found = true;
                passed += p.unwrap_or(0);
                failed += f.unwrap_or(0);
            }
            continue;
        }
        // bun and mocha print each count on its own line.
        let words: Vec<&str> = lower.split_whitespace().collect();
        if let [count, word] = words.as_slice() {
            if let Ok(count) = count.parse::<u64>() {
                match *word {
                    "pass" | "passing" => {
                        passed += count;
                        found = true;
                    }
                    "fail" | "failing" => {
                        failed += count;
                        found = true;
                    }
                    _ => {}
                }
            }
        }
    }
    (found && passed + failed > 0).then_some((passed, failed))
}

/// The numbers before `passed` and `failed` words in one summary line.
fn counts_in(line: &str, pass_words: &[&str], fail_words: &[&str]) -> (Option<u64>, Option<u64>) {
    let words: Vec<&str> = line
        .split(|character: char| {
            character.is_whitespace() || matches!(character, ',' | ';' | '|' | '(' | ')')
        })
        .filter(|word| !word.is_empty())
        .collect();
    let mut passed = None;
    let mut failed = None;
    for pair in words.windows(2) {
        let Ok(count) = pair[0].parse::<u64>() else {
            continue;
        };
        let word = pair[1].trim_end_matches(['.', ':']);
        if pass_words.contains(&word) {
            passed = Some(passed.unwrap_or(0) + count);
        } else if fail_words.contains(&word) {
            failed = Some(failed.unwrap_or(0) + count);
        }
    }
    (passed, failed)
}

/// The second line of a failed row: the first line of the error that says something, preferring
/// one that names an error ("error[E0425]: cannot find value `x`") over the command's exit code
/// and the harness's wrappers.
pub fn failure_reason(output: &str) -> String {
    let lines: Vec<&str> = output
        .lines()
        .take(RESULT_SCAN_LINES)
        .map(|line| {
            js_trim(
                line.trim_start_matches("<tool_use_error>")
                    .trim_end_matches("</tool_use_error>"),
            )
        })
        .filter(|line| {
            !line.is_empty() && !line.starts_with("<system-reminder>") && !is_wrapper_line(line)
        })
        .collect();
    let lower_first = lines.first().map(|line| line.to_ascii_lowercase());
    if lower_first.as_deref().is_some_and(|line| {
        line.contains("doesn't want to proceed") || line.contains("tool use was rejected")
    }) {
        return "Declined".to_string();
    }
    let named = lines.iter().find(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("error") || lower.contains("error:") || lower.contains("error[")
    });
    let line = named.or(lines.first()).copied().unwrap_or_default();
    truncate_tool_preview(line, REASON_LENGTH)
}

/// "Exit code 1", "Wall time: 0.4 seconds", "Script completed", "Output:": a harness's wrapper
/// around what the tool said.
fn is_wrapper_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("exit code")
        || lower.starts_with("process exited with code")
        || lower.starts_with("wall time")
        || lower == "output:"
        || lower.starts_with("script completed")
        || lower.starts_with("chunk id")
        || lower.starts_with("original token count")
}

// ---------------------------------------------------------------------------
// The live card
// ---------------------------------------------------------------------------

/// Turns the cards on or off; every cached projection was built under the other setting.
pub fn set_enabled(view: &mut crate::state::TranscriptViewState, enabled: bool) {
    if view.tool_cards != enabled {
        view.tool_cards = enabled;
        view.invalidate();
    }
}

/// A cached model's card still says the right thing: a call it drew as running is stopped once
/// the session stops working, and an unanswered call it drew as stopped runs again when the
/// session works. `unanswered` is [`has_unanswered_call`] for the message the card was projected
/// from.
///
/// CDXC:SessionChat 2026-10-30 WHY: only an unanswered call's look follows the session's activity. A stopped card whose calls all answered (a cancelled background task, `is_stopped_task`, beside completed or failed ones) does not change while the agent works, and judging it by the stopped flag alone projected the row again on every pass.
pub fn card_fresh(model: &Value, unanswered: bool, working: bool) -> bool {
    let card = &model["toolCard"];
    !((card["running"] == true && !working) || (card["stopped"] == true && working && unanswered))
}

/// Whether the message has a call that never answered, which its card draws running while the
/// session works and stopped once it stops. A cancelled background task is stopped for good.
pub fn has_unanswered_call(blocks: &[ChatBlock]) -> bool {
    pair_tool_blocks(blocks).iter().any(|pair| {
        pair.call.is_some()
            && pair.result.is_none()
            && !crate::transcript::background_rows::is_stopped_task(pair)
    })
}

/// The message whose card is live while the agent works: the newest agent row, with the
/// tool-only rows after it that the fold merges into it (`fold_tool_messages`). A live card's
/// header and rows move as they change; it no longer decides whether the card is open (the
/// 2026-11-08 decision above), so a row streamed from the terminal taking its place changes no
/// card's height.
pub fn live_card_id(messages: &[ChatMessage], working: bool) -> Option<String> {
    if !working {
        return None;
    }
    let anchor = messages
        .iter()
        .rev()
        .find(|message| message.role == ChatRole::User || !is_tool_only_message(message))?;
    matches!(anchor.role, ChatRole::Assistant | ChatRole::Reasoning).then(|| anchor.id.clone())
}

/// Marks the live card's row (`toolCardLive`), in the row's own copy of its model.
pub fn mark_live_card(live: Option<&str>, message: &ChatMessage, row: &mut std::sync::Arc<Value>) {
    if live != Some(message.id.as_str()) || !row.get("toolCard").is_some_and(Value::is_object) {
        return;
    }
    if let Value::Object(fields) = std::sync::Arc::make_mut(row) {
        fields.insert("toolCardLive".to_string(), Value::Bool(true));
    }
}

/// A call waiting on an approval: its row says it waits, in what it asks to do, and the header
/// says the card waits (`approval_wait.rs`). `tool` is the waiting call's tool row and `files`
/// its file rows.
pub fn mark_card_waiting(
    fields: &mut Map<String, Value>,
    tool: Option<usize>,
    files: std::ops::Range<usize>,
) {
    let Some(Value::Object(card)) = fields.get_mut("toolCard") else {
        return;
    };
    let mut marked = false;
    if let Some(Value::Array(entries)) = card.get_mut("entries") {
        for entry in entries.iter_mut() {
            let index = entry["index"].as_u64().unwrap_or(u64::MAX) as usize;
            let hit = match entry["kind"].as_str() {
                Some("tool") => tool == Some(index),
                Some("file") => files.contains(&index),
                _ => false,
            };
            // The session reads as idle while it waits on an approval, so the waiting call can
            // be drawn stopped; it waits all the same.
            if !hit || !(entry["status"] == "running" || entry["status"] == "stopped") {
                continue;
            }
            if let Some(waiting) = entry.get("waitingLabel").cloned() {
                entry["label"] = waiting;
            }
            entry["status"] = "waiting".into();
            entry["result"] = "Waiting for approval".into();
            marked = true;
        }
    }
    if marked {
        card.insert("title".to_string(), "Waiting for approval".into());
        // The header says it waits, and the card opens to the call that waits (`waiting`).
        card.insert("liveLabel".to_string(), "".into());
        card.insert("liveMore".to_string(), "".into());
        card.insert("waiting".to_string(), Value::Bool(true));
        fields.insert("toolCardLive".to_string(), Value::Bool(true));
    }
}

#[cfg(test)]
#[path = "tool_card_tests.rs"]
mod tests;
