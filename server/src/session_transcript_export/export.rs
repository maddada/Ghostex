use super::*;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Everything the export engine needs. Field names mirror what a server
/// handler already has in hand: the agent name from
/// `session_chat_agent_for_session`, `agentSessionId`/`agentSessionPath` from
/// `runtimeSettings`, and the Ghostex session id/title from the session record.
#[derive(Clone, Copy, Debug)]
pub struct SessionTranscriptExportRequest<'a> {
    /// Provider name as stored on the session (`claude`, `openclaude`,
    /// `codex`, `grok`, `pi`, `omp`).
    pub agent: Option<&'a str>,
    pub agent_session_id: Option<&'a str>,
    pub agent_session_path: Option<&'a str>,
    /// Ghostex session id — only used to name the file.
    pub session_id: &'a str,
    pub session_title: Option<&'a str>,
    /// Exports directory (the caller owns the app-data lookup, so the engine
    /// stays independent of `AppState`).
    pub exports_dir: &'a Path,
    /// `None` uses `SessionTranscriptExportSelection::default()`.
    pub selection: Option<&'a SessionTranscriptExportSelection>,
    /// Ghostex's data folder (pasted files, images, earlier handoffs) and the session's project:
    /// files the transcript names are copied next to it from the first, never from the second
    /// (`carry_files.rs`). `None` data folder: nothing is copied.
    pub data_dir: Option<&'a Path>,
    pub project_path: Option<&'a Path>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTranscriptExportOutcome {
    /// Absolute path of the written markdown file.
    pub path: PathBuf,
    pub bytes: usize,
    /// Transcript the export was parsed from (successor-adopted for Claude).
    pub source_path: PathBuf,
    pub agent: SessionChatTranscriptAgent,
    /// Records that survived the selection, i.e. what the file actually shows.
    pub rendered_entries: usize,
    /// Records parsed from the transcript, whether rendered or not.
    pub parsed_entries: usize,
    /// The conversation's turns, and how many of them the file holds.
    pub turns: usize,
    pub kept_turns: usize,
    /// Files the transcript named, copied next to it, its links pointing at the copies.
    pub carried_files: Vec<CarriedFile>,
}

/// What the dialog's History estimate reads: the conversation's turns, and the file's size for
/// each "last N turns" and for all of it, under the asked-for Include selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTranscriptTurnCounts {
    pub turns: usize,
    pub whole_bytes: usize,
    /// Index `i` is the size of the last `i + 1` turns.
    pub last_turns_bytes: Vec<usize>,
}

/// Counts what [`export_session_transcript`] would write, without writing anything.
pub fn count_session_transcript(
    request: &SessionTranscriptExportRequest<'_>,
) -> Result<SessionTranscriptTurnCounts, SessionTranscriptExportError> {
    let loaded = load_transcript(request)?;
    let parts = render_parts(&loaded.title, &loaded.transcript, loaded.selection);
    Ok(SessionTranscriptTurnCounts {
        turns: parts.turn_count(),
        whole_bytes: parts.whole_bytes(),
        last_turns_bytes: parts.last_turns_bytes(),
    })
}

struct LoadedTranscript<'a> {
    agent: SessionChatTranscriptAgent,
    source_path: PathBuf,
    transcript: ParsedTranscript,
    selection: &'a SessionTranscriptExportSelection,
    title: String,
}

/// Distinct, non-degrading failures. There is deliberately no "export what we
/// could" path: a half-transcript handed to the next agent is worse than a
/// clear error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionTranscriptExportError {
    /// The session's agent has no transcript format we can parse.
    UnsupportedAgent {
        agent: Option<String>,
    },
    /// Neither `agentSessionId` nor a usable `agentSessionPath` was supplied.
    MissingSessionReference,
    /// The agent is supported but no transcript file exists for the session.
    TranscriptNotFound {
        agent: SessionChatTranscriptAgent,
        agent_session_id: Option<String>,
    },
    TranscriptUnreadable {
        path: PathBuf,
    },
    /// The transcript exists but holds nothing the export can render.
    EmptyTranscript {
        path: PathBuf,
    },
    ExportsDirectoryUnwritable {
        path: PathBuf,
    },
    WriteFailed {
        path: PathBuf,
    },
}

impl SessionTranscriptExportError {
    /// Matches the `DomainStateError` codes the RPC layer already speaks.
    pub fn code(&self) -> &'static str {
        match self {
            SessionTranscriptExportError::UnsupportedAgent { .. } => "unsupportedAgent",
            SessionTranscriptExportError::MissingSessionReference => "invalidParams",
            SessionTranscriptExportError::TranscriptNotFound { .. } => "transcriptNotFound",
            SessionTranscriptExportError::TranscriptUnreadable { .. } => "transcriptUnreadable",
            SessionTranscriptExportError::EmptyTranscript { .. } => "transcriptEmpty",
            SessionTranscriptExportError::ExportsDirectoryUnwritable { .. }
            | SessionTranscriptExportError::WriteFailed { .. } => "internalError",
        }
    }
}

impl std::fmt::Display for SessionTranscriptExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionTranscriptExportError::UnsupportedAgent { agent } => match agent {
                Some(agent) => write!(
                    formatter,
                    "Exporting a transcript is not supported for {agent} sessions."
                ),
                None => write!(
                    formatter,
                    "This session has no agent, so there is no transcript to export."
                ),
            },
            SessionTranscriptExportError::MissingSessionReference => write!(
                formatter,
                "This session has not reported an agent session id yet."
            ),
            SessionTranscriptExportError::TranscriptNotFound {
                agent,
                agent_session_id,
            } => match agent_session_id {
                Some(agent_session_id) => write!(
                    formatter,
                    "No {} transcript file was found for session {agent_session_id}.",
                    agent_display_name(*agent)
                ),
                None => write!(
                    formatter,
                    "No {} transcript file was found for this session.",
                    agent_display_name(*agent)
                ),
            },
            SessionTranscriptExportError::TranscriptUnreadable { path } => {
                write!(
                    formatter,
                    "Could not read the transcript at {}.",
                    path.display()
                )
            }
            SessionTranscriptExportError::EmptyTranscript { path } => write!(
                formatter,
                "The transcript at {} has no conversation to export yet.",
                path.display()
            ),
            SessionTranscriptExportError::ExportsDirectoryUnwritable { path } => write!(
                formatter,
                "Could not create the exports directory at {}.",
                path.display()
            ),
            SessionTranscriptExportError::WriteFailed { path } => {
                write!(
                    formatter,
                    "Could not write the export to {}.",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for SessionTranscriptExportError {}

pub(super) fn agent_display_name(agent: SessionChatTranscriptAgent) -> &'static str {
    match agent {
        SessionChatTranscriptAgent::Antigravity => "Antigravity CLI",
        SessionChatTranscriptAgent::Claude => "Claude",
        SessionChatTranscriptAgent::Codex => "Codex",
        SessionChatTranscriptAgent::Copilot => "GitHub Copilot",
        SessionChatTranscriptAgent::Cursor => "Cursor CLI",
        SessionChatTranscriptAgent::Empryo => "Empryo",
        SessionChatTranscriptAgent::Grok => "Grok",
        SessionChatTranscriptAgent::Hermes => "Hermes Agent",
        SessionChatTranscriptAgent::OpenCode => "OpenCode",
        SessionChatTranscriptAgent::Pi => "Pi",
        SessionChatTranscriptAgent::Zcode => "ZCode",
        SessionChatTranscriptAgent::Freebuff => "Freebuff",
        SessionChatTranscriptAgent::Dsh => "DeepSeek Harness",
        SessionChatTranscriptAgent::Gbots => "GBots",
    }
}

fn load_transcript<'a>(
    request: &SessionTranscriptExportRequest<'a>,
) -> Result<LoadedTranscript<'a>, SessionTranscriptExportError> {
    let agent = resolve_session_chat_transcript_agent(request.agent).ok_or_else(|| {
        SessionTranscriptExportError::UnsupportedAgent {
            agent: request
                .agent
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    })?;
    let agent_session_id = request
        .agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_session_path = request
        .agent_session_path
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if agent_session_id.is_none() && agent_session_path.is_none() {
        return Err(SessionTranscriptExportError::MissingSessionReference);
    }

    let source_path = resolve_export_transcript_path(agent, agent_session_id, agent_session_path)
        .ok_or_else(|| SessionTranscriptExportError::TranscriptNotFound {
        agent,
        agent_session_id: agent_session_id.map(str::to_string),
    })?;

    let lines = read_lines_lossy(&source_path).ok_or_else(|| {
        SessionTranscriptExportError::TranscriptUnreadable {
            path: source_path.clone(),
        }
    })?;
    let mut transcript = parse_transcript(agent, &source_path, &lines);
    transcript.meta.agent = agent;
    transcript.meta.source_path = source_path.clone();
    if transcript.meta.agent_session_id.is_none() {
        transcript.meta.agent_session_id = agent_session_id.map(str::to_string);
    }

    let selection = request.selection.unwrap_or(&DEFAULT_SELECTION);
    /*
    A transcript that exists but has produced nothing the selection renders is
    reported as empty rather than exported: a file holding only the metadata
    header would look like a successful export while transferring no context at
    all.
    */
    let has_conversation = transcript.entries.iter().any(|entry| {
        entry.section != TranscriptExportSection::SessionMeta && selection.includes(entry.section)
    });
    if !has_conversation {
        return Err(SessionTranscriptExportError::EmptyTranscript { path: source_path });
    }

    let title = export_title(request.session_title, &transcript);
    Ok(LoadedTranscript {
        agent,
        source_path,
        transcript,
        selection,
        title,
    })
}

static DEFAULT_SELECTION: std::sync::LazyLock<SessionTranscriptExportSelection> =
    std::sync::LazyLock::new(SessionTranscriptExportSelection::default);

/// Parse the session's transcript and write the markdown export.
pub fn export_session_transcript(
    request: &SessionTranscriptExportRequest<'_>,
) -> Result<SessionTranscriptExportOutcome, SessionTranscriptExportError> {
    let LoadedTranscript {
        agent,
        source_path,
        transcript,
        selection,
        title,
    } = load_transcript(request)?;
    let rendered_entries = transcript
        .entries
        .iter()
        .filter(|entry| selection.includes(entry.section))
        .count();
    let parts = render_parts(&title, &transcript, selection);
    let turns = parts.turn_count();
    let kept_turns = selection.last_turns.map_or(turns, |last| last.min(turns));
    let markdown = parts.assemble(selection.last_turns);

    let path = unique_export_path(request.exports_dir, &title, request.session_id)?;
    let (markdown, carried_files) = match request.data_dir {
        Some(data_dir) => {
            let roots = CarryRoots::new(Some(data_dir), request.project_path, &source_path);
            let (head, body) = markdown.split_at(parts.head_len());
            let folder = carried_files_folder(&path);
            let (body, carried) = carry_referenced_files(body, &roots, &folder);
            (
                format!("{head}{}{body}", carried_files_block(&carried)),
                carried,
            )
        }
        None => (markdown, Vec::new()),
    };
    let bytes = markdown.len();
    fs::write(&path, markdown.as_bytes())
        .map_err(|_| SessionTranscriptExportError::WriteFailed { path: path.clone() })?;

    Ok(SessionTranscriptExportOutcome {
        path,
        bytes,
        source_path,
        agent,
        rendered_entries,
        parsed_entries: transcript.entries.len(),
        turns,
        kept_turns,
        carried_files,
    })
}

/*
Claude resumes and compactions continue a conversation in a NEW file that
carries the stale session id in its head records; chat view adopts the
successor, so export has to as well or a resumed session exports the abandoned
half. `owned_session_ids` is empty here on purpose: export is a one-shot read
with no follower registry to consult, and reading a transcript another live
session is bound to is harmless.
*/
fn resolve_export_transcript_path(
    agent: SessionChatTranscriptAgent,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Option<PathBuf> {
    /*
    CDXC:AgentProviders 2026-08-22: chat follows grok's live
    `updates.jsonl`, but an export renders a FINISHED conversation, and the
    parser below reads the persisted `chat_history.jsonl` records. Resolve that
    file directly rather than the shared chat path.
    */
    if agent == SessionChatTranscriptAgent::Grok {
        return agent_session_id.and_then(crate::session_chat::find_grok_chat_history);
    }
    let path = resolve_session_chat_transcript_path(agent, agent_session_id, agent_session_path)?;
    if agent != SessionChatTranscriptAgent::Claude {
        return Some(path);
    }
    let Some(session_id) = agent_session_id else {
        return Some(path);
    };
    let last_substantive_ms = last_substantive_transcript_timestamp_ms(&path).unwrap_or_default();
    match find_claude_successor_transcript(session_id, &path, last_substantive_ms, &[]) {
        SessionChatSuccessorOutcome::Found(successor) => Some(successor.path),
        _ => Some(path),
    }
}
