use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::agent_transcripts::resolve_session_transcript_path;
use crate::domain::{DomainRepository, DomainStateError};

use super::*;

pub(crate) struct AgentMetadataTitle {
    agent_session_id: Option<String>,
    provider: &'static str,
    record_revision: Option<String>,
    title: String,
    updated_at: Option<String>,
}

/// CDXC:SessionTitles 2026-09-15 WHY:
/// Custom launch profiles keep their configured agent ID, so matching that ID against provider names skipped metadata sync and left confirmed Claude renames pending forever.
/// Resolve the underlying launch provider only for metadata reads; the session retains its configured identity.
fn metadata_session_identity(session: &Value) -> ResolvedIdentity {
    let runtime_settings = object_field(session, "runtimeSettings");
    let mut identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings,
        startup_text: None,
    });
    if identity
        .agent_id
        .as_deref()
        .is_some_and(|agent_id| agent_id.starts_with("custom-"))
    {
        identity.agent_id = session_launch_agent_provider_id(session);
    }
    identity
}

/*
CDXC:SessionTitles 2026-09-11 WHY:
Older Ghostex versions claimed Codex title jobs while waiting for its provisional 36-character prompt prefix to become a generated name.
Metadata reconciliation still recognizes the provisional prefix to retire those legacy claims when the final title arrives; new Codex sessions never claim a job.
Compare against the raw prompt because Codex truncates the text it received, before Ghostex strips filler.
*/
pub(crate) const CODEX_PROVISIONAL_THREAD_NAME_MAX_CHARS: usize = 36;

pub(crate) fn is_codex_provisional_thread_name(
    first_prompt: Option<&str>,
    thread_name: &str,
) -> bool {
    let Some(first_prompt) = first_prompt else {
        return false;
    };
    let collapsed = first_prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let expected = collapsed
        .chars()
        .take(CODEX_PROVISIONAL_THREAD_NAME_MAX_CHARS)
        .collect::<String>();
    let thread_name = thread_name.trim();
    !expected.is_empty() && (thread_name == expected || thread_name == expected.trim())
}

impl AgentMetadataTitle {
    pub(crate) fn title(&self) -> &str {
        &self.title
    }
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

/// CDXC:SessionTitles 2026-09-10 WHY:
/// Metadata sync and the title worker write the same runtime settings, so the read/modify/write must be atomic to prevent an old running flag from overwriting the worker's completed state.
/// Adopting Codex's final name also finishes its waiting attempt, even if the worker is already gone; manual Generate Name keeps ownership of its replacement title.
pub(crate) fn reconcile_agent_metadata_title(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    home_dir: &Path,
    pending_mismatch_status: &str,
) -> Result<AgentTitleReconcileResult, DomainStateError> {
    let _identity_source =
        crate::session_identity_log::identity_write_source("titleMetadataReconcile");
    let db = repository.connection();
    let transaction = db
        .is_autocommit()
        .then(|| rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate))
        .transpose()
        .map_err(sql_error)?;
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
    let identity = metadata_session_identity(&session);
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
            repository,
            &session,
            home_dir,
            &identity,
            pending_title.as_deref(),
            pending_requested_at.as_deref(),
        )
    });
    let Some(metadata_title) = metadata_title else {
        if let Some(restored) = restore_title_taken_from_another_conversation(
            repository,
            lifecycle,
            &session,
            &runtime_settings,
            identity.agent_session_id.as_deref(),
        )? {
            if let Some(transaction) = transaction {
                transaction.commit().map_err(sql_error)?;
            }
            return Ok(restored);
        }
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "metadata-title-missing".to_string(),
            session: Some(session),
        });
    };

    crate::session_chat_app_command::resolve_latest_session_chat_app_command_title(
        &lifecycle.project_id,
        &lifecycle.session_id,
        &metadata_title.title,
        metadata_title.record_revision.as_deref(),
    );

    // CDXC:Coordinators 2026-10-05 SEE-ALSO: keeps_its_given_title in server/src/coordinators/title.rs; only a rename the user requested (the pending title) may replace a coordinator's or a thread's name.
    let user_requested_title = pending_title
        .as_deref()
        .is_some_and(|pending_title| titles_match(pending_title, &metadata_title.title));
    if !user_requested_title
        && session.get("title").and_then(Value::as_str) != Some(metadata_title.title.as_str())
        && crate::coordinators::keeps_its_given_title(db, &session)
    {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: true,
            reason: "keeps-its-given-title".to_string(),
            session: Some(session),
        });
    }
    // A rename already given up on stays failed until the agent confirms it or the user renames
    // again, so its name is still offered by the next Rename.
    let rename_failed = read_text_from_map(&runtime_settings, "pendingAgentTitleRequestStatus")
        .as_deref()
        == Some("failed");
    let pending_status = pending_title.as_deref().map(|pending_title| {
        if titles_match(pending_title, &metadata_title.title) {
            "confirmed"
        } else if rename_failed {
            "failed"
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
    let source_agent_session_id = metadata_title
        .agent_session_id
        .clone()
        .or_else(|| identity.agent_session_id.clone());
    let current_title = session.get("title").and_then(Value::as_str);
    if current_title != Some(metadata_title.title.as_str()) {
        next_runtime_settings.insert(
            TITLE_METADATA_RESTORE_KEY.to_string(),
            json!({
                "agentSessionId": source_agent_session_id,
                "title": metadata_title.title,
                "previous": {
                    "title": current_title,
                    "titleSource": runtime_settings.get("titleSource"),
                    "titleMetadataSource": runtime_settings.get("titleMetadataSource"),
                    "titleMetadataProvider": runtime_settings.get("titleMetadataProvider"),
                },
            }),
        );
    }
    if let Some(updated_at) = metadata_title.updated_at.as_deref() {
        next_runtime_settings.insert("titleMetadataUpdatedAt".to_string(), json!(updated_at));
    }
    if let Some(status) = pending_status {
        next_runtime_settings.insert("pendingAgentTitleRequestStatus".to_string(), json!(status));
    }
    let completed_codex_auto_title = identity.agent_id.as_deref() == Some("codex")
        && read_text_from_map(&runtime_settings, "gxserverFirstPromptAutoTitleStatus").as_deref()
            == Some("running")
        && !runtime_settings.contains_key("gxserverManualTitleGenerationRequestedAt")
        && !is_codex_provisional_thread_name(
            read_text_from_map(&runtime_settings, "firstUserMessage").as_deref(),
            &metadata_title.title,
        );
    if completed_codex_auto_title {
        next_runtime_settings.remove(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY);
        next_runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("skipped"),
        );
        next_runtime_settings.insert(
            "gxserverFirstPromptAutoTitleReason".to_string(),
            json!("agentAutoTitle"),
        );
    }
    let codex_fork_auto_title_pending = identity.agent_id.as_deref() == Some("codex")
        && runtime_settings
            .get("forkFirstPromptAutoTitlePending")
            .and_then(Value::as_bool)
            == Some(true);
    if codex_fork_auto_title_pending {
        next_runtime_settings.remove("forkFirstPromptAutoTitlePending");
        next_runtime_settings.remove("gxserverForkInitialRenameStatus");
        next_runtime_settings.remove("gxserverForkInitialRenameUpdatedAt");
        next_runtime_settings.insert("autoTitleFromFirstPrompt".to_string(), Value::Bool(true));
    }
    let needs_update = completed_codex_auto_title
        || session.get("title").and_then(Value::as_str) != Some(metadata_title.title.as_str())
        || runtime_settings.get("titleSource") != next_runtime_settings.get("titleSource")
        || runtime_settings.get("titleMetadataSource")
            != next_runtime_settings.get("titleMetadataSource")
        || runtime_settings.get("titleMetadataProvider")
            != next_runtime_settings.get("titleMetadataProvider")
        || runtime_settings.get("agentSessionId") != next_runtime_settings.get("agentSessionId")
        || runtime_settings.get("titleMetadataUpdatedAt")
            != next_runtime_settings.get("titleMetadataUpdatedAt")
        || runtime_settings.get("pendingAgentTitleRequestStatus")
            != next_runtime_settings.get("pendingAgentTitleRequestStatus")
        || runtime_settings.get("forkFirstPromptAutoTitlePending")
            != next_runtime_settings.get("forkFirstPromptAutoTitlePending")
        || runtime_settings.get("gxserverForkInitialRenameStatus")
            != next_runtime_settings.get("gxserverForkInitialRenameStatus")
        || runtime_settings.get("gxserverForkInitialRenameUpdatedAt")
            != next_runtime_settings.get("gxserverForkInitialRenameUpdatedAt")
        || runtime_settings.get("autoTitleFromFirstPrompt")
            != next_runtime_settings.get("autoTitleFromFirstPrompt");

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
    if let Some(transaction) = transaction {
        transaction.commit().map_err(sql_error)?;
    }
    Ok(AgentTitleReconcileResult {
        changed: true,
        metadata_title_found: true,
        reason: "metadata-title-applied".to_string(),
        session: Some(updated),
    })
}

/// Gives up on the user rename requested at `requested_at` when the agent still has not confirmed
/// it: the request is marked `failed`, which drops the name from the title and publishes it as the
/// attempt the next Rename offers. Returns whether it did; a newer or settled request is left alone.
/// SEE-ALSO: `pending_session_rename_title` in server/src/presentation/session_attributes.rs.
pub(crate) fn fail_pending_session_rename(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    requested_at: &str,
) -> Result<bool, DomainStateError> {
    let db = repository.connection();
    let transaction = db
        .is_autocommit()
        .then(|| rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate))
        .transpose()
        .map_err(sql_error)?;
    let Some(session) = repository.get_session(&lifecycle.project_id, &lifecycle.session_id)?
    else {
        return Ok(false);
    };
    let runtime_settings = object_field(&session, "runtimeSettings");
    let current = |key: &str| read_text_from_map(&runtime_settings, key);
    if current("pendingAgentTitleRequestRequestedAt").as_deref() != Some(requested_at)
        || matches!(
            current("pendingAgentTitleRequestStatus").as_deref(),
            Some("confirmed" | "failed")
        )
    {
        return Ok(false);
    }
    let mut next_runtime_settings = runtime_settings.clone();
    next_runtime_settings.insert(
        "pendingAgentTitleRequestStatus".to_string(),
        json!("failed"),
    );
    next_runtime_settings.insert(
        "pendingAgentTitleRequestFailedAt".to_string(),
        json!(now_iso()),
    );
    let mut update = lifecycle_update(lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(next_runtime_settings),
    );
    repository.update_session(&update)?;
    if let Some(transaction) = transaction {
        transaction.commit().map_err(sql_error)?;
    }
    Ok(true)
}

/// What a title taken from agent metadata replaced, and which conversation it was read from.
const TITLE_METADATA_RESTORE_KEY: &str = "titleMetadataRestore";

/// CDXC:SessionTitles 2026-10-05 WHY:
/// A session whose identity briefly pointed at another live session's conversation (a reused Windows pid grafted coordinator G4snt's process tree under thread G0hhy on 2026-10-04) took that conversation's `/rename` title, and nothing ever undid it: once the identity was back, the session's own transcript had no rename record, so reconciliation found no title and kept the wrong one. Every metadata title therefore records the conversation it came from and the title it replaced. When the session's conversation is no longer that one, its own metadata has no title, the title is still the one written, no rename is pending, and that conversation belongs to ANOTHER running or sleeping session, the replaced title comes back. Any other mismatch (`/clear` or `/resume` into a conversation without a name) keeps the title, since the user may have chosen it, and retires the record so the check runs once.
/// SEE-ALSO: server/src/zmx/process_identity.rs `resolve_process_tree_agent_identity`, .dependencies/wmx/src/process_snapshot.rs.
fn restore_title_taken_from_another_conversation(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    session: &Value,
    runtime_settings: &serde_json::Map<String, Value>,
    current_agent_session_id: Option<&str>,
) -> Result<Option<AgentTitleReconcileResult>, DomainStateError> {
    let Some(restore) = runtime_settings
        .get(TITLE_METADATA_RESTORE_KEY)
        .and_then(Value::as_object)
    else {
        return Ok(None);
    };
    let source = restore
        .get("agentSessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let current = current_agent_session_id
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let (Some(source), Some(current)) = (source, current) else {
        return Ok(None);
    };
    if source == current {
        return Ok(None);
    }
    let written_title = restore.get("title").and_then(Value::as_str);
    let still_written =
        written_title.is_some() && session.get("title").and_then(Value::as_str) == written_title;
    let rename_pending =
        read_text_from_map(runtime_settings, "pendingAgentTitleRequestTitle").is_some();
    let previous = restore.get("previous").and_then(Value::as_object);
    let previous_title = previous
        .and_then(|previous| previous.get("title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty());
    let owned_by_another_session = still_written
        && !rename_pending
        && previous_title.is_some()
        && repository
            .list_sessions_with_agent_session_id(source)?
            .iter()
            .any(|owner| {
                owner.get("sessionId").and_then(Value::as_str)
                    != Some(lifecycle.session_id.as_str())
                    && crate::agents::is_active_identity_owner(owner)
            });
    let mut next_runtime_settings = runtime_settings.clone();
    next_runtime_settings.remove(TITLE_METADATA_RESTORE_KEY);
    let mut update = lifecycle_update(lifecycle);
    if owned_by_another_session {
        for key in [
            "titleSource",
            "titleMetadataSource",
            "titleMetadataProvider",
        ] {
            match previous
                .and_then(|previous| previous.get(key))
                .filter(|value| !value.is_null())
            {
                Some(value) => next_runtime_settings.insert(key.to_string(), value.clone()),
                None => next_runtime_settings.remove(key),
            };
        }
        next_runtime_settings.insert("titleMetadataCheckedAt".to_string(), json!(now_iso()));
        update.insert("title".to_string(), json!(previous_title));
    }
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(next_runtime_settings),
    );
    let updated = repository.update_session(&update)?;
    Ok(Some(AgentTitleReconcileResult {
        changed: owned_by_another_session,
        metadata_title_found: false,
        reason: if owned_by_another_session {
            "metadata-title-from-another-session-restored"
        } else {
            "metadata-title-missing"
        }
        .to_string(),
        session: Some(updated),
    }))
}

/*
CDXC:SessionTitles 2026-08-18:
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
    /*
    Hermes keeps its session names in the `sessions` table of its own state
    database rather than in any per-session file, and names a session twice on
    its own: instantly from the opening message, then again once a small model
    upgrades that name. Reading the row is what lets both land on the card.
    */
    HermesStateDb {
        agent_session_id: String,
        state_db_path: PathBuf,
    },
    /*
    CDXC:AgentProviders 2026-09-03:
    Antigravity names every conversation itself about a second after the first
    prompt and writes `title:"…"` to `annotations/<conversationId>.pbtxt`
    under its app data dir; its `/rename <name>` rewrites the same file. That
    file is the only live copy of the name (the summaries database is written
    after the CLI exits), so it is the record the sidebar follows.
    */
    AntigravityAnnotation {
        annotation_path: PathBuf,
    },
    /*
    CDXC:SessionTitles 2026-10-17 WHY:
    OMP 18.11 keeps a session's name in the first line of its session file (`{"type":"title","title":…,"source":"auto"|"user"}`, padded so it is rewritten in place), both the name it gives itself and the one `/rename <title>` sets. Without this source a user's rename was never confirmed and flipped back to OMP's terminal title after 8 seconds.
    */
    OmpSessionHeader {
        session_path: PathBuf,
    },
    /*
    CDXC:SessionTitles 2026-10-17 WHY:
    Grok Build cuts its session name to 40 characters with an ellipsis in the terminal title (`<name…> - grok`), and gxserver never stores an ellipsized window title, so once the ` - grok` suffix was dropped from stored titles a long Grok name could not reach the session row at all. Grok 1.0.62 keeps the whole name in `summary.json` beside the session's `updates.jsonl`: `generated_title` (its own or a `/rename`), else `session_summary`, the same order as its `display_title`.
    */
    GrokSummary {
        summary_path: PathBuf,
    },
}

impl AgentMetadataTitleSource {
    pub(crate) fn revision_paths(&self) -> Vec<&Path> {
        match self {
            Self::ClaudeTranscript { transcript_path } => vec![transcript_path.as_path()],
            Self::CodexSessionIndex { index_paths, .. } => {
                index_paths.iter().map(PathBuf::as_path).collect()
            }
            Self::HermesStateDb { state_db_path, .. } => vec![state_db_path.as_path()],
            Self::AntigravityAnnotation { annotation_path } => vec![annotation_path.as_path()],
            Self::OmpSessionHeader { session_path } => vec![session_path.as_path()],
            Self::GrokSummary { summary_path } => vec![summary_path.as_path()],
        }
    }
}

pub(crate) fn read_agent_metadata_title(
    home_dir: &Path,
    session: &Value,
) -> Option<AgentMetadataTitle> {
    match agent_metadata_title_source(home_dir, session)? {
        AgentMetadataTitleSource::ClaudeTranscript { transcript_path } => {
            read_claude_transcript_title(&transcript_path)
        }
        AgentMetadataTitleSource::CodexSessionIndex {
            agent_session_id,
            index_paths,
        } => read_codex_session_index_title(&index_paths, &agent_session_id).filter(|metadata| {
            let initial_title = provisional_fork_title(session);
            let inherited_title = initial_title
                .as_deref()
                .and_then(|title| title.strip_prefix("Fork: "));
            let pending_title = read_text_from_map(
                &object_field(session, "runtimeSettings"),
                "pendingAgentTitleRequestTitle",
            );
            pending_title
                .as_deref()
                .is_some_and(|title| titles_match(title, &metadata.title))
                || !inherited_title.is_some_and(|title| titles_match(title, &metadata.title))
        }),
        AgentMetadataTitleSource::HermesStateDb {
            agent_session_id,
            state_db_path,
        } => read_hermes_state_db_title(&state_db_path, &agent_session_id),
        AgentMetadataTitleSource::AntigravityAnnotation { annotation_path } => {
            read_antigravity_annotation_title(&annotation_path)
        }
        AgentMetadataTitleSource::OmpSessionHeader { session_path } => {
            read_omp_session_header_title(&session_path)
        }
        // A fork's summary starts as a copy of its parent's, so the inherited name waits behind
        // the provisional `Fork: ` title (CDXC:SessionFork) the way Codex's does.
        AgentMetadataTitleSource::GrokSummary { summary_path } => {
            read_grok_summary_title(&summary_path).filter(|metadata| {
                let initial_title = provisional_fork_title(session);
                let inherited_title = initial_title
                    .as_deref()
                    .and_then(|title| title.strip_prefix("Fork: "));
                !inherited_title.is_some_and(|title| titles_match(title, &metadata.title))
            })
        }
    }
}

pub(crate) fn agent_metadata_title_source(
    home_dir: &Path,
    session: &Value,
) -> Option<AgentMetadataTitleSource> {
    let identity = metadata_session_identity(session);
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
        Some("hermes-agent")
            if crate::session_chat_hermes::is_safe_hermes_session_id(agent_session_id) =>
        {
            Some(AgentMetadataTitleSource::HermesStateDb {
                agent_session_id: agent_session_id.to_string(),
                state_db_path: crate::session_chat_hermes::hermes_state_db_path(
                    &crate::session_chat_hermes::hermes_home(),
                    agent_session_id,
                ),
            })
        }
        Some("antigravity")
            if crate::session_chat_antigravity_mirror::is_safe_antigravity_session_id(
                agent_session_id,
            ) =>
        {
            Some(AgentMetadataTitleSource::AntigravityAnnotation {
                annotation_path: crate::session_chat_antigravity_mirror::antigravity_app_data_dir()
                    .join("annotations")
                    .join(format!("{agent_session_id}.pbtxt")),
            })
        }
        Some("omp") => Some(AgentMetadataTitleSource::OmpSessionHeader {
            session_path: PathBuf::from(identity.agent_session_path.as_deref()?)
                .canonicalize()
                .ok()
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(&format!("_{agent_session_id}.jsonl")))
                })?,
        }),
        Some("grok") => Some(AgentMetadataTitleSource::GrokSummary {
            summary_path: Path::new(identity.agent_session_path.as_deref()?)
                .parent()
                .filter(|folder| {
                    folder
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name == agent_session_id)
                })?
                .join("summary.json"),
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
CDXC:SessionTitles 2026-08-18:
Claude Code rewrites its `custom-title` state record on every turn, so the
current name always sits within the last few kilobytes of a live transcript.
Scan a bounded tail window rather than the whole file: these transcripts reach
several megabytes and the metadata sync pass re-reads every running session's
transcript each second. The transcript belongs to exactly one session, so the
newest record wins without matching the embedded `sessionId`, which diverges
from the resolved identity on resumed and forked Claude sessions.
*/
pub(crate) const CLAUDE_TRANSCRIPT_TITLE_TAIL_BYTES: u64 = 256 * 1024;

/// One transcript's newest `custom-title` record among its complete lines, as of `end`.
#[derive(Clone)]
struct ClaudeTitleScan {
    identity: String,
    /// The end of the last complete line read.
    end: u64,
    /// A hash of the bytes just before `end`, so a rewritten transcript is read again in full.
    guard: u64,
    /// The newest `custom-title` record before `end`: its line's offset and its normalized title,
    /// `None` when that title does not normalize (which hides every older record too).
    newest: Option<(u64, Option<String>)>,
}

/// How many bytes before a cached scan's end must be unchanged for the scan to resume there.
const CLAUDE_TITLE_RESUME_GUARD_BYTES: u64 = 256;

fn claude_title_resume_guard(file: &fs::File, end: u64) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let start = end.saturating_sub(CLAUDE_TITLE_RESUME_GUARD_BYTES);
    let mut bytes = vec![0; (end - start) as usize];
    crate::session_chat::read_exact_at(file, &mut bytes, start).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}

/// The `custom-title` record a transcript line holds: `Some(title)` with the title normalized
/// (itself `None` when it does not normalize), or `None` for any other line.
fn claude_custom_title_line(line: &[u8]) -> Option<Option<String>> {
    let line = String::from_utf8_lossy(line);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let entry = serde_json::from_str::<Value>(line).ok()?;
    let entry = entry.as_object()?;
    if entry.get("type").and_then(Value::as_str) != Some("custom-title") {
        return None;
    }
    Some(normalize_metadata_title(entry.get("customTitle")))
}

/// CDXC:SessionTitles 2026-10-23 WHY:
/// The tail window above was read and every line in it parsed on each pass, so a working Claude session (whose transcript changes every second) cost a 256 KB read and parse per second. The newest `custom-title` record among the complete lines is now remembered with the offset the read reached, and while the transcript only grows (same file, unchanged bytes before that offset) only the appended bytes are read: a record among them is newer, and otherwise the remembered one stands while it is still inside the window. The answer is the same as the full window read: the newest `custom-title` line that starts inside the last 256 KB, the unterminated last line included, and `None` when that record's title does not normalize.
pub(crate) fn read_claude_transcript_title(transcript_path: &Path) -> Option<AgentMetadataTitle> {
    static SCANS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, ClaudeTitleScan>>,
    > = std::sync::OnceLock::new();
    let scans = SCANS.get_or_init(Default::default);
    let identity = crate::session_chat::read_transcript_file_version(transcript_path)
        .ok()?
        .identity;
    let mut file = fs::File::open(transcript_path).ok()?;
    let length = file.metadata().ok()?.len();
    let window_start = length.saturating_sub(CLAUDE_TRANSCRIPT_TITLE_TAIL_BYTES);
    // A line counts when it starts inside the window; a partial first line is cut off.
    let in_window = |offset: u64| window_start == 0 || offset > window_start;
    let previous = scans
        .lock()
        .ok()
        .and_then(|scans| scans.get(transcript_path).cloned())
        .filter(|previous| {
            previous.identity == identity
                && previous.end <= length
                && in_window(previous.end)
                && claude_title_resume_guard(&file, previous.end) == Some(previous.guard)
        });
    let read_from = previous
        .as_ref()
        .map_or(window_start, |previous| previous.end);
    file.seek(SeekFrom::Start(read_from)).ok()?;
    let mut bytes = Vec::with_capacity(length.saturating_sub(read_from) as usize);
    file.read_to_end(&mut bytes).ok()?;
    let mut text_start = read_from;
    if previous.is_none() && read_from > 0 {
        let first_newline = bytes.iter().position(|byte| *byte == b'\n')?;
        bytes.drain(..=first_newline);
        text_start += first_newline as u64 + 1;
    }
    let complete = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |last_newline| last_newline + 1);
    let mut newest = None;
    let mut line_end = complete.saturating_sub(1);
    for line in bytes[..complete.saturating_sub(1)].rsplit(|byte| *byte == b'\n') {
        let line_start = line_end - line.len();
        if complete > 0 {
            if let Some(title) = claude_custom_title_line(line) {
                newest = Some((text_start + line_start as u64, title));
                break;
            }
        }
        line_end = line_start.saturating_sub(1);
    }
    if newest.is_none() {
        newest = previous
            .and_then(|previous| previous.newest)
            .filter(|(offset, _)| in_window(*offset));
    }
    let end = text_start + complete as u64;
    if let Some(guard) = claude_title_resume_guard(&file, end) {
        if let Ok(mut scans) = scans.lock() {
            if scans.len() >= 1024 {
                scans.clear();
            }
            scans.insert(
                transcript_path.to_path_buf(),
                ClaudeTitleScan {
                    identity,
                    end,
                    guard,
                    newest: newest.clone(),
                },
            );
        }
    }
    let (offset, title) = claude_custom_title_line(&bytes[complete..])
        .map(|title| (end, title))
        .or(newest)?;
    Some(AgentMetadataTitle {
        agent_session_id: None,
        provider: "claude-transcript",
        record_revision: Some(format!("{}:{offset}", transcript_path.to_string_lossy())),
        title: title?,
        updated_at: None,
    })
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
            record_revision: None,
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
The agent names a session twice on its own — instantly from the opening
message, then again once a small model upgrades that name — and records which
stage a stored name came from. Both stages are adopted, so a card is named the
moment the session starts and sharpens a moment later, and the provenance rides
along in the record revision so an upgrade that keeps the same text is still
seen as the same title rather than a new one.
*/
pub(crate) fn read_hermes_state_db_title(
    state_db_path: &Path,
    agent_session_id: &str,
) -> Option<AgentMetadataTitle> {
    let session_title =
        crate::session_chat_hermes::read_hermes_session_title(state_db_path, agent_session_id)?;
    let title = normalize_metadata_title(Some(&Value::String(session_title.title)))?;
    Some(AgentMetadataTitle {
        agent_session_id: Some(agent_session_id.to_string()),
        provider: "hermes-state-db",
        record_revision: session_title
            .title_source
            .map(|source| format!("{agent_session_id}:{source}")),
        title,
        updated_at: None,
    })
}

/// `title:"…"` from the annotation text proto. The value is a proto string
/// literal, so the usual backslash escapes are decoded; a file without a
/// title field (or with an empty one) means the conversation has no name yet.
pub(crate) fn read_antigravity_annotation_title(
    annotation_path: &Path,
) -> Option<AgentMetadataTitle> {
    let text = fs::read_to_string(annotation_path).ok()?;
    let raw = parse_pbtxt_string_field(&text, "title")?;
    let title = normalize_metadata_title(Some(&Value::String(raw)))?;
    let modified_ns = fs::metadata(annotation_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Some(AgentMetadataTitle {
        agent_session_id: None,
        provider: "antigravity-annotation",
        record_revision: Some(format!(
            "{}:{modified_ns}",
            annotation_path.to_string_lossy()
        )),
        title,
        updated_at: None,
    })
}

/// The name on an OMP session file's first line, read from a bounded head.
pub(crate) fn read_omp_session_header_title(session_path: &Path) -> Option<AgentMetadataTitle> {
    let mut head = Vec::new();
    fs::File::open(session_path)
        .ok()?
        .take(16 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    let head = String::from_utf8_lossy(&head);
    let record = serde_json::from_str::<Value>(head.lines().next()?).ok()?;
    if record.get("type").and_then(Value::as_str) != Some("title") {
        return None;
    }
    let mut title = normalize_metadata_title(record.get("title"))?;
    // A name the user typed comes back inside OMP's own card (`🏷️ OMP: <title>`, its
    // `formatCardTitle`); the user's words are the name.
    if record.get("source").and_then(Value::as_str) == Some("user") {
        if let Some(words) = omp_card_title_words(&title) {
            title = words.to_string();
        }
    }
    let updated_at = record
        .get("updatedAt")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(AgentMetadataTitle {
        agent_session_id: None,
        provider: "omp-session-header",
        record_revision: Some(format!(
            "{}:{}",
            session_path.to_string_lossy(),
            updated_at.as_deref().unwrap_or_default()
        )),
        title,
        updated_at,
    })
}

/// The name Grok Build shows for a session, from its `summary.json`.
pub(crate) fn read_grok_summary_title(summary_path: &Path) -> Option<AgentMetadataTitle> {
    let summary = serde_json::from_str::<Value>(&fs::read_to_string(summary_path).ok()?).ok()?;
    let title = normalize_metadata_title(
        summary
            .get("generated_title")
            .filter(|title| title.as_str().is_some_and(|title| !title.trim().is_empty()))
            .or_else(|| summary.get("session_summary")),
    )?;
    let updated_at = summary
        .get("updated_at")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(AgentMetadataTitle {
        agent_session_id: None,
        provider: "grok-summary",
        record_revision: Some(format!(
            "{}:{}",
            summary_path.to_string_lossy(),
            updated_at.as_deref().unwrap_or_default()
        )),
        title,
        updated_at,
    })
}

/// The title inside an OMP card line, `<icon> <CODE>: <title>` (its `CARD_LINE`: an icon of 1 to
/// 8 non-ASCII characters and a code of 1 to 6 capitals or digits).
fn omp_card_title_words(title: &str) -> Option<&str> {
    let (icon, rest) = title.split_once(' ')?;
    let (code, words) = rest.split_once(": ")?;
    let icon_ok = (1..=8).contains(&icon.chars().count()) && icon.chars().all(|ch| !ch.is_ascii());
    let code_ok = (1..=6).contains(&code.len())
        && code
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit());
    (icon_ok && code_ok && !words.trim().is_empty()).then_some(words)
}

fn parse_pbtxt_string_field(text: &str, field: &str) -> Option<String> {
    let mut rest = text;
    loop {
        let start = rest.find(field)?;
        let after = &rest[start + field.len()..];
        let boundary_ok = start == 0
            || !rest[..start]
                .chars()
                .next_back()
                .is_some_and(|previous| previous.is_ascii_alphanumeric() || previous == '_');
        let after_trimmed = after.trim_start();
        if boundary_ok && after_trimmed.starts_with(':') {
            let value = after_trimmed[1..].trim_start();
            let quote = value.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let mut decoded = String::new();
            let mut chars = value[1..].chars();
            while let Some(character) = chars.next() {
                match character {
                    '\\' => match chars.next()? {
                        'n' => decoded.push('\n'),
                        't' => decoded.push('\t'),
                        'r' => decoded.push('\r'),
                        other => decoded.push(other),
                    },
                    character if character == quote => return Some(decoded),
                    other => decoded.push(other),
                }
            }
            return None;
        }
        rest = after;
    }
}

/*
CDXC:SessionTitles 2026-08-22:
Plain `codex` launches do not always expose their active rollout through argv,
an open file descriptor, or a hook before the user renames the session. A
pending rename still has an exact, independently written confirmation: Codex
appends that requested title to `session_index.jsonl` after the request time.
Use only that post-request exact-title record, and adopt its session id, so the
sidebar can confirm the rename without guessing from transcript recency.
*/
pub(crate) fn read_pending_codex_rename_metadata_title(
    repository: &DomainRepository<'_>,
    session: &Value,
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
    let project_id = read_text_value(session, "projectId")?;
    let session_id = read_text_value(session, "sessionId")?;
    let runtime = object_field(session, "runtimeSettings");
    let fork_context = if read_text_from_map(&runtime, "forkedFromSessionId").is_some() {
        Some(pending_codex_fork_context(repository, session)?)
    } else {
        None
    };
    let process_session_id = fork_context.as_ref().and_then(|_| {
        let name = read_text_value(session, "zmxName")?;
        let identities =
            crate::zmx::read_zmx_session_process_identities(std::slice::from_ref(&name), home_dir)
                .ok()?;
        let process = identities.get(&name)?;
        (process.agent_id.as_deref() == Some("codex") && process.agent_session_path.is_some())
            .then(|| process.agent_session_id.clone())
            .flatten()
    });
    if let Some((parent_id, cwd)) = &fork_context {
        if process_session_id.is_none() {
            let sessions = repository.list_sessions_excluding_stopped(None).ok()?;
            for other in sessions {
                if other["projectId"] == session["projectId"]
                    && other["sessionId"] == session["sessionId"]
                {
                    continue;
                }
                let other_runtime = object_field(&other, "runtimeSettings");
                let pending_fork =
                    matches!(
                        read_text_from_map(&other_runtime, "gxserverForkInitialRenameStatus")
                            .as_deref(),
                        Some("pending" | "applied")
                    ) || read_text_from_map(&other_runtime, "pendingAgentTitleRequestStatus")
                        .as_deref()
                        == Some("pending");
                if pending_fork
                    && metadata_session_identity(&other).agent_session_id.is_none()
                    && pending_codex_fork_context(repository, &other)
                        .is_some_and(|context| context == (parent_id.clone(), cwd.clone()))
                {
                    return None;
                }
            }
        }
    }
    let created_at = fork_context
        .as_ref()
        .map(|_| chrono::DateTime::parse_from_rfc3339(session["createdAt"].as_str()?).ok());
    let created_at = match created_at {
        Some(value) => Some(value?),
        None => None,
    };
    let mut candidate = None;
    let mut seen_ids = std::collections::HashSet::new();
    for index_path in
        get_codex_session_index_candidate_paths(home_dir, identity.agent_session_path.as_deref())
    {
        let Ok(text) = fs::read_to_string(&index_path) else {
            continue;
        };
        for line in text.lines().rev() {
            let Ok(entry) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let Some(agent_session_id) = entry
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            if !seen_ids.insert(agent_session_id.to_ascii_lowercase())
                || process_session_id
                    .as_deref()
                    .is_some_and(|id| id != agent_session_id)
            {
                continue;
            }
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
            let owned: bool = repository.connection().query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE (projectId <> ?1 OR sessionId <> ?2) AND lower(trim(json_extract(runtimeSettingsJson, '$.agentSessionId'))) = lower(?3))",
                rusqlite::params![project_id, session_id, agent_session_id],
                |row| row.get(0),
            ).ok()?;
            if owned {
                continue;
            }
            if let Some((parent_id, cwd)) = &fork_context {
                let matches_fork = crate::resume_lookup::codex_transcript_paths(
                    index_path.parent()?,
                    agent_session_id,
                    None,
                )
                .iter()
                .any(|path| {
                    let Some(meta) = crate::session_chat_successor::read_codex_session_meta(path)
                    else {
                        return false;
                    };
                    meta.session_id == agent_session_id
                        && !meta.is_subagent
                        && meta.forked_from_id.as_deref() == Some(parent_id.as_str())
                        && meta
                            .cwd
                            .as_deref()
                            .and_then(canonical_codex_fork_cwd)
                            .as_ref()
                            == Some(cwd)
                        && meta
                            .timestamp
                            .as_deref()
                            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                            .zip(created_at)
                            .is_some_and(|(created, requested)| created >= requested)
                });
                if !matches_fork {
                    continue;
                }
            }
            if candidate.is_some() {
                return None;
            }
            candidate = Some(AgentMetadataTitle {
                agent_session_id: Some(agent_session_id.to_string()),
                provider: "codex-session-index-pending-rename",
                record_revision: None,
                title,
                updated_at: Some(updated_at.to_string()),
            });
        }
    }
    candidate
}

/// CDXC:SessionFork 2026-09-23 WHY:
/// Automatic forks share a title, so a title acknowledgement alone cannot identify their owner. Require the exact parent and canonical folder, reject competing unresolved forks and already-owned conversations, and prefer an exact live process identity when available.
fn pending_codex_fork_context(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Option<(String, String)> {
    if metadata_session_identity(session).agent_id.as_deref() != Some("codex") {
        return None;
    }
    let runtime = object_field(session, "runtimeSettings");
    let parent = repository
        .get_session(
            &read_text_value(session, "projectId")?,
            &read_text_from_map(&runtime, "forkedFromSessionId")?,
        )
        .ok()??;
    let parent_identity = metadata_session_identity(&parent);
    if parent_identity.agent_id.as_deref() != Some("codex") {
        return None;
    }
    Some((
        parent_identity.agent_session_id?,
        canonical_codex_fork_cwd(&read_text_value(session, "cwd")?)?,
    ))
}

fn canonical_codex_fork_cwd(cwd: &str) -> Option<String> {
    let canonical = fs::canonicalize(cwd).ok()?.to_string_lossy().into_owned();
    #[cfg(windows)]
    let canonical = canonical.to_lowercase();
    Some(canonical)
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

pub(crate) fn get_codex_root_from_session_path(
    agent_session_path: Option<&str>,
) -> Option<PathBuf> {
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
