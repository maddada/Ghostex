use super::*;

fn prompt_stash_request_path(state_dir: &Path, project_id: &str, session_id: &str) -> PathBuf {
    state_dir
        .join("prompt-stash-requests")
        .join(format!("{project_id}-{session_id}"))
}

fn prompt_handoff_response_path(state_dir: &Path, request_id: &str) -> PathBuf {
    state_dir
        .join("prompt-handoffs")
        .join(format!("{request_id}.json"))
}

/// What the CLI's prompt-editor handshake reported about the composer draft it
/// just moved out of the agent TUI. `prompt_id` is `None` when the composer was
/// empty; `created` marks a stash row this capture owns (as opposed to an
/// update of an existing one), so a caller that only wanted the text can delete
/// it again without destroying a prompt the user had stashed themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedTerminalDraft {
    pub content: Option<String>,
    pub draft_version: Option<crate::session_chat_draft_versions::DraftVersion>,
    pub created: bool,
    pub prompt_id: Option<String>,
    /// What a `PullClaudeQueue` or `PullGrokQueue` step took out of the agent's own queue, oldest
    /// first.
    pub queued_messages: Vec<String>,
}

/*
CDXC:Drafts 2026-08-18:
Terminal → chat draft transfer, shared by every host. The bytes a user typed
into the agent TUI live only in that TUI's composer, so the sole way to read
them is the agent's prompt-editor contract: drop a one-shot `handoff:<id>`
marker, ask the agent to open its external editor, and let `ghostex
prompt-editor` stash the composer into Saved Prompts, clear it, and answer
through the response file. Running it here rather than in each client is what
lets remote gpui sessions and the phone use it at all; they have no filesystem
on the agent's machine.
*/
pub(super) async fn preserve_terminal_draft(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    prompt_editor_input: &str,
    generation: &AtomicU64,
    job_generation: u64,
) -> Result<CapturedTerminalDraft, SessionChatSendError> {
    run_terminal_draft_capture(
        state_dir,
        project_id,
        session_id,
        zmx_name,
        prompt_editor_input,
        Some((generation, job_generation)),
    )
    .await
    .map_err(|message| {
        /*
        CDXC:AgentScreenDetection 2026-08-19:
        The generation, not the message text, is what says whether this send was
        superseded while the handshake ran. Anything else here is the CLI in the
        pane failing to answer its prompt-editor invocation at all, which is
        exactly the evidence the terminal-notice escalation acts on.
        */
        if job_generation != generation.load(Ordering::SeqCst) {
            SessionChatSendError::not_attempted(message)
        } else {
            SessionChatSendError::new(SessionChatSendFailure::PreserveTerminalDraft, message)
        }
    })
}

/*
CDXC:SessionChat 2026-08-24:
Standalone terminal-draft capture for the `/api/handoffSessionChatDraft`
endpoint, as a QUEUED job.

This used to run the handshake directly, on the reasoning that the stash
marker's `create_new` open already excludes a concurrent chat send's preserve
step and that a view switch never races a send from the same client. Both
halves were wrong. The marker only stops two CAPTURES from overlapping; it says
nothing about the rest of a send sequence, so the handoff's editor invocation,
which makes the CLI stash and CLEAR the composer, could land between another
sequence's paste and its Enter and turn that send into an empty-line submit.
And the desktop fires this on every terminal→chat view switch, which is
emphatically not serialized against the sends that same client just made.

Riding the queue cannot self-deadlock: the only caller is the HTTP handler, and
the in-worker draft capture (`SessionChatSendStep::PreserveTerminalDraft`) calls
`run_terminal_draft_capture` directly rather than coming back through here, so
nothing ever enqueues onto the queue it is currently draining.
*/
pub async fn capture_session_chat_terminal_draft(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    agent: Option<&str>,
) -> Result<CapturedTerminalDraft, String> {
    let (completion_tx, completion_rx) = oneshot::channel();
    let (draft_tx, draft_rx) = oneshot::channel();
    queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        "session-chat-draft-handoff",
        vec![SessionChatSendStep::PreserveTerminalDraft {
            replacement: None,
            state_dir: state_dir.to_path_buf(),
            prompt_editor_input: session_chat_prompt_editor_input(agent).to_string(),
        }],
        Some(completion_tx),
        Some(draft_tx),
        None,
    )?;
    completion_rx
        .await
        .map_err(|_| {
            "The session chat send worker stopped before preserving the draft.".to_string()
        })?
        .map_err(|error| error.message)?;
    draft_rx
        .await
        .map_err(|_| "The terminal draft capture reported no result.".to_string())
}

/// The text in the agent's input box (Empryo, ZCode), read inside the session's send queue so no
/// send is typing into it at the time. `None` for an empty box.
pub(crate) async fn read_input_box_terminal_draft(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    agent: &str,
) -> Result<Option<String>, String> {
    let (completion_tx, completion_rx) = oneshot::channel();
    let (draft_tx, draft_rx) = oneshot::channel();
    let mut steps = Vec::new();
    if agent == "empryo" {
        steps.push(SessionChatSendStep::SelectEmpryoTab {
            wait_ms: crate::session_chat_empryo_tabs::EMPRYO_SEND_TAB_WAIT_MS,
        });
    }
    steps.push(SessionChatSendStep::ReadInputBoxDraft {
        agent: agent.to_string(),
    });
    queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        "session-chat-draft-handoff",
        steps,
        Some(completion_tx),
        Some(draft_tx),
        None,
    )?;
    completion_rx
        .await
        .map_err(|_| "The session chat send worker stopped before reading the draft.".to_string())?
        .map_err(|error| error.message)?;
    Ok(draft_rx.await.ok().and_then(|draft| draft.content))
}

/// After OpenCode's editor closes (it applies the editor's result as it exits), asks its Ghostex
/// TUI plugin to run `prompt.clear`.
async fn clear_opencode_prompt(project_id: &str, session_id: &str) {
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (project, session) = (project_id.to_string(), session_id.to_string());
    let cleared = tokio::task::spawn_blocking(move || {
        crate::pi_chat_channel::request(
            &project,
            &session,
            "promptClear",
            serde_json::json!({}),
            Duration::from_secs(5),
        )
    })
    .await
    .unwrap_or(Err(crate::pi_chat_channel::ChannelError::NoAnswer));
    if let Err(reason) = cleared {
        crate::pi_chat_channel::log_screen_fallback(
            project_id,
            session_id,
            "opencodePromptClear",
            &reason.to_string(),
        );
    }
}

/// Close Codex's side conversation when one is on screen; the step itself checks the screen
/// before every Ctrl+C, so this is a no-op anywhere else.
pub fn queue_codex_side_conversation_close(project_id: &str, session_id: &str, zmx_name: &str) {
    if let Err(error) = queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        "session-chat-codex-side-close",
        vec![SessionChatSendStep::DismissClaudePanel {
            agent: Some("codex".to_string()),
            timeout_ms: SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
        }],
        None,
        None,
        None,
    ) {
        log_session_chat_paste_verification(
            LogLevel::Error,
            "sessionChatCodexSideCloseNotQueued",
            project_id,
            session_id,
            zmx_name,
            "session-chat-codex-side-close",
            0,
            SESSION_CHAT_COMPOSER_WAIT_TIMEOUT_MS,
            &error,
        );
    }
}

async fn run_terminal_draft_capture(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    prompt_editor_input: &str,
    cancellation: Option<(&AtomicU64, u64)>,
) -> Result<CapturedTerminalDraft, String> {
    if prompt_editor_input == SESSION_CHAT_GROK_PROMPT_EDITOR_INPUT {
        let screen = capture_session_terminal_text(zmx_name).await;
        if !screen
            .as_deref()
            .is_some_and(grok_draft::has_capturable_draft)
        {
            return Ok(CapturedTerminalDraft::default());
        }
    }
    let request_id = format!(
        "chat-{}-{}",
        std::process::id(),
        SESSION_CHAT_DRAFT_PRESERVE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    );
    let marker_path = prompt_stash_request_path(state_dir, project_id, session_id);
    let response_path = prompt_handoff_response_path(state_dir, &request_id);
    let Some(marker_parent) = marker_path.parent() else {
        return Err("The terminal draft stash path is unavailable.".to_string());
    };
    fs::create_dir_all(marker_parent)
        .map_err(|_| "The terminal draft stash path could not be created.".to_string())?;
    if let Some(response_parent) = response_path.parent() {
        fs::create_dir_all(response_parent)
            .map_err(|_| "The terminal draft response path could not be created.".to_string())?;
    }
    let _ = fs::remove_file(&response_path);
    let stale_marker = fs::metadata(&marker_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > PROMPT_STASH_REQUEST_FRESHNESS);
    if stale_marker {
        let _ = fs::remove_file(&marker_path);
    }
    let mut marker = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&marker_path)
        .map_err(|_| "The terminal draft is already being moved or stashed.".to_string())?;
    if marker
        .write_all(format!("handoff:{request_id}\n").as_bytes())
        .is_err()
    {
        let _ = fs::remove_file(&marker_path);
        return Err("The terminal draft stash request could not be written.".to_string());
    }

    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatPreserveTerminalDraft",
        "session-chat-preserve-draft",
        prompt_editor_input,
    );
    // OpenCode's Ghostex TUI plugin opens its prompt editor through the chat channel
    // (pi_chat_channel.rs), so no key is typed at the TUI.
    let opened_by_plugin = prompt_editor_input == SESSION_CHAT_OPENCODE_PROMPT_EDITOR_INPUT && {
        let (project, session) = (project_id.to_string(), session_id.to_string());
        let opened = tokio::task::spawn_blocking(move || {
            crate::pi_chat_channel::request(
                &project,
                &session,
                "promptEditor",
                serde_json::json!({}),
                Duration::from_secs(5),
            )
        })
        .await
        .unwrap_or(Err(crate::pi_chat_channel::ChannelError::NoAnswer));
        if let Err(reason) = &opened {
            crate::pi_chat_channel::log_screen_fallback(
                project_id,
                session_id,
                "opencodePromptEditor",
                &reason.to_string(),
            );
        }
        opened.is_ok()
    };
    /*
    CDXC:Drafts 2026-11-11 WHY:
    The user wants Ghostex to stop installing OpenCode's plugin, which cleared the TUI's prompt with `prompt.clear` after the editor handed the draft over. OpenCode 2.2.7 keeps its prompt when the editor returns an empty file (`if (!content) return`), but a lone newline normalizes to an empty prompt (`normalizePromptContent`), so where the plugin did not open the editor, the editor hands back a newline and OpenCode clears its own prompt. Ctrl+C was not used: on an empty prompt it exits OpenCode.
    */
    if prompt_editor_input == SESSION_CHAT_OPENCODE_PROMPT_EDITOR_INPUT && !opened_by_plugin {
        let _ = fs::write(
            &marker_path,
            format!("handoff:{request_id}\nempty:newline\n"),
        );
    }
    let delivered = if opened_by_plugin {
        Ok(())
    } else if prompt_editor_input == SESSION_CHAT_GROK_PROMPT_EDITOR_INPUT {
        grok_draft::open_editor(project_id, session_id, zmx_name, cancellation).await
    } else {
        let zmx_name_owned = zmx_name.to_string();
        // OpenCode's editor key is whatever its cli.json binds (session_chat_opencode/config.rs).
        let prompt_editor_input =
            if prompt_editor_input == SESSION_CHAT_OPENCODE_PROMPT_EDITOR_INPUT {
                let config_dir = crate::agent_hooks::config::resolve_config_directory(
                    &crate::resume_lookup::home_dir(),
                    true,
                    "OPENCODE_CONFIG_DIR",
                    ".config/opencode",
                    None,
                );
                match crate::session_chat_opencode::prompt_editor_keys(&config_dir) {
                    Ok(keys) => keys,
                    Err(error) => {
                        let _ = fs::remove_file(&marker_path);
                        return Err(error.message);
                    }
                }
            } else {
                prompt_editor_input.to_string()
            };
        tokio::task::spawn_blocking(move || {
            crate::zmx::session_chat_zmx_write(&zmx_name_owned, &prompt_editor_input)
        })
        .await
        .unwrap_or_else(|_| Err("The terminal draft capture worker stopped.".to_string()))
        .map(|_| ())
    };
    if let Err(message) = delivered {
        let _ = fs::remove_file(&marker_path);
        return Err(message);
    }

    let started = std::time::Instant::now();
    loop {
        let cancelled = cancellation.is_some_and(|(generation, job_generation)| {
            job_generation != generation.load(Ordering::SeqCst)
        });
        if cancelled {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&response_path);
            return Err(SESSION_CHAT_SEND_CANCELLED.to_string());
        }
        if let Ok(text) = fs::read_to_string(&response_path) {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                let _ = fs::remove_file(&response_path);
                if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
                    return Err("The terminal draft could not be saved.".to_string());
                }
                // An empty composer is a successful capture of nothing: the
                // CLI answers `empty` without touching Saved Prompts.
                if response.get("empty").and_then(serde_json::Value::as_bool) == Some(true) {
                    return Ok(CapturedTerminalDraft::default());
                }
                // OpenCode keeps its prompt when the editor hands back an empty file, so the
                // plugin clears it with OpenCode's own `prompt.clear` once the editor has closed;
                // the text is already saved.
                if opened_by_plugin {
                    clear_opencode_prompt(project_id, session_id).await;
                }
                return Ok(CapturedTerminalDraft {
                    content: response
                        .get("content")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    draft_version: response
                        .get("draftVersion")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .and_then(|v| serde_json::from_value(v).ok()),
                    created: response
                        .get("created")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    prompt_id: response
                        .get("promptId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    queued_messages: Vec::new(),
                });
            }
        }
        if started.elapsed() >= SESSION_CHAT_DRAFT_PRESERVE_TIMEOUT {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&response_path);
            return Err("The terminal did not finish preserving its current draft.".to_string());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
