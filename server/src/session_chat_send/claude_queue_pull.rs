use super::*;

/// The diagnostic source of every write a take-back makes.
pub(super) const TAKE_BACK_SOURCE: &str = "session-chat-queue-take-back";
/// Claude Code's own key for its queue: Up in an empty input box brings every queued message back
/// into the box ("Press up to edit queued messages").
const CLAUDE_QUEUE_PULL_KEY: &str = "\u{1b}[A";
/// How long the pull waits for Claude to answer Up, on the transcript and in its input box.
const CLAUDE_QUEUE_PULL_WAIT_MS: u64 = 3_000;
const CLAUDE_QUEUE_PULL_POLL_MS: u64 = 100;
/// Verified Ctrl+U bursts spent emptying the box before the pull gives up.
const CLAUDE_QUEUE_CLEAR_ATTEMPTS: usize = 10;
/// Backspace, which leaves Claude's shell mode once its `!` line is empty.
const CLAUDE_LEAVE_SHELL_MODE: &str = "\u{7f}";
/// How many of the transcript's newest messages the queue check reads. A queued message waits only
/// until Claude's next tool boundary, so its enqueue row is always among them.
const CLAUDE_QUEUE_CHECK_MESSAGES: usize = 200;
/// How long the queue check waits for Claude to record a message typed just before the take-back.
const CLAUDE_QUEUE_CHECK_WAIT_MS: u64 = 1_000;

/// Runs inside the session's send queue (the `PullClaudeQueue` step), so no other send types into
/// the box between the Up and the clear.
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// Claude Code (2.1.296) has no way to take one message out of its queue: Up in an empty input box moves every editable queued message into the box (one `popAll` transcript row each), and Ctrl+C, the usual chat clear, interrupts a running turn instead of emptying the box (checked live). So the pull presses Up, reads what came back off the transcript rather than the screen (a long paste shows only as `[Pasted text #1]` there), and empties the box with Ctrl+U, which deletes back to the line start and joins lines when repeated, verified against the screen.
/// CDXC:SessionChat 2026-10-16 WHY: Up is pressed only while the transcript shows a message still waiting in Claude's queue (`claude_queue_holds_messages`). With nothing queued, Up recalls the last prompt into the box instead; emptying it afterwards failed for a multi-line pasted prompt the screen reader could not read, so the recalled prompt stayed in Claude's box and the user later sent it again by mistake. This supersedes the 2026-10-11 rule that pressed Up and emptied whatever came back.
pub(crate) async fn pull_claude_queue(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    transcript: &Path,
    cancelled: &(impl Fn() -> bool + Sync + ?Sized),
) -> Result<Vec<String>, SessionChatSendError> {
    let screen = capture_session_terminal_text_vt(zmx_name)
        .await
        .ok_or_else(|| {
            SessionChatSendError::new(
                SessionChatSendFailure::ComposerNotReady,
                "Claude's screen could not be read, so nothing was taken back.".to_string(),
            )
        })?;
    let notice =
        crate::session_chat_notice::classify_session_chat_terminal_notice(Some("claude"), &screen);
    let ready = crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some("claude"),
        &screen,
        notice.as_ref(),
    );
    // Up means something else on any other screen: it moves a dialog's highlight or a menu's row.
    if ready.state != crate::session_chat_composer::SessionChatComposerState::Ready {
        return Err(SessionChatSendError::new(
            SessionChatSendFailure::ComposerNotReady,
            "Claude is not showing its input box, so nothing was taken back.".to_string(),
        ));
    }
    let input = crate::session_chat_composer::session_chat_composer_input("claude", &screen)
        .ok_or_else(|| {
            SessionChatSendError::new(
                SessionChatSendFailure::ComposerNotReady,
                "Claude's input box could not be read, so nothing was taken back.".to_string(),
            )
        })?;
    // Up only reaches the queue from an empty box, and the clear below would erase what is there.
    if !input.is_empty() || input.shell_mode {
        return Err(SessionChatSendError::new(
            SessionChatSendFailure::ComposerNotCleared,
            "Claude's input box has text in it. Send or clear it in the terminal, then try again."
                .to_string(),
        ));
    }
    if cancelled() {
        return Err(SessionChatSendError::not_attempted(
            SESSION_CHAT_SEND_CANCELLED.to_string(),
        ));
    }
    if !claude_queue_holds_messages(transcript).await? {
        return Ok(Vec::new());
    }
    let offset = fs::metadata(transcript).map_or(0, |meta| meta.len());
    write_session_chat_payload(
        project_id,
        session_id,
        zmx_name,
        source,
        CLAUDE_QUEUE_PULL_KEY,
    )
    .await
    .map_err(|message| SessionChatSendError::new(SessionChatSendFailure::Write, message))?;
    let deadline = Instant::now() + Duration::from_millis(CLAUDE_QUEUE_PULL_WAIT_MS);
    let mut popped;
    loop {
        popped = popped_queue_messages(transcript, offset);
        let filled = capture_session_terminal_text_vt(zmx_name)
            .await
            .and_then(|screen| {
                crate::session_chat_composer::session_chat_composer_input("claude", &screen)
            })
            .is_some_and(|input| !input.is_empty());
        // The box is emptied only once it shows what came back, so the clear cannot run ahead of
        // Claude's repaint and leave the messages in the box.
        if (filled && !popped.is_empty()) || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(CLAUDE_QUEUE_PULL_POLL_MS)).await;
    }
    empty_claude_input(project_id, session_id, zmx_name, source).await?;
    Ok(popped)
}

/// Whether Claude's transcript shows a message still waiting in its queue: an enqueue row that no
/// removal, delivery or `popAll` released yet. A message typed just before the take-back is waited
/// for briefly, because Claude records it a moment after the Enter.
async fn claude_queue_holds_messages(transcript: &Path) -> Result<bool, SessionChatSendError> {
    let deadline = Instant::now() + Duration::from_millis(CLAUDE_QUEUE_CHECK_WAIT_MS);
    loop {
        let tail = crate::session_chat::read_session_chat_transcript_tail_file(
            transcript,
            CLAUDE_QUEUE_CHECK_MESSAGES,
            crate::session_chat::session_chat_line_decoder(
                crate::session_chat::SessionChatTranscriptAgent::Claude,
            ),
            false,
            None,
            None,
            crate::session_chat::session_chat_lineage_extractor(
                crate::session_chat::SessionChatTranscriptAgent::Claude,
            ),
        )
        .map_err(|_| {
            SessionChatSendError::new(
                SessionChatSendFailure::ComposerNotReady,
                "Claude's transcript could not be read, so nothing was taken back.".to_string(),
            )
        })?;
        if !tail.outstanding_queued_prompts.is_empty() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(CLAUDE_QUEUE_PULL_POLL_MS)).await;
    }
}

/// The `popAll` rows Claude wrote after `offset`, oldest first.
fn popped_queue_messages(transcript: &Path, offset: u64) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut bytes = Vec::new();
    let read = fs::File::open(transcript).and_then(|mut file| {
        file.seek(SeekFrom::Start(offset))?;
        file.read_to_end(&mut bytes)
    });
    if read.is_err() {
        return Vec::new();
    }
    let complete = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(&bytes[..0], |end| &bytes[..end]);
    String::from_utf8_lossy(complete)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|record| record["type"] == "queue-operation" && record["operation"] == "popAll")
        .filter_map(|record| record["content"].as_str().map(str::to_string))
        .collect()
}

/// Ctrl+U until a capture shows Claude's box empty and out of shell mode.
async fn empty_claude_input(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
) -> Result<(), SessionChatSendError> {
    for _ in 0..CLAUDE_QUEUE_CLEAR_ATTEMPTS {
        let input = capture_session_terminal_text_vt(zmx_name)
            .await
            .and_then(|screen| {
                crate::session_chat_composer::session_chat_composer_input("claude", &screen)
            });
        let clear = match input {
            Some(input) if input.is_empty() && !input.shell_mode => return Ok(()),
            Some(input) if input.is_empty() => Some(CLAUDE_LEAVE_SHELL_MODE.to_string()),
            Some(input) => Some(AGENT_TUI_CLEAR_INPUT_LINE.repeat(
                (input.rows * 2 + AGENT_TUI_CLEAR_LINE_SLACK).min(AGENT_TUI_CLEAR_MAX_LINES),
            )),
            None => None,
        };
        if let Some(clear) = clear {
            write_session_chat_payload(project_id, session_id, zmx_name, source, &clear)
                .await
                .map_err(|message| {
                    SessionChatSendError::new(SessionChatSendFailure::Write, message)
                })?;
        }
        tokio::time::sleep(Duration::from_millis(SESSION_CHAT_CLEAR_INPUT_SETTLE_MS)).await;
    }
    Err(SessionChatSendError::new(
        SessionChatSendFailure::ComposerNotCleared,
        "Claude's queued messages are in its input box in the terminal; Ghostex could not empty it."
            .to_string(),
    ))
}

async fn pull_claude_queue_in_send_queue(
    target: &SessionChatSendTarget,
    transcript: PathBuf,
) -> Result<Vec<String>, SessionChatSendError> {
    let (completion_tx, completion_rx) = oneshot::channel();
    let (draft_tx, draft_rx) = oneshot::channel();
    queue_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        TAKE_BACK_SOURCE,
        vec![SessionChatSendStep::PullClaudeQueue { transcript }],
        Some(completion_tx),
        Some(draft_tx),
        None,
    )
    .map_err(SessionChatSendError::not_attempted)?;
    completion_rx.await.map_err(|_| {
        SessionChatSendError::not_attempted(
            "The session chat send worker stopped before taking the message back.".to_string(),
        )
    })??;
    Ok(draft_rx
        .await
        .map(|draft| draft.queued_messages)
        .unwrap_or_default())
}

/// A queued message's text without its picture references and with whitespace folded, so the
/// chat bubble's copy text and Claude's queue row compare equal.
pub(super) fn take_back_key(text: &str) -> String {
    let (text, _) = split_image_references(text);
    let (text, _) = strip_bare_image_markers(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How many messages per session [`remember_claude_sent_pictures`] keeps.
const SENT_PICTURES_PER_SESSION: usize = 32;

static SENT_PICTURES: OnceLock<Mutex<HashMap<String, Vec<(String, Vec<String>)>>>> =
    OnceLock::new();

/// Remembers the pictures a chat send attached to a Claude message, for a take-back.
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// A picture sent as an attachment (the queue strip, a send that waited for a slow-starting agent) is pasted into Claude as a path, and Claude's queue row records only `[Image #1]` with no path, so the picture could not come back to the composer or be typed into the queue again. The composer's own `[Image #N](path)` text needs none of this. Only the last few messages per session are kept, in memory.
pub(crate) fn remember_claude_sent_pictures(
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
) {
    if image_paths.is_empty() {
        return;
    }
    let Ok(mut map) = SENT_PICTURES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    else {
        return;
    };
    let sent = map.entry(format!("{project_id}|{session_id}")).or_default();
    sent.push((take_back_key(text), image_paths.to_vec()));
    if sent.len() > SENT_PICTURES_PER_SESSION {
        sent.remove(0);
    }
}

/// Gives the bare `[Image #N]` markers in a queue row the paths of the pictures that send carried.
fn restore_sent_pictures(project_id: &str, session_id: &str, message: &str) -> String {
    if strip_bare_image_markers(message).1 == 0 {
        return message.to_string();
    }
    let key = take_back_key(message);
    let paths = SENT_PICTURES.get().and_then(|map| {
        let map = map.lock().ok()?;
        map.get(&format!("{project_id}|{session_id}"))?
            .iter()
            .rev()
            .find(|(sent, _)| *sent == key)
            .map(|(_, paths)| paths.clone())
    });
    let Some(paths) = paths else {
        return message.to_string();
    };
    let mut paths = paths.into_iter();
    let mut restored = String::with_capacity(message.len());
    let mut rest = message;
    while let Some((_, end)) = next_bare_image_marker(rest) {
        restored.push_str(&rest[..end]);
        if let Some(path) = paths.next() {
            restored.push_str(&format!("({path})"));
        }
        rest = &rest[end..];
    }
    restored.push_str(rest);
    restored
}

/// The next `[Image #N]` marker not followed by `(`: its start and end.
fn next_bare_image_marker(text: &str) -> Option<(usize, usize)> {
    const OPEN: &str = "[Image #";
    let mut from = 0;
    while let Some(found) = text[from..].find(OPEN) {
        let start = from + found;
        let after = &text[start + OPEN.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let end = start + OPEN.len() + digits + 1;
        if digits > 0 && after[digits..].starts_with(']') && !text[end..].starts_with('(') {
            return Some((start, end));
        }
        from = start + OPEN.len();
    }
    None
}

/// `text` without its bare `[Image #N]` markers, and how many there were.
fn strip_bare_image_markers(text: &str) -> (String, usize) {
    let mut kept = String::with_capacity(text.len());
    let mut count = 0;
    let mut rest = text;
    while let Some((start, end)) = next_bare_image_marker(rest) {
        kept.push_str(&rest[..start]);
        count += 1;
        rest = &rest[end..];
    }
    kept.push_str(rest);
    (kept, count)
}

/// Splits `[Image #N](path)` references (the form Claude records a pasted picture in, and the
/// form chat's composer writes) out of `text`: the text without them, and their paths in order.
fn split_image_references(text: &str) -> (String, Vec<String>) {
    const OPEN: &str = "[Image #";
    let mut kept = String::with_capacity(text.len());
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let label = after[digits..]
            .strip_prefix('·')
            .unwrap_or(&after[digits..]);
        let reference = label
            .strip_prefix("](")
            .filter(|_| digits > 0)
            .and_then(|destination| {
                let end = destination.find([')', '\n', '\r'])?;
                (destination.as_bytes()[end] == b')').then_some((destination, end))
            });
        match reference {
            Some((destination, end)) => {
                kept.push_str(&rest[..start]);
                let path = destination[..end].trim();
                if !path.is_empty() {
                    paths.push(path.to_string());
                }
                rest = &destination[end + 1..];
            }
            None => {
                kept.push_str(&rest[..start + OPEN.len()]);
                rest = after;
            }
        }
    }
    kept.push_str(rest);
    (kept.trim().to_string(), paths)
}

/// A take-back the agent's screen refused or a write failed: the RPC error the core shows.
fn take_back_failure(
    endpoint_path: String,
    request_id: String,
    error: SessionChatSendError,
) -> RoutedResponse {
    domain_error_response(
        endpoint_path,
        request_id,
        DomainStateError {
            code: if error.composer_not_ready() || error.prompt_waiting() {
                "composerNotReady"
            } else {
                "sessionInputFailed"
            },
            message: error.message,
        },
    )
}

/// `/api/takeBackSessionChatQueuedMessage`: takes a message the agent CLI queued behind its running
/// turn back out of that queue and answers with its text, for the chat composer.
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// Back to composer (the user's decision is in packages/gx-chat-core/src/composer/queue_take_back.rs) must leave the agent with nothing queued for the message and an empty input box before the text is handed back. Claude gives back its whole queue at once, so the other messages are typed into its queue again, oldest first, and any that cannot be are handed back too: nothing is sent twice or lost. When the agent already took the message, `takenBack` is false and nothing is handed back for it. Grok Build deletes just the one message from its queue (grok_queue_pull.rs), and the chat's own text for it is handed back.
pub(crate) async fn handle_take_back_session_chat_queued_message_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let target = match resolve_session_chat_send_target(
        state,
        &params,
        "takeBackSessionChatQueuedMessage",
    ) {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let wanted = take_back_key(&text);
    if wanted.is_empty() && split_image_references(&text).1.is_empty() {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request("takeBackSessionChatQueuedMessage requires text."),
        );
    }
    match session_chat_agent_for_session(&target.session).as_deref() {
        Some("claude") => {}
        Some("grok") => {
            return match take_back_grok_queued_message(&target, text.clone()).await {
                Ok(taken) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(
                        request_id,
                        json!({ "takenBack": taken, "text": if taken { text.trim() } else { "" } }),
                    ),
                ),
                Err(error) => take_back_failure(endpoint_path, request_id, error),
            };
        }
        _ => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError::bad_request(
                    "Only Claude Code's and Grok Build's own queues can give a message back to the chat.",
                ),
            );
        }
    }
    let transcript = crate::session_chat::resolve_session_chat_transcript_agent(Some("claude"))
        .and_then(|agent| {
            crate::session_chat::resolve_session_chat_transcript_path(
                agent,
                read_runtime_text(&target.session, "agentSessionId").as_deref(),
                read_runtime_text(&target.session, "agentSessionPath").as_deref(),
            )
        });
    let Some(transcript) = transcript else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "notFound",
                message: "Claude's transcript could not be found, so nothing was taken back."
                    .to_string(),
            },
        );
    };
    let popped = match pull_claude_queue_in_send_queue(&target, transcript).await {
        Ok(popped) => popped,
        Err(error) => return take_back_failure(endpoint_path, request_id, error),
    };
    let mut popped: Vec<String> = popped
        .iter()
        .map(|message| {
            restore_sent_pictures(
                &target.project_id,
                &target.session_id,
                &crate::session_chat_decode_claude::strip_claude_pasted_content_envelopes(message),
            )
        })
        .collect();
    // Identical queued texts are interchangeable, so the newest one comes back.
    let taken = popped
        .iter()
        .rposition(|message| take_back_key(message) == wanted)
        .map(|index| popped.remove(index));
    let mut handed_back: Vec<String> = taken.iter().cloned().collect();
    for message in popped {
        let (body, images) = split_image_references(&message);
        remember_claude_sent_pictures(&target.project_id, &target.session_id, &body, &images);
        let steps = build_session_chat_message_steps(Some("claude"), &body, &images, false);
        if execute_session_chat_send(
            &target.project_id,
            &target.session_id,
            &target.zmx_name,
            TAKE_BACK_SOURCE,
            steps,
        )
        .await
        .is_err()
        {
            handed_back.push(message);
        }
    }
    let handed_back = handed_back
        .iter()
        .map(|message| message.trim())
        .filter(|message| !message.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({ "takenBack": taken.is_some(), "text": handed_back }),
        ),
    )
}
