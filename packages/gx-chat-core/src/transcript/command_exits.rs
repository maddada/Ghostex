//! A command that failed inside a Codex code-mode `exec` script, marked on the script's result.
//!
//! CDXC:SessionChat 2026-10-28 WHY: A script that prints only a command's output (`text(r.output)`)
//! records no exit code, so a failed `cargo build` showed a green check. gxserver decodes the
//! command's own `CommandExecution` item, which names its exit code and command but no call id,
//! into a hidden row ("Command exited with code 101: cargo build --features nope", wrapped in
//! `<system-reminder>` so a core without this pass drops it). This pass gives that exit to the
//! open call whose script ran the command: the result is marked failed and leads with
//! "Exit code 101", the way gxserver already writes a failed command whose script printed its
//! record. The hidden row is then dropped with the other harness rows.
//! SEE-ALSO: server/src/session_chat_decode_codex.rs (`CODEX_COMMAND_EXIT_PREFIX`).

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::Value;

/// The start of gxserver's row; the exit code, ": ", the command and `</system-reminder>` follow.
pub const COMMAND_EXIT_PREFIX: &str = "<system-reminder>Command exited with code ";
const COMMAND_EXIT_SUFFIX: &str = "</system-reminder>";

/// The exit code and command a gxserver exit row names.
fn command_exit(message: &ChatMessage) -> Option<(i64, &str)> {
    if message.role != ChatRole::System {
        return None;
    }
    let [ChatBlock::Text { text }] = message.blocks.as_slice() else {
        return None;
    };
    let rest = text
        .strip_prefix(COMMAND_EXIT_PREFIX)?
        .strip_suffix(COMMAND_EXIT_SUFFIX)?;
    let (code, command) = rest.split_once(": ")?;
    Some((code.parse().ok()?, command))
}

/// Whether a call's arguments ran `command`: its text, or its text as a quoted string inside a
/// script (`cmd:"cargo build"`), appears in them.
fn ran(input: &Value, command: &str) -> bool {
    let text = match input {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.contains(command) {
        return true;
    }
    let quoted = serde_json::to_string(command).unwrap_or_default();
    quoted.len() > 2 && text.contains(&quoted[1..quoted.len() - 1])
}

/// Whether a result already leads with its exit code ("Exit code 1", "Exit code: 1").
fn leads_with_exit_code(output: &str) -> bool {
    output.trim_start().starts_with("Exit code")
}

/// The messages with every failed command's exit given to the result of the call that ran it.
pub fn link_command_exits(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    if !messages
        .iter()
        .any(|message| command_exit(message).is_some())
    {
        return messages;
    }
    let mut messages = messages;
    // Calls still waiting for their result, oldest first, and the exits found for them.
    let mut open: Vec<(String, Value)> = Vec::new();
    let mut exits: Vec<(String, i64)> = Vec::new();
    for message in &mut messages {
        if let Some((code, command)) = command_exit(message) {
            // The newest open call that ran it; parallel scripts each name their own command.
            if let Some((id, _)) = open.iter().rev().find(|(_, input)| ran(input, command)) {
                if !exits.iter().any(|(known, _)| known == id) {
                    exits.push((id.clone(), code));
                }
            }
            continue;
        }
        for block in &mut message.blocks {
            match block {
                ChatBlock::ToolCall {
                    call_id: Some(id),
                    input,
                    ..
                } => open.push((id.clone(), input.clone())),
                ChatBlock::ToolResult {
                    call_id: Some(id),
                    output,
                    is_error,
                    ..
                } => {
                    open.retain(|(open_id, _)| open_id != id);
                    if let Some(at) = exits.iter().position(|(known, _)| known == id) {
                        let (_, code) = exits.remove(at);
                        *is_error = Some(true);
                        if !leads_with_exit_code(output) {
                            *output = format!("Exit code {code}\n{output}");
                        }
                    }
                }
                _ => {}
            }
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(id: &str, role: &str, blocks: Value) -> ChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": role, "blocks": blocks, "timestamp": 1, "source": "transcript"
        }))
        .unwrap()
    }

    fn exit_row(id: &str, code: i64, command: &str) -> ChatMessage {
        message(
            id,
            "system",
            json!([{ "type": "text", "text": format!("{COMMAND_EXIT_PREFIX}{code}: {command}{COMMAND_EXIT_SUFFIX}") }]),
        )
    }

    fn call(id: &str, script: &str) -> ChatMessage {
        message(
            id,
            "assistant",
            json!([{ "type": "tool-call", "name": "exec", "input": script, "callId": id }]),
        )
    }

    fn result(id: &str, output: &str) -> ChatMessage {
        message(
            &format!("r-{id}"),
            "tool",
            json!([{ "type": "tool-result", "output": output, "callId": id }]),
        )
    }

    fn result_of(messages: &[ChatMessage], id: &str) -> (String, Option<bool>) {
        messages
            .iter()
            .flat_map(|message| &message.blocks)
            .find_map(|block| match block {
                ChatBlock::ToolResult {
                    call_id: Some(call_id),
                    output,
                    is_error,
                    ..
                } if call_id == id => Some((output.clone(), *is_error)),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_failed_command_marks_the_script_that_ran_it() {
        let linked = link_command_exits(vec![
            call("c1", "const r=await tools.exec_command({cmd:\"cargo build --features nope\"}); text(r.output);"),
            exit_row("e1", 101, "cargo build --features nope"),
            result("c1", "Script completed\nOutput:\nerror: no feature nope"),
        ]);
        let (output, failed) = result_of(&linked, "c1");
        assert_eq!(failed, Some(true));
        assert_eq!(
            output,
            "Exit code 101\nScript completed\nOutput:\nerror: no feature nope"
        );
    }

    #[test]
    fn parallel_scripts_each_take_their_own_command() {
        let linked = link_command_exits(vec![
            call("c1", "tools.exec_command({cmd:\"cargo test\"})"),
            call("c2", "tools.exec_command({cmd:\"rg -n \\\"x y\\\" src\"})"),
            exit_row("e1", 1, "rg -n \"x y\" src"),
            result("c1", "ok"),
            result("c2", ""),
        ]);
        assert_eq!(result_of(&linked, "c1").1, None);
        assert_eq!(
            result_of(&linked, "c2"),
            ("Exit code 1\n".to_string(), Some(true))
        );
    }

    #[test]
    fn a_result_that_already_says_its_exit_code_keeps_its_text() {
        let linked = link_command_exits(vec![
            call("c1", "tools.exec_command({cmd:\"false\"})"),
            exit_row("e1", 1, "false"),
            result("c1", "Exit code 1\n"),
        ]);
        assert_eq!(
            result_of(&linked, "c1"),
            ("Exit code 1\n".to_string(), Some(true))
        );
    }

    #[test]
    fn the_exit_row_is_hidden_and_an_unmatched_one_changes_nothing() {
        let row = exit_row("e1", 2, "make");
        assert!(crate::transcript::noise::is_hidden_message(&row));
        let rows = vec![
            call("c1", "tools.exec_command({cmd:\"ls\"})"),
            row,
            result("c1", "a\nb"),
        ];
        let linked = link_command_exits(rows.clone());
        assert_eq!(linked, rows);
    }
}
