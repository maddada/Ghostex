use super::*;

// ---------------------------------------------------------------------------
// Delivery verification
// ---------------------------------------------------------------------------

/// BLOCKING (filesystem). One tick of the two-tier check over everything the
/// transcript gained since the send.
pub(super) fn poll_transcript_for_send(
    probe: &SessionChatSendProbe,
    cursor: &mut WatchdogCursor,
    decoder: SessionChatLineDecoder,
    needle: &str,
    raw_needles: &[String],
) -> bool {
    if cursor.path.is_none() {
        // Codex creates the rollout lazily, so a brand-new session has no file
        // to sample at send time; everything in it is post-send by definition.
        cursor.path = resolve_session_chat_transcript_path(
            probe.transcript_agent,
            probe.agent_session_id.as_deref(),
            probe.agent_session_path.as_deref(),
        );
        if cursor.path.is_some() {
            cursor.base_offset = 0;
            cursor.state.rebase(0);
            // A file that only appeared now was never sampled at send time, so
            // "everything past the baseline is post-send" is an assumption, not
            // a measurement. Good enough to look for the sent text in; not good
            // enough to read other people's turns as evidence against it.
            cursor.baseline_trusted = false;
        }
    }
    let Some(path) = cursor.path.clone() else {
        return false;
    };
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    if metadata.len() < cursor.state.offset {
        // The file was replaced or rewritten under us; re-read it whole rather
        // than tailing an offset that no longer exists. The send's baseline is
        // gone with it, so the mismatch scan stops trusting it.
        cursor.base_offset = 0;
        cursor.state.rebase(0);
        cursor.baseline_trusted = false;
    }

    let mut matched = false;
    let decoded = {
        let mut on_batch = |batch: Vec<SessionChatMessage>| {
            if !matched {
                matched = batch
                    .iter()
                    .any(|message| user_message_matches(message, needle));
            }
        };
        read_incremental_transcript_messages(
            &path,
            &mut cursor.state,
            decoder,
            Some(&mut on_batch),
            None,
            None,
            None,
        )
    };
    if matched {
        return true;
    }
    if let Ok(messages) = decoded {
        if messages
            .iter()
            .any(|message| user_message_matches(message, needle))
        {
            return true;
        }
    }

    /*
    Second tier: the decoders skip rows that still prove delivery — Claude's
    `queue-operation` enqueue record (typed while a turn ran) and Codex's
    `response_item` message lane. Both carry the text verbatim inside a JSON
    string, so the needle is escaped the same way before the scan. Input the CLI
    intercepted adds its own shapes to look for: it is never a user turn, but
    Claude does record `<command-name>` / `<bash-input>` rows naming it.
    */
    /*
    The same appended window answers the mismatch question below, so it is read
    once. Skipping the read entirely when neither tier can use it keeps a short
    send on a session with no usable baseline at zero filesystem cost.
    */
    let scan_for_mismatch = cursor.baseline_trusted && !cursor.agent_answered_mismatch;
    if raw_needles.is_empty() && !scan_for_mismatch {
        return false;
    }
    let Some(appended) =
        read_appended_text(&path, cursor.base_offset, WATCHDOG_RAW_SCAN_LIMIT_BYTES)
    else {
        return false;
    };
    if raw_needles
        .iter()
        .any(|raw_needle| appended.contains(raw_needle))
    {
        return true;
    }
    if scan_for_mismatch {
        observe_mismatched_input(probe.transcript_agent, cursor, &appended, needle);
    }
    false
}

fn user_message_matches(message: &SessionChatMessage, needle: &str) -> bool {
    message.role == SessionChatRole::User
        && watchdog_text_matches(&message_plain_text(message), needle)
}

/// Whitespace-folded containment, or a prefix that carries most of the send.
pub(super) fn watchdog_text_matches(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let text = normalize_watchdog_text(text);
    if text.is_empty() {
        return false;
    }
    if text.contains(needle) {
        return true;
    }
    needle.starts_with(&text)
        && text.chars().count() * 100 >= needle.chars().count() * WATCHDOG_PREFIX_MATCH_PERCENT
}

fn message_plain_text(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Agent TUIs re-wrap and re-indent what the user typed, so both sides of the
/// comparison are reduced to single-spaced words.
pub(super) fn normalize_watchdog_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Mismatched-input evidence
// ---------------------------------------------------------------------------

/*
CDXC:AgentScreenDetection 2026-08-24:
The tiers above answer "did our message arrive?" and can only ever fail to prove
it. This one answers the sharper question "was something ELSE submitted in its
place?", which is what actually happened in the incident this exists for: the
send's trailing Enter committed the composer before the pasted text had been
ingested into it, so the agent recorded an EMPTY user turn, started answering it,
and the message stayed behind in the composer until another terminal write
replaced or cleared it.

It cannot reuse the decoders. Both of them drop a contentless user row on the
floor (`claude_content_blocks` returns no blocks for an empty string, and
`extract_string` discards empty text), which is precisely the row that proves
the failure — so this reads the appended records itself, and deliberately reads
only the two lanes that mean "the human's composer submitted this": Claude's
`user` rows and Codex's event lanes. Harness-injected user turns, tool results,
queue rows and Codex's envelope-carrying `response_item` message twin are all
excluded, because none of them is something a person submitted.
*/
pub(super) enum WatchdogRecord {
    /// A composer submission, with its plain text — EMPTY when a bare Enter
    /// submitted nothing.
    UserSubmission(String),
    /// The agent produced output of its own: a turn is under way.
    AgentOutput,
}

/// BLOCKING-free: scans the already-read window. Re-derived from the whole
/// window every poll so record ORDER is never carried across polls; only the
/// verdict is, and only ever in one direction.
fn observe_mismatched_input(
    agent: SessionChatTranscriptAgent,
    cursor: &mut WatchdogCursor,
    appended: &str,
    needle: &str,
) {
    let mut mismatch: Option<bool> = None;
    let mut agent_answered = false;
    for line in appended.lines() {
        match classify_watchdog_record(agent, line) {
            Some(WatchdogRecord::UserSubmission(text)) => {
                if mismatch.is_none() && !watchdog_text_matches(&text, needle) {
                    mismatch = Some(text.trim().is_empty());
                }
            }
            // Only output that follows the mismatched turn says the agent went
            // to work on it; the tail of the turn our send was typed into does
            // not.
            Some(WatchdogRecord::AgentOutput) => agent_answered |= mismatch.is_some(),
            None => {}
        }
    }
    let Some(submitted_empty) = mismatch else {
        return;
    };
    cursor.mismatched_input.get_or_insert(submitted_empty);
    cursor.agent_answered_mismatch |= agent_answered;
}

pub(super) fn classify_watchdog_record(
    agent: SessionChatTranscriptAgent,
    line: &str,
) -> Option<WatchdogRecord> {
    let record = parse_json_object(line)?;
    match agent {
        SessionChatTranscriptAgent::Claude => claude_watchdog_record(&record),
        SessionChatTranscriptAgent::Codex => codex_watchdog_record(&record),
        // No catalogued record shapes, so no evidence either way. The delivery
        // tiers and the 10s deadline still cover these agents unchanged.
        SessionChatTranscriptAgent::Antigravity
        | SessionChatTranscriptAgent::Copilot
        | SessionChatTranscriptAgent::Grok
        | SessionChatTranscriptAgent::Cursor
        | SessionChatTranscriptAgent::Empryo
        | SessionChatTranscriptAgent::Hermes
        | SessionChatTranscriptAgent::OpenCode
        | SessionChatTranscriptAgent::Pi
        | SessionChatTranscriptAgent::Zcode
        | SessionChatTranscriptAgent::Freebuff
        | SessionChatTranscriptAgent::Dsh => None,
    }
}

/// Whether a transcript line records a prompt the agent took: a composer submission, or a prompt
/// Claude moved out of its queue into the running turn. Read by the queued prompt's play button
/// (session_chat_send/queued_prompt_escape.rs).
pub(crate) fn transcript_line_records_prompt(
    agent: SessionChatTranscriptAgent,
    line: &str,
) -> bool {
    if matches!(
        classify_watchdog_record(agent, line),
        Some(WatchdogRecord::UserSubmission(_))
    ) {
        return true;
    }
    agent == SessionChatTranscriptAgent::Claude
        && parse_json_object(line).is_some_and(|record| {
            record.get("type").and_then(Value::as_str) == Some("attachment")
                && record
                    .get("attachment")
                    .and_then(Value::as_object)
                    .is_some_and(|attachment| {
                        attachment.get("type").and_then(Value::as_str) == Some("queued_command")
                            && attachment
                                .get("prompt")
                                .and_then(Value::as_str)
                                .is_some_and(|prompt| !prompt.trim().is_empty())
                    })
        })
}

fn claude_watchdog_record(record: &Map<String, Value>) -> Option<WatchdogRecord> {
    match record.get("type").and_then(Value::as_str)? {
        "assistant" => Some(WatchdogRecord::AgentOutput),
        "user" => {
            // Harness plumbing wearing the user role: replayed summaries, the
            // injected meta turns, and the marker row an interrupt writes.
            if record.get("isMeta") == Some(&Value::Bool(true))
                || record.get("isSynthetic") == Some(&Value::Bool(true))
                || record.get("isCompactSummary") == Some(&Value::Bool(true))
                || record.contains_key("interruptedMessageId")
            {
                return None;
            }
            let message = record.get("message")?.as_object()?;
            let text = claude_submitted_text(message.get("content")?)?;
            user_submission_record(text)
        }
        // `queue-operation` and `attachment` rows describe a prompt the CLI is
        // HOLDING, not one it submitted past ours, and the raw needle tier
        // already reads them as delivery proof.
        _ => None,
    }
}

/// The text a Claude `user` row submitted. `None` for content that is not a
/// composer submission at all (tool results, images, attachments).
fn claude_submitted_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut parts: Vec<&str> = Vec::new();
            for item in items {
                match item {
                    Value::String(text) => parts.push(text),
                    Value::Object(block) => match block.get("type").and_then(Value::as_str) {
                        Some("text") => parts.push(
                            block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        ),
                        _ => return None,
                    },
                    _ => return None,
                }
            }
            Some(parts.join(" "))
        }
        _ => None,
    }
}

fn codex_watchdog_record(record: &Map<String, Value>) -> Option<WatchdogRecord> {
    let payload = record.get("payload")?.as_object()?;
    let payload_type = payload.get("type").and_then(Value::as_str)?;
    match record.get("type").and_then(Value::as_str)? {
        "event_msg" => match payload_type {
            "user_message" => {
                user_submission_record(payload.get("message").and_then(Value::as_str)?.to_string())
            }
            "agent_message" | "task_started" => Some(WatchdogRecord::AgentOutput),
            "item_completed" => {
                let item = payload.get("item")?.as_object()?;
                match item.get("type").and_then(Value::as_str)? {
                    "UserMessage" => {
                        user_submission_record(codex_item_submitted_text(item.get("content")?)?)
                    }
                    "AgentMessage" => Some(WatchdogRecord::AgentOutput),
                    _ => None,
                }
            }
            _ => None,
        },
        /*
        The `response_item` message lane is the envelope-carrying twin of the
        event lane (see `codex_response_item`), so its user rows are NOT
        submissions; its assistant and tool lanes are still the agent working.
        */
        "response_item" => match payload_type {
            "reasoning" | "function_call" | "local_shell_call" | "custom_tool_call"
            | "web_search_call" | "tool_search_call" => Some(WatchdogRecord::AgentOutput),
            "message" => (payload.get("role").and_then(Value::as_str) == Some("assistant"))
                .then_some(WatchdogRecord::AgentOutput),
            _ => None,
        },
        _ => None,
    }
}

/// `item_completed` UserMessage content, which spells its text block `text`.
fn codex_item_submitted_text(content: &Value) -> Option<String> {
    let items = content.as_array()?;
    let mut parts: Vec<&str> = Vec::new();
    for item in items {
        let block = item.as_object()?;
        match block.get("type").and_then(Value::as_str) {
            Some("text" | "Text" | "input_text") => parts.push(
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            // The slash-skill chip carries no typed text of its own.
            Some("skill") => {}
            _ => return None,
        }
    }
    Some(parts.join(" "))
}

/// Wraps submitted text as evidence, minus the harness envelopes that ride the
/// user role. An EMPTY submission is kept: it is the whole point of this tier.
fn user_submission_record(text: String) -> Option<WatchdogRecord> {
    let probe = SessionChatMessage {
        id: String::new(),
        role: SessionChatRole::User,
        blocks: vec![text_block(text.clone())],
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
    };
    (!is_noise_message(&probe)).then_some(WatchdogRecord::UserSubmission(text))
}

/// The needle as it appears INSIDE a JSON string on disk (no surrounding
/// quotes). `None` when the text is too short for a substring scan to be
/// evidence. Non-ASCII text that the agent wrote with `\uXXXX` escapes will not
/// match here — that is what the decoded tier is for.
pub(super) fn json_escaped_needle(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.chars().count() < WATCHDOG_RAW_SCAN_MIN_CHARS {
        return None;
    }
    let encoded = serde_json::to_string(trimmed).ok()?;
    let inner = encoded.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.is_empty()).then(|| inner.to_string())
}

/// A message the CLI executes itself instead of sending to the model; recognised by
/// `parse_agent_cli_command` (CDXC:AgentScreenDetection in session_chat_send/input_bytes.rs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InterceptedInput {
    /// The `/command` token, prefix included.
    SlashCommand(String),
    /// The command body with the `!` stripped, which is how both CLIs echo and
    /// (for Claude) record it.
    ShellEscape(String),
}

impl InterceptedInput {
    pub(super) fn detect(text: &str) -> Option<Self> {
        Some(
            match crate::session_chat_send::parse_agent_cli_command(text)? {
                crate::session_chat_send::AgentCliCommand::Slash(name) => {
                    Self::SlashCommand(format!("/{name}"))
                }
                crate::session_chat_send::AgentCliCommand::Shell(command) => {
                    Self::ShellEscape(command.to_string())
                }
            },
        )
    }

    /// Extra raw-scan needles that PROVE the CLI took the input. Claude names
    /// the command in a record of its own; Codex contributes nothing here,
    /// which is what the suppression exists for.
    pub(super) fn delivery_needles(&self) -> Vec<String> {
        match self {
            // Both the tag and the command name are JSON-escape-free, so this
            // matches the raw transcript bytes exactly as written.
            Self::SlashCommand(command) => {
                vec![format!("<command-name>{command}</command-name>")]
            }
            // `<bash-input>` carries the command without its `!`, so the typed
            // text never matches it; the stripped body does.
            Self::ShellEscape(command) => json_escaped_needle(command).into_iter().collect(),
        }
    }
}

pub(super) fn read_appended_text(path: &Path, offset: u64, limit: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buffer: Vec<u8> = Vec::new();
    file.take(limit).read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}
