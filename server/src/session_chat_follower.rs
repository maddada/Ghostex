use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::DomainRepository;
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::{
    first_prompt_agent_name, normalize_agent_name, read_runtime_text, read_session_text,
    session_observer_key, AppState, SessionChatFollowerEntry,
};
use crate::session_chat::*;
use crate::session_chat_options::{forget_session_chat_options, SessionChatOptionDetector};
use crate::session_chat_paths::resolve_session_chat_transcript_path;
use crate::session_chat_successor::{
    codex_rollout_session_id, find_claude_successor_transcript, find_codex_successor_transcript,
    is_uuid_transcript_stem, last_codex_record_timestamp_ms,
    last_substantive_transcript_timestamp_ms, SessionChatSuccessorOutcome,
};
use crate::storage::open_gxserver_database;

pub(crate) struct FollowerFileState {
    incremental: SessionChatIncrementalState,
    watched_version: Option<TranscriptFileVersion>,
    watched_boundary: String,
}

impl FollowerFileState {
    pub(crate) fn new() -> Self {
        Self {
            incremental: SessionChatIncrementalState::new(),
            watched_version: None,
            watched_boundary: String::new(),
        }
    }
}

pub(crate) enum FollowerDrainOutcome {
    /// stat/read failed — the path is gone; return to resolve-poll.
    Missing,
    Idle,
    Snapshot {
        tail: SessionChatTailFileResult,
        appended: Vec<SessionChatMessage>,
        appended_lifecycle: Option<SessionChatTurnLifecycle>,
        content_replaced: bool,
    },
    Appended {
        batches: Vec<Vec<SessionChatMessage>>,
        lifecycle: Option<SessionChatTurnLifecycle>,
        /// Prompts published by an earlier drain that this one proved
        /// abandoned (see `superseded_prompt_id`).
        superseded: Vec<String>,
        /// The recorded text of an API refusal row inside this drain's
        /// appended window (CDXC:SessionChatApiRefusal), for the notice card.
        api_refusal: Option<String>,
    },
}

pub(crate) fn follower_drain_once(
    file_path: &Path,
    limit: usize,
    agent: SessionChatTranscriptAgent,
    decode: SessionChatLineDecoder,
    decode_lifecycle: Option<SessionChatLifecycleDecoder>,
    state: &mut FollowerFileState,
    want_snapshot: bool,
) -> FollowerDrainOutcome {
    let lineage = session_chat_lineage_extractor(agent);
    // Hermes's transcript is a mirror of its SQLite rows; freshen it before the
    // generic file logic reads it so each tick sees the latest turn state. An
    // in-place rewind rewrite swaps the inode, which the identity check below
    // reports as `content_replaced`.
    if agent == SessionChatTranscriptAgent::Hermes {
        crate::session_chat_hermes::sync_hermes_transcript_mirror_for_path(file_path);
    }
    let Ok(current) = read_transcript_file_version(file_path) else {
        return FollowerDrainOutcome::Missing;
    };
    let current_boundary =
        boundary_fingerprint(file_path, state.incremental.offset).unwrap_or_default();
    let identity_changed = state
        .watched_version
        .as_ref()
        .is_some_and(|watched| watched.identity != current.identity);
    let same_size_version_changed = state.watched_version.as_ref().is_some_and(|watched| {
        watched.identity == current.identity && watched.size == current.size && *watched != current
    });
    let content_replaced = identity_changed
        || same_size_version_changed
        || current.size < state.incremental.offset
        || (state.incremental.offset > 0 && state.watched_boundary != current_boundary);
    if agent == SessionChatTranscriptAgent::Pi {
        if !want_snapshot && !content_replaced && current.size == state.incremental.offset {
            state.watched_version = Some(current);
            return FollowerDrainOutcome::Idle;
        }
        let Ok(tail) = read_pi_session_chat_transcript_tail_file(file_path, limit, false, None)
        else {
            return FollowerDrainOutcome::Missing;
        };
        state.incremental.rebase(tail.consumed_to);
        state.watched_boundary =
            boundary_fingerprint(file_path, state.incremental.offset).unwrap_or_default();
        state.watched_version = read_transcript_file_version(file_path)
            .ok()
            .or(Some(current));
        return FollowerDrainOutcome::Snapshot {
            tail,
            appended: Vec::new(),
            appended_lifecycle: None,
            content_replaced,
        };
    }
    if content_replaced {
        state.incremental.reset();
    }

    let outcome = if want_snapshot || content_replaced {
        match read_session_chat_transcript_tail_file(
            file_path,
            limit,
            decode,
            false,
            None,
            decode_lifecycle,
            lineage,
        ) {
            Err(_) => return FollowerDrainOutcome::Missing,
            Ok(tail) => {
                state.incremental.rebase(tail.consumed_to);
                state
                    .incremental
                    .seed_queued_prompts(tail.outstanding_queued_prompts.clone());
                // Pick up anything written after consumed_to before we settle.
                let mut appended_lifecycle: Option<SessionChatTurnLifecycle> = None;
                let mut capture_lifecycle =
                    |next: SessionChatTurnLifecycle| appended_lifecycle = Some(next);
                let capture_lifecycle: &mut dyn FnMut(SessionChatTurnLifecycle) =
                    &mut capture_lifecycle;
                let mut appended = read_incremental_transcript_messages(
                    file_path,
                    &mut state.incremental,
                    decode,
                    None,
                    decode_lifecycle,
                    Some(capture_lifecycle),
                    lineage,
                )
                .unwrap_or_default();
                // The window itself is already filtered, so anything the
                // trailing read retracts can only be inside `appended`.
                let superseded = state.incremental.take_superseded_prompt_ids();
                if !superseded.is_empty() {
                    appended.retain(|message| !superseded.contains(&message.id));
                }
                FollowerDrainOutcome::Snapshot {
                    tail,
                    appended,
                    appended_lifecycle,
                    content_replaced,
                }
            }
        }
    } else if current.size != state.incremental.offset {
        let appended_from = state.incremental.offset;
        let mut batches: Vec<Vec<SessionChatMessage>> = Vec::new();
        let mut lifecycle: Option<SessionChatTurnLifecycle> = None;
        let mut push_batch = |batch: Vec<SessionChatMessage>| batches.push(batch);
        let push_batch: &mut dyn FnMut(Vec<SessionChatMessage>) = &mut push_batch;
        let mut capture_lifecycle = |next: SessionChatTurnLifecycle| lifecycle = Some(next);
        let capture_lifecycle: &mut dyn FnMut(SessionChatTurnLifecycle) = &mut capture_lifecycle;
        match read_incremental_transcript_messages(
            file_path,
            &mut state.incremental,
            decode,
            Some(push_batch),
            decode_lifecycle,
            Some(capture_lifecycle),
            lineage,
        ) {
            Err(_) => return FollowerDrainOutcome::Missing,
            Ok(remaining) => {
                if !remaining.is_empty() {
                    batches.push(remaining);
                }
                // A prompt abandoned inside this same drain never reaches a
                // client, so it is dropped from the batch instead of being
                // published and retracted in the same breath.
                let mut superseded = state.incremental.take_superseded_prompt_ids();
                if !superseded.is_empty() {
                    let abandoned: HashSet<String> = superseded.iter().cloned().collect();
                    let mut removed_before_publishing: HashSet<String> = HashSet::new();
                    for batch in batches.iter_mut() {
                        batch.retain(|message| {
                            if abandoned.contains(&message.id) {
                                removed_before_publishing.insert(message.id.clone());
                                return false;
                            }
                            true
                        });
                    }
                    batches.retain(|batch| !batch.is_empty());
                    // Only ids an EARLIER drain already published have to be
                    // retracted; the rest never reached a client.
                    superseded.retain(|id| !removed_before_publishing.contains(id));
                }
                /*
                CDXC:SessionChatApiRefusal 2026-08-28:
                The decoded batch renders the refusal row as ordinary assistant
                text; the structured fields that PROVE it is a refusal never
                survive decoding, so the freshly appended window is re-read
                once and scanned raw. Appended rows only — a snapshot re-reads
                history, and resurrecting an old refusal as a fresh card on
                every subscribe would be noise.
                */
                let api_refusal = (agent == SessionChatTranscriptAgent::Claude)
                    .then(|| {
                        scan_claude_api_refusal(file_path, appended_from, state.incremental.offset)
                    })
                    .flatten();
                if batches.is_empty()
                    && lifecycle.is_none()
                    && superseded.is_empty()
                    && api_refusal.is_none()
                {
                    FollowerDrainOutcome::Idle
                } else {
                    FollowerDrainOutcome::Appended {
                        batches,
                        lifecycle,
                        superseded,
                        api_refusal,
                    }
                }
            }
        }
    } else {
        FollowerDrainOutcome::Idle
    };

    state.watched_boundary =
        boundary_fingerprint(file_path, state.incremental.offset).unwrap_or_default();
    match read_transcript_file_version(file_path) {
        // A write raced the drain: keep the start version so the next 1s
        // reconcile observes the difference and drains again.
        Ok(completed) if completed == current => state.watched_version = Some(completed),
        _ => state.watched_version = Some(current),
    }
    outcome
}

/// Cap on the refusal re-read. A drain window is normally one reconcile tick
/// (~1s) of writes; the refusal row itself is small and ends its turn, so the
/// NEWEST bytes are kept when the window somehow exceeds the cap.
const API_REFUSAL_SCAN_LIMIT_BYTES: u64 = 1024 * 1024;

/// BLOCKING (runs on the drain's blocking task). Scans the appended byte
/// window `[from, to)` for a Claude API refusal row; the LAST one wins.
fn scan_claude_api_refusal(path: &Path, from: u64, to: u64) -> Option<String> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    if to <= from {
        return None;
    }
    let start = from.max(to.saturating_sub(API_REFUSAL_SCAN_LIMIT_BYTES));
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buffer: Vec<u8> = Vec::new();
    file.take(to - start).read_to_end(&mut buffer).ok()?;
    let window = String::from_utf8_lossy(&buffer);
    let mut refusal: Option<String> = None;
    // A capped window starts mid-line; that first partial line just fails to
    // parse and contributes nothing.
    for line in window.lines() {
        if let Some(text) = crate::session_chat_decode_claude::claude_api_refusal_text(line) {
            refusal = Some(text);
        }
    }
    refusal
}

fn session_chat_frame(
    config: &SessionChatFollowerConfig,
    frame_type: &str,
    epoch: i64,
    seq: i64,
) -> Map<String, Value> {
    let mut frame = Map::new();
    frame.insert("type".to_string(), json!(frame_type));
    frame.insert("projectId".to_string(), json!(config.project_id));
    frame.insert("sessionId".to_string(), json!(config.session_id));
    frame.insert("epoch".to_string(), json!(epoch));
    frame.insert("seq".to_string(), json!(seq));
    frame.insert(
        "protocolVersion".to_string(),
        json!(config.protocol_version),
    );
    frame.insert("serverId".to_string(), json!(config.server_id));
    frame
}

fn insert_optional_lifecycle(
    frame: &mut Map<String, Value>,
    lifecycle: Option<&SessionChatTurnLifecycle>,
) {
    if let Some(lifecycle) = lifecycle {
        if let Ok(value) = serde_json::to_value(lifecycle) {
            frame.insert("lifecycle".to_string(), value);
        }
    }
}

fn insert_optional_agent_session_id(
    frame: &mut Map<String, Value>,
    config: &SessionChatFollowerConfig,
) {
    if let Some(agent_session_id) = config
        .agent_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        frame.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
}

fn insert_optional_prompt(
    frame: &mut Map<String, Value>,
    prompt: Option<&SessionChatInteractivePrompt>,
) {
    if let Some(prompt) = prompt {
        if let Ok(value) = serde_json::to_value(prompt) {
            frame.insert("prompt".to_string(), value);
        }
    }
}

/// Detected model/effort. Absent ⇒ the field is omitted ⇒ clients keep their
/// own truth (older daemons behave the same way).
pub(crate) fn insert_optional_selected_options(
    frame: &mut Map<String, Value>,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
) {
    if let Some(selected_options) = selected_options {
        frame.insert("selectedOptions".to_string(), selected_options.to_value());
    }
}

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Terminal-state notice (login expired, trust dialog, usage limit, undelivered
send). Absent ⇒ the field is omitted ⇒ clients CLEAR the card, exactly like
`prompt` and unlike `selectedOptions`. Every frame that can carry it must
therefore carry the CURRENT value, never `None` as a shorthand for "unchanged".
*/
fn insert_optional_terminal_notice(
    frame: &mut Map<String, Value>,
    terminal_notice: Option<&crate::session_chat_notice::SessionChatTerminalNotice>,
) {
    if let Some(terminal_notice) = terminal_notice {
        frame.insert("terminalNotice".to_string(), terminal_notice.to_value());
    }
}

/*
CDXC:SessionChatTerminalActivity 2026-08-22:
Everything one terminal capture tells a client, travelling as ONE value. The
notice card and the transcript's activity row are read from the same screen and
are always restated together — carrying them as two parallel parameters through
four frame builders is how they would eventually drift out of step, with a
stale progress row surviving a frame that cleared its notice (or the reverse).
Both halves keep `prompt` semantics: absent ⇒ the field is omitted ⇒ the client
CLEARS it, so every producer restates the CURRENT value.
*/
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionChatScreenState<'a> {
    pub prompt: Option<&'a SessionChatInteractivePrompt>,
    pub notice: Option<&'a crate::session_chat_notice::SessionChatTerminalNotice>,
    pub activity: Option<&'a crate::session_chat_terminal_activity::SessionChatTerminalActivity>,
    /*
    CDXC:SessionChatAgentFleet 2026-08-23: the sub-agents the screen is
    painting right now. Rides here for the same reason the activity row does —
    one capture, one value, so the fleet strip can never survive a frame that
    cleared the progress row it was read beside.
    */
    pub fleet: Option<&'a crate::session_chat_agent_fleet::SessionChatAgentFleet>,
    /*
    CDXC:SessionChatScreenProbed 2026-08-22:
    True once a WHOLE screen capture has actually been read for this session.

    Every other screen-derived field omits itself when it has nothing to say,
    which leaves a client unable to tell "the model is still being detected"
    from "detection ran and this agent's screen names no model". The composer
    needs exactly that distinction to decide between a loading skeleton and a
    plain unset pill — a stopped or sleeping session has no screen at all and
    must never sit under a spinner waiting for a value that is not coming.

    Same rule as `captured` in SessionChatTerminalDetection, which is where
    this comes from: only a capture that succeeded whole counts.
    */
    pub probed: bool,
}

pub(crate) fn insert_screen_state(
    frame: &mut Map<String, Value>,
    screen: SessionChatScreenState<'_>,
) {
    insert_optional_terminal_notice(frame, screen.notice);
    if let Some(activity) = screen.activity {
        frame.insert("terminalActivity".to_string(), activity.to_value());
    }
    if let Some(fleet) = screen.fleet {
        frame.insert("agentFleet".to_string(), fleet.to_value());
    }
    if screen.probed {
        frame.insert("screenProbed".to_string(), Value::Bool(true));
    }
}

/*
CDXC:SessionChatQueueCarriage 2026-08-21:
Queue + draft ride snapshot / replaced / state frames only. `queue` is written
even when empty — present is the daemon capability probe — while `draft` is
written only when the server actually holds one, because an omitted draft means
UNCHANGED and never "cleared". Read only when a frame that carries it is
actually being emitted, never on the reconcile tick.

CDXC:SessionChatFollowerLiveness 2026-08-24: the READ moved out of the frame
builders. `emit_sequenced`'s build closure runs under the stream's emit-order
mutex and must not block, but the reader opens the state database — SQLite busy
contention there held the mutex every other publisher (hook ingest, options
detection) has to take. The snapshot is taken first and only inserted here.
*/
fn insert_optional_queue(
    frame: &mut Map<String, Value>,
    queue: Option<&crate::session_chat_queue::SessionChatQueueSnapshot>,
) {
    let Some(queue) = queue else {
        return;
    };
    queue.insert_into(frame);
}

/// Reads the queue snapshot a state/snapshot frame will carry, outside the
/// emit lock. Absent reader ⇒ absent fields (the client's "no queue" probe).
fn read_optional_queue(
    config: &SessionChatFollowerConfig,
) -> Option<crate::session_chat_queue::SessionChatQueueSnapshot> {
    config.queue_reader.as_ref().map(|reader| reader())
}

/*
CDXC:SessionChatAppCommands 2026-08-23:
Rides on `config` rather than on a new parameter through four frame builders:
the rows live in a process-global store keyed by exactly the ids the config
already carries, and unlike the screen state they are not read from a capture
this frame took, so bundling them into SessionChatScreenState would tie a
value with its own lifetime to one that must never survive a frame that
cleared it.
*/
fn insert_optional_app_commands(
    frame: &mut Map<String, Value>,
    config: &SessionChatFollowerConfig,
) {
    crate::session_chat_app_command::insert_session_chat_app_commands(
        frame,
        &config.project_id,
        &config.session_id,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_state_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    status: SessionChatStatus,
    prompt: Option<&SessionChatInteractivePrompt>,
    working: Option<bool>,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: SessionChatScreenState<'_>,
) {
    let queue = read_optional_queue(config);
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, "sessionChatState", epoch, seq);
            frame.insert("status".to_string(), json!(status.as_str()));
            insert_optional_prompt(&mut frame, prompt.or(screen.prompt));
            if let Some(working) = working {
                frame.insert("working".to_string(), json!(working));
            }
            insert_optional_selected_options(&mut frame, selected_options);
            insert_screen_state(&mut frame, screen);
            insert_optional_queue(&mut frame, queue.as_ref());
            insert_optional_app_commands(&mut frame, config);
            insert_optional_agent_session_id(&mut frame, config);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_snapshot_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    frame_type: &str,
    tail: &SessionChatTailFileResult,
    // CDXC:SessionChatForkStitch 2026-08-28: the tailed rollout was opened by
    // `codex fork`, so older rows live in an ancestor file the read path can
    // stitch in. Keeps the client's scroll-up gate open past this file's top.
    has_fork_ancestor: bool,
    prompt: Option<&SessionChatInteractivePrompt>,
    working: bool,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: SessionChatScreenState<'_>,
) {
    let queue = read_optional_queue(config);
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, frame_type, epoch, seq);
            frame.insert(
                "messages".to_string(),
                serde_json::to_value(&tail.messages).unwrap_or(Value::Array(Vec::new())),
            );
            insert_optional_lifecycle(&mut frame, tail.lifecycle.as_ref());
            frame.insert(
                "hasMore".to_string(),
                json!(tail.has_more || has_fork_ancestor),
            );
            frame.insert("hasMoreExact".to_string(), json!(true));
            frame.insert("beforeOffset".to_string(), json!(tail.before_offset));
            let status = if tail.messages.is_empty() {
                SessionChatStatus::Empty
            } else {
                SessionChatStatus::Ready
            };
            frame.insert("status".to_string(), json!(status.as_str()));
            frame.insert("working".to_string(), json!(working));
            insert_optional_prompt(&mut frame, prompt.or(screen.prompt));
            insert_optional_selected_options(&mut frame, selected_options);
            insert_screen_state(&mut frame, screen);
            insert_optional_queue(&mut frame, queue.as_ref());
            insert_optional_app_commands(&mut frame, config);
            insert_optional_agent_session_id(&mut frame, config);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

fn emit_appended_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    messages: &[SessionChatMessage],
    lifecycle: Option<&SessionChatTurnLifecycle>,
    superseded_message_ids: &[String],
) {
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, "sessionChatAppended", epoch, seq);
            frame.insert(
                "messages".to_string(),
                serde_json::to_value(messages).unwrap_or(Value::Array(Vec::new())),
            );
            // Omitted when empty: daemons and clients that predate the field
            // then behave exactly as before.
            if !superseded_message_ids.is_empty() {
                frame.insert(
                    "supersededMessageIds".to_string(),
                    json!(superseded_message_ids),
                );
            }
            insert_optional_lifecycle(&mut frame, lifecycle);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

/// How often a follower may pay for a successor directory scan while the
/// transcript it tails stays substantively stale.
pub(crate) const SUCCESSOR_SCAN_INTERVAL: Duration = Duration::from_millis(30_000);

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/*
CDXC:SessionChatIdentity 2026-08-02:
Runs when the tailed transcript has had no `user`/`assistant` record for
SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS and re-resolving the stored identity landed
back on that same file. Adoption is persisted through the registry FIRST: if the
write is refused the follower keeps tailing what it has, so the chat can never
show a conversation the rest of the daemon does not agree with (and the next
staleness check cannot flap back and forth between two files).
*/
async fn detect_and_adopt_successor_transcript(
    transcript_agent: SessionChatTranscriptAgent,
    config: &SessionChatFollowerConfig,
    stale_path: &Path,
    stored_agent_session_id: Option<&str>,
    logged_notice: &mut Option<String>,
) -> Option<SessionChatIdentityAdoption> {
    let hooks = config.successor_hooks.clone()?;
    let stem = stale_path.file_stem()?.to_str()?;
    /*
    CDXC:SessionChatIdentity 2026-08-24:
    Codex joined Claude here because `codex fork` DOES change the session id (a
    new rollout whose opening `session_meta` carries `forked_from_id`). The two
    agents differ only in how the tailed file names its session and in what
    counts as the file's last record; the outcome handling below is shared.
    */
    let stale_session_id = match transcript_agent {
        // Claude's filename stem IS the session id.
        SessionChatTranscriptAgent::Claude => {
            let stale_session_id = stem.to_string();
            if !is_uuid_transcript_stem(&stale_session_id) {
                return None;
            }
            stale_session_id
        }
        // Codex stems are `rollout-<ts>-<uuid>`; only the trailing uuid is it.
        SessionChatTranscriptAgent::Codex => codex_rollout_session_id(stem)?,
        SessionChatTranscriptAgent::Cursor
        | SessionChatTranscriptAgent::Grok
        | SessionChatTranscriptAgent::Hermes
        | SessionChatTranscriptAgent::Pi => return None,
    };
    // The agent is now narrowed to Claude or Codex; a bool keeps the blocking
    // scan below free of arms that could silently absorb a future agent.
    let tails_claude_transcript = transcript_agent == SessionChatTranscriptAgent::Claude;
    let now_ms = now_epoch_ms();
    let stale_substantive_idle_ms = config.tuning.successor_stale_substantive_idle_ms;
    let scan_path = stale_path.to_path_buf();
    let scan_stale_session_id = stale_session_id.clone();
    let bound_agent_session_ids = hooks.bound_agent_session_ids.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        // Claude keys staleness on the last SUBSTANTIVE row because its dead
        // files keep taking null-timestamp housekeeping appends; Codex has no
        // such split, so every timestamped rollout record counts.
        let last_record_ms = if tails_claude_transcript {
            last_substantive_transcript_timestamp_ms(&scan_path)
        } else {
            last_codex_record_timestamp_ms(&scan_path)
        }?;
        if now_ms.saturating_sub(last_record_ms) < stale_substantive_idle_ms {
            return None;
        }
        let owned = bound_agent_session_ids();
        let find = if tails_claude_transcript {
            find_claude_successor_transcript
        } else {
            find_codex_successor_transcript
        };
        Some(find(
            &scan_stale_session_id,
            &scan_path,
            last_record_ms,
            &owned,
        ))
    })
    .await
    .ok()
    .flatten()?;

    // Repeat scans of an unchanged directory must not spam the log.
    let mut log_once = |key: String, notice: SessionChatSuccessorNotice| {
        if logged_notice.as_deref() != Some(key.as_str()) {
            *logged_notice = Some(key);
            (hooks.log)(notice);
        }
    };

    match outcome {
        SessionChatSuccessorOutcome::NotFound => None,
        SessionChatSuccessorOutcome::Ambiguous {
            predecessor_session_id,
            candidate_session_ids,
        } => {
            let key = format!(
                "ambiguous|{predecessor_session_id}|{}",
                candidate_session_ids.join(",")
            );
            log_once(
                key,
                SessionChatSuccessorNotice::Ambiguous {
                    predecessor_session_id,
                    candidate_session_ids,
                },
            );
            None
        }
        SessionChatSuccessorOutcome::OwnedByAnotherSession {
            candidate_session_ids,
        } => {
            let key = format!("owned|{}", candidate_session_ids.join(","));
            log_once(
                key,
                SessionChatSuccessorNotice::OwnedByAnotherSession {
                    predecessor_session_id: stale_session_id,
                    candidate_session_ids,
                },
            );
            None
        }
        SessionChatSuccessorOutcome::Found(successor) => {
            let adoption = SessionChatIdentityAdoption {
                previous_agent_session_id: stored_agent_session_id.map(str::to_string),
                predecessor_transcript_session_id: stale_session_id.clone(),
                agent_session_id: successor.agent_session_id.clone(),
                agent_session_path: successor.path.to_string_lossy().into_owned(),
                lineage: successor.lineage.as_str(),
                hops: successor.hops,
            };
            let adopt_identity = hooks.adopt_identity.clone();
            let persisted_input = adoption.clone();
            let persisted = tokio::task::spawn_blocking(move || adopt_identity(persisted_input))
                .await
                .unwrap_or(false);
            if !persisted {
                let key = format!("rejected|{}", successor.agent_session_id);
                log_once(
                    key,
                    SessionChatSuccessorNotice::AdoptionRejected {
                        agent_session_id: successor.agent_session_id,
                        reason: "registry-identity-write-refused",
                    },
                );
                return None;
            }
            *logged_notice = None;
            (hooks.log)(SessionChatSuccessorNotice::Adopted(adoption.clone()));
            Some(adoption)
        }
    }
}

/*
Per-session follower task. Runs only while ≥1 client subscribes AND the
session is running (the server.rs registry enforces both). `resnapshot` is
signaled when another subscriber joins a live follower: every subscribe must
be answered by an authoritative snapshot, so the follower starts a fresh
generation (epoch bump, seq reset) and re-reads the tail instead of being
torn down and respawned mid-drain.
*/
pub async fn run_session_chat_follower(
    mut config: SessionChatFollowerConfig,
    stream: Arc<SessionChatStream>,
    resnapshot: Arc<tokio::sync::Notify>,
    // CDXC:SessionChatFollowerLiveness 2026-08-24: the task's own progress
    // signal, read by `sync_session_chat_follower_for_session`.
    heartbeat: Arc<SessionChatFollowerHeartbeat>,
    emit: SessionChatFrameEmitter,
) {
    let read_live_state = || match config.state_reader.as_ref() {
        Some(reader) => reader(),
        None => SessionChatLiveState::default(),
    };
    // Cached detection only: frames must never pay for a process spawn. Carries
    // BOTH the model/effort pills and the terminal-state notice.
    let read_cached_detection = || {
        config
            .options_reader
            .as_ref()
            .map(|reader| reader(crate::session_chat_options::SessionChatOptionsReadMode::Cached))
            .unwrap_or_default()
    };
    let Some(transcript_agent) = resolve_session_chat_transcript_agent(config.agent.as_deref())
    else {
        loop {
            let epoch = stream.begin_generation();
            emit_state_frame(
                &emit,
                &config,
                &stream,
                epoch,
                SessionChatStatus::Unsupported,
                None,
                None,
                None,
                SessionChatScreenState::default(),
            );
            heartbeat.park();
            resnapshot.notified().await;
            heartbeat.unpark();
        }
    };
    let decode = session_chat_line_decoder(transcript_agent);
    let decode_lifecycle = session_chat_lifecycle_decoder(transcript_agent);

    let mut epoch = stream.begin_generation();
    let mut want_snapshot = true;
    let mut emitted_starting = false;
    let mut resolved: Option<PathBuf> = None;
    let mut resolve_delay = INITIAL_RESOLVE_POLL;
    let mut file_state = FollowerFileState::new();
    // Rolling AskUserQuestion state folded over everything decoded so far, plus
    // the last prompt/working pair actually published to clients.
    let mut transcript_prompt = SessionChatTranscriptPromptState::default();
    let mut published_prompt: Option<SessionChatInteractivePrompt> = None;
    let mut published_working = false;
    let mut published_state_valid = false;
    let mut identity = SessionChatFollowerIdentity {
        agent_session_id: config.agent_session_id.clone(),
        agent_session_path: config.agent_session_path.clone(),
    };
    let mut last_transcript_change = std::time::Instant::now();
    let mut last_staleness_check = std::time::Instant::now();
    let mut last_successor_scan = std::time::Instant::now();
    // "Adopt none and log once" for an ambiguous successor set.
    let mut logged_successor_ambiguity: Option<String> = None;
    // Model/effort the follower has published, plus counters that pace the
    // fast startup probes and periodic steady-state re-detects.
    let mut published_options: Option<crate::session_chat_options::SessionChatDetectedOptions> =
        None;
    // Terminal-state notice the follower has published. Tracked separately
    // because it can legitimately go back to `None` (the screen cleared), which
    // MUST be published as an omitted field.
    let mut published_notice: Option<crate::session_chat_notice::SessionChatTerminalNotice> = None;
    /*
    CDXC:SessionChatTerminalActivity 2026-08-22: the progress row the follower
    has published. Tracked like the notice (it can legitimately go back to
    `None` when the work finishes) but compared on its NUMBERS too, because a
    moving percentage is the whole point of publishing it again.
    */
    let mut published_activity: Option<
        crate::session_chat_terminal_activity::SessionChatTerminalActivity,
    > = None;
    /*
    CDXC:SessionChatAgentFleet 2026-08-23: deliberately NOT cleared when the
    main agent goes idle, unlike the activity row above. A `⏺` status line is
    stale scrollback the moment Claude stops, but sub-agents outlive the
    turn that spawned them — clearing on idle would blank the strip exactly when
    it is the only thing telling the user work is still running.
    */
    let mut published_fleet: Option<crate::session_chat_agent_fleet::SessionChatAgentFleet> = None;
    // CDXC:SessionChatScreenProbed 2026-08-22: latched, not sampled. It answers
    // "has detection run for this session yet", so a later capture failure (the
    // session stopped, the daemon went away) must not put the composer back
    // under a loading skeleton.
    let mut published_screen_probed = false;
    /*
    CDXC:SessionChatForkStitch 2026-08-28:
    A `codex fork` rollout carries no pre-fork rows, so a snapshot that included
    the whole file would report `hasMore: false` and close the client's scroll-up
    gate on history that /api/readSessionChat can still stitch in. The flag says
    "scroll-back continues past the top of this file"; it is resolved once per
    transcript path (the cached lineage lookup below) and never per frame.
    */
    let mut fork_ancestor_path: Option<PathBuf> = None;
    let mut has_fork_ancestor = false;
    let mut reconcile_ticks: u64 = 0;
    let mut startup_option_reconcile_ticks: u64 = 0;

    /*
    CDXC:SessionChatSeedDetection 2026-08-22:
    Probe once, here, before the first frame goes out.

    The snapshot frame reads detection from the shared cache and never spawns,
    which was right when a capture cost a login shell and a process. On a cold
    cache — the first chat open after a gxserver start — that meant the
    snapshot carried NO model/effort at all, and the client's seed read (which
    does force a detection) is explicitly outranked by the first frame, so its
    freshly detected value was discarded. The pills then stayed blank until the
    startup probe below fired on the second reconcile, a second or more later,
    and only then snapped to the real model. That flash of "Model"/"Options"
    turning into "Opus 5"/"High" is what this removes.

    A capture is now a direct socket read (CDXC:SessionChatScreenCapture),
    ~0.1ms typical and ~6ms against a very large scrollback, so paying for one
    before the snapshot is cheaper than the frame it rides on. The deadline
    exists only for a wedged daemon that accepts the connection and never
    answers: the capture's own read timeout is 5s, and stalling a subscribe
    that long to populate a pill is not a trade worth making. Missing the
    deadline is not an error — the startup probe below still runs.
    */
    if let Some(reader) = config.options_reader.clone() {
        let _ = tokio::time::timeout(
            SEED_OPTION_DETECTION_DEADLINE,
            tokio::task::spawn_blocking(move || {
                reader(crate::session_chat_options::SessionChatOptionsReadMode::Refresh)
            }),
        )
        .await;
    }

    loop {
        heartbeat.stamp();
        if resolved.is_none() {
            let agent_session_id = identity.agent_session_id.clone();
            let agent_session_path = identity.agent_session_path.clone();
            resolved = tokio::task::spawn_blocking(move || {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();
            if resolved.is_none() {
                if !emitted_starting {
                    let live = read_live_state();
                    let detection = read_cached_detection();
                    emit_state_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        SessionChatStatus::Starting,
                        live.prompt.as_ref(),
                        Some(live.working),
                        detection.options.as_ref(),
                        SessionChatScreenState {
                            prompt: detection.prompt.as_ref(),
                            notice: detection.notice.as_ref(),
                            activity: live
                                .working
                                .then_some(detection.activity.as_ref())
                                .flatten(),
                            fleet: detection.fleet.as_ref(),
                            probed: published_screen_probed || detection.attempted,
                        },
                    );
                    published_notice = detection.notice;
                    published_activity = if live.working {
                        detection.activity
                    } else {
                        None
                    };
                    published_fleet = detection.fleet;
                    emitted_starting = true;
                }
                heartbeat.park();
                tokio::select! {
                    _ = tokio::time::sleep(resolve_delay) => {}
                    _ = resnapshot.notified() => {
                        epoch = stream.begin_generation();
                        emitted_starting = false;
                        want_snapshot = true;
                    }
                }
                heartbeat.unpark();
                resolve_delay = (resolve_delay * 2).min(MAX_RESOLVE_POLL);
                // A stale hook identity is the usual reason the path never
                // appears: re-read the session's current identity each poll.
                identity.adopt(read_live_state());
                continue;
            }
            want_snapshot = true;
            file_state = FollowerFileState::new();
            transcript_prompt = SessionChatTranscriptPromptState::default();
            last_transcript_change = std::time::Instant::now();
        }

        let path = resolved.clone().expect("resolved transcript path");
        let drain_limit = config.limit;
        let drain_want_snapshot = want_snapshot;
        let mut drain_state = std::mem::replace(&mut file_state, FollowerFileState::new());
        let Ok((returned_state, outcome)) = tokio::task::spawn_blocking(move || {
            let outcome = follower_drain_once(
                &path,
                drain_limit,
                transcript_agent,
                decode,
                decode_lifecycle,
                &mut drain_state,
                drain_want_snapshot,
            );
            (drain_state, outcome)
        })
        .await
        else {
            return;
        };
        file_state = returned_state;
        // One live-state read per reconcile: it opens the domain database.
        let live = read_live_state();

        match outcome {
            FollowerDrainOutcome::Missing => {
                // Rotation to a missing path — resolve-poll again and deliver
                // an authoritative frame once the successor file appears.
                resolved = None;
                resolve_delay = INITIAL_RESOLVE_POLL;
                epoch = stream.begin_generation();
                emitted_starting = false;
                want_snapshot = true;
                continue;
            }
            FollowerDrainOutcome::Snapshot {
                tail,
                appended,
                appended_lifecycle,
                content_replaced,
            } => {
                let frame_type = if want_snapshot {
                    "sessionChatSnapshot"
                } else {
                    if content_replaced {
                        epoch = stream.begin_generation();
                    }
                    "sessionChatReplaced"
                };
                // The tail window replaces everything the client had, so the
                // question fold restarts from it.
                transcript_prompt = SessionChatTranscriptPromptState::default();
                transcript_prompt.advance(&tail.messages);
                transcript_prompt.advance(&appended);
                // A subscribing client gets the detected pills value and any
                // terminal-state notice with its snapshot, so it needs no
                // separate read.
                let snapshot_detection = read_cached_detection();
                let prompt = resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt)
                    .or_else(|| snapshot_detection.prompt.clone());
                let lineage_path = resolved.clone().expect("resolved transcript path");
                if fork_ancestor_path.as_ref() != Some(&lineage_path) {
                    let probe_path = lineage_path.clone();
                    has_fork_ancestor = tokio::task::spawn_blocking(move || {
                        crate::session_chat_fork_stitch::codex_transcript_has_fork_ancestor(
                            transcript_agent,
                            &probe_path,
                        )
                    })
                    .await
                    .unwrap_or(false);
                    fork_ancestor_path = Some(lineage_path);
                }
                emit_snapshot_frame(
                    &emit,
                    &config,
                    &stream,
                    epoch,
                    frame_type,
                    &tail,
                    has_fork_ancestor,
                    prompt.as_ref(),
                    live.working,
                    snapshot_detection.options.as_ref(),
                    SessionChatScreenState {
                        prompt: None,
                        notice: snapshot_detection.notice.as_ref(),
                        activity: live
                            .working
                            .then_some(snapshot_detection.activity.as_ref())
                            .flatten(),
                        fleet: snapshot_detection.fleet.as_ref(),
                        probed: published_screen_probed || snapshot_detection.attempted,
                    },
                );
                published_screen_probed = published_screen_probed || snapshot_detection.attempted;
                if snapshot_detection.options.is_some() {
                    published_options = snapshot_detection.options;
                }
                published_notice = snapshot_detection.notice;
                published_activity = if live.working {
                    snapshot_detection.activity
                } else {
                    None
                };
                published_fleet = snapshot_detection.fleet;
                published_prompt = prompt;
                published_working = live.working;
                published_state_valid = true;
                want_snapshot = false;
                last_transcript_change = std::time::Instant::now();
                if !appended.is_empty() || appended_lifecycle.is_some() {
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &appended,
                        appended_lifecycle.as_ref(),
                        &[],
                    );
                }
            }
            FollowerDrainOutcome::Appended {
                batches,
                lifecycle,
                superseded,
                api_refusal,
            } => {
                last_transcript_change = std::time::Instant::now();
                /*
                CDXC:SessionChatApiRefusal 2026-08-28:
                Stored in the watchdog store on purpose: it inherits the
                store's dismissal identity, its 10-minute expiry, and its
                retirement by the next send (the send watchdog clears the
                store when a new message goes in — which is exactly when a
                refusal card stops being news). Each refusal row is seen by
                exactly one drain window, so this publishes once per refusal.
                */
                if let Some(refusal) = api_refusal {
                    crate::session_chat_notice::set_session_chat_watchdog_notice(
                        &config.project_id,
                        &config.session_id,
                        crate::session_chat_notice::session_chat_api_refusal_notice(refusal),
                    );
                    if let Some(notice_publisher) = config.notice_publisher.as_ref() {
                        notice_publisher();
                    }
                }
                if batches.is_empty() {
                    // Lifecycle-only and retraction-only frames ARE emitted.
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &[],
                        lifecycle.as_ref(),
                        &superseded,
                    );
                } else {
                    let last_index = batches.len() - 1;
                    for (index, batch) in batches.iter().enumerate() {
                        transcript_prompt.advance(batch);
                        let batch_lifecycle = if index == last_index {
                            lifecycle.as_ref()
                        } else {
                            None
                        };
                        // The retraction rides the FIRST frame so a client can
                        // never re-order it after the rows that replace it.
                        let batch_superseded: &[String] =
                            if index == 0 { &superseded } else { &[] };
                        emit_appended_frame(
                            &emit,
                            &config,
                            &stream,
                            epoch,
                            batch,
                            batch_lifecycle,
                            batch_superseded,
                        );
                    }
                }
            }
            FollowerDrainOutcome::Idle => {}
        }

        /*
        CDXC:SessionChatCore 2026-08-01:
        Interactive cards used to depend entirely on agent hooks. When the
        installed hook script does not forward toolName/toolInput the card never
        appeared, and when it never reports PostToolUse a card answered in the
        terminal stayed on screen forever. The transcript itself answers both:
        a trailing AskUserQuestion tool call with no tool result means "pending",
        a tool result after it means "answered". The hook prompt still wins when
        both exist, so approvals and richer hook payloads are unaffected.
        */
        let effective_prompt = resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt)
            .or_else(|| read_cached_detection().prompt);
        let became_ready = published_state_valid && published_working && !live.working;
        // A `⏺` row remains in terminal scrollback after Claude stops. Clear
        // that stale status on the ready transition, but retain Claude's
        // explicit background-shell activity: those shells outlive the main
        // turn by definition and the screen row is their liveness signal.
        if !live.working
            && !published_activity
                .as_ref()
                .is_some_and(|activity| activity.remains_live_when_ready())
        {
            published_activity = None;
        }
        if !published_state_valid
            || effective_prompt != published_prompt
            || live.working != published_working
        {
            if published_state_valid {
                emit_state_frame(
                    &emit,
                    &config,
                    &stream,
                    epoch,
                    if live.working {
                        SessionChatStatus::Working
                    } else {
                        SessionChatStatus::Ready
                    },
                    effective_prompt.as_ref(),
                    Some(live.working),
                    published_options.as_ref(),
                    SessionChatScreenState {
                        prompt: None,
                        notice: published_notice.as_ref(),
                        activity: published_activity.as_ref(),
                        fleet: published_fleet.as_ref(),
                        probed: published_screen_probed,
                    },
                );
            }
            published_prompt = effective_prompt;
            published_working = live.working;
            published_state_valid = true;
        }

        /*
        CDXC:SessionChatDetectedOptions 2026-08-01:
        Model/effort probe: a newly launched agent can paint its footer just
        after the seed probe read an empty screen. Probe each 1s reconcile for
        up to ten seconds until both values arrive, then retain the ~30s
        steady-state cadence that catches direct TUI changes. The follower only
        exists while subscribed, and a frame is emitted only when the detected
        value actually changed.

        `reconcile_ticks > 1` skips the first pass on purpose: the subscribe's
        own probe (CDXC:SessionChatSeedDetection) already captured at t=0 and
        the snapshot frame published it, so probing again immediately would
        capture the same unchanged screen twice.

        CDXC:SessionChatTerminalNotices 2026-08-19:
        The same probe classifies the captured screen, so a trust dialog or an
        expired login reaches chat on this cadence for free. A notice may also
        legitimately CLEAR, which the options half can never do — but only a
        capture that actually succeeded proves a clean screen, so a failed or
        capped read leaves the published notice standing.
        */
        reconcile_ticks = reconcile_ticks.wrapping_add(1);
        let startup_probe_due = published_state_valid
            && config.options_reader.is_some()
            && reconcile_ticks > 1
            && startup_option_reconcile_ticks
                < crate::session_chat_options::SESSION_CHAT_OPTION_STARTUP_RECONCILE_TICKS
            && published_options.as_ref().map_or(true, |options| {
                options.selection.model.is_none() || options.selection.effort.is_none()
            });
        if startup_probe_due {
            startup_option_reconcile_ticks += 1;
        }
        /*
        CDXC:SessionChatTerminalActivity 2026-08-22:
        The steady 30s cadence is right for state that either holds or does not
        (a login screen, a model pill), and useless for a progress bar: a
        compaction can be over before the second sample lands. So the interval
        is chosen by what the LAST probe found —

          - a live activity ⇒ every few ticks, because its numbers are the
            reason to publish again at all;
          - the agent working with no activity known ⇒ a middle cadence, which
            is what discovers an AUTOMATIC compaction (nothing announces it,
            and the user never typed a command we could hang a re-detect on);
          - idle ⇒ the original 30s, plus one immediate probe on the working
            → ready edge so a newly painted background-shell footer is not
            hidden until the next steady sample.

        Only followed sessions probe at all, and only while a client is
        subscribed, so the faster tiers are bounded by what is actually on
        screen in front of someone.
        */
        let probe_interval_ticks = if published_activity.is_some() || published_fleet.is_some() {
            crate::session_chat_options::SESSION_CHAT_ACTIVITY_RECONCILE_INTERVAL_TICKS
        } else if published_working {
            crate::session_chat_options::SESSION_CHAT_WORKING_RECONCILE_INTERVAL_TICKS
        } else {
            crate::session_chat_options::SESSION_CHAT_OPTION_RECONCILE_INTERVAL_TICKS
        };
        let periodic_probe_due = became_ready || reconcile_ticks % probe_interval_ticks == 0;
        if published_state_valid
            && config.options_reader.is_some()
            && (startup_probe_due || periodic_probe_due)
        {
            let reader = config.options_reader.clone();
            /*
            CDXC:SessionChatFollowerLiveness 2026-08-24:
            Awaited inline on the reconcile loop, so a capture that never
            answers used to stall the follower forever while the transcript
            grew. Missing the deadline means this pass simply publishes
            nothing and changes no published state — the next probe tick
            tries again.
            */
            let probe = tokio::time::timeout(
                STEADY_OPTION_DETECTION_DEADLINE,
                tokio::task::spawn_blocking(move || {
                    reader
                        .map(|reader| {
                            reader(crate::session_chat_options::SessionChatOptionsReadMode::Refresh)
                        })
                        .unwrap_or_default()
                }),
            )
            .await;
            if let Ok(Ok(mut detection)) = probe {
                if !published_working
                    && !detection
                        .activity
                        .as_ref()
                        .is_some_and(|activity| activity.remains_live_when_ready())
                {
                    detection.activity = None;
                }
                let options_changed = detection
                    .options
                    .as_ref()
                    .is_some_and(|detected| !detected.same_selection(published_options.as_ref()));
                let notice_changed = detection.captured
                    && !crate::session_chat_notice::same_session_chat_terminal_notice(
                        detection.notice.as_ref(),
                        published_notice.as_ref(),
                    );
                // Same capture rule as the notice: only a WHOLE capture proves
                // the progress line is gone, so a capped read leaves the row
                // standing.
                let activity_changed = detection.captured
                    && !crate::session_chat_terminal_activity::same_session_chat_terminal_activity(
                        detection.activity.as_ref(),
                        published_activity.as_ref(),
                    );
                /*
                CDXC:SessionChatScreenProbed 2026-08-22: the first successful
                capture is publishable on its own, even when it changed
                nothing. An agent whose screen names no model detects nothing
                forever, and without this the composer would never hear that
                detection HAD run and would hold its loading skeleton for the
                life of the session.
                */
                // Same capture rule again: only a whole screen proves the fleet
                // is gone, and only the roster counts as a change — the per-row
                // clocks tick locally off `detectedAt`.
                let fleet_changed = detection.captured
                    && !crate::session_chat_agent_fleet::same_session_chat_agent_fleet(
                        detection.fleet.as_ref(),
                        published_fleet.as_ref(),
                    );
                let detected_prompt =
                    resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt)
                        .or_else(|| detection.prompt.clone());
                let prompt_changed = detection.captured && detected_prompt != published_prompt;
                let probed_changed = detection.attempted && !published_screen_probed;
                if options_changed
                    || notice_changed
                    || activity_changed
                    || fleet_changed
                    || prompt_changed
                    || probed_changed
                {
                    if options_changed {
                        published_options = detection.options;
                    }
                    if notice_changed {
                        published_notice = detection.notice;
                    }
                    if activity_changed {
                        published_activity = detection.activity;
                    }
                    if fleet_changed {
                        published_fleet = detection.fleet;
                    }
                    if prompt_changed {
                        published_prompt = detected_prompt;
                    }
                    published_screen_probed = published_screen_probed || detection.attempted;
                    emit_state_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        if published_working {
                            SessionChatStatus::Working
                        } else {
                            SessionChatStatus::Ready
                        },
                        published_prompt.as_ref(),
                        Some(published_working),
                        published_options.as_ref(),
                        SessionChatScreenState {
                            prompt: None,
                            notice: published_notice.as_ref(),
                            activity: published_activity.as_ref(),
                            fleet: published_fleet.as_ref(),
                            probed: published_screen_probed,
                        },
                    );
                }
            }
        }

        /*
        CDXC:SessionChatCore 2026-08-01:
        Stale-identity guard. `/clear` and `resume` make the agent start a NEW
        transcript file while the old one stays on disk, so the follower keeps
        tailing a file that will never grow again and the chat freezes at the
        switch point — with no Missing outcome to recover from. When the tailed
        file has been silent, re-derive the path from the session's CURRENT
        identity; a different file is treated exactly like a content
        replacement.

        CDXC:SessionChatIdentity 2026-08-02:
        The hook-driven re-resolution above only runs while hooks report the
        session as `working`. The case successor detection must recover from is
        precisely the one where hooks never fire at all (a background-job
        continuation writes a NEW transcript and nothing updates the registry),
        so `working` cannot gate it. It gets its own, slower cadence instead:
        every SUCCESSOR_SCAN_INTERVAL while the tailed file stays silent.
        */
        // Codex is in scope too: `codex fork` changes the session id
        // (CDXC:SessionChatIdentity 2026-08-24).
        let successor_scan_due = matches!(
            transcript_agent,
            SessionChatTranscriptAgent::Claude | SessionChatTranscriptAgent::Codex
        ) && config.successor_hooks.is_some()
            && last_successor_scan.elapsed() >= config.tuning.successor_scan_interval;
        if last_transcript_change.elapsed() >= config.tuning.stale_transcript_idle
            && ((published_working
                && last_staleness_check.elapsed() >= config.tuning.stale_transcript_idle)
                || successor_scan_due)
        {
            last_staleness_check = std::time::Instant::now();
            identity.adopt(live);
            let agent_session_id = identity.agent_session_id.clone();
            let agent_session_path = identity.agent_session_path.clone();
            let re_resolved = tokio::task::spawn_blocking(move || {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();
            if let Some(next_path) = re_resolved {
                if Some(&next_path) != resolved.as_ref() {
                    resolved = Some(next_path);
                    file_state = FollowerFileState::new();
                    transcript_prompt = SessionChatTranscriptPromptState::default();
                    epoch = stream.begin_generation();
                    want_snapshot = true;
                    published_state_valid = false;
                    last_transcript_change = std::time::Instant::now();
                    continue;
                }
                /*
                CDXC:SessionChatIdentity 2026-08-02:
                Re-resolution landed on the SAME file, so the registry identity
                itself is stale. Look for a transcript that proves it continues
                this one and re-bind the session to it.
                */
                if successor_scan_due {
                    last_successor_scan = std::time::Instant::now();
                    if let Some(adopted) = detect_and_adopt_successor_transcript(
                        transcript_agent,
                        &config,
                        &next_path,
                        identity.agent_session_id.as_deref(),
                        &mut logged_successor_ambiguity,
                    )
                    .await
                    {
                        identity.agent_session_id = Some(adopted.agent_session_id.clone());
                        identity.agent_session_path = Some(adopted.agent_session_path.clone());
                        config.agent_session_id = Some(adopted.agent_session_id);
                        config.agent_session_path = Some(adopted.agent_session_path.clone());
                        resolved = Some(PathBuf::from(&adopted.agent_session_path));
                        file_state = FollowerFileState::new();
                        transcript_prompt = SessionChatTranscriptPromptState::default();
                        epoch = stream.begin_generation();
                        want_snapshot = true;
                        published_state_valid = false;
                        last_transcript_change = std::time::Instant::now();
                        continue;
                    }
                }
            }
        }

        heartbeat.park();
        tokio::select! {
            _ = tokio::time::sleep(config.tuning.reconcile_interval) => {}
            _ = resnapshot.notified() => {
                epoch = stream.begin_generation();
                want_snapshot = true;
            }
        }
        heartbeat.unpark();
    }
}

/// Identity the follower is currently tailing. Seeded from the spawn config and
/// refreshed from the live session so a hook update that did not respawn the
/// task still reaches the resolver.
pub(crate) struct SessionChatFollowerIdentity {
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
}

impl SessionChatFollowerIdentity {
    fn adopt(&mut self, live: SessionChatLiveState) {
        if live.agent_session_id.is_some() {
            self.agent_session_id = live.agent_session_id;
        }
        if live.agent_session_path.is_some() {
            self.agent_session_path = live.agent_session_path;
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive prompts (upstream chat spec §8.1-§8.3): question/approval cards
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatCore 2026-07-31:
Session Chat follower registry. Lifecycle mirrors zmx_title_observers (synced
from schedule_presentation_session_delta, boot sync, shutdown stop-all) with
one addition: followers are refcounted by /api/events `subscribeSessionChat`
clients, so a task only tails a transcript while somebody is watching AND the
session is running. Frames go out as plain hub broadcasts tagged with
projectId/sessionId (clients filter); they deliberately do NOT take
lock_presentation_event_sequence because chat epoch/seq is decoupled from the
presentation revision stream.
*/

pub(crate) fn session_chat_agent_for_session(session: &Value) -> Option<String> {
    let agent_id = normalize_agent_name(first_prompt_agent_name(session).as_deref());
    if let Some(agent) = crate::session_chat::session_chat_transcript_agent_id(agent_id.as_deref())
    {
        return Some(agent.to_string());
    }

    let agent_icon = session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("icon"))
        .and_then(Value::as_str);
    crate::session_chat::session_chat_transcript_agent_id(agent_icon).map(str::to_string)
}

pub(crate) fn session_chat_identity_fingerprint(session: &Value) -> String {
    format!(
        "{}|{}|{}",
        session_chat_agent_for_session(session).unwrap_or_default(),
        read_runtime_text(session, "agentSessionId").unwrap_or_default(),
        read_runtime_text(session, "agentSessionPath").unwrap_or_default(),
    )
}

pub(crate) fn is_session_chat_followable_session(session: &Value) -> bool {
    read_session_text(session, "lifecycleState").as_deref() == Some("running")
}

pub(crate) fn session_agent_activity(session: &Value) -> Option<&str> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
        .and_then(Value::as_object)
        .and_then(|agent_activity| agent_activity.get("activity"))
        .and_then(Value::as_str)
}

/*
CDXC:SessionChatCore 2026-08-01:
The chat channel's own working truth. Agent hooks are the only source that knows
a turn started before the transcript flushes its first row, so every chat surface
(gpui, web, mobile) reads it here instead of each host wiring its own session
activity prop — desktop had none at all, so its Working marker and Stop button
were dead.
*/
pub(crate) fn session_chat_hook_working(session: &Value) -> bool {
    session_agent_activity(session) == Some("working")
}

pub(crate) fn sync_session_chat_follower_for_session(
    state: &AppState,
    session: &Value,
    _reason: &str,
) {
    let Some(project_id) = read_session_text(session, "projectId") else {
        return;
    };
    let Some(session_id) = read_session_text(session, "sessionId") else {
        return;
    };
    let key = session_observer_key(&project_id, &session_id);
    let Ok(mut followers) = state.session_chat_followers.lock() else {
        return;
    };
    let Some(entry) = followers.get_mut(&key) else {
        return; // Nobody subscribed — nothing to follow.
    };
    if entry.subscribers == 0 || !is_session_chat_followable_session(session) {
        if let Some(task) = entry.task.take() {
            task.abort();
        }
        return;
    }
    let fingerprint = session_chat_identity_fingerprint(session);
    /*
    CDXC:SessionChatFollowerLiveness 2026-08-24:
    `is_finished()` alone only catches a task that RETURNED. A task wedged in an
    inline await (a blocking read against a daemon that never answers, a path
    resolution on a stalled filesystem) stays "unfinished" forever, so this
    healed nothing while chat sat frozen. The task's heartbeat closes that gap:
    running but no reconcile progress within the wedge deadline is dead too, and
    respawning starts a fresh generation + snapshot, so subscribed clients
    recover on the next frame.
    */
    let task_running = entry.task.as_ref().is_some_and(|task| !task.is_finished());
    let wedged = task_running
        && entry
            .heartbeat
            .is_wedged(crate::session_chat::SESSION_CHAT_FOLLOWER_WEDGE_DEADLINE);
    if wedged {
        log_session_chat_follower_wedged(state, &project_id, &session_id);
    }
    let task_alive = task_running && !wedged;
    if task_alive && entry.fingerprint == fingerprint {
        return;
    }
    if let Some(task) = entry.task.take() {
        task.abort();
    }
    entry.fingerprint = fingerprint;
    // Authoritative snapshot/replaced frames re-read the CURRENT stored
    // interactive prompt so a card pending across subscribe/rotation is
    // never dropped by a stale copy.
    let state_reader: crate::session_chat::SessionChatStateReader = {
        let paths = state.paths.clone();
        let server_id = state.metadata.server_id.clone();
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        Arc::new(move || {
            let read = || -> Option<crate::session_chat::SessionChatLiveState> {
                let db = open_gxserver_database(&paths).ok()?;
                let repository = DomainRepository::new(&db, server_id.as_str());
                let session = repository.get_session(&project_id, &session_id).ok()??;
                Some(crate::session_chat::SessionChatLiveState {
                    prompt: crate::agents::session_chat_prompt_setting(&session)
                        .as_deref()
                        .and_then(crate::session_chat::parse_stored_session_chat_prompt),
                    working: session_chat_hook_working(&session),
                    agent_session_id: read_runtime_text(&session, "agentSessionId"),
                    agent_session_path: read_runtime_text(&session, "agentSessionPath"),
                })
            };
            read().unwrap_or_default()
        })
    };
    let agent = session_chat_agent_for_session(session);
    let terminal_agent = crate::session_chat_composer::session_chat_composer_agent_id(session)
        .or_else(|| agent.clone());
    // Detection source for snapshot/replaced frames (cached) and the follower's
    // ~30s probe (refresh). Both run through the shared 5s-TTL cache.
    // CDXC:SessionChatTerminalNotices 2026-08-19: the reader also answers with
    // the session's terminal notice, watchdog-first, so the follower never
    // learns about the watchdog store.
    let options_reader: crate::session_chat_options::SessionChatOptionsReader = {
        let detector = SessionChatOptionDetector::new(state);
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        let terminal_agent = terminal_agent.clone();
        Arc::new(move |mode| {
            let mut detection = match mode {
                crate::session_chat_options::SessionChatOptionsReadMode::Cached => {
                    detector.cached(&project_id, &session_id)
                }
                crate::session_chat_options::SessionChatOptionsReadMode::Refresh => detector
                    .detect_blocking(&project_id, &session_id, terminal_agent.as_deref(), true),
            };
            detection.notice = crate::session_chat_notice::resolve_session_chat_terminal_notice(
                &project_id,
                &session_id,
                detection.notice,
            );
            detection
        })
    };
    /*
    CDXC:SessionChatIdentity 2026-08-02:
    Registry access the follower needs to re-bind a session whose Claude
    conversation was continued in a new transcript (compaction / background-job
    resume, where no agent hook ever reports the new id): the ids other sessions
    already own, a compare-and-set identity write through the passive path, and
    a log sink. All three run on the follower's blocking pool.
    */
    let successor_hooks = {
        let bound_paths = state.paths.clone();
        let bound_server_id = state.metadata.server_id.clone();
        let bound_project_id = project_id.clone();
        let bound_session_id = session_id.clone();
        let adopt_paths = state.paths.clone();
        let adopt_server_id = state.metadata.server_id.clone();
        let adopt_project_id = project_id.clone();
        let adopt_session_id = session_id.clone();
        let logger = state.logger.clone();
        let log_server_id = state.metadata.server_id.clone();
        let log_project_id = project_id.clone();
        let log_session_id = session_id.clone();
        crate::session_chat::SessionChatSuccessorHooks {
            bound_agent_session_ids: Arc::new(move || {
                let read = || -> Option<Vec<String>> {
                    let db = open_gxserver_database(&bound_paths).ok()?;
                    let repository = DomainRepository::new(&db, bound_server_id.as_str());
                    let sessions = repository.list_sessions(None).ok()?;
                    Some(
                        sessions
                            .iter()
                            .filter(|candidate| {
                                read_session_text(candidate, "projectId").as_deref()
                                    != Some(bound_project_id.as_str())
                                    || read_session_text(candidate, "sessionId").as_deref()
                                        != Some(bound_session_id.as_str())
                            })
                            /*
                            CDXC:SessionChatIdentity 2026-08-02:
                            ONLY sessions that could actually be tailing the id.
                            The registry keeps every session ever created (3487
                            stopped rows on the machine this was debugged on),
                            and stopped rows still carry the agentSessionIds of
                            conversations that have since been continued — the
                            first cut excluded those too, which silently blocked
                            every real adoption.
                            */
                            .filter(|candidate| crate::agents::is_active_identity_owner(candidate))
                            .filter_map(|candidate| read_runtime_text(candidate, "agentSessionId"))
                            .collect(),
                    )
                };
                read().unwrap_or_default()
            }),
            adopt_identity: Arc::new(move |adoption| {
                let write = || -> Option<bool> {
                    let db = open_gxserver_database(&adopt_paths).ok()?;
                    let repository = DomainRepository::new(&db, adopt_server_id.as_str());
                    crate::agents::apply_transcript_successor_session_identity(
                        &repository,
                        &adopt_project_id,
                        &adopt_session_id,
                        adoption.previous_agent_session_id.as_deref(),
                        &adoption.agent_session_id,
                        &adoption.agent_session_path,
                    )
                    .ok()
                };
                write().unwrap_or(false)
            }),
            log: Arc::new(move |notice| {
                let (level, event, details) = match notice {
                    crate::session_chat::SessionChatSuccessorNotice::Adopted(adoption) => (
                        // Warn, not Info: the persisted gxserver log keeps only
                        // warn/error unless Debugging Mode is on, and a session
                        // whose stored identity had drifted off its live
                        // conversation is exactly what a support bundle needs.
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptAdopted",
                        json!({
                            "agentSessionId": adoption.agent_session_id,
                            "agentSessionPath": adoption.agent_session_path,
                            "hops": adoption.hops,
                            "lineage": adoption.lineage,
                            "predecessorTranscriptSessionId":
                                adoption.predecessor_transcript_session_id,
                            "previousAgentSessionId": adoption.previous_agent_session_id,
                        }),
                    ),
                    crate::session_chat::SessionChatSuccessorNotice::AdoptionRejected {
                        agent_session_id,
                        reason,
                    } => (
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptRejected",
                        json!({ "agentSessionId": agent_session_id, "reason": reason }),
                    ),
                    crate::session_chat::SessionChatSuccessorNotice::Ambiguous {
                        predecessor_session_id,
                        candidate_session_ids,
                    } => (
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptAmbiguous",
                        json!({
                            "candidateAgentSessionIds": candidate_session_ids,
                            "predecessorAgentSessionId": predecessor_session_id,
                        }),
                    ),
                    crate::session_chat::SessionChatSuccessorNotice::OwnedByAnotherSession {
                        predecessor_session_id,
                        candidate_session_ids,
                    } => (
                        LogLevel::Warn,
                        "sessionChatSuccessorTranscriptOwned",
                        json!({
                            "candidateAgentSessionIds": candidate_session_ids,
                            "predecessorAgentSessionId": predecessor_session_id,
                        }),
                    ),
                };
                let mut details = details;
                if let Some(object) = details.as_object_mut() {
                    object.insert("projectId".to_string(), json!(log_project_id));
                    object.insert("sessionId".to_string(), json!(log_session_id));
                }
                let _ = logger.log(GxserverLogInput {
                    level,
                    event: event.to_string(),
                    server_id: Some(log_server_id.clone()),
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: Some(details),
                });
            }),
        }
    };
    /*
    CDXC:SessionChatQueueCarriage 2026-08-21:
    Snapshot / replaced / state frames carry the session's prompt queue and
    synced draft, so a client that subscribes mid-session sees the same rows the
    device that queued them sees. Read lazily inside the frame builders, never
    on the reconcile tick.
    */
    let queue_reader: crate::session_chat::SessionChatQueueReader = {
        let paths = state.paths.clone();
        let project_id = project_id.clone();
        let session_id = session_id.clone();
        Arc::new(move || {
            crate::session_chat_queue::read_session_chat_queue_snapshot(
                &paths,
                &project_id,
                &session_id,
            )
        })
    };
    let notice_publisher = crate::session_chat_options::session_chat_terminal_notice_publisher(
        state,
        &project_id,
        &session_id,
    );
    let config = crate::session_chat::SessionChatFollowerConfig {
        project_id,
        session_id,
        agent,
        agent_session_id: read_runtime_text(session, "agentSessionId"),
        agent_session_path: read_runtime_text(session, "agentSessionPath"),
        limit: entry.limit,
        protocol_version: GXSERVER_PROTOCOL_VERSION,
        server_id: state.metadata.server_id.clone(),
        state_reader: Some(state_reader),
        options_reader: Some(options_reader),
        queue_reader: Some(queue_reader),
        successor_hooks: Some(successor_hooks),
        notice_publisher: Some(notice_publisher),
        tuning: crate::session_chat::SessionChatFollowerTuning::default(),
    };
    let event_hub = state.event_hub.clone();
    let emit: crate::session_chat::SessionChatFrameEmitter =
        Arc::new(move |event| event_hub.broadcast(event));
    // CDXC:SessionChatFollowerLiveness 2026-08-24: a fresh heartbeat per task, so
    // the aborted one's last stamp can never be read as the new task's progress.
    let heartbeat = Arc::new(crate::session_chat::SessionChatFollowerHeartbeat::new());
    entry.heartbeat = heartbeat.clone();
    entry.task = Some(tokio::spawn(
        crate::session_chat::run_session_chat_follower(
            config,
            entry.stream.clone(),
            entry.resnapshot.clone(),
            heartbeat,
            emit,
        ),
    ));
}

fn log_session_chat_follower_wedged(state: &AppState, project_id: &str, session_id: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionChatFollowerWedged".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "projectId": project_id,
            "sessionId": session_id,
        })),
    });
}

pub(crate) fn sync_session_chat_followers_for_all_sessions(state: &AppState, reason: &str) {
    let subscribed_keys: Vec<String> = {
        let Ok(followers) = state.session_chat_followers.lock() else {
            return;
        };
        followers
            .iter()
            .filter(|(_, entry)| entry.subscribers > 0)
            .map(|(key, _)| key.clone())
            .collect()
    };
    if subscribed_keys.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions(None) else {
        return;
    };
    let mut seen_keys: HashSet<String> = HashSet::new();
    for session in sessions {
        if let (Some(project_id), Some(session_id)) = (
            read_session_text(&session, "projectId"),
            read_session_text(&session, "sessionId"),
        ) {
            let key = session_observer_key(&project_id, &session_id);
            if subscribed_keys.contains(&key) {
                seen_keys.insert(key);
                sync_session_chat_follower_for_session(state, &session, reason);
            }
        }
    }
    // Subscribed sessions that vanished: stop their tasks. The refcounted
    // entry itself lives until the subscribers unsubscribe or disconnect.
    if let Ok(mut followers) = state.session_chat_followers.lock() {
        for key in subscribed_keys {
            if !seen_keys.contains(&key) {
                if let Some(entry) = followers.get_mut(&key) {
                    if let Some(task) = entry.task.take() {
                        task.abort();
                    }
                }
            }
        }
    }
}

pub(crate) fn stop_session_chat_follower(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    _reason: &str,
) {
    if let Ok(mut followers) = state.session_chat_followers.lock() {
        if let Some(entry) = followers.get_mut(&session_observer_key(project_id, session_id)) {
            if let Some(task) = entry.task.take() {
                task.abort();
            }
        }
    }
    // A killed/slept session's statusline is gone; a stale detection must not
    // outlive it.
    forget_session_chat_options(state, project_id, session_id);
}

pub(crate) fn stop_all_session_chat_followers(state: &AppState) {
    if let Ok(mut followers) = state.session_chat_followers.lock() {
        for (_, mut entry) in followers.drain() {
            if let Some(task) = entry.task.take() {
                task.abort();
            }
        }
    }
    if let Ok(mut cache) = state.session_chat_option_cache.lock() {
        cache.clear();
    }
}

pub(crate) fn subscribe_session_chat_follower(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    limit: usize,
    new_subscriber: bool,
) {
    {
        let Ok(mut followers) = state.session_chat_followers.lock() else {
            return;
        };
        let entry = followers
            .entry(session_observer_key(project_id, session_id))
            .or_insert_with(|| SessionChatFollowerEntry {
                subscribers: 0,
                fingerprint: String::new(),
                limit,
                task: None,
                stream: Arc::new(crate::session_chat::SessionChatStream::new()),
                resnapshot: Arc::new(tokio::sync::Notify::new()),
                heartbeat: Arc::new(crate::session_chat::SessionChatFollowerHeartbeat::new()),
            });
        if new_subscriber {
            entry.subscribers += 1;
        }
        /*
        The window only ever GROWS. Snapshot/replaced frames carry the
        follower's tail window, so a client that already displays 900 rows and
        re-subscribes (reconnect, second host on the same session) must not be
        answered with the 300-row default — that visibly shrinks the list.
        A running follower holds its limit in its spawn config, so a raise
        takes effect by dropping the task: the sync below respawns it, which
        starts a fresh generation and emits the wider snapshot.
        */
        let raised = limit > entry.limit;
        entry.limit = entry.limit.max(limit);
        let task_alive = entry.task.as_ref().is_some_and(|task| !task.is_finished());
        if raised && task_alive {
            if let Some(task) = entry.task.take() {
                task.abort();
            }
        } else if task_alive {
            // Every subscribe is answered with an authoritative snapshot: a
            // live follower re-reads the tail in a fresh generation.
            entry.resnapshot.notify_one();
        }
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
        sync_session_chat_follower_for_session(state, &session, "session-chat-subscribe");
    }
}

pub(crate) fn unsubscribe_session_chat_follower(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let Ok(mut followers) = state.session_chat_followers.lock() else {
        return;
    };
    let key = session_observer_key(project_id, session_id);
    let Some(entry) = followers.get_mut(&key) else {
        return;
    };
    entry.subscribers = entry.subscribers.saturating_sub(1);
    if entry.subscribers == 0 {
        if let Some(task) = entry.task.take() {
            task.abort();
        }
        followers.remove(&key);
    }
}
