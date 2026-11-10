//! The two rows one slash command renders as, and the escaped-markup contract around them.
//!
//! Ported from `packages/core-ui/chat/session-chat-local-command-transcript.ts`.

use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

/*
CDXC:SessionChat 2026-09-10 WHY:
The escaped-markup contract, one place for both halves of it. gxserver marks a harness marker whose
payload it escaped (Codex's `!` commands, and the slash commands it archives and replays in
session_chat_local_command.rs), because the reader strips markup out of these rows to find their
text and would otherwise eat a command or an output that contains `<…>`. The attribute string has to
match gxserver's `ESCAPED_MARKUP_ATTRIBUTE` byte for byte.
*/
pub const ESCAPED_MARKUP_ATTRIBUTE: &str = "data-ghostex-escaped=\"html\"";

const CODEX_LOCAL_COMMAND_INPUT: &str = "<bash-input data-ghostex-escaped=\"html\">";
const CODEX_LOCAL_COMMAND_OUTPUT: &str = "<bash-stdout data-ghostex-escaped=\"html\">";

pub fn decode_escaped_markup(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn text_block(block: Option<&ChatBlock>) -> Option<&str> {
    match block {
        Some(ChatBlock::Text { text }) => Some(text),
        _ => None,
    }
}

/// Splits Codex's two-block local-command row into the command and its output, so each gets its own
/// marker. Identical text to the rows gxserver replays from its archive.
pub fn normalize_local_command_messages(messages: &[ChatRow]) -> Vec<ChatRow> {
    let mut normalized = Vec::with_capacity(messages.len());
    for message in messages {
        let command = text_block(message.blocks.first());
        let output = text_block(message.blocks.get(1));
        let splits = message.role == ChatRole::User
            && message.source == ChatSource::Transcript
            && message.blocks.len() == 2
            && command.is_some_and(|text| text.starts_with(CODEX_LOCAL_COMMAND_INPUT))
            && output.is_some_and(|text| text.starts_with(CODEX_LOCAL_COMMAND_OUTPUT));
        if !splits {
            normalized.push(message.clone());
            continue;
        }
        let mut first = message.clone();
        let edit = std::sync::Arc::make_mut(&mut first);
        edit.id = format!("{}:command", message.id);
        edit.blocks = vec![message.blocks[0].clone()];
        let mut second = message.clone();
        let edit = std::sync::Arc::make_mut(&mut second);
        edit.id = format!("{}:output", message.id);
        edit.blocks = vec![message.blocks[1].clone()];
        normalized.push(first);
        normalized.push(second);
    }
    normalized
}

/// A user turn that is one slash command (`/rename Fix parser`, `/review`, `/plugin:skill args`):
/// one line, a `/` and a command name, then nothing or a space and its arguments. A path
/// (`/Users/me/notes.md`) is not one: its name runs into a second `/`.
///
/// The projection marks such a bubble `slashCommand` and every renderer draws its text in the
/// chat's monospace (`surface_skill_invocation_user_turns` turns the harness's envelopes into this
/// text). SEE-ALSO: apps/desktop/src/app/native_chat/transcript.rs,
/// apps/mobile/app/src/chat/native/transcript/MessageRow.tsx (`UserMessage`).
pub fn is_slash_command_line(text: &str) -> bool {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    if trimmed.contains('\n') {
        return false;
    }
    let name_len = rest
        .bytes()
        .take_while(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.')
        })
        .count();
    name_len > 0
        && rest.as_bytes()[0].is_ascii_alphanumeric()
        && rest[name_len..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

/// The tool name a `!` shell command's card carries; the name makes it a command tool, so the card
/// gets the terminal glyph and its Command and Result blocks.
pub const SHELL_COMMAND_TOOL_NAME: &str = "Shell";

/// The body of the first `<tag>` or `<tag data-ghostex-escaped="html">` element in `text`, entities
/// decoded.
fn harness_tag_body(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let body_start = if let Some(rest) = rest.strip_prefix('>') {
        text.len() - rest.len()
    } else {
        let attribute = rest
            .strip_prefix(' ')?
            .strip_prefix(ESCAPED_MARKUP_ATTRIBUTE)?;
        text.len() - attribute.strip_prefix('>')?.len()
    };
    let close = format!("</{tag}>");
    let end = body_start + text[body_start..].find(&close)?;
    Some(decode_escaped_markup(&text[body_start..end]))
}

fn joined_text(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// The command of a `!` shell row: Claude's `<bash-input>` row, or the escaped marker gxserver
/// writes for a Codex `!` command.
pub fn shell_command_input(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !trimmed.starts_with("<bash-input") || !trimmed.ends_with("</bash-input>") {
        return None;
    }
    let command = harness_tag_body(trimmed, "bash-input")?;
    let command = command.trim();
    (!command.is_empty()).then(|| command.to_string())
}

/// The chat text a `!` shell row stands for (`!aws login`), the same text the composer sent, so an
/// echo and its recorded row agree.
pub fn shell_command_prompt_text(message: &ChatMessage) -> Option<String> {
    if message.role != ChatRole::User {
        return None;
    }
    shell_command_input(&joined_text(message)).map(|command| format!("!{command}"))
}

/// What a `!` command printed, stdout and stderr apart: the card tints stderr as error text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
}

impl ShellOutput {
    fn merged(&self) -> String {
        match (self.stdout.trim().is_empty(), self.stderr.trim().is_empty()) {
            (_, true) => self.stdout.clone(),
            (true, false) => self.stderr.clone(),
            (false, false) => format!("{}\n{}", self.stdout, self.stderr),
        }
    }

    /// Only stderr had anything to say. Neither transcript records an exit code, so this is the
    /// only failure signal there is.
    fn failed(&self) -> bool {
        self.stdout.trim().is_empty() && !self.stderr.trim().is_empty()
    }
}

/// Whether `text` is a `!` command's printed output, the row that folds into the command's card.
pub fn is_shell_command_output(text: &str) -> bool {
    shell_command_output(text).is_some()
}

fn shell_command_output(text: &str) -> Option<ShellOutput> {
    let trimmed = text.trim();
    if !trimmed.starts_with("<bash-stdout") && !trimmed.starts_with("<bash-stderr") {
        return None;
    }
    let clean = |body: Option<String>| {
        crate::transcript::noise::strip_chat_ansi(&body.unwrap_or_default().replace("\r\n", "\n"))
            .trim_end()
            .to_string()
    };
    Some(ShellOutput {
        stdout: clean(harness_tag_body(trimmed, "bash-stdout")),
        stderr: clean(harness_tag_body(trimmed, "bash-stderr")),
    })
}

/// The card a `!` command renders as: a user turn holding the command as a shell tool call and its
/// output as that call's result. `output` is `None` while the command still runs and has printed
/// nothing. The result keeps stdout and stderr merged for search and copy; the call input carries
/// stderr on its own, which only the card reads ([`shell_card`]).
pub fn shell_command_message(
    template: &ChatMessage,
    command: &str,
    output: Option<ShellOutput>,
) -> ChatMessage {
    let call_id = Some(format!("shell-command:{}", template.id));
    let mut input = serde_json::json!({ "command": command });
    if let Some(output) = output
        .as_ref()
        .filter(|output| !output.stderr.trim().is_empty())
    {
        input["stderr"] = output.stderr.clone().into();
    }
    let mut blocks = vec![ChatBlock::ToolCall {
        name: SHELL_COMMAND_TOOL_NAME.to_string(),
        input,
        call_id: call_id.clone(),
    }];
    if let Some(output) = output.filter(|output| !output.merged().trim().is_empty()) {
        blocks.push(ChatBlock::ToolResult {
            output: output.merged(),
            is_error: output.failed().then_some(true),
            call_id,
            patch: None,
        });
    }
    let mut message = template.clone();
    message.role = ChatRole::User;
    message.blocks = blocks;
    message
}

/// Whether a row is a `!` command's card, which renders as the tool card in the user's place.
pub fn is_shell_command_message(message: &ChatMessage) -> bool {
    message.role == ChatRole::User
        && matches!(
            message.blocks.first(),
            Some(ChatBlock::ToolCall { name, .. }) if name == SHELL_COMMAND_TOOL_NAME
        )
        && message.blocks.iter().all(|block| {
            matches!(
                block,
                ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
            )
        })
}

/*
CDXC:SessionChat 2026-10-07 DECISION:
User: a `!` shell command sent to the agent ("! aws login") showed nothing in chat, then two plain "Local command" / "Local command output" rows; "let's please just show this in a new component similar to the ones we already have". Claude writes a `!` command as two user rows, `<bash-input> cmd</bash-input>` and `<bash-stdout>…</bash-stdout><bash-stderr>…</bash-stderr>` (its own output HTML-escaped), and only once the command has finished; gxserver writes a Codex `!` command as the same pair of escaped markers in one row, split above. The pair folds into ONE user row holding the command as a shell tool call and the output (stdout, then stderr) as its result, so every renderer draws the tool card where the user's message sits and the row opens its own turn, which the collapsed markers never did (summary mode filed them under the previous turn's work, or dropped them when no turn came before).
SEE-ALSO: session/pending.rs (the echo drawn as this card until the row lands), apps/desktop/src/app/native_chat/transcript.rs and apps/mobile/app/src/chat (the card in the user's place).
*/
pub fn fold_shell_commands(messages: &[ChatRow]) -> Vec<ChatRow> {
    let mut folded = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let command = (message.role == ChatRole::User)
            .then(|| shell_command_input(&joined_text(message)))
            .flatten();
        let Some(command) = command else {
            folded.push(message.clone());
            index += 1;
            continue;
        };
        // Claude writes a prompt queued meanwhile ahead of the command's own two rows, so a queue
        // row can sit between them; the output still belongs to the command.
        let mut next = index + 1;
        while messages.get(next).is_some_and(|row| row.queued) {
            next += 1;
        }
        let output = messages
            .get(next)
            .filter(|row| row.role == ChatRole::User)
            .and_then(|row| shell_command_output(&joined_text(row)));
        folded.push(std::sync::Arc::new(shell_command_message(
            message,
            &command,
            output.clone(),
        )));
        if output.is_some() {
            folded.extend(messages[index + 1..next].iter().cloned());
            index = next + 1;
        } else {
            index += 1;
        }
    }
    folded
}

/// A `!` line sent while the agent was busy waits in Claude's own queue, and Claude records the
/// queue entry without its `!`. While the line's echo stands in as its Shell card, that queue row is
/// the same send a second time, so it is left out; the command's own rows replace the echo once the
/// queue releases it.
pub fn without_shell_echo_queue_rows(
    transcript: Vec<ChatRow>,
    pending: &[ChatRow],
) -> Vec<ChatRow> {
    let commands: Vec<String> = pending
        .iter()
        .filter(|message| is_shell_command_message(message))
        .filter_map(|message| match message.blocks.first() {
            Some(ChatBlock::ToolCall { input, .. }) => input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .map(collapse_spaces),
            _ => None,
        })
        .collect();
    if commands.is_empty() {
        return transcript;
    }
    transcript
        .into_iter()
        .filter(|message| {
            !(message.queued
                && message.role == ChatRole::User
                && commands.contains(&collapse_spaces(&joined_text(message))))
        })
        .collect()
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How far before gxserver's estimate of a running command's start its recorded row may be stamped
/// and still be the same run: the probe's clock and the CLI's clock disagree a little.
const LIVE_SHELL_START_SLACK_MS: i64 = 10_000;

/// The running Shell card, with what the terminal shows of the command's output so far.
///
/// `activity` is gxserver's `shell-command` screen activity. Claude runs one `!` command at a time,
/// so the output belongs to the newest pending `!` echo; a command typed straight into the terminal
/// has no echo and gets its own running card, until the transcript records a `!` row for that run.
/// Either way the recorded card replaces it once the command ends, and the output never shows twice.
pub fn with_live_shell_output(
    mut pending: Vec<ChatRow>,
    activity: Option<&serde_json::Value>,
    transcript: &[ChatRow],
) -> Vec<ChatRow> {
    let Some(activity) = activity else {
        return pending;
    };
    let field = |key: &str| activity.get(key).and_then(serde_json::Value::as_str);
    let command = field("label").unwrap_or_default().trim();
    if command.is_empty() {
        return pending;
    }
    let output = ShellOutput {
        stdout: field("detail").unwrap_or_default().to_string(),
        stderr: String::new(),
    };
    if let Some(index) = pending
        .iter()
        .rposition(|message| is_shell_command_message(message))
    {
        let echo_command = match pending[index].blocks.first() {
            Some(ChatBlock::ToolCall { input, .. }) => input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(command)
                .to_string(),
            _ => command.to_string(),
        };
        pending[index] = std::sync::Arc::new(shell_command_message(
            &pending[index],
            &echo_command,
            Some(output),
        ));
        return pending;
    }
    let detected_at = field("detectedAt").unwrap_or_default();
    let elapsed_ms = activity
        .get("elapsedSeconds")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
        * 1000;
    let started_at = crate::session::startup_sends::parse_iso_ms(Some(detected_at))
        .map(|detected| detected - elapsed_ms);
    let recorded = transcript.iter().any(|message| {
        shell_command_prompt_text(message).is_some()
            && match (message.timestamp, started_at) {
                (Some(stamp), Some(started)) => stamp >= started - LIVE_SHELL_START_SLACK_MS,
                _ => false,
            }
    });
    if recorded {
        return pending;
    }
    let template = ChatMessage {
        id: format!("terminal-shell:{detected_at}"),
        role: ChatRole::User,
        blocks: Vec::new(),
        async_questions: None,
        timestamp: started_at,
        source: ChatSource::Hook,
        turn_id: None,
        byte_offset: None,
        queued: false,
        deferred_work: None,
        startup_delivery: None,
        api_error: None,
        mention_paths: None,
        custom_message: None,
    };
    pending.push(std::sync::Arc::new(shell_command_message(
        &template,
        command,
        Some(output),
    )));
    pending
}

/*
CDXC:SessionChat 2026-10-08 DECISION:
User: "clicking on the header or line doesn't collapse this … I don't like that at the top we're showing the same command twice once in the header and again in the "Command" area". Then: "I dont like the word Shell also. Prefer if we can remove that and just have the icon instead there". The Shell card is its own card rather than a tool row with Command / Result blocks: the header is the terminal icon (labelled "Shell command" for assistive tech) followed by the command, with no "Shell" word; then "please show "! " start of the command not just the command directly", so the header reads `! aws login`, the line kept to one line (the full text in a tooltip and on the copy button), and a status on the right ("Failed" when only stderr printed; the 2026-10-12 decision below removed "Running"); the body is only the output, stdout then stderr in the error tone, with the command repeated at its top only when the header had to cut a multi-line command. Then: "expanding the shell card looks ugly please keep the chat bubble as it is and expand a new bubble below it that has the rest of the content (with some gap between them)", so the header bubble keeps its collapsed size and the output opens as a second bubble below it, toggled by the header. The card opens by default while its output streams in and folds like every other disclosure, so a reader can close it mid-run.
SEE-ALSO: `shell_command_card` in apps/desktop/src/app/native_chat/tool_run.rs, `ShellCommandCard` in apps/mobile/app/src/chat/native/transcript/ToolRows.tsx.

CDXC:SessionChat 2026-10-12 DECISION:
User, on the `!` command's bubble: "please move the copy button to be to the left of the command chat bubble (outside it floating, like it is for other messages) also please remove the word "running" from there". The copy button is no longer part of the card's header; the renderers draw it as a message action beside the bubble. The header's status is empty while the command runs, and only "Failed" remains.
*/
/// What the renderers draw for a `!` command's card, or `Value::Null` for any other row.
pub fn shell_card(message: &ChatMessage) -> serde_json::Value {
    if !is_shell_command_message(message) {
        return serde_json::Value::Null;
    }
    let mut input = None;
    let mut output = String::new();
    let mut failed = false;
    for block in &message.blocks {
        match block {
            ChatBlock::ToolCall { input: call, .. } => input = Some(call),
            ChatBlock::ToolResult {
                output: result,
                is_error,
                ..
            } => {
                output = result.clone();
                failed = is_error.unwrap_or(false);
            }
            _ => {}
        }
    }
    let field = |key: &str| {
        input
            .and_then(|input| input.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let command = field("command");
    let stderr = field("stderr");
    // The result merged stderr after stdout; the card draws stderr apart in the error tone.
    let stdout = if stderr.is_empty() {
        output
    } else if failed {
        String::new()
    } else {
        output
            .strip_suffix(stderr.as_str())
            .map(|rest| rest.strip_suffix('\n').unwrap_or(rest).to_string())
            .unwrap_or(output)
    };
    // The recorded rows land only once the command has finished; an echo or the terminal's live
    // card is a command still running.
    let running = message.source != ChatSource::Transcript;
    // The only status the header carries is a failure; a running card is told by its streaming
    // output, not by a label (CDXC:SessionChat 2026-10-12 DECISION above).
    let status = if failed && !running { "Failed" } else { "" };
    // The header reads as the line the user typed; the tooltip and the copy button keep the bare
    // command, which is what pastes into a shell.
    let headline = format!("! {}", command.lines().next().unwrap_or_default());
    let multiline = command.trim_end().contains('\n');
    let stdout = crate::transcript::tool_rows::clip_tool_body(&stdout);
    let stderr = crate::transcript::tool_rows::clip_tool_body(&stderr);
    let has_body = !stdout.trim().is_empty() || !stderr.trim().is_empty() || multiline;
    serde_json::json!({
        "command": headline,
        "fullCommand": command,
        "commandBody": if multiline { command.as_str() } else { "" },
        "running": running,
        "status": status,
        "failed": failed,
        "stdout": stdout,
        "stderr": stderr,
        "hasBody": has_body,
        // A live card opens on its output as it streams in; a recorded one opens on demand.
        "openByDefault": running && has_body,
    })
}
