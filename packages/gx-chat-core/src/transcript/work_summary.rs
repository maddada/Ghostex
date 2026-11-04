//! What a finished turn's work fold says while it is closed, and what each tool row reports on its
//! right: the counts beside "Worked for 2m 5s", every call's duration, and a failed call's reason.
//!
//! CDXC:SessionChat 2026-10-18 DECISION: User picked the "proposed" work and reply look (docs/2026-10-10/chat-cards-questions/06-work.html): "Worked for 2m 5s" carries its counts beside it (tool calls, files; the failure chip was dropped 2026-10-12, see `work_stats`) so the closed fold still tells the story; tool rows get a check or cross and their duration on the right, and a failed call is tinted and says what failed. The same day the user took back the design's labelled reply toolbar (Copy · Annotate · Save): a reply's actions stay the icon-only buttons shown on hover. The GPUI chat (`completed_work_row.rs`, `tool_run.rs`) and the phone (`TranscriptItemRow.tsx`, `ToolRows.tsx`) draw what this file decides. User (2026-10-18): "Say Worked for 12s not Worked 12s. this is in all places we mention how long we worked please", superseding the design's "Worked 2m 5s" wording (`turns::worked_duration_label`).

use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage};
use serde_json::{json, Value};

use crate::transcript::background_rows::background_task_source;
use crate::transcript::jsstr::js_trim;
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks};
use crate::transcript::tool_rows::is_command_tool;
use crate::transcript::tool_summary::truncate_tool_preview;

/// How long a failed call's reason may run beside its preview.
const FAILURE_NOTE_LENGTH: usize = 60;

/// How far past a message the scan for its calls' results looks before it gives up.
const RESULT_SCAN_LIMIT: usize = 400;

/// "0.1s", "14.3s", "2m 5s", "1h 4m": how long one call ran.
pub fn tool_duration_label(ms: i64) -> String {
    if ms < 60_000 {
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    let seconds = (ms as f64 / 1000.0).round() as i64;
    if seconds < 3600 {
        let (minutes, rest) = (seconds / 60, seconds % 60);
        return if rest > 0 {
            format!("{minutes}m {rest}s")
        } else {
            format!("{minutes}m")
        };
    }
    let (hours, minutes) = (seconds / 3600, (seconds % 3600) / 60);
    if minutes > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{hours}h")
    }
}

/// How long each of `message`'s calls ran, by call id: from the row that recorded the call to the
/// row that recorded its result.
///
/// The transcript folds a run of tool-only rows into the message before them, so the merged
/// message's blocks no longer say when each was written; the times come from the rows as they
/// arrived, in `timeline` (the session's messages, or a deferred turn's rows). A call whose agent
/// records no call id, or whose result is not in yet, has no duration.
pub fn tool_durations<'a>(
    message: &ChatMessage,
    timelines: impl IntoIterator<Item = &'a [ChatMessage]>,
) -> HashMap<String, i64> {
    tool_spans(message, timelines)
        .into_iter()
        .filter_map(|(id, (called, answered))| {
            let ms = answered - called;
            // Two rows stamped with the same instant say nothing about how long the call ran.
            (ms > 0).then_some((id, ms))
        })
        .collect()
}

/// When each of `message`'s answered calls started and ended, by call id, in epoch milliseconds:
/// [`tool_durations`]' reading, kept as the two ends so a tool-call card can time its whole burst
/// (`tool_card.rs`).
pub fn tool_spans<'a>(
    message: &ChatMessage,
    timelines: impl IntoIterator<Item = &'a [ChatMessage]>,
) -> HashMap<String, (i64, i64)> {
    let mut wanted: HashMap<&str, (Option<i64>, Option<i64>)> = HashMap::new();
    for block in &message.blocks {
        if let ChatBlock::ToolCall {
            call_id: Some(id), ..
        } = block
        {
            wanted.entry(id.as_str()).or_default();
        }
    }
    if wanted.is_empty() {
        return HashMap::new();
    }
    for timeline in timelines {
        let Some(start) = timeline.iter().position(|row| row.id == message.id) else {
            continue;
        };
        let mut open = wanted.len();
        for row in timeline[start..].iter().take(RESULT_SCAN_LIMIT) {
            for block in &row.blocks {
                match block {
                    ChatBlock::ToolCall {
                        call_id: Some(id), ..
                    } => {
                        if let Some(times) = wanted.get_mut(id.as_str()) {
                            times.0 = times.0.or(row.timestamp);
                        }
                    }
                    ChatBlock::ToolResult {
                        call_id: Some(id), ..
                    } => {
                        if let Some(times) = wanted.get_mut(id.as_str()) {
                            if times.1.is_none() && row.timestamp.is_some() {
                                times.1 = row.timestamp;
                                open -= 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if open == 0 {
                break;
            }
        }
        break;
    }
    wanted
        .into_iter()
        .filter_map(|(id, (called, answered))| {
            let called = called.or(message.timestamp)?;
            Some((id.to_string(), (called, answered?)))
        })
        .collect()
}

/// "30ms", "1.2s", "41s", "1m 4s": a call's time on a tool-call card (`tool_card.rs`), shorter than
/// [`tool_duration_label`] because it sits beside the call's result.
pub fn tool_time_label(ms: i64) -> String {
    if ms < 1000 {
        return format!("{}ms", ms.max(1));
    }
    if ms < 10_000 {
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    if ms < 60_000 {
        return format!("{}s", (ms as f64 / 1000.0).round() as i64);
    }
    tool_duration_label(ms)
}

/// What a failed call's row says after its preview: the first line of what the tool reported,
/// without the harness's own error wrapper.
///
/// CDXC:SessionChat 2026-10-26 DECISION: User, about a failed Bash row that read "Exit code 1" after its red title: "please don't show 'Exit code 1' here for this kind of element in our chat transcript". Claude Code reports a failed command as "Exit code <n>" followed by the output; the row's red title and cross already say it failed, so a command call says nothing after its title (the next line of output does not take its place) and the exit code stays in the expanded row's output. Other tools' reasons are unchanged. A background command's own "exit code" reason (`background_rows.rs`) is a different row and keeps it.
pub fn failure_note(tool_name: Option<&str>, output: &str) -> String {
    let line = output
        .lines()
        .map(|line| {
            js_trim(
                line.trim_start_matches("<tool_use_error>")
                    .trim_end_matches("</tool_use_error>"),
            )
        })
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if tool_name.is_some_and(is_command_tool) && is_exit_code_line(line) {
        return String::new();
    }
    truncate_tool_preview(line, FAILURE_NOTE_LENGTH)
}

/// "Exit code 1": the whole line Claude Code's Bash tool opens a failed result with.
fn is_exit_code_line(line: &str) -> bool {
    line.strip_prefix("Exit code ").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// The counts beside a finished turn's "Worked for 2m 5s": its tool calls and the files it changed.
///
/// CDXC:SessionChat 2026-10-12 DECISION: User: "no need to show "Failed" here" (the "1 failed" chip beside "Worked 5m 19s"). The fold counts only tool calls and files; a failed call still shows its cross and reason on its own row once the fold is opened. Supersedes the 2026-10-11 design's failure chip ("a failure that was fixed").
///
/// Each entry is `{kind, label}`; `kind` is `tools` or `files`, which the renderers turn into the
/// chip's glyph. A turn whose rows are still on the server (`deferredWork`) knows only its files
/// until it is opened.
pub fn work_stats(rows: &[&ChatMessage], changed_files: usize) -> Vec<Value> {
    let calls: usize = rows
        .iter()
        .map(|message| {
            // A background task's end counts as the call it reports (`background_rows.rs`).
            let message = background_task_source(message);
            let (_, tools) = split_blocks(&message.blocks);
            pair_tool_blocks(tools)
                .iter()
                .filter(|pair| pair.call_name().is_some() && pair.call_input().is_some())
                .count()
        })
        .sum();
    let mut stats = Vec::new();
    if calls > 0 {
        stats.push(json!({
            "kind": "tools",
            "label": format!("{calls} tool {}", if calls == 1 { "call" } else { "calls" }),
        }));
    }
    if changed_files > 0 {
        stats.push(json!({
            "kind": "files",
            "label": format!("{changed_files} {}", if changed_files == 1 { "file" } else { "files" }),
        }));
    }
    stats
}

#[cfg(test)]
mod failure_note_tests {
    use super::failure_note;

    #[test]
    fn a_failed_bash_result_shows_no_exit_code_line() {
        assert_eq!(
            failure_note(Some("Bash"), "Exit code 1\nboom: not found"),
            ""
        );
        assert_eq!(failure_note(Some("Bash"), "Exit code 127"), "");
    }

    #[test]
    fn a_bash_failure_that_says_something_else_keeps_it() {
        assert_eq!(
            failure_note(Some("Bash"), "command not found: foo"),
            "command not found: foo"
        );
    }

    #[test]
    fn a_non_command_failure_keeps_its_summary() {
        assert_eq!(
            failure_note(Some("Read"), "Exit code 1\nboom"),
            "Exit code 1"
        );
        assert_eq!(
            failure_note(
                Some("Read"),
                "<tool_use_error>File does not exist.</tool_use_error>"
            ),
            "File does not exist."
        );
    }

    #[test]
    fn a_background_commands_reason_is_unchanged() {
        assert_eq!(
            failure_note(Some("Background command"), "exit code 1\nx"),
            "exit code 1"
        );
    }
}

#[cfg(test)]
mod failure_row_tests {
    use std::collections::HashMap;

    use ghostex_gx_protocol::ChatBlock;
    use serde_json::json;

    use crate::transcript::tool_fold::pair_tool_blocks;
    use crate::transcript::transcript_rows::tool_rows;

    fn row(name: &str, output: &str, is_error: bool) -> serde_json::Value {
        let blocks = [
            ChatBlock::ToolCall {
                name: name.to_string(),
                input: json!({ "command": "gh issue list", "description": "List issues" }),
                call_id: Some("c1".to_string()),
            },
            ChatBlock::ToolResult {
                output: output.to_string(),
                is_error: is_error.then_some(true),
                call_id: Some("c1".to_string()),
                patch: None,
            },
        ];
        let pairs = pair_tool_blocks(blocks.iter());
        tool_rows(&pairs, "", None, &HashMap::new()).remove(0)
    }

    #[test]
    fn rows_show_no_exit_code_for_a_failed_command_and_keep_other_summaries() {
        let failed_bash = row("Bash", "Exit code 1\nboom", true);
        assert_eq!(failed_bash["failure"], "");
        assert_eq!(failed_bash["status"], "failed");
        assert!(failed_bash["hasDetail"].as_bool().unwrap());
        assert_eq!(
            row("Read", "File does not exist.", true)["failure"],
            "File does not exist."
        );
        let done = row("Bash", "Exit code 1\nnot an error result", false);
        assert_eq!(done["failure"], "");
        assert_eq!(done["status"], "done");
    }
}
