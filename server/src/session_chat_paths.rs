use std::fs::{self, File};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::resume_lookup::{expand_home, home_dir};
use crate::session_chat::*;

/// Hook-supplied `agentSessionPath` wins when it points at an existing .jsonl
/// file; otherwise fall back to the per-agent session-id search. Grok's hooks
/// report `updates.jsonl`, which is exactly the file chat follows
/// (`CDXC:SessionChatGrokUpdates`), so every agent's supplied path is taken
/// as-is now.
pub fn resolve_session_chat_transcript_path(
    agent: SessionChatTranscriptAgent,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = agent_session_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let expanded = expand_home(path);
        if expanded
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("jsonl")
            && expanded.is_file()
        {
            return Some(expanded);
        }
    }
    let session_id = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    match agent {
        SessionChatTranscriptAgent::Claude => find_claude_chat_transcript(session_id),
        SessionChatTranscriptAgent::Codex => {
            crate::agent_transcripts::find_codex_transcript(session_id)
        }
        SessionChatTranscriptAgent::Grok => find_grok_session_update_log(session_id),
        SessionChatTranscriptAgent::Pi => find_pi_family_chat_transcript(session_id),
    }
}

fn configured_agent_directory(env_key: &str, fallback: &str) -> PathBuf {
    std::env::var(env_key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home(&value))
        .unwrap_or_else(|| home_dir().join(fallback))
}

fn find_pi_family_chat_transcript(session_id: &str) -> Option<PathBuf> {
    let pi_agent_dir = configured_agent_directory("PI_CODING_AGENT_DIR", ".pi/agent");
    let omp_agent_dir = std::env::var("PI_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home(&value).join("agent"))
        .unwrap_or_else(|| home_dir().join(".omp/agent"));
    let mut candidates = Vec::new();
    for agent_dir in [pi_agent_dir, omp_agent_dir] {
        collect_pi_family_transcript_candidates(&agent_dir, session_id, &mut candidates);
    }
    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    candidates.into_iter().next().map(|(path, _)| path)
}

fn collect_pi_family_transcript_candidates(
    agent_dir: &Path,
    session_id: &str,
    candidates: &mut Vec<(PathBuf, std::time::SystemTime)>,
) {
    let suffix = format!("_{session_id}.jsonl");
    let mut collect_file = |path: PathBuf| {
        let file_name_matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == format!("{session_id}.jsonl") || name.ends_with(&suffix));
        if !file_name_matches {
            return;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            return;
        };
        if metadata.is_file() {
            candidates.push((
                path,
                metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            ));
        }
    };
    if let Ok(legacy_entries) = fs::read_dir(agent_dir) {
        for entry in legacy_entries.flatten() {
            collect_file(entry.path());
        }
    }
    let sessions_dir = agent_dir.join("sessions");
    let Ok(project_dirs) = fs::read_dir(sessions_dir) else {
        return;
    };
    for project_dir in project_dirs.flatten() {
        let path = project_dir.path();
        if path.is_file() {
            collect_file(path);
            continue;
        }
        let Ok(files) = fs::read_dir(path) else {
            continue;
        };
        for file in files.flatten() {
            collect_file(file.path());
        }
    }
}

/*
Claude filename stems are the camelCase `sessionId`, but hooks report the
snake_case `session_id`, and the two diverge on resumed/forked files
(transcript-format spec §1.1). Try the filename first; when it misses, scan
recent project transcripts for the hook id embedded in their records.
*/
fn find_claude_chat_transcript(session_id: &str) -> Option<PathBuf> {
    if let Some(path) = crate::agent_transcripts::find_claude_transcript(session_id) {
        return Some(path);
    }
    find_claude_transcript_by_embedded_session_id(session_id)
}

const CLAUDE_EMBEDDED_ID_SCAN_FILE_LIMIT: usize = 50;
pub(crate) const CLAUDE_EMBEDDED_ID_SCAN_HEAD_BYTES: u64 = 256 * 1024;

fn find_claude_transcript_by_embedded_session_id(session_id: &str) -> Option<PathBuf> {
    let mut candidates: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for root in crate::agent_transcripts::claude_project_roots() {
        let Ok(project_dirs) = fs::read_dir(&root) else {
            continue;
        };
        for project_dir in project_dirs.flatten() {
            let Ok(files) = fs::read_dir(project_dir.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(metadata) = file.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let modified = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                candidates.push((path, modified));
            }
        }
    }
    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    for (path, _) in candidates
        .into_iter()
        .take(CLAUDE_EMBEDDED_ID_SCAN_FILE_LIMIT)
    {
        if head_declares_session_id(&path, session_id) {
            return Some(path);
        }
    }
    None
}

/// Sub-agent transcripts (`agent-<hash>.jsonl`) record the PARENT session's id
/// in every row, so they match an embedded-id scan for the main session and are
/// usually the newest file in the directory. They are never the session's own
/// transcript.
pub(crate) fn is_claude_sidechain_transcript_name(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.starts_with("agent-"))
}

/*
CDXC:SessionChatCore 2026-08-01:
The scan used to accept any file whose first 256KiB merely CONTAINED the literal
`"session_id":"<id>"`. Transcripts quote each other constantly (orchestrator
prompts, hook payloads, tool results), so an unrelated transcript could be
adopted and the chat would then tail a completely different conversation. The id
must now be the value of a record's OWN top-level `sessionId`/`session_id`
field, sidechain files are rejected outright, and rows flagged `isSidechain` do
not count.
*/
/// Bounded head read that never hands back a torn final line.
pub(crate) fn read_transcript_head_complete_lines(
    path: &Path,
    head_limit_bytes: u64,
) -> Option<String> {
    let file = File::open(path).ok()?;
    let head_length = file
        .metadata()
        .map(|metadata| metadata.len().min(head_limit_bytes))
        .unwrap_or(0) as usize;
    if head_length == 0 {
        return None;
    }
    let mut buffer = vec![0u8; head_length];
    read_exact_at(&file, &mut buffer, 0).ok()?;
    let head = String::from_utf8_lossy(&buffer);
    // The last line of a truncated head is very likely partial: never parse it.
    match head.rfind('\n') {
        Some(end) => Some(head[..end].to_string()),
        None if (head_length as u64) < head_limit_bytes => Some(head.into_owned()),
        None => None,
    }
}

pub(crate) fn head_declares_session_id(path: &Path, session_id: &str) -> bool {
    if is_claude_sidechain_transcript_name(path) {
        return false;
    }
    let Some(complete_head) =
        read_transcript_head_complete_lines(path, CLAUDE_EMBEDDED_ID_SCAN_HEAD_BYTES)
    else {
        return false;
    };
    for line in complete_head.lines() {
        let Some(record) = parse_json_object(line) else {
            continue;
        };
        if record.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        let declared = extract_string(record.get("sessionId"))
            .or_else(|| extract_string(record.get("session_id")));
        if declared.as_deref() == Some(session_id) {
            return true;
        }
    }
    false
}

const GROK_SESSION_ID_MAX_LENGTH: usize = 128;
pub const GROK_SESSION_UPDATE_LOG_FILE: &str = "updates.jsonl";
pub const GROK_CHAT_HISTORY_FILE: &str = "chat_history.jsonl";

pub(crate) fn is_safe_grok_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= GROK_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Grok layout: `$GROK_HOME/sessions/<url-encoded-cwd>/<session-id>/<file>`
/// (with a `summary.json` sidecar in the same directory).
fn find_grok_session_file(session_id: &str, file_name: &str) -> Option<PathBuf> {
    if !is_safe_grok_session_id(session_id) {
        return None;
    }
    let root = configured_agent_directory("GROK_HOME", ".grok").join("sessions");
    let entries = fs::read_dir(&root).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let candidate = entry.path().join(session_id).join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Chat reads the live update stream (`CDXC:SessionChatGrokUpdates`).
fn find_grok_session_update_log(session_id: &str) -> Option<PathBuf> {
    find_grok_session_file(session_id, GROK_SESSION_UPDATE_LOG_FILE)
}

/// Export reads the persisted conversation instead.
pub fn find_grok_chat_history(session_id: &str) -> Option<PathBuf> {
    find_grok_session_file(session_id, GROK_CHAT_HISTORY_FILE)
}

// ---------------------------------------------------------------------------
// Successor transcript detection (Claude continuation / compaction)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatIdentity 2026-08-02:
Claude Code starts a NEW `<uuid>.jsonl` when a conversation is compacted or
resumed, and the registry only learns the new id from an agent hook. Hooks never
fire for background-job continuations, so a Ghostex session can keep its stored
`agentSessionId` pointing at a conversation that stopped receiving turns days
ago: chat then tails a dead file forever while the pane runs the successor.

Two traps make the naive checks useless:
  * the DEAD file keeps getting appends (`agent-name` / `mode` /
    `permission-mode` records with a null timestamp), so its mtime looks live —
    staleness MUST key on the last SUBSTANTIVE (`user`/`assistant`) record;
  * transcripts quote each other constantly, so "the file mentions the stale id"
    proves nothing. Lineage has to be structural.

The detector therefore proves, per candidate, BOTH:
  1. own identity — a head record whose OWN top-level `sessionId`/`session_id`
     is the candidate's own filename stem (the 6d27b5150 guardrail), and
  2. lineage to the SPECIFIC stale id — either a head record whose own
     `sessionId`/`session_id` field IS the stale id (Claude copies the
     predecessor id into the successor's records), or the compact-continuation
     user record combined with a `file-history-snapshot` inherited from the
     stale session.
Shared cwd is never a lineage signal: many concurrent sessions share one project
directory.

Codex RESUME stays out of scope: it re-plays `session_meta` into the new rollout
with the id the daemon already stores (the "first-session_meta-wins" rule in the
decoders), so the stored identity keeps resolving.

Codex FORK is not (CDXC:SessionChatIdentity 2026-08-24). `codex fork` opens a new
`rollout-<ts>-<uuid>.jsonl` under a NEW id, leaves the predecessor dead, and
never tells the registry — so it froze chat exactly like a Claude compaction.
`find_codex_successor_transcript` handles it, proving lineage from the opening
`session_meta`'s `payload.forked_from_id`.

Grok writes one directory per session id with no continuation mechanism at all.
*/
