use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::agent_transcripts::resolve_session_transcript_path;

use super::*;

pub(crate) struct AgentMetadataTitle {
    agent_session_id: Option<String>,
    provider: &'static str,
    title: String,
    updated_at: Option<String>,
}

pub(crate) struct AgentTitleReconcileResult {
    pub(crate) changed: bool,
    pub(crate) metadata_title_found: bool,
    pub(crate) reason: String,
    pub(crate) session: Option<Value>,
}

pub(crate) fn reconcile_agent_metadata_title_for_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    home_dir: &Path,
    pending_mismatch_status: &str,
) -> Result<bool, DomainStateError> {
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let result =
        reconcile_agent_metadata_title(repository, &lifecycle, home_dir, pending_mismatch_status)?;
    Ok(result.changed)
}

pub(crate) fn reconcile_agent_metadata_title(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    home_dir: &Path,
    pending_mismatch_status: &str,
) -> Result<AgentTitleReconcileResult, DomainStateError> {
    let Some(session) = repository.get_session(&lifecycle.project_id, &lifecycle.session_id)?
    else {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "session-missing".to_string(),
            session: None,
        });
    };
    let runtime_settings = object_field(&session, "runtimeSettings");
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(&session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: runtime_settings.clone(),
        startup_text: None,
    });
    if !is_agent_associated(&session, &identity) {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "not-agent-associated".to_string(),
            session: Some(session),
        });
    }
    let pending_title = read_text_from_map(&runtime_settings, "pendingAgentTitleRequestTitle");
    let pending_requested_at =
        read_text_from_map(&runtime_settings, "pendingAgentTitleRequestRequestedAt");
    let metadata_title = read_agent_metadata_title(home_dir, &session).or_else(|| {
        read_pending_codex_rename_metadata_title(
            home_dir,
            &identity,
            pending_title.as_deref(),
            pending_requested_at.as_deref(),
        )
    });
    let Some(metadata_title) = metadata_title else {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "metadata-title-missing".to_string(),
            session: Some(session),
        });
    };

    let pending_status = pending_title.as_deref().map(|pending_title| {
        if titles_match(pending_title, &metadata_title.title) {
            "confirmed"
        } else {
            pending_mismatch_status
        }
    });
    let mut next_runtime_settings = runtime_settings.clone();
    next_runtime_settings.insert("titleMetadataCheckedAt".to_string(), json!(now_iso()));
    next_runtime_settings.insert(
        "titleMetadataProvider".to_string(),
        json!(metadata_title.provider),
    );
    next_runtime_settings.insert("titleMetadataSource".to_string(), json!("agent-metadata"));
    next_runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
    if let Some(agent_session_id) = metadata_title.agent_session_id.as_deref() {
        next_runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    if let Some(updated_at) = metadata_title.updated_at.as_deref() {
        next_runtime_settings.insert("titleMetadataUpdatedAt".to_string(), json!(updated_at));
    }
    if let Some(status) = pending_status {
        next_runtime_settings.insert("pendingAgentTitleRequestStatus".to_string(), json!(status));
    }
    let needs_update = session.get("title").and_then(Value::as_str)
        != Some(metadata_title.title.as_str())
        || runtime_settings.get("titleSource") != next_runtime_settings.get("titleSource")
        || runtime_settings.get("titleMetadataSource")
            != next_runtime_settings.get("titleMetadataSource")
        || runtime_settings.get("titleMetadataProvider")
            != next_runtime_settings.get("titleMetadataProvider")
        || runtime_settings.get("agentSessionId") != next_runtime_settings.get("agentSessionId")
        || runtime_settings.get("titleMetadataUpdatedAt")
            != next_runtime_settings.get("titleMetadataUpdatedAt")
        || runtime_settings.get("pendingAgentTitleRequestStatus")
            != next_runtime_settings.get("pendingAgentTitleRequestStatus");

    if !needs_update {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: true,
            reason: "metadata-title-already-current".to_string(),
            session: Some(session),
        });
    }

    let mut update = lifecycle_update(lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(next_runtime_settings),
    );
    update.insert("title".to_string(), Value::String(metadata_title.title));
    let updated = repository.update_session(&update)?;
    Ok(AgentTitleReconcileResult {
        changed: true,
        metadata_title_found: true,
        reason: "metadata-title-applied".to_string(),
        session: Some(updated),
    })
}

/*
CDXC:GxserverAgentTitles 2026-08-18:
A rename of an agent session is only confirmed once the Agent CLI writes the
new name into its own session metadata, so every agent Ghostex renames through
`/rename` needs a reader here. Codex publishes `thread_name` in the shared
`session_index.jsonl`; Claude Code writes a `custom-title` record into the
session transcript. While Claude had no reader its renames stayed pending
forever, `title` was never promoted, and the sidebar card kept the previous
name until Claude happened to push an unrelated terminal title.
*/
pub(crate) enum AgentMetadataTitleSource {
    ClaudeTranscript {
        transcript_path: PathBuf,
    },
    CodexSessionIndex {
        agent_session_id: String,
        index_paths: Vec<PathBuf>,
    },
}

impl AgentMetadataTitleSource {
    pub(crate) fn revision_paths(&self) -> Vec<&Path> {
        match self {
            Self::ClaudeTranscript { transcript_path } => vec![transcript_path.as_path()],
            Self::CodexSessionIndex { index_paths, .. } => {
                index_paths.iter().map(PathBuf::as_path).collect()
            }
        }
    }
}

pub(crate) fn read_agent_metadata_title(home_dir: &Path, session: &Value) -> Option<AgentMetadataTitle> {
    match agent_metadata_title_source(home_dir, session)? {
        AgentMetadataTitleSource::ClaudeTranscript { transcript_path } => {
            read_claude_transcript_title(&transcript_path)
        }
        AgentMetadataTitleSource::CodexSessionIndex {
            agent_session_id,
            index_paths,
        } => read_codex_session_index_title(&index_paths, &agent_session_id),
    }
}

pub(crate) fn agent_metadata_title_source(
    home_dir: &Path,
    session: &Value,
) -> Option<AgentMetadataTitleSource> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings,
        startup_text: None,
    });
    let agent_session_id = identity.agent_session_id.as_deref()?.trim();
    if agent_session_id.is_empty() {
        return None;
    }
    match identity.agent_id.as_deref() {
        Some("claude") => Some(AgentMetadataTitleSource::ClaudeTranscript {
            transcript_path: resolve_session_transcript_path(
                "claude",
                Some(agent_session_id),
                identity.agent_session_path.as_deref(),
            )?,
        }),
        Some("codex") => Some(AgentMetadataTitleSource::CodexSessionIndex {
            agent_session_id: agent_session_id.to_string(),
            index_paths: get_codex_session_index_candidate_paths(
                home_dir,
                identity.agent_session_path.as_deref(),
            ),
        }),
        _ => None,
    }
}

pub(crate) fn agent_metadata_title_revision(home_dir: &Path, session: &Value) -> Option<String> {
    let source = agent_metadata_title_source(home_dir, session)?;
    let mut revisions = Vec::new();
    for path in source.revision_paths() {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|modified| {
                modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_nanos())
            })
            .unwrap_or_default();
        revisions.push(format!(
            "{}:{}:{modified_ns}",
            path.to_string_lossy(),
            metadata.len(),
        ));
    }
    (!revisions.is_empty()).then(|| revisions.join("|"))
}

/*
CDXC:GxserverAgentTitles 2026-08-18:
Claude Code rewrites its `custom-title` state record on every turn, so the
current name always sits within the last few kilobytes of a live transcript.
Scan a bounded tail window rather than the whole file: these transcripts reach
several megabytes and the metadata sync pass re-reads every running session's
transcript each second. The transcript belongs to exactly one session, so the
newest record wins without matching the embedded `sessionId`, which diverges
from the resolved identity on resumed and forked Claude sessions.
*/
pub(crate) const CLAUDE_TRANSCRIPT_TITLE_TAIL_BYTES: u64 = 256 * 1024;

pub(crate) fn read_claude_transcript_title(transcript_path: &Path) -> Option<AgentMetadataTitle> {
    let tail = read_transcript_tail_text(transcript_path, CLAUDE_TRANSCRIPT_TITLE_TAIL_BYTES)?;
    for line in tail.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(entry) = entry.as_object() else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("custom-title") {
            continue;
        }
        let title = normalize_metadata_title(entry.get("customTitle"))?;
        return Some(AgentMetadataTitle {
            agent_session_id: None,
            provider: "claude-transcript",
            title,
            updated_at: None,
        });
    }
    None
}

pub(crate) fn read_transcript_tail_text(path: &Path, tail_bytes: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(tail_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity(length.saturating_sub(start) as usize);
    file.read_to_end(&mut bytes).ok()?;
    if start > 0 {
        match bytes.iter().position(|byte| *byte == b'\n') {
            Some(first_newline) => {
                bytes.drain(..=first_newline);
            }
            None => bytes.clear(),
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn read_codex_session_index_title(
    index_paths: &[PathBuf],
    agent_session_id: &str,
) -> Option<AgentMetadataTitle> {
    for index_path in index_paths {
        if let Some(title) = read_codex_session_index_title_from_path(index_path, agent_session_id)
        {
            return Some(title);
        }
    }
    None
}

pub(crate) fn read_codex_session_index_title_from_path(
    index_path: &Path,
    agent_session_id: &str,
) -> Option<AgentMetadataTitle> {
    let text = fs::read_to_string(index_path).ok()?;
    for line in text.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(entry) = entry.as_object() else {
            continue;
        };
        if entry.get("id").and_then(Value::as_str) != Some(agent_session_id) {
            continue;
        }
        let title = normalize_metadata_title(
            entry
                .get("thread_name")
                .or_else(|| entry.get("title"))
                .or_else(|| entry.get("name")),
        )?;
        return Some(AgentMetadataTitle {
            agent_session_id: Some(agent_session_id.to_string()),
            provider: "codex-session-index",
            title,
            updated_at: entry
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        });
    }
    None
}

/*
CDXC:GxserverAgentTitles 2026-08-22:
Plain `codex` launches do not always expose their active rollout through argv,
an open file descriptor, or a hook before the user renames the session. A
pending rename still has an exact, independently written confirmation: Codex
appends that requested title to `session_index.jsonl` after the request time.
Use only that post-request exact-title record, and adopt its session id, so the
sidebar can confirm the rename without guessing from transcript recency.
*/
pub(crate) fn read_pending_codex_rename_metadata_title(
    home_dir: &Path,
    identity: &ResolvedIdentity,
    pending_title: Option<&str>,
    pending_requested_at: Option<&str>,
) -> Option<AgentMetadataTitle> {
    if identity.agent_id.as_deref() != Some("codex") || identity.agent_session_id.is_some() {
        return None;
    }
    let pending_title = pending_title?.trim();
    let requested_at = chrono::DateTime::parse_from_rfc3339(pending_requested_at?.trim()).ok()?;
    for index_path in
        get_codex_session_index_candidate_paths(home_dir, identity.agent_session_path.as_deref())
    {
        let Ok(text) = fs::read_to_string(index_path) else {
            continue;
        };
        for line in text.lines().rev() {
            let Ok(entry) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let Some(title) = normalize_metadata_title(
                entry
                    .get("thread_name")
                    .or_else(|| entry.get("title"))
                    .or_else(|| entry.get("name")),
            ) else {
                continue;
            };
            if !titles_match(pending_title, &title) {
                continue;
            }
            let Some(updated_at) = entry
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Ok(updated_at_parsed) = chrono::DateTime::parse_from_rfc3339(updated_at) else {
                continue;
            };
            if updated_at_parsed < requested_at {
                continue;
            }
            let Some(agent_session_id) = entry
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            return Some(AgentMetadataTitle {
                agent_session_id: Some(agent_session_id.to_string()),
                provider: "codex-session-index-pending-rename",
                title,
                updated_at: Some(updated_at.to_string()),
            });
        }
    }
    None
}

pub(crate) fn get_codex_session_index_candidate_paths(
    home_dir: &Path,
    agent_session_path: Option<&str>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = get_codex_root_from_session_path(agent_session_path) {
        roots.push(root);
    }
    let home_root = home_dir.join(".codex");
    if !roots.iter().any(|root| root == &home_root) {
        roots.push(home_root);
    }
    roots
        .into_iter()
        .map(|root| root.join("session_index.jsonl"))
        .collect()
}

pub(crate) fn get_codex_root_from_session_path(agent_session_path: Option<&str>) -> Option<PathBuf> {
    let normalized_path = agent_session_path?.trim().replace('\\', "/");
    if normalized_path.is_empty() {
        return None;
    }
    let sessions_marker_index = normalized_path.rfind("/sessions/")?;
    (sessions_marker_index > 0).then(|| PathBuf::from(&normalized_path[..sessions_marker_index]))
}

pub(crate) fn normalize_metadata_title(value: Option<&Value>) -> Option<String> {
    let title = get_visible_terminal_title(value?.as_str()?)?
        .trim()
        .to_string();
    (!title.is_empty() && !is_rejected_resume_title(&title)).then_some(title)
}

pub(crate) fn titles_match(left: &str, right: &str) -> bool {
    left.split_whitespace().collect::<Vec<_>>().join(" ")
        == right.split_whitespace().collect::<Vec<_>>().join(" ")
}

