use super::*;

pub(crate) struct FollowerFileState {
    pub(super) incremental: SessionChatIncrementalState,
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
        /// appended window (CDXC:AgentScreenDetection), for the notice card.
        api_refusal: Option<String>,
    },
}

/// Authoritative window read: the tail, plus whatever landed while it was being
/// taken. `None` means the file could not be read at all.
fn follower_snapshot_drain(
    file_path: &Path,
    limit: usize,
    decode: SessionChatLineDecoder,
    decode_lifecycle: Option<SessionChatLifecycleDecoder>,
    lineage: Option<SessionChatLineageExtractor>,
    state: &mut FollowerFileState,
    content_replaced: bool,
) -> Option<FollowerDrainOutcome> {
    let mut retried = false;
    loop {
        let mut tail = read_session_chat_transcript_tail_file(
            file_path,
            limit,
            decode,
            false,
            None,
            decode_lifecycle,
            lineage,
        )
        .ok()?;
        state.incremental.rebase(tail.consumed_to);
        state.incremental.codex_stats = tail.codex_stats.clone();
        state
            .incremental
            .seed_queued_prompts(tail.outstanding_queued_prompts.clone());
        state
            .incremental
            .seed_leaf_row_id(tail.newest_tree_row_id.clone());
        state
            .incremental
            .seed_codex_thoughts(tail.codex_thoughts.clone());
        state.incremental.seed_last_message_timestamp(
            tail.messages
                .iter()
                .rev()
                .find_map(|message| message.timestamp),
        );
        // Pick up anything written after consumed_to before we settle.
        let mut appended_lifecycle: Option<SessionChatTurnLifecycle> = None;
        let mut capture_lifecycle =
            |next: SessionChatTurnLifecycle| appended_lifecycle = Some(next);
        let capture_lifecycle: &mut dyn FnMut(SessionChatTurnLifecycle) = &mut capture_lifecycle;
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
        // A delivery can land after the tail captured its queued copy.
        // Apply those retractions to both halves of the snapshot.
        let superseded = state.incremental.take_superseded_prompt_ids();
        if !superseded.is_empty() {
            tail.messages
                .retain(|message| !superseded.contains(&message.id));
            appended.retain(|message| !superseded.contains(&message.id));
        }
        /*
        CDXC:SessionChat 2026-09-02:
        The rewind can be exactly what landed between the tail read and this
        trailing read, and those rows are published raw. One retry re-takes the
        window with them inside it, where the branch rules apply; the second
        pass consumes them, so it cannot ask for a third.
        */
        if state.incremental.take_active_branch_change() && !retried {
            retried = true;
            state.incremental.reset();
            continue;
        }
        return Some(FollowerDrainOutcome::Snapshot {
            tail,
            appended,
            appended_lifecycle,
            content_replaced,
        });
    }
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
    if agent == SessionChatTranscriptAgent::OpenCode {
        crate::session_chat_opencode::refresh_for_path(file_path);
    }
    // Hermes's transcript is a mirror of its SQLite rows; freshen it before the
    // generic file logic reads it so each tick sees the latest turn state. An
    // in-place rewind rewrite swaps the inode, which the identity check below
    // reports as `content_replaced`.
    if agent == SessionChatTranscriptAgent::Zcode {
        crate::session_chat_zcode::sync_zcode_transcript_mirror_for_path(file_path);
    }
    if agent == SessionChatTranscriptAgent::Freebuff {
        crate::session_chat_freebuff::sync_freebuff_transcript_mirror_for_path(file_path);
    }
    if agent == SessionChatTranscriptAgent::Dsh {
        crate::session_chat_dsh::sync_dsh_transcript_mirror_for_path(file_path);
    }
    if agent == SessionChatTranscriptAgent::Hermes {
        crate::session_chat_hermes::sync_hermes_transcript_mirror_for_path(file_path);
    }
    // Cursor's mirror splices the store's thinking into the raw jsonl; same
    // freshen-before-read contract, same rename-on-rewrite signalling.
    if agent == SessionChatTranscriptAgent::Cursor {
        crate::session_chat_cursor_mirror::sync_cursor_transcript_mirror_for_path(file_path);
    }
    // Empryo's mirror splits each turn record into rows; same contract.
    if agent == SessionChatTranscriptAgent::Empryo {
        crate::session_chat_empryo_mirror::sync_empryo_transcript_mirror_for_path(file_path);
    }
    // Antigravity's mirror splits the CLI's step log into chat rows; same
    // freshen-before-read contract, same rename-on-rewrite signalling.
    if agent == SessionChatTranscriptAgent::Antigravity {
        crate::session_chat_antigravity_mirror::sync_antigravity_transcript_mirror_for_path(
            file_path,
        );
    }
    // Codex restored the plain rollout of a compressed one this tails: resolve it again.
    if crate::codex_rollout_archive::is_superseded_mirror(file_path) {
        return FollowerDrainOutcome::Missing;
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

    let mut outcome = if want_snapshot || content_replaced {
        match follower_snapshot_drain(
            file_path,
            limit,
            decode,
            decode_lifecycle,
            lineage,
            state,
            content_replaced,
        ) {
            None => return FollowerDrainOutcome::Missing,
            Some(outcome) => outcome,
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
                CDXC:AgentScreenDetection 2026-08-28:
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
                /*
                CDXC:SessionChat 2026-09-02:
                A prompt that re-attached above the leaf (or an explicit leaf
                marker) makes the client's whole window wrong, not just the
                rows in this drain: the dead branch it has to lose can reach
                back past the top of that window. The append is therefore
                dropped in favour of a fresh generation, which is the same
                path a resubscribe takes and leaves the page memo consistent
                with what the client is now holding.
                */
                if state.incremental.take_active_branch_change() {
                    state.incremental.reset();
                    match follower_snapshot_drain(
                        file_path,
                        limit,
                        decode,
                        decode_lifecycle,
                        lineage,
                        state,
                        true,
                    ) {
                        None => return FollowerDrainOutcome::Missing,
                        Some(outcome) => outcome,
                    }
                } else if batches.is_empty()
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

    if let FollowerDrainOutcome::Snapshot { tail, .. } = &mut outcome {
        if crate::session_chat_fork_stitch::stitch_session_chat_snapshot(
            agent, file_path, limit, tail,
        )
        .is_err()
        {
            return FollowerDrainOutcome::Missing;
        }
    }
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
