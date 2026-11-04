use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage};
use serde_json::{json, Value};

use super::*;
use crate::transcript::file_changes::split_file_changes;
use crate::transcript::tool_fold::{pair_tool_blocks, split_blocks};
use crate::transcript::transcript_rows::{file_rows, tool_rows};
use crate::transcript::work_summary::tool_time_label;

const WD: &str = "/repo";

fn call(id: &str, name: &str, input: Value) -> ChatBlock {
    ChatBlock::ToolCall {
        name: name.to_string(),
        input,
        call_id: Some(id.to_string()),
    }
}

fn result(id: &str, output: &str, is_error: bool) -> ChatBlock {
    ChatBlock::ToolResult {
        output: output.to_string(),
        is_error: is_error.then_some(true),
        call_id: Some(id.to_string()),
        patch: None,
    }
}

/// The card the projection builds for these blocks.
fn card_with(blocks: &[ChatBlock], spans: &HashMap<String, (i64, i64)>, working: bool) -> Value {
    let (_, tools) = split_blocks(blocks);
    let (remaining, changes) = split_file_changes(tools.iter().copied());
    let pairs = pair_tool_blocks(remaining);
    let rows = tool_rows(&pairs, "", Some(WD), &HashMap::new());
    let files = file_rows(&changes, "m1", Some(WD));
    tool_card(&tools, &rows, &files, spans, Some(WD), working).expect("a card")
}

fn card(blocks: &[ChatBlock]) -> Value {
    card_with(blocks, &HashMap::new(), true)
}

fn entry(blocks: &[ChatBlock]) -> Value {
    card(blocks)["entries"][0].clone()
}

#[test]
fn a_read_changes_tense_and_counts_its_lines() {
    let read = call(
        "c1",
        "Read",
        json!({"file_path": "/repo/src/notice_choices.rs"}),
    );
    let running = entry(&[read.clone()]);
    assert_eq!(running["label"], "Reading src/notice_choices.rs");
    assert_eq!(running["status"], "running");
    assert_eq!(running["waitingLabel"], "Read src/notice_choices.rs");
    let output = "     1\u{2192}use a;\n     2\u{2192}use b;\n     3\u{2192}\n<system-reminder>\nx\n</system-reminder>";
    let done = entry(&[read, result("c1", output, false)]);
    assert_eq!(done["label"], "Read src/notice_choices.rs");
    assert_eq!(done["status"], "done");
    assert_eq!(done["result"], "3 lines");
    assert_eq!(done["glyph"], "file");
}

#[test]
fn a_described_command_keeps_its_description_and_reports_its_tests() {
    let run = call(
        "c1",
        "Bash",
        json!({"command": "cargo test -p gx-chat-core", "description": "Run the chat core tests"}),
    );
    assert_eq!(entry(&[run.clone()])["label"], "Run the chat core tests");
    let output = "running 200 tests\ntest result: ok. 200 passed; 0 failed; 0 ignored\n\nrunning 14 tests\ntest result: ok. 14 passed; 0 failed";
    let done = entry(&[run, result("c1", output, false)]);
    assert_eq!(done["label"], "Run the chat core tests");
    assert_eq!(done["result"], "214 passed");
}

#[test]
fn an_undescribed_command_reads_ran_and_a_failure_says_why() {
    let run = call("c1", "Bash", json!({"command": "cargo check"}));
    assert_eq!(entry(&[run.clone()])["label"], "Running cargo check");
    let output = "Exit code 101\n    Checking gx v0.1.0\nerror[E0425]: cannot find value `x` in this scope\n  --> src/lib.rs:3:5";
    let failed = entry(&[run, result("c1", output, true)]);
    // A command that exited non-zero did run.
    assert_eq!(failed["label"], "Ran cargo check");
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["result"], "exit 101");
    assert_eq!(
        failed["reason"],
        "error[E0425]: cannot find value `x` in this scope"
    );
}

#[test]
fn jest_pytest_and_bun_summaries_count() {
    let jest = "Tests:       1 failed, 213 passed, 214 total";
    assert_eq!(
        tool_result("Bash", Some("Ran"), jest, true),
        "1 failed, 213 passed"
    );
    let pytest = "============ 12 passed in 0.31s ============";
    assert_eq!(tool_result("Bash", Some("Ran"), pytest, false), "12 passed");
    let bun = " 41 pass\n 0 fail\n 120 expect() calls";
    assert_eq!(tool_result("Bash", Some("Ran"), bun, false), "41 passed");
    assert_eq!(tool_result("Bash", Some("Ran"), "ok", false), "");
    assert_eq!(
        tool_result(
            "exec_command",
            Some("Ran"),
            "Process exited with code 2\nboom",
            true
        ),
        "exit 2"
    );
}

#[test]
fn searches_and_globs_count_what_they_found() {
    let grep = call(
        "c1",
        "Grep",
        json!({"pattern": "fold_tool", "path": "/repo/src"}),
    );
    let found = entry(&[
        grep.clone(),
        result("c1", "Found 6 files\nsrc/a.rs\nsrc/b.rs", false),
    ]);
    assert_eq!(found["label"], "Searched for fold_tool in src");
    assert_eq!(found["result"], "6 files");
    let lines = entry(&[
        grep.clone(),
        result("c1", "src/a.rs:1:x\nsrc/b.rs:9:y", false),
    ]);
    assert_eq!(lines["result"], "2 matches");
    assert_eq!(
        entry(&[grep.clone()])["label"],
        "Searching for fold_tool in src"
    );
    let none = entry(&[grep, result("c1", "No matches found", false)]);
    assert_eq!(none["result"], "No matches");
    let glob = call("c2", "Glob", json!({"pattern": "**/*.rs"}));
    let files = entry(&[
        glob,
        result("c2", "/repo/a.rs\n/repo/b.rs\n/repo/c.rs", false),
    ]);
    assert_eq!(files["label"], "Found files matching **/*.rs");
    assert_eq!(files["result"], "3 files");
}

#[test]
fn a_failed_fetch_says_what_failed() {
    let fetch = call(
        "c1",
        "WebFetch",
        json!({"url": "https://docs.rs/serde_json", "prompt": "x"}),
    );
    assert_eq!(
        entry(&[fetch.clone()])["label"],
        "Fetching docs.rs/serde_json"
    );
    let failed = entry(&[fetch, result("c1", "Request timed out after 30s", true)]);
    assert_eq!(failed["label"], "Could not fetch docs.rs/serde_json");
    assert_eq!(failed["reason"], "Request timed out after 30s");
    assert_eq!(failed["result"], "");
}

#[test]
fn a_declined_call_says_declined() {
    let output = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing";
    assert_eq!(failure_reason(output), "Declined");
    assert_eq!(
        failure_reason("<tool_use_error>File does not exist.</tool_use_error>"),
        "File does not exist."
    );
}

#[test]
fn an_edit_is_a_row_with_its_counts() {
    let edit = call(
        "c1",
        "Edit",
        json!({"file_path": "/repo/src/document.rs", "old_string": "a\nb", "new_string": "a\nc\nd"}),
    );
    let running = entry(&[edit.clone()]);
    assert_eq!(running["kind"], "file");
    assert_eq!(running["label"], "Editing document.rs");
    assert_eq!(running["glyph"], "edit");
    let done = entry(&[edit.clone(), result("c1", "ok", false)]);
    assert_eq!(done["label"], "Edited document.rs");
    // The same counts the file change card shows (`file_change_counts`).
    assert_eq!(done["added"], 3);
    assert_eq!(done["removed"], 2);
    let write = call(
        "c2",
        "Write",
        json!({"file_path": "/repo/new.rs", "content": "x\n"}),
    );
    assert_eq!(entry(&[write.clone()])["label"], "Creating new.rs");
    assert_eq!(
        entry(&[write, result("c2", "ok", false)])["label"],
        "Created new.rs"
    );
    let failed = entry(&[
        edit,
        result(
            "c1",
            "<tool_use_error>String to replace not found in file.</tool_use_error>",
            true,
        ),
    ]);
    assert_eq!(failed["label"], "Could not edit document.rs");
    assert_eq!(failed["reason"], "String to replace not found in file.");
}

#[test]
fn rows_keep_the_order_the_calls_were_made() {
    let blocks = [
        call("c1", "Read", json!({"file_path": "/repo/a.rs"})),
        result("c1", "x", false),
        call(
            "c2",
            "Edit",
            json!({"file_path": "/repo/a.rs", "old_string": "x", "new_string": "y"}),
        ),
        result("c2", "ok", false),
        call("c3", "Bash", json!({"command": "cargo test"})),
    ];
    let card = card(&blocks);
    let kinds: Vec<(String, u64)> = card["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["kind"].as_str().unwrap().to_string(),
                entry["index"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        vec![("tool".into(), 0), ("file".into(), 0), ("tool".into(), 1)]
    );
}

#[test]
fn mcp_calls_read_as_a_verb_on_their_app() {
    let linear = call(
        "c1",
        "mcp__claude_ai_Linear__get_issue",
        json!({"id": "SPX-412"}),
    );
    let running = entry(&[linear.clone()]);
    assert_eq!(running["label"], "Reading SPX-412 in Linear");
    assert_eq!(running["app"], "linear");
    assert_eq!(running["appColor"], "#5E6AD2");
    assert_eq!(
        entry(&[linear.clone(), result("c1", "{}", false)])["label"],
        "Read SPX-412 in Linear"
    );
    assert_eq!(
        entry(&[linear, result("c1", "Not found", true)])["label"],
        "Could not read SPX-412 in Linear"
    );
    let search = call(
        "c2",
        "mcp__github__search_issues",
        json!({"query": "fold bug"}),
    );
    let searched = entry(&[search, result("c2", "[{\"n\":1},{\"n\":2}]", false)]);
    assert_eq!(
        searched["label"],
        "Searched for \u{201c}fold bug\u{201d} in GitHub"
    );
    assert_eq!(searched["result"], "2 results");
    let notion = call(
        "c3",
        "mcp__claude_ai_Notion__notion-fetch",
        json!({"id": "abc123"}),
    );
    assert_eq!(entry(&[notion])["label"], "Reading abc123 in Notion");
    let list = call("c4", "mcp__linear__list_issues", json!({"team": "SPX"}));
    assert_eq!(entry(&[list])["label"], "Listing issues in Linear");
    // An app the card does not know keeps today's wording and the generic icon.
    let unknown = call("c5", "mcp__spaceo__spaceo_click", json!({"index": 3}));
    let unknown = entry(&[unknown, result("c5", "ok", false)]);
    assert_eq!(unknown["label"], "Used spaceo click");
    assert_eq!(unknown["app"], Value::Null);
}

#[test]
fn the_header_counts_fails_and_times_the_burst() {
    let mut blocks = Vec::new();
    let mut spans = HashMap::new();
    for index in 0..6 {
        let id = format!("c{index}");
        blocks.push(call(
            &id,
            "Read",
            json!({"file_path": format!("/repo/{index}.rs")}),
        ));
        blocks.push(result(&id, "x", index == 3));
        spans.insert(id, (1_000 + index * 10_000, 1_000 + index * 10_000 + 4_000));
    }
    let done = card_with(&blocks, &spans, true);
    assert_eq!(done["count"], 6);
    assert_eq!(done["running"], false);
    assert_eq!(done["title"], "Used 6 tools");
    assert_eq!(done["failed"], 1);
    assert_eq!(done["failedLabel"], "1 failed");
    assert_eq!(done["time"], "54.0s");
    assert_eq!(
        done["foldedLabel"],
        "Used 6 tools \u{b7} 1 failed \u{b7} 54.0s"
    );
    assert_eq!(done["icons"].as_array().unwrap().len(), MAX_STACKED_ICONS);
    assert_eq!(done["moreIcons"], 2);
    assert_eq!(done["entries"][0]["time"], "4.0s");

    blocks.push(call("c9", "Bash", json!({"command": "ls"})));
    let running = card_with(&blocks, &spans, true);
    assert_eq!(running["title"], "Using 7 tools");
    assert_eq!(running["time"], "");
    assert_eq!(
        card_with(&blocks[..2], &HashMap::new(), true)["title"],
        "Used 1 tool"
    );
}

#[test]
fn an_unanswered_call_stops_when_the_turn_ends() {
    let blocks = [call("c1", "Bash", json!({"command": "sleep 100"}))];
    let stopped = card_with(&blocks, &HashMap::new(), false);
    assert_eq!(stopped["entries"][0]["status"], "stopped");
    assert_eq!(stopped["entries"][0]["label"], "Run sleep 100");
    assert_eq!(stopped["title"], "Used 1 tool");
    let model = json!({"toolCard": stopped});
    assert!(card_fresh(&model, false));
    assert!(!card_fresh(&model, true));
    let running = json!({"toolCard": card_with(&blocks, &HashMap::new(), true)});
    assert!(card_fresh(&running, true));
    assert!(!card_fresh(&running, false));
}

#[test]
fn questions_and_sent_messages_stay_out_of_the_card() {
    let blocks = [
        call(
            "c1",
            "AskUserQuestion",
            json!({"questions": [{"question": "Which?", "options": [{"label": "A"}]}]}),
        ),
        result(
            "c1",
            "User has answered your questions: \"Which?\"=\"A\".",
            false,
        ),
    ];
    let (_, tools) = split_blocks(&blocks);
    let pairs = pair_tool_blocks(tools.iter().copied());
    let rows = tool_rows(&pairs, "", Some(WD), &HashMap::new());
    assert_eq!(rows[0]["exchange"], true);
    assert!(tool_card(&tools, &rows, &[], &HashMap::new(), Some(WD), true).is_none());
}

#[test]
fn a_waiting_call_says_it_waits() {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "toolCard".to_string(),
        card(&[call(
            "c1",
            "Bash",
            json!({"command": "git push origin main"}),
        )]),
    );
    mark_card_waiting(&mut fields, Some(0), 0..0);
    let card = &fields["toolCard"];
    assert_eq!(card["title"], "Waiting for approval");
    assert_eq!(card["entries"][0]["status"], "waiting");
    assert_eq!(card["entries"][0]["label"], "Run git push origin main");
    assert_eq!(card["entries"][0]["result"], "Waiting for approval");
    assert_eq!(fields["toolCardLive"], true);
}

fn message(id: &str, role: &str, blocks: Value) -> ChatMessage {
    serde_json::from_value(json!({
        "id": id, "role": role, "blocks": blocks, "timestamp": 1, "source": "transcript"
    }))
    .unwrap()
}

#[test]
fn the_newest_agent_row_holds_the_live_card() {
    let text = json!([{"type": "text", "text": "Looking"}]);
    let tool = json!([{"type": "tool-call", "name": "Read", "input": {}, "callId": "c1"}]);
    let messages = vec![
        message("u1", "user", json!([{"type": "text", "text": "go"}])),
        message("a1", "assistant", text.clone()),
        message("t1", "assistant", tool.clone()),
    ];
    assert_eq!(live_card_id(&messages, true).as_deref(), Some("a1"));
    assert_eq!(live_card_id(&messages, false), None);
    let mut later = messages.clone();
    later.push(message("a2", "assistant", text));
    assert_eq!(live_card_id(&later, true).as_deref(), Some("a2"));
    let mut prompt = messages;
    prompt.push(message(
        "u2",
        "user",
        json!([{"type": "text", "text": "more"}]),
    ));
    assert_eq!(live_card_id(&prompt, true), None);
}

#[test]
fn times_are_short() {
    assert_eq!(tool_time_label(30), "30ms");
    assert_eq!(tool_time_label(1_234), "1.2s");
    assert_eq!(tool_time_label(41_200), "41s");
    assert_eq!(tool_time_label(64_000), "1m 4s");
}

#[test]
fn a_codex_exec_that_only_patched_is_its_file_row() {
    let script = json!("text(await tools.apply_patch(\"*** Begin Patch\\n*** Update File: /repo/src/math.js\\n@@\\n-  return 1;\\n+  return 0;\\n }\\n*** End Patch\"));\n");
    let card = card(&[call("c1", "exec", script), result("c1", "Success", false)]);
    let entries = card["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["kind"], "file");
    assert_eq!(entries[0]["label"], "Edited math.js");
}

#[test]
fn a_codex_exec_command_reads_ran_and_its_exit_code() {
    let script = json!("const r=await tools.exec_command({cmd:\"cargo build --features nope\",workdir:\"/repo\"}); text(r);");
    let running = entry(&[call("c1", "exec", script.clone())]);
    assert_eq!(running["label"], "Running cargo build --features nope");
    let failed = entry(&[
        call("c1", "exec", script),
        result(
            "c1",
            "Exit code 101\nerror: none of the selected packages contains these features: nope",
            true,
        ),
    ]);
    assert_eq!(failed["label"], "Ran cargo build --features nope");
    assert_eq!(failed["result"], "exit 101");
    assert_eq!(
        failed["reason"],
        "error: none of the selected packages contains these features: nope"
    );
}

#[test]
fn codex_script_output_is_read_past_its_header() {
    let read = json!("const r=await tools.exec_command({cmd:\"sed -n 1,40p src/lib.rs\",workdir:\"/repo\"}); text(r.output);");
    let done = entry(&[
        call("c1", "exec", read),
        result(
            "c1",
            "Script completed\nWall time 0.1 seconds\nOutput:\nfn a() {}\nfn b() {}\n",
            false,
        ),
    ]);
    assert_eq!(done["label"], "Read src/lib.rs");
    assert_eq!(done["result"], "2 lines");
    let search = json!("const r=await tools.exec_command({cmd:\"rg -n folded src\",workdir:\"/repo\"}); text(r.output);");
    let found = entry(&[
        call("c2", "exec", search),
        result(
            "c2",
            "Script completed\nWall time 0.4 seconds\nOutput:\nsrc/lib.rs:7:x\nsrc/lib.rs:8:y",
            false,
        ),
    ]);
    assert_eq!(found["result"], "2 matches");
}

#[test]
fn the_phone_asks_the_core_for_an_app_logo() {
    use crate::query::{answer_query, Query};
    let state = crate::state::ChatState::default();
    let context = crate::state::ChatContext::default();
    let query = Query::from_wire("toolAppLogo").expect("a query");
    assert_eq!(query.as_str(), "toolAppLogo");
    let svg: Option<String> =
        serde_json::from_str(&answer_query(&state, &context, query, &[json!("linear")]).unwrap())
            .unwrap();
    assert!(svg.unwrap().starts_with("<svg"));
    let unknown: Option<String> =
        serde_json::from_str(&answer_query(&state, &context, query, &[json!("spaceo")]).unwrap())
            .unwrap();
    assert!(unknown.is_none());
}
