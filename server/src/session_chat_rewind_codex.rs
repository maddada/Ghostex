//! CDXC:SessionChat 2026-09-07 DECISION:
//! User: Codex rewind requires Ghostex to report the session idle, clears the input, presses Escape twice quickly, moves Left once per later prompt (the latest starts selected), and presses Enter.
//! CDXC:SessionChat 2026-09-25 WHY:
//! Codex 0.153.4 forks before the selected prompt; 0.156.1 can trim the same thread. Verify the retained prompts before adopting either result through the existing identity writer so chat, pagination and resume agree.
//! The highlight is SGR reverse video, absent from plain captures. CSI-u Escape avoids the terminal parser holding a bare Escape while waiting for another byte.

use super::*;
use crate::domain::DomainRepository;
use crate::session_chat_fork_stitch::read_session_chat_tail_page_stitched;
use crate::session_chat_send::SessionChatSendTarget;
use crate::session_chat_successor::{find_codex_successor_transcript, read_codex_session_meta};
use crate::session_chat_tail::SessionChatTailPage;
use crate::storage::open_gxserver_database;

#[path = "session_chat_rewind_codex_state.rs"]
mod state;
use state::{pending_rewind, write_pending_rewind, RetainedPrompts};

const ESCAPE: &str = "\u{1b}[27u";
const LEFT: &str = "\u{1b}[1;1D";

#[derive(Clone, Debug)]
pub(super) struct CodexRewindPlan {
    paths: crate::paths::GxserverPaths,
    server_id: String,
    transcript_path: PathBuf,
    agent_session_id: String,
    message_id: String,
    pub(super) synchronizing_since: Option<i64>,
    /// Newest first, including inherited prompts, with stable transcript IDs.
    prompts: Vec<(String, String)>,
    retained_prompts: Option<RetainedPrompts>,
}

fn prompts(path: &Path) -> Result<Vec<(String, String)>, DomainStateError> {
    let mut result = Vec::new();
    let mut cursor = None;
    loop {
        let page = read_session_chat_tail_page_stitched(
            SessionChatTranscriptAgent::Codex,
            path,
            1000,
            cursor,
        )
        .map_err(|error| {
            message_not_found(format!("Codex's transcript could not be read: {error}"))
        })?;
        let SessionChatTailPage::Page {
            messages,
            has_more,
            before_offset,
            ..
        } = page.page
        else {
            return Err(message_not_found("Codex's transcript is unavailable."));
        };
        result.extend(
            messages
                .into_iter()
                .rev()
                .filter(|message| message.role == SessionChatRole::User && !message.queued)
                .map(|message| (message.id.clone(), message_text(&message))),
        );
        if !has_more {
            return Ok(result);
        }
        if cursor == Some(before_offset) {
            return Err(message_not_found(
                "Codex's transcript pagination did not advance.",
            ));
        }
        cursor = Some(before_offset);
    }
}

fn idle(session: &Value) -> Result<(), DomainStateError> {
    if crate::presentation::effective_lifecycle_state(session) != "running" {
        return Err(session_not_running("The Codex session is not running."));
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if crate::presentation::presentation_activity(session, &now) == "working" {
        return Err(agent_busy(
            "Codex is still working. Wait for it to finish, or stop it, and then rewind.",
        ));
    }
    Ok(())
}

pub(super) async fn rewind(
    state: &AppState,
    target: SessionChatSendTarget,
    message_id: &str,
) -> Result<Value, DomainStateError> {
    let pending = pending_rewind(&target.session)?;
    if let Some(pending) = &pending {
        if pending.target_message_id != message_id || pending.zmx_name != target.zmx_name {
            return Err(agent_busy("The previous Codex rewind still needs synchronization. Retry it before starting another rewind."));
        }
    } else {
        idle(&target.session)?;
    }
    let transcript_path = if let Some(pending) = &pending {
        pending.previous_transcript_path.clone()
    } else {
        crate::session_chat::resolve_session_chat_transcript_path(
            SessionChatTranscriptAgent::Codex,
            read_runtime_text(&target.session, "agentSessionId").as_deref(),
            read_runtime_text(&target.session, "agentSessionPath").as_deref(),
        )
        .ok_or_else(|| message_not_found("This session has no Codex transcript yet."))?
    };
    let meta = read_codex_session_meta(&transcript_path)
        .ok_or_else(|| message_not_found("Codex's transcript identity could not be read."))?;
    if pending
        .as_ref()
        .is_some_and(|pending| pending.previous_agent_session_id != meta.session_id)
    {
        return Err(agent_busy(
            "The pending rewind's original conversation changed.",
        ));
    }
    let prompts = prompts(&transcript_path)?;
    let target_index = prompts.iter().position(|(id, _)| id == message_id);
    if pending.is_none() && target_index.is_none() {
        return Err(message_not_found(
            "That message is not an active user prompt of this conversation.",
        ));
    }
    let retained_prompts = pending
        .as_ref()
        .and_then(|pending| pending.retained_prompts.clone())
        .or_else(|| target_index.map(|index| RetainedPrompts::new(&prompts[index + 1..])));
    let presses = target_index.unwrap_or(0);
    let first_line = target_index
        .map(|index| prompt_first_line(&prompts[index].1))
        .unwrap_or_default();
    if pending.is_none() && first_line.is_empty() {
        return Err(message_not_found(
            "This prompt has no text to verify in Codex's rewind picker.",
        ));
    }
    let _guard = RewindInFlightGuard::claim(&target.project_id, &target.session_id)
        .ok_or_else(|| agent_busy("A rewind is already running for this session."))?;
    let job_id = register_rewind_job(RewindPlan {
        claude_target: None,
        target_first_line: first_line,
        presses,
        codex: Some(CodexRewindPlan {
            paths: state.paths.clone(),
            server_id: state.metadata.server_id.clone(),
            transcript_path,
            agent_session_id: meta.session_id,
            message_id: message_id.to_string(),
            synchronizing_since: pending.as_ref().map(|pending| pending.started_at),
            prompts,
            retained_prompts,
        }),
    });
    let sent = execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-rewind",
        vec![SessionChatSendStep::DriveSessionChatRewind { job_id }],
    )
    .await;
    let outcome = take_rewind_job_outcome(job_id);
    if sent.is_err() || outcome.as_ref().is_some_and(Result::is_err) {
        let db = open_gxserver_database(&state.paths)
            .map_err(|error| session_not_running(error.to_string()))?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        if repository
            .get_session(&target.project_id, &target.session_id)?
            .map(|session| pending_rewind(&session))
            .transpose()?
            .flatten()
            .is_some()
        {
            return Ok(
                json!({"ok": true, "targetMessageId": message_id, "leafId": null,
                "synchronizationPending": true,
                "warning": "Ghostex could not synchronize the submitted Codex rewind. Retry synchronization to reconnect this chat without rewinding again."}),
            );
        }
    }
    let warning = match outcome.as_ref() {
        Some(Err(error)) if error.code == "rewindCleanupFailed" => Some(error.message.clone()),
        Some(Err(error)) => {
            return Err(DomainStateError {
                code: error.code,
                message: error.message.clone(),
            })
        }
        _ => None,
    };
    sent.map_err(|error| agent_busy(error.message))?;
    if outcome.is_none() {
        return Err(agent_busy(
            "The terminal queue dropped the rewind before it ran.",
        ));
    }
    crate::session_chat_follower::request_session_chat_resnapshot(
        state,
        &target.project_id,
        &target.session_id,
    );
    log_session_chat_rewind(
        LogLevel::Info,
        "sessionChatRewound",
        json!({
            "projectId": target.project_id, "sessionId": target.session_id,
            "targetMessageId": message_id, "agent": "codex",
        }),
        None,
    );
    Ok(json!({"ok": true, "targetMessageId": message_id, "leafId": null, "warning": warning}))
}

fn picker_open(screen: &str) -> bool {
    crate::session_chat_codex_pager::detect_codex_transcript_pager(screen).is_some()
}

/// Extract only characters painted with SGR reverse-video. Codex can leave
/// the prompt glyph dim while reversing the body, including question replies.
fn selected_prompt(screen: &str) -> Option<PromptSelection> {
    if !picker_open(screen) {
        return None;
    }
    let mut reversed = false;
    let mut selected = Vec::new();
    let mut first_row = None;
    let mut first_marker = None;
    let lines: Vec<&str> = screen.lines().collect();
    let end = content_end(&lines)?;
    let mut last_highlighted = None;
    for (row, line) in lines[..end].iter().enumerate() {
        let mut chars = line.chars().peekable();
        let mut text = String::new();
        let mut row_marker = None;
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' && chars.peek() == Some(&'[') {
                chars.next();
                let mut params = String::new();
                for ch in chars.by_ref() {
                    if ('@'..='~').contains(&ch) {
                        if ch == 'm' {
                            for part in params.split(';') {
                                match part {
                                    "" | "0" | "27" => reversed = false,
                                    "7" => reversed = true,
                                    _ => {}
                                }
                            }
                        }
                        break;
                    }
                    params.push(ch);
                }
            } else {
                if row_marker.is_none() && !ch.is_whitespace() {
                    row_marker = Some((ch, reversed));
                }
                if reversed {
                    text.push(ch);
                    last_highlighted = Some(row);
                }
            }
        }
        let text = text.trim();
        if !text.is_empty() {
            if first_row.is_none() {
                first_marker = row_marker;
            }
            first_row.get_or_insert(row);
            selected.push(collapse_spaces(text));
        }
    }
    let (marker @ ('›' | '»'), marker_reversed) = first_marker? else {
        return None;
    };
    let prompt = if marker_reversed {
        selected.first()?.strip_prefix(marker)?.trim()
    } else {
        selected.first()?.trim()
    };
    selected[0] = prompt.to_string();
    Some(PromptSelection {
        row: first_row?,
        lines: selected,
        clipped: last_highlighted == end.checked_sub(1)
            && crate::session_chat_options::strip_ansi_sgr(lines[end]).starts_with("──"),
        viewport: crate::session_chat_options::strip_ansi_sgr(lines[end])
            .trim()
            .to_string(),
    })
}

#[derive(Debug, PartialEq, Eq)]
struct PromptSelection {
    row: usize,
    lines: Vec<String>,
    clipped: bool,
    viewport: String,
}

/// CDXC:SessionChat 2026-09-14 WHY:
/// Codex wraps long tokens and clips long prompts at the pager footer; blank lines are still part of the highlighted prompt.
/// Compare each visible line to the transcript, permitting an unfinished suffix only when the highlight reaches the viewport edge.
/// The pager position also distinguishes repeated prompts that scroll to the same screen row; branch adoption still verifies the retained transcript IDs and text exactly.
impl PromptSelection {
    fn matches(&self, expected: &str) -> bool {
        let expected = collapse_spaces(expected);
        if expected.is_empty() {
            return false;
        }
        let mut rest = expected.as_str();
        for line in &self.lines {
            let Some(next) = rest.strip_prefix(line.as_str()) else {
                return false;
            };
            rest = next.trim_start();
        }
        rest.is_empty() || self.clipped
    }
}

fn content_end(lines: &[&str]) -> Option<usize> {
    let footer = lines.iter().rposition(|line| {
        crate::session_chat_codex_pager::transcript_pager_footer(line).is_some()
    })?;
    Some(
        lines[..footer]
            .iter()
            .rposition(|line| crate::session_chat_options::strip_ansi_sgr(line).starts_with("──"))
            .unwrap_or(footer),
    )
}

/// Opening the picker always selects the newest prompt. Codex 0.153.4 can
/// omit reverse video on that first paint after a fork, although Left/Right
/// paints it correctly. Verify the newest visible prompt on open; use the
/// actual highlight for every subsequent navigation step.
fn initial_prompt(screen: &str) -> Option<PromptSelection> {
    if !picker_open(screen) {
        return None;
    }
    if let Some(selected) = selected_prompt(screen) {
        return Some(selected);
    }
    let raw: Vec<&str> = screen.lines().collect();
    let end = content_end(&raw)?;
    let lines: Vec<String> = screen
        .lines()
        .take(end)
        .map(crate::session_chat_options::strip_ansi_sgr)
        .collect();
    let row = lines
        .iter()
        .rposition(|line| line.starts_with('›') || line.starts_with('»'))?;
    let first = lines[row].trim_start_matches(['›', '»']).trim();
    let mut selected = vec![collapse_spaces(first)];
    let mut clipped = true;
    for line in &lines[row + 1..] {
        // User continuation lines are indented; assistant/history markers
        // start at column zero, including after an empty user line.
        if line.starts_with(['•', '›', '»']) {
            clipped = false;
            break;
        }
        selected.push(collapse_spaces(line));
    }
    Some(PromptSelection {
        row,
        lines: selected,
        clipped: clipped
            && lines.last().is_some_and(|line| !line.trim().is_empty())
            && crate::session_chat_options::strip_ansi_sgr(raw[end]).starts_with("──"),
        viewport: crate::session_chat_options::strip_ansi_sgr(raw[end])
            .trim()
            .to_string(),
    })
}

async fn capture_vt(driver: &RewindDriver<'_>) -> Option<String> {
    let name = driver.zmx_name.to_string();
    let capture =
        tokio::task::spawn_blocking(move || crate::zmx::read_zmx_session_screen_capture_vt(&name))
            .await
            .ok()?
            .ok()?;
    (!capture.truncated).then_some(capture.text)
}

/// CDXC:SessionChat 2026-09-12 WHY:
/// Loading older Codex history moves the current highlight to another screen row before the requested prompt is selected.
/// Wait for the expected prompt and for history loading to finish; a changed screen position alone cannot prove a Left press completed.
async fn selection(
    driver: &RewindDriver<'_>,
    previous: Option<&PromptSelection>,
    expected: &str,
) -> Result<PromptSelection, DomainStateError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(6);
    let expected = collapse_spaces(expected);
    let mut mismatched = false;
    loop {
        if (driver.cancelled)() {
            return Err(agent_busy("The rewind was cancelled by another action."));
        }
        if let Some(screen) = capture_vt(driver).await {
            let loading = screen.lines().next().is_some_and(|header| {
                crate::session_chat_options::strip_ansi_sgr(header)
                    .contains("loading older history")
            });
            let selected = if previous.is_none() {
                initial_prompt(&screen)
            } else {
                selected_prompt(&screen)
            };
            if let Some(selected) = selected {
                mismatched = !selected.matches(&expected);
                if !loading && !mismatched && previous != Some(&selected) {
                    return Ok(selected);
                }
            }
        }
        if std::time::Instant::now() >= deadline {
            if mismatched {
                return Err(dialog_mismatch(
                    "selection",
                    "The highlighted prompt does not match the transcript.",
                ));
            }
            return Err(rewind_timeout("selection"));
        }
        tokio::time::sleep(Duration::from_millis(REWIND_POLL_MS)).await;
    }
}

fn live_session(
    driver: &RewindDriver<'_>,
    plan: &CodexRewindPlan,
) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&plan.paths)
        .map_err(|error| session_not_running(error.to_string()))?;
    DomainRepository::new(&db, &plan.server_id)
        .get_session(driver.project_id, driver.session_id)?
        .ok_or_else(|| session_not_running("The session no longer exists."))
}

pub(super) async fn drive(
    driver: &RewindDriver<'_>,
    plan: &RewindPlan,
    codex: &CodexRewindPlan,
) -> Result<(), DomainStateError> {
    if let Some(started) = codex.synchronizing_since {
        adopt_branch(driver, plan, codex, started).await?;
        return write_pending_rewind(driver, codex, None);
    }
    let started_at_composer = driver
        .capture()
        .await
        .is_some_and(|screen| !picker_open(&screen));
    let result = drive_inner(driver, plan, codex).await;
    if started_at_composer && result.is_err() {
        if let Some(footer) = driver
            .capture()
            .await
            .and_then(|screen| crate::session_chat_codex_pager::transcript_pager_footer(&screen))
        {
            let cancel = if footer.ends_with("esc back") {
                ESCAPE
            } else {
                "q"
            };
            let _ = driver.write(cancel).await;
        }
    }
    result
}

async fn drive_inner(
    driver: &RewindDriver<'_>,
    plan: &RewindPlan,
    codex: &CodexRewindPlan,
) -> Result<(), DomainStateError> {
    let session = live_session(driver, codex)?;
    idle(&session)?;
    if read_runtime_text(&session, "agentSessionId").as_deref() != Some(&codex.agent_session_id)
        || prompts(&codex.transcript_path)? != codex.prompts
    {
        return Err(agent_busy(
            "The conversation changed while the rewind was queued. Try again.",
        ));
    }
    let started = drive_picker(
        driver,
        plan,
        codex,
        || {
            let live = live_session(driver, codex)?;
            idle(&live)?;
            if read_runtime_text(&live, "agentSessionId").as_deref()
                != Some(&codex.agent_session_id)
            {
                return Err(agent_busy("The conversation changed while rewinding."));
            }
            Ok(())
        },
        |started| write_pending_rewind(driver, codex, Some(started)),
    )
    .await?;
    adopt_branch(driver, plan, codex, started).await?;
    write_pending_rewind(driver, codex, None)
}

async fn drive_picker(
    driver: &RewindDriver<'_>,
    plan: &RewindPlan,
    codex: &CodexRewindPlan,
    check_idle: impl Fn() -> Result<(), DomainStateError>,
    record_submission: impl Fn(i64) -> Result<(), DomainStateError>,
) -> Result<i64, DomainStateError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let screen = driver
            .capture()
            .await
            .ok_or_else(|| session_not_running("Codex's screen could not be read."))?;
        let ready = crate::session_chat_composer::detect_session_chat_composer_readiness(
            Some("codex"),
            &screen,
            None,
        );
        if picker_open(&screen)
            || ready.state != crate::session_chat_composer::SessionChatComposerState::Ready
        {
            return Err(agent_busy(
                "Codex is not showing its input box. Close the terminal dialog first.",
            ));
        }
        // A new branch starts its MCP connections again while already painting
        // the composer. Escape interrupts that startup instead of priming rewind.
        let starting =
            screen.lines().rev().take(8).any(|line| {
                line.contains("esc to interrupt") || line.contains("tab to queue message")
            });
        if !starting {
            break;
        }
        if (driver.cancelled)() || std::time::Instant::now() >= deadline {
            return Err(agent_busy(
                "Codex is still starting its connections. Wait for it to finish before rewinding.",
            ));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    check_idle()?;
    if (driver.cancelled)() {
        return Err(agent_busy("The rewind was cancelled."));
    }
    driver
        .write(&crate::session_chat_send::build_agent_tui_clear_input(
            crate::session_chat_send::AGENT_TUI_CLEAR_MAX_LINES,
        ))
        .await?;
    tokio::time::sleep(Duration::from_millis(
        crate::session_chat_send::SESSION_CHAT_CLEAR_INPUT_SETTLE_MS,
    ))
    .await;
    // Opening the preview is Codex's own empty-input proof: it ignores this
    // shortcut while a draft remains. Never submit until the preview is proven.
    driver.write(ESCAPE).await?;
    tokio::time::sleep(Duration::from_millis(120)).await;
    driver.write(ESCAPE).await?;
    let mut selected = selection(driver, None, &codex.prompts[0].1).await?;
    for index in 1..=plan.presses {
        driver.write(LEFT).await?;
        selected = selection(driver, Some(&selected), &codex.prompts[index].1).await?;
    }
    check_idle()?;
    if (driver.cancelled)() {
        return Err(agent_busy("The rewind was cancelled."));
    }
    let started = chrono::Utc::now().timestamp_millis();
    record_submission(started)?;
    driver.write("\r").await?;
    driver
        .wait_for("close", |screen| (!picker_open(screen)).then_some(()))
        .await?;
    Ok(started)
}

async fn adopt_branch(
    driver: &RewindDriver<'_>,
    plan: &RewindPlan,
    codex: &CodexRewindPlan,
    started: i64,
) -> Result<(), DomainStateError> {
    let next = wait_for_branch(driver, plan, codex, started).await?;
    let name = driver.zmx_name.to_string();
    let home_dir = codex.paths.home_dir.clone();
    let live = tokio::task::spawn_blocking(move || {
        crate::zmx::read_zmx_session_process_identities(&[name.clone()], &home_dir)
            .ok()?
            .remove(&name)
    })
    .await
    .map_err(|error| session_not_running(error.to_string()))?
    .filter(|identity| identity.agent_id.as_deref() == Some("codex"))
    .ok_or_else(|| session_not_running("Codex's process identity could not be verified."))?;
    if let Some(path) = &next.path {
        if live.agent_session_id.as_deref() != Some(&next.agent_session_id)
            || live.agent_session_path.as_deref().map(Path::new) != Some(path.as_path())
        {
            return Err(agent_busy(
                "Codex changed conversations before synchronization finished.",
            ));
        }
    }
    let process_id = live
        .process_id
        .ok_or_else(|| session_not_running("Codex's process could not be identified."))?;
    let db = open_gxserver_database(&codex.paths)
        .map_err(|error| session_not_running(error.to_string()))?;
    let repository = DomainRepository::new(&db, &codex.server_id);
    if crate::agents::apply_verified_rewind_session_identity(
        &repository,
        driver.project_id,
        driver.session_id,
        &codex.agent_session_id,
        &next.agent_session_id,
        &next
            .path
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        process_id,
    )? {
        crate::zmx::invalidate_zmx_process_identity_cache();
        return Ok(());
    }
    return Err(agent_busy(
        "Codex rewound, but Ghostex could not synchronize its conversation identity.",
    ));
}

struct RewoundConversation {
    agent_session_id: String,
    path: Option<PathBuf>,
}

/// Codex creates an entirely fresh, unnamed thread before the first prompt;
/// its rollout is intentionally not written until that prompt is submitted.
/// The live footer supplies its UUID during this pre-transcript state.
fn empty_conversation_id(screen: &str, old_id: &str) -> Option<String> {
    let footer = screen.lines().rev().find(|line| !line.trim().is_empty())?;
    footer.split('·').map(str::trim).find_map(|part| {
        uuid::Uuid::parse_str(part)
            .ok()
            .filter(|_| part != old_id)
            .map(|_| part.to_string())
    })
}

/// CDXC:SessionChat 2026-09-25 WHY:
/// Codex restores the draft even when it rejects a mid-turn steer before reverting anything. Require both that explicit rejection and the target's position inside its persisted turn before releasing the pending request; a restored composer alone proves no rewind.
fn rejected_steer(path: &Path, message_id: &str, screen: &str) -> bool {
    use std::io::BufRead;
    if !collapse_spaces(&crate::session_chat_options::strip_ansi_sgr(screen)).contains(
        "Failed to edit the selected prompt: the selected prompt is a steer and cannot be edited independently",
    ) {
        return false;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut turns = std::collections::HashSet::new();
    for line in std::io::BufReader::new(file).lines() {
        let Ok(line) = line else { return false };
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = &record["payload"];
        if record["type"] != "event_msg"
            || payload["type"] != "item_completed"
            || payload["item"]["type"] != "UserMessage"
        {
            continue;
        }
        let Some(turn_id) = payload["turn_id"].as_str() else {
            continue;
        };
        let first_in_turn = turns.insert(turn_id.to_string());
        if payload["item"]["id"].as_str() == Some(message_id) {
            return !first_in_turn;
        }
    }
    false
}

async fn wait_for_branch(
    driver: &RewindDriver<'_>,
    _plan: &RewindPlan,
    codex: &CodexRewindPlan,
    started: i64,
) -> Result<RewoundConversation, DomainStateError> {
    crate::zmx::invalidate_zmx_process_identity_cache();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let name = driver.zmx_name.to_string();
        let home = codex.paths.home_dir.clone();
        let live = tokio::task::spawn_blocking(move || {
            crate::zmx::read_zmx_session_process_identities(&[name.clone()], &home)
                .ok()?
                .remove(&name)
        })
        .await
        .map_err(|error| session_not_running(error.to_string()))?;
        if let Some(path) = live
            .as_ref()
            .filter(|live| live.agent_session_id.as_deref() == Some(&codex.agent_session_id))
            .and_then(|live| live.agent_session_path.as_deref())
            .map(PathBuf::from)
            .filter(|path| path != &codex.transcript_path)
        {
            if read_codex_session_meta(&path)
                .is_some_and(|meta| meta.session_id == codex.agent_session_id)
                && codex.retained_prompts.as_ref() == Some(&RetainedPrompts::new(&prompts(&path)?))
            {
                return Ok(RewoundConversation {
                    agent_session_id: codex.agent_session_id.clone(),
                    path: Some(path),
                });
            }
        }
        let current_prompts = prompts(&codex.transcript_path)?;
        let same_thread_rewound = codex
            .retained_prompts
            .as_ref()
            .is_some_and(|expected| RetainedPrompts::new(&current_prompts) == *expected);
        if same_thread_rewound
            && read_codex_session_meta(&codex.transcript_path)
                .is_some_and(|meta| meta.session_id == codex.agent_session_id)
        {
            return Ok(RewoundConversation {
                agent_session_id: codex.agent_session_id.clone(),
                path: Some(codex.transcript_path.clone()),
            });
        }
        if current_prompts
            .iter()
            .any(|(id, _)| id == &codex.message_id)
        {
            if let Some(screen) = driver.capture().await {
                if rejected_steer(&codex.transcript_path, &codex.message_id, &screen) {
                    write_pending_rewind(driver, codex, None)?;
                    return Err(DomainStateError {
                        code: "rewindRejected",
                        message: "Codex cannot rewind a message sent during an existing turn independently. Choose the first prompt of that turn instead. Nothing was rewound; the restored draft has not been submitted.".into(),
                    });
                }
            }
        }
        if codex
            .retained_prompts
            .as_ref()
            .is_some_and(|expected| expected.count == 0)
        {
            if let Some(screen) = driver.capture().await {
                if let Some(agent_session_id) =
                    empty_conversation_id(&screen, &codex.agent_session_id)
                {
                    return Ok(RewoundConversation {
                        agent_session_id,
                        path: None,
                    });
                }
            }
        }
        let old = codex.agent_session_id.clone();
        let path = codex.transcript_path.clone();
        let zmx_name = driver.zmx_name.to_string();
        let home_dir = codex.paths.home_dir.clone();
        let candidate = tokio::task::spawn_blocking(move || {
            let candidate = find_codex_successor_transcript(&old, &path, started, &[]);
            if let crate::session_chat::SessionChatSuccessorOutcome::Found(next) = &candidate {
                let identities =
                    crate::zmx::read_zmx_session_process_identities(&[zmx_name.clone()], &home_dir)
                        .ok()?;
                let live = identities.get(&zmx_name)?;
                if live.agent_session_id.as_deref() != Some(&next.agent_session_id)
                    || live.agent_session_path.as_deref().map(Path::new)
                        != Some(next.path.as_path())
                {
                    return None;
                }
            }
            Some(candidate)
        })
        .await
        .map_err(|error| session_not_running(error.to_string()))?;
        if let Some(crate::session_chat::SessionChatSuccessorOutcome::Found(next)) = candidate {
            if codex.retained_prompts.as_ref() != Some(&RetainedPrompts::new(&prompts(&next.path)?))
            {
                return Err(dialog_mismatch(
                    "branch",
                    "Codex's new conversation does not end before the selected prompt.",
                ));
            }
            return Ok(RewoundConversation {
                agent_session_id: next.agent_session_id,
                path: Some(next.path),
            });
        }
        if std::time::Instant::now() >= deadline {
            return Err(DomainStateError { code: "timeout", message: format!("Codex did not confirm the retained conversation for prompt {}. Inspect the terminal before retrying.", codex.message_id) });
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighted_prompt_requires_reverse_video_and_edit_footer() {
        let screen = "› ordinary history\n\x1b[7m› selected first line\x1b[0m\n\x1b[7m  selected second line\x1b[0m\nq close   esc / ← to edit prev   → to edit next   enter to edit message\x1b[0m\x1b[23;73H";
        let selected = selected_prompt(screen).unwrap();
        assert_eq!(selected.row, 1);
        assert!(selected.matches("selected first line\nselected second line"));
        assert!(!selected.matches("selected first line\nselected second line extra"));
        assert_eq!(selected_prompt("\x1b[7m› selected\x1b[0m"), None);
        assert_eq!(selected_prompt(&screen.replace("\x1b[7m", "")), None);
    }

    #[test]
    fn initial_selection_is_the_latest_prompt_even_without_highlight_paint() {
        let screen = "› first prompt\n\n• first answer\n\n› latest prompt\nwrapped line\n\n• latest answer\nq to quit   esc/← to edit prev   → to edit next   enter to edit message";
        let selected = initial_prompt(screen).unwrap();
        assert_eq!(selected.row, 4);
        assert!(selected.matches("latest prompt wrapped line"));
        assert_eq!(
            initial_prompt(&screen.replace("› latest", "\u{1b}[0m\u{1b}[1m› \u{1b}[0mlatest"))
                .unwrap(),
            selected
        );
        assert_eq!(initial_prompt("› composer draft"), None);
    }

    #[test]
    fn blank_lines_and_wrapped_tokens_keep_the_full_prompt_verifiable() {
        let screen = "\x1b[7m› first\x1b[0m\n\x1b[7m  \x1b[0m\n\x1b[7m  café 中文 🙂 abcdef\x1b[0m\n\x1b[7m  ghijkl\x1b[0m\n\n• answer\n── 100% ─\nq close   esc/← to edit prev   → to edit next   enter to edit message";
        let expected = "first\n\ncafé 中文 🙂 abcdefghijkl";
        let selected = initial_prompt(screen).unwrap();
        assert!(selected.matches(expected));
        assert!(!selected.matches("first"));
        assert!(!selected.matches(&format!("{expected} more")));
        assert!(initial_prompt(&screen.replace("\x1b[7m", ""))
            .unwrap()
            .matches(expected));
    }

    #[test]
    fn only_viewport_clipping_allows_a_partial_prompt() {
        let screen = "\x1b[7m› first\x1b[0m\n\x1b[7m  \x1b[0m\n\x1b[7m  second\x1b[0m\n── 84% ─\nq close   esc/← to edit prev   → to edit next   enter to edit message";
        let selected = initial_prompt(screen).unwrap();
        assert!(selected.matches("first\n\nsecond\nthird"));
        assert!(!selected.matches("first\n\ndifferent\nthird"));
        assert!(!initial_prompt(&screen.replace("── 84%", "\n── 84%"))
            .unwrap()
            .matches("first second third"));
        assert_ne!(
            selected_prompt(screen),
            selected_prompt(&screen.replace("84%", "72%"))
        );
        assert_eq!(
            selected_prompt(screen),
            selected_prompt(&screen.replace("\nq close", "\n q close"))
        );
    }

    #[test]
    fn working_session_cannot_rewind() {
        let mut session = json!({"lifecycleState": "running", "runtimeSettings": {
            "agentActivity": {"activity": "working", "workingSource": "explicit", "agentName": "codex"}
        }});
        assert_eq!(idle(&session).unwrap_err().code, "agentBusy");
        session["runtimeSettings"]["agentActivity"]["activity"] = json!("idle");
        assert!(idle(&session).is_ok());
        session["lifecycleState"] = json!("stopped");
        assert_eq!(idle(&session).unwrap_err().code, "sessionNotRunning");
    }

    #[test]
    fn empty_rewind_identity_drops_the_previous_transcript_path() {
        use crate::agents::{ResolvedIdentity, SessionIdentityUpdateSource};
        let previous = ResolvedIdentity {
            agent_id: Some("codex".into()),
            agent_session_id: Some("old".into()),
            agent_session_path: Some("old.jsonl".into()),
        };
        let observed = ResolvedIdentity {
            agent_id: Some("codex".into()),
            agent_session_id: Some("new".into()),
            agent_session_path: None,
        };
        let next = crate::agents::merge_observed_session_identity(&observed, &previous);
        assert!(next.agent_session_path.is_none());
        let runtime = crate::agents::apply_session_identity_runtime_settings(
            &previous,
            &next,
            json!({"agentSessionId": "old", "agentSessionPath": "old.jsonl"})
                .as_object()
                .unwrap()
                .clone(),
            SessionIdentityUpdateSource::Passive,
            Some("codex".into()),
        );
        assert_eq!(runtime.get("agentSessionId"), Some(&json!("new")));
        assert!(!runtime.contains_key("agentSessionPath"));
    }

    #[tokio::test]
    #[ignore = "Requires an explicitly disposable idle Codex zmx session and rollout path"]
    async fn live_codex_rewind_latest_then_oldest() {
        let name = std::env::var("GHOSTEX_CODEX_REWIND_TEST_ZMX").expect("disposable session");
        let mut path =
            PathBuf::from(std::env::var("GHOSTEX_CODEX_REWIND_TEST_ROLLOUT").expect("rollout"));
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        crate::storage::initialize_gxserver_storage(&paths).unwrap();
        let db = open_gxserver_database(&paths).unwrap();
        let repository = DomainRepository::new(&db, "rewind-test");
        let project = repository
            .create_project(
                json!({"name": "Rewind test", "path": temp.path()})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        let project_id = project["projectId"].as_str().unwrap();
        let session = repository.create_session(json!({
            "projectId": project_id, "agentId": "codex", "kind": "agent",
            "runtimeSettings": {"agentName": "codex", "agentSessionId": read_codex_session_meta(&path).unwrap().session_id, "agentSessionPath": path},
        }).as_object().unwrap(), false).unwrap();
        let session_id = session["sessionId"].as_str().unwrap();
        let driver = RewindDriver {
            project_id,
            session_id,
            zmx_name: &name,
            source: "codex-rewind-live-test",
            cancelled: &|| false,
        };
        for latest in [true, false] {
            let rows = prompts(&path).unwrap();
            assert!(
                rows.len() >= 2,
                "seed at least three prompts before running"
            );
            let presses = if latest { 0 } else { rows.len() - 1 };
            let meta = read_codex_session_meta(&path).unwrap();
            let codex = CodexRewindPlan {
                paths: paths.clone(),
                server_id: "rewind-test".into(),
                transcript_path: path.clone(),
                agent_session_id: meta.session_id,
                message_id: rows[presses].0.clone(),
                synchronizing_since: None,
                retained_prompts: Some(RetainedPrompts::new(&rows[presses + 1..])),
                prompts: rows,
            };
            let plan = RewindPlan {
                codex: None,
                claude_target: None,
                target_first_line: prompt_first_line(&codex.prompts[presses].1),
                presses,
            };
            // Start with an unsent multiline draft, including the prompt the
            // previous rewind restored. The production clear must remove it.
            driver
                .write(
                    &crate::session_chat_send::wrap_terminal_bracketed_paste_text(
                        "unsent draft\nsecond line",
                    ),
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            let started = drive_picker(&driver, &plan, &codex, || Ok(()), |_| Ok(()))
                .await
                .unwrap();
            let next = wait_for_branch(&driver, &plan, &codex, started)
                .await
                .unwrap();
            let current = repository
                .get_session(project_id, session_id)
                .unwrap()
                .unwrap();
            let mut runtime = current["runtimeSettings"].as_object().unwrap().clone();
            runtime.insert(
                "sessionChatPendingCodexRewind".into(),
                json!(state::PendingCodexRewind {
                    previous_agent_session_id: codex.agent_session_id.clone(),
                    previous_transcript_path: codex.transcript_path.clone(),
                    target_message_id: codex.message_id.clone(),
                    started_at: started,
                    zmx_name: name.clone(),
                    retained_prompts: codex.retained_prompts.clone(),
                }),
            );
            repository.update_session(json!({"projectId": project_id, "sessionId": session_id, "runtimeSettings": runtime}).as_object().unwrap()).unwrap();
            let mut retry_codex = codex.clone();
            retry_codex.synchronizing_since = Some(started);
            let retry_plan = RewindPlan {
                codex: Some(retry_codex),
                ..plan.clone()
            };
            driver.run(&retry_plan).await.unwrap();
            let stored = repository
                .get_session(project_id, session_id)
                .unwrap()
                .unwrap();
            assert_eq!(
                read_runtime_text(&stored, "agentSessionId").as_deref(),
                Some(next.agent_session_id.as_str())
            );
            assert_eq!(
                read_runtime_text(&stored, "agentSessionPath")
                    .as_deref()
                    .map(PathBuf::from),
                next.path
            );
            assert!(pending_rewind(&stored).unwrap().is_none());
            adopt_branch(&driver, &plan, &codex, started).await.unwrap();
            if let Some(next_path) = next.path {
                assert_eq!(prompts(&next_path).unwrap(), codex.prompts[presses + 1..]);
                path = next_path;
            } else {
                assert_eq!(presses + 1, codex.prompts.len());
            }
            eprintln!(
                "Verified rewind with {presses} Left presses: {}",
                next.agent_session_id
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}
