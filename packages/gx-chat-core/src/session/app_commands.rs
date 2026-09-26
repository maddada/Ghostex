//! The rows for commands Ghostex typed into the agent itself.
//!
//! Ported from the app-command half of `packages/core-ui/chat/session-chat-pending.ts` and from
//! `session-chat-local-command-transcript.ts`.
//!
//! CDXC:SessionChat 2026-08-23:
//! Commands GHOSTEX typed into the agent (the auto-title `/rename`, a fork's provisional title)
//! reuse the marker lane rather than getting a surface of their own: it is the same fact, "a
//! command went to the terminal", and only the sender differs. They say who sent them, because
//! "Ran /rename Fix parser" reads as something the user did and would be the second most confusing
//! thing on screen after no row at all. They retire against the agent's OWN record of the same
//! command: Claude Code writes a `<command-name>` envelope for everything it intercepts, so
//! rendering both would double the row, while Codex writes nothing, which is the whole reason these
//! exist.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};
use serde_json::Value;

use crate::session::startup_sends::parse_iso_ms;
use crate::session::text::{is_js_space, normalize_pending_text, parse_command_envelope};
use crate::state::CommandMarker;

/// System row carrying a Codex goal: blocks are status, objective, usage.
pub const CODEX_GOAL_ID_PREFIX: &str = "app-command-goal:";

/// CDXC:SessionChat 2026-09-10 WHY:
/// The escaped-markup contract. gxserver marks a harness marker whose payload it escaped (Codex's
/// `!` commands, and the slash commands it archives and replays in `session_chat_local_command.rs`),
/// because the reader strips markup out of these rows to find their text and would otherwise eat a
/// command or an output that contains `<…>`. The attribute string has to match gxserver's
/// `ESCAPED_MARKUP_ATTRIBUTE` byte for byte.
pub const ESCAPED_MARKUP_ATTRIBUTE: &str = "data-ghostex-escaped=\"html\"";

/// `sessionChatEscapedMarker`: the escaped marker gxserver writes, for a row this client
/// synthesizes.
pub fn escaped_marker(tag: &str, body: &str) -> String {
    let escaped = body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<{tag} {ESCAPED_MARKUP_ATTRIBUTE}>{escaped}</{tag}>")
}

/// `sessionChatLocalCommandTexts`: the two rows one slash command renders as, the command and its
/// output when the CLI printed any.
///
/// Identical text to the rows gxserver replays from its archive, so the live row and the archived
/// one are the same row twice.
pub fn local_command_texts(command: &str, output: Option<&str>) -> Vec<String> {
    let trimmed = command.trim_matches(is_js_space);
    let (name, args) = match trimmed.find(is_js_space) {
        Some(at) => (&trimmed[..at], trimmed[at..].trim_matches(is_js_space)),
        None => (trimmed, ""),
    };
    // The space between the two markers is load-bearing: readers strip the markup to get the text,
    // and without it the name and its arguments would become one word.
    let mut texts = vec![if args.is_empty() {
        escaped_marker("command-name", name)
    } else {
        format!(
            "{} {}",
            escaped_marker("command-name", name),
            escaped_marker("command-args", args)
        )
    }];
    if let Some(output) = output.filter(|output| !output.is_empty()) {
        texts.push(escaped_marker("local-command-stdout", output));
    }
    texts
}

fn text(entry: &Value, key: &str) -> Option<String> {
    entry.get(key).and_then(Value::as_str).map(str::to_string)
}

fn flag(entry: &Value, key: &str) -> bool {
    entry.get(key).and_then(Value::as_bool) == Some(true)
}

/// `Date.parse(entry.sentAt) || 0`: a stamp the row's ordering needs, zero when there is none.
fn sent_at_or_zero(entry: &Value) -> i64 {
    parse_iso_ms(entry.get("sentAt").and_then(Value::as_str)).unwrap_or(0)
}

/// `Date.parse(entry.sentAt) || null`, where JavaScript's `||` also turns a zero stamp into null.
fn sent_at_or_null(entry: &Value) -> Option<i64> {
    match sent_at_or_zero(entry) {
        0 => None,
        stamp => Some(stamp),
    }
}

fn joined_text(message: &ChatMessage, separator: &str) -> String {
    message
        .blocks
        .iter()
        .map(|block| match block {
            ChatBlock::Text { text } => text.as_str(),
            _ => "",
        })
        .collect::<Vec<_>>()
        .join(separator)
}

fn row(id: String, role: ChatRole, blocks: Vec<&str>, timestamp: Option<i64>) -> ChatMessage {
    ChatMessage {
        id,
        role,
        blocks: blocks
            .into_iter()
            .map(|text| ChatBlock::Text {
                text: text.to_string(),
            })
            .collect(),
        async_questions: None,
        timestamp,
        source: ChatSource::Client,
        turn_id: None,
        byte_offset: None,
        queued: false,
        deferred_work: None,
        startup_delivery: None,
    }
}

/// `/^\/(?:rename|name|title)(?:\s+(.+))?$/is`: the title the command carries, if it is one.
///
/// `Some(None)` is a bare `/rename`, which is only meaningful once the agent publishes the name it
/// selected; `None` is any other command.
fn rename_argument(command: &str) -> Option<Option<String>> {
    let trimmed = command.trim_matches(is_js_space);
    let rest = trimmed.strip_prefix('/')?;
    let lower = rest.to_lowercase();
    let name = ["rename", "name", "title"]
        .into_iter()
        .find(|name| lower.starts_with(name))?;
    let tail = &rest[name.len()..];
    if tail.is_empty() {
        return Some(None);
    }
    if !tail.starts_with(is_js_space) {
        return None;
    }
    let argument = tail.trim_start_matches(is_js_space);
    if argument.is_empty() {
        // `\s+(.+)` needs at least one character after the whitespace, so a trailing run of spaces
        // makes the optional group fail and the whole pattern match the bare form.
        return Some(None);
    }
    Some(Some(argument.to_string()))
}

/// `sessionChatAppCommandsAsMessages`.
pub fn app_commands_as_messages(
    commands: &[Value],
    transcript: &[ChatMessage],
) -> Vec<ChatMessage> {
    let recorded: Vec<String> = transcript
        .iter()
        .filter_map(|message| {
            let envelope = parse_command_envelope(&joined_text(message, "\n"))?;
            Some(normalize_pending_text(&format!(
                "{} {}",
                envelope.name, envelope.args
            )))
        })
        .collect();
    let mut rows = Vec::new();
    for entry in commands {
        let id = text(entry, "id").unwrap_or_default();
        let command = text(entry, "command").unwrap_or_default();
        let archive_id = text(entry, "archiveId");
        let local_command = flag(entry, "localCommand");
        // A live archive row retires only against its own persisted occurrence.
        if let Some(archive_id) = archive_id.as_deref() {
            if transcript
                .iter()
                .any(|message| message.id == format!("local-command:{archive_id}"))
            {
                continue;
            }
        } else if local_command && recorded.contains(&normalize_pending_text(&command)) {
            continue;
        }
        // A command the user sent from chat renders as the same two rows the archive replays.
        // Ghostex's own sends and Codex's dialog answers keep the surfaces below: those are not
        // commands the reader typed.
        if local_command {
            let stamp = sent_at_or_zero(entry);
            for (index, body) in local_command_texts(&command, text(entry, "output").as_deref())
                .into_iter()
                .enumerate()
            {
                rows.push(row(
                    if index == 0 {
                        format!("app-command-local:{id}")
                    } else {
                        format!("app-command-local:{id}:output")
                    },
                    ChatRole::User,
                    vec![&body],
                    Some(stamp + index as i64),
                ));
            }
            continue;
        }
        if let Some(goal) = entry.get("goal") {
            rows.push(row(
                format!("{CODEX_GOAL_ID_PREFIX}{id}"),
                ChatRole::System,
                vec![
                    goal.get("status")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    goal.get("objective")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    goal.get("usage")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ],
                sent_at_or_null(entry),
            ));
            continue;
        }
        if let Some(output) = text(entry, "output") {
            if output.is_empty() {
                continue;
            }
            rows.push(row(
                format!("app-command-output:{id}"),
                ChatRole::System,
                vec![&command, &output],
                sent_at_or_null(entry),
            ));
            continue;
        }
        if recorded.contains(&normalize_pending_text(&command)) {
            continue;
        }
        if let Some(argument) = rename_argument(&command) {
            let title = text(entry, "title")
                .map(|title| title.trim_matches(is_js_space).to_string())
                .filter(|title| !title.is_empty())
                .or_else(|| argument.map(|value| value.trim_matches(is_js_space).to_string()))
                .filter(|title| !title.is_empty());
            // A bare `/rename` is only meaningful once the agent publishes the name it selected.
            // Wait for that metadata instead of showing an implementation command with no
            // user-facing result.
            let Some(title) = title else { continue };
            rows.push(row(
                format!("app-command:{id}"),
                ChatRole::System,
                vec!["Ghostex auto named this session", &title],
                sent_at_or_null(entry),
            ));
            continue;
        }
        rows.push(row(
            format!("app-command:{id}"),
            ChatRole::System,
            vec![&format!("Ghostex sent {command}")],
            sent_at_or_null(entry),
        ));
    }
    rows
}

/// CDXC:SessionChat 2026-09-10 WHY:
/// A command typed from chat still gets its instant client marker: the composer cannot know whether
/// the daemon it talks to archives commands. Once the daemon's own row for that command is on
/// screen, the live local-command acknowledgement or the archived envelope a read replays, the
/// marker is the same fact twice, drawn as a user bubble beside a `Slash command` row.
pub fn local_command_identities(
    app_commands: &[Value],
    messages: &[ChatMessage],
) -> Vec<(String, String)> {
    // Insertion-ordered, because the TypeScript's `Map` was and the consumer takes the FIRST
    // unconsumed match.
    let mut covered: Vec<(String, String)> = Vec::new();
    let mut set = |key: String, value: String| match covered
        .iter_mut()
        .find(|(existing, _)| *existing == key)
    {
        Some(entry) => entry.1 = value,
        None => covered.push((key, value)),
    };
    for entry in app_commands
        .iter()
        .filter(|entry| flag(entry, "localCommand"))
    {
        let key = text(entry, "archiveId").or_else(|| text(entry, "id"));
        if let Some(key) = key {
            set(
                key,
                normalize_pending_text(&text(entry, "command").unwrap_or_default()),
            );
        }
    }
    for message in messages {
        if !message.id.starts_with("local-command:") || message.id.ends_with(":output") {
            continue;
        }
        let Some(envelope) = parse_command_envelope(&joined_text(message, "\n")) else {
            continue;
        };
        set(
            message.id["local-command:".len()..].to_string(),
            normalize_pending_text(&format!("{} {}", envelope.name, envelope.args)),
        );
    }
    covered
}

/// `retireSessionChatMarkersCoveredByLocalCommands`.
pub fn retire_markers_covered_by_local_commands(
    marker_messages: &[ChatMessage],
    app_commands: &[Value],
    messages: &[ChatMessage],
    markers: &[CommandMarker],
) -> Vec<ChatMessage> {
    let covered = local_command_identities(app_commands, messages);
    let mut consumed: Vec<&str> = Vec::new();
    let mut kept = Vec::new();
    for message in marker_messages {
        if !matches!(message.role, ChatRole::User) {
            kept.push(message.clone());
            continue;
        }
        let marker = markers
            .iter()
            .find(|entry| format!("command:{}", entry.id) == message.id);
        let text = normalize_pending_text(&joined_text(message, "\n"));
        let matched = covered.iter().find(|(id, candidate)| {
            *candidate == text
                && !consumed.contains(&id.as_str())
                && !marker.is_some_and(|marker| marker.local_command_ids_before.contains(id))
        });
        match matched {
            Some((id, _)) => consumed.push(id),
            None => kept.push(message.clone()),
        }
    }
    kept
}

/// `reconcileSessionChatLocalCommandOutput`: keep the archived command's position while live probes
/// refine its result.
pub fn reconcile_local_command_output(
    messages: &[ChatMessage],
    commands: &[Value],
) -> Vec<ChatMessage> {
    let mut result = messages.to_vec();
    for command in commands {
        if !flag(command, "localCommand") {
            continue;
        }
        let (Some(archive_id), Some(output)) = (
            text(command, "archiveId"),
            text(command, "output").filter(|output| !output.is_empty()),
        ) else {
            continue;
        };
        let id = format!("local-command:{archive_id}");
        let Some(at) = result.iter().position(|message| message.id == id) else {
            continue;
        };
        let Some(body) =
            local_command_texts(&text(command, "command").unwrap_or_default(), Some(&output))
                .into_iter()
                .nth(1)
        else {
            continue;
        };
        let mut row = result[at].clone();
        row.id = format!("{id}:output");
        row.blocks = vec![ChatBlock::Text { text: body }];
        row.timestamp = Some(result[at].timestamp.unwrap_or(0) + 1);
        match result.iter().position(|message| message.id == row.id) {
            Some(output_at) => {
                // A native transcript result remains authoritative over a screen capture.
                let native = result[output_at].blocks.iter().any(|block| match block {
                    ChatBlock::Text { text } => text
                        .trim_start_matches(is_js_space)
                        .starts_with("<local-command-stdout>"),
                    _ => false,
                });
                if !native {
                    result[output_at] = row;
                }
            }
            None => result.insert(at + 1, row),
        }
    }
    result
}
