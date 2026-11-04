use super::*;

/// How often a follower may pay for a successor directory scan while the
/// transcript it tails stays substantively stale.
pub(crate) const SUCCESSOR_SCAN_INTERVAL: Duration = Duration::from_millis(30_000);

/// Idle screen refresh remains independent of the faster statusline-file watch.
pub(super) const UNRESOLVED_STEADY_PROBE_INTERVAL: Duration = Duration::from_secs(30);

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/*
CDXC:SessionIdentity 2026-08-02:
Runs when the tailed transcript has had no `user`/`assistant` record for
SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS and re-resolving the stored identity landed
back on that same file. Adoption is persisted through the registry FIRST: if the
write is refused the follower keeps tailing what it has, so the chat can never
show a conversation the rest of the daemon does not agree with (and the next
staleness check cannot flap back and forth between two files).
*/
pub(super) async fn detect_and_adopt_successor_transcript(
    transcript_agent: SessionChatTranscriptAgent,
    config: &SessionChatFollowerConfig,
    stale_path: &Path,
    stored_agent_session_id: Option<&str>,
    logged_notice: &mut Option<String>,
) -> Option<SessionChatIdentityAdoption> {
    let hooks = config.successor_hooks.clone()?;
    let stem = stale_path.file_stem()?.to_str()?;
    /*
    CDXC:SessionIdentity 2026-08-24:
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
        SessionChatTranscriptAgent::Antigravity
        | SessionChatTranscriptAgent::Copilot
        | SessionChatTranscriptAgent::Cursor
        | SessionChatTranscriptAgent::Empryo
        | SessionChatTranscriptAgent::Grok
        | SessionChatTranscriptAgent::Hermes
        | SessionChatTranscriptAgent::OpenCode
        | SessionChatTranscriptAgent::Pi
        | SessionChatTranscriptAgent::Zcode
        | SessionChatTranscriptAgent::Freebuff
        | SessionChatTranscriptAgent::Dsh => return None,
    };
    // The agent is now narrowed to Claude or Codex; a bool keeps the blocking
    // scan below free of arms that could silently absorb a future agent.
    let tails_claude_transcript = transcript_agent == SessionChatTranscriptAgent::Claude;
    let now_ms = now_epoch_ms();
    let stale_substantive_idle_ms = config.tuning.successor_stale_substantive_idle_ms;
    let scan_path = stale_path.to_path_buf();
    let scan_stale_session_id = stale_session_id.clone();
    let bound_agent_session_ids = hooks.bound_agent_session_ids.clone();
    let pending_fork_child_since_ms = hooks.pending_fork_child_since_ms.clone();
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
        let outcome = find(&scan_stale_session_id, &scan_path, last_record_ms, &owned);
        /*
        CDXC:SessionFork 2026-09-02:
        A child forked from this session owns its transcript from the moment it
        launches, even though it cannot say so until its first hook lands. A
        proven successor that began after that launch is the child's
        conversation, so it is reported as owned rather than adopted; see
        `SessionChatSuccessorHooks::pending_fork_child_since_ms`.
        */
        let SessionChatSuccessorOutcome::Found(successor) = outcome else {
            return Some(outcome);
        };
        let Some(pending_since_ms) = pending_fork_child_since_ms(&scan_stale_session_id) else {
            return Some(SessionChatSuccessorOutcome::Found(successor));
        };
        let first_record_ms = if tails_claude_transcript {
            first_substantive_transcript_timestamp_ms(&successor.path)
        } else {
            first_codex_record_timestamp_ms(&successor.path)
        };
        if first_record_ms.is_some_and(|first_ms| first_ms >= pending_since_ms) {
            return Some(SessionChatSuccessorOutcome::OwnedByAnotherSession {
                candidate_session_ids: vec![successor.agent_session_id],
            });
        }
        Some(SessionChatSuccessorOutcome::Found(successor))
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

/// Binds a Freebuff or DeepSeek Harness session that has no conversation id yet to the chat its CLI
/// started (`CDXC:SessionIdentity` in session_chat_freebuff.rs and session_chat_dsh.rs), persisting
/// it through the registry first.
pub(super) async fn adopt_unbound_agent_chat(
    config: &SessionChatFollowerConfig,
    lineage: &'static str,
) -> Option<(String, String)> {
    let hooks = config.successor_hooks.clone()?;
    tokio::task::spawn_blocking(move || {
        let (id, path) = (hooks.unbound_agent_chat)()?;
        let path = path.to_string_lossy().into_owned();
        (hooks.adopt_identity)(SessionChatIdentityAdoption {
            previous_agent_session_id: None,
            predecessor_transcript_session_id: String::new(),
            agent_session_id: id.clone(),
            agent_session_path: path.clone(),
            lineage,
            hops: 0,
        })
        .then_some((id, path))
    })
    .await
    .ok()
    .flatten()
}
