use serde_json::{Map, Value};

#[cfg(not(windows))]
mod claude_background;
mod claude_identity;
mod empryo_wake;
mod hermes_profile;
#[cfg(windows)]
mod windows;
use super::*;
#[cfg(not(windows))]
use claude_background::build_claude_attach_or_resume_command;
pub(crate) use empryo_wake::seed_empryo_wake_session;
#[cfg(windows)]
pub(crate) use windows::*;

pub(crate) fn build_agent_resume_plan(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Value {
    if let Some(plan) = crate::agentbox::box_resume_plan(session) {
        return plan;
    }
    let input = to_agent_resume_input(project, session, settings);
    // CDXC:Sessions 2026-09-08 WHY:
    // Detected conversations already have an exact transcript and provider home; resolving by title or the current account could open a different conversation.
    if session
        .pointer("/runtimeSettings/externalSession")
        .and_then(Value::as_bool)
        == Some(true)
    {
        if let (Some(agent @ ("claude" | "codex")), Some(id), Some(home), Some(command)) = (
            input.agent_id.as_deref(),
            input.agent_session_id.as_deref(),
            session
                .pointer("/runtimeSettings/externalAgentHome")
                .and_then(Value::as_str),
            input.agent_command.as_deref(),
        ) {
            let variable = if agent == "codex" {
                "CODEX_HOME"
            } else {
                "CLAUDE_CONFIG_DIR"
            };
            let selector = if agent == "codex" {
                "resume"
            } else {
                "--resume"
            };
            #[cfg(windows)]
            let command = format!(
                "$env:{variable}={}; {command} {selector} {}",
                quote_shell_arg(home),
                quote_shell_arg(id)
            );
            #[cfg(not(windows))]
            let command = format!(
                "env {variable}={} {command} {selector} {}",
                quote_shell_arg(home),
                quote_shell_arg(id)
            );
            let startup = wrap_restored_terminal_resume_command(&command, &command, None);
            return serde_json::json!({
                "agentId": agent, "primaryCommand": command, "displayCommand": command,
                "copyCommand": command, "startupText": as_atuin_ignored_shell_input(&startup),
                "startupTextDisposition": "queueAfterTerminalReady"
            });
        }
    }
    let primary_command =
        build_agent_resume_command(&input, ResumeCommandOptions { display: false });
    let display_command = primary_command
        .as_ref()
        .and_then(|_| build_agent_resume_command(&input, ResumeCommandOptions { display: true }))
        .or_else(|| primary_command.clone());
    let fallback_command = build_agent_resume_fallback_command(&input);
    let copy_command = build_agent_resume_copy_command(&input);
    let mut plan = Map::new();
    insert_optional_string(&mut plan, "agentId", input.agent_id.clone());
    insert_optional_string(&mut plan, "baseCommand", input.agent_lookup_command.clone());
    insert_optional_string(&mut plan, "copyCommand", copy_command);
    insert_optional_string(&mut plan, "displayCommand", display_command.clone());
    insert_optional_string(&mut plan, "fallbackCommand", fallback_command.clone());
    insert_optional_string(
        &mut plan,
        "lookupCommand",
        input.agent_lookup_command.clone(),
    );
    insert_optional_string(&mut plan, "primaryCommand", primary_command.clone());
    insert_optional_string(&mut plan, "runtimeCommand", input.agent_command.clone());
    if let Some(command) = primary_command {
        /*
        CDXC:Zmx 2026-06-22-06:58:
        Provider startup must feed zmx the same restored-session startup script shape as TypeScript gxserver. Wrap daemon-owned resume commands before they reach attach metadata or `startSessionProvider` so wake/start paths print restore context and keep the command in the initial provider startup text instead of changing zmx lifecycle decisions.

        CDXC:AgentProviders 2026-06-22-07:47:
        Resume planning must keep TypeScript's separate primary/display/copy/fallback command roles. Exact Codex restores validate the stored id first, then the startup wrapper can try a trusted-title fallback without making Copy Resume include lookup shell code.
        */
        let startup_text = wrap_restored_terminal_resume_command(
            &command,
            display_command.as_deref().unwrap_or(&command),
            fallback_command.as_deref(),
        );
        plan.insert(
            "startupText".to_string(),
            Value::String(as_atuin_ignored_shell_input(&startup_text)),
        );
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("queueAfterTerminalReady".to_string()),
        );
    } else {
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("none".to_string()),
        );
    }
    Value::Object(plan)
}

pub(crate) fn get_agent_startup_text_for_session(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Option<String> {
    /*
    CDXC:ServerApi 2026-06-22-06:53:
    zmx attach metadata must preserve TypeScript's startup-text precedence: explicit renderer text, then queued fresh-launch text, then the daemon-owned agent resume plan shaped by current agent settings. This keeps missing-provider reattach/wake metadata from dropping restorable agent commands after the Rust cutover.
    */
    build_agent_resume_plan(project, session, settings)
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: account switching submits one clean account-specific resume command in the existing terminal, without the restore script or title-lookup fallback.
pub(crate) fn account_switch_resume_command(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Option<String> {
    let input = to_agent_resume_input(project, session, settings);
    let command = input.agent_command.as_deref()?;
    let resume = match input.agent_id.as_deref()? {
        "claude" => {
            let reference = get_claude_session_reference(&input)?;
            let id = get_claude_session_id(Some(&reference))?;
            if claude_transcript_written(&input, &id) {
                build_claude_resume_invocation(command, &quote_shell_arg(&id))
            } else {
                build_claude_fresh_invocation(command, &quote_shell_arg(&id))
            }
        }
        "codex" => {
            let id = get_codex_session_reference(&input)?;
            if !is_uuid(&id) {
                return None;
            }
            if codex_rollout_written(&input, &id) {
                build_codex_resume_invocation(command, &quote_shell_arg(&id))
            } else {
                command.to_string()
            }
        }
        _ => return None,
    };
    // A raw shell write must be one input line, including custom command arguments.
    (!resume.chars().any(char::is_control)).then_some(resume)
}

/// CDXC:AgentProviders 2026-10-04 WHY:
/// Claude writes no transcript until the first prompt, so switching the account of a session nobody has typed into resumed a conversation that does not exist: Claude printed "No conversation found with session ID" and exited, and the switch failed. Such a session starts fresh under the same id, which keeps the session's conversation identity, title and draft, and lets its hooks report the id Ghostex already stores.
fn claude_transcript_written(input: &AgentResumeInput, id: &str) -> bool {
    input
        .agent_session_path
        .as_deref()
        .map(crate::resume_lookup::expand_home)
        .is_some_and(|path| path.is_file())
        || crate::agent_transcripts::find_claude_transcript(id).is_some()
}

fn build_claude_fresh_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} --session-id {shell_reference}")
}

/// CDXC:SessionIdentity 2026-10-17 WHY:
/// Codex shows a thread id from launch but writes its rollout only once the first prompt arrives (codex-cli 0.163.0), so waking or switching the account of a Codex session nobody had typed into resumed a thread Codex never saved: it printed "No saved session found", Ghostex added "Unable to restore Codex session", the pane fell back to the shell and the message that woke it was lost. Such a session starts Codex fresh (Codex takes no thread id at launch); its hooks then report the new thread id. A stored rollout path that exists, or a rollout found in any Codex home, still resumes, also once Codex has compressed it to `.jsonl.zst` (Codex decompresses it on resume).
fn codex_rollout_written(input: &AgentResumeInput, id: &str) -> bool {
    input
        .agent_session_path
        .as_deref()
        .map(crate::resume_lookup::expand_home)
        .is_some_and(|path| {
            let mut compressed = path.clone().into_os_string();
            compressed.push(".zst");
            path.is_file() || std::path::Path::new(&compressed).is_file()
        })
        || crate::agent_transcripts::find_codex_transcript(id).is_some()
}

/// CDXC:AgentProviders 2026-10-31 WHY:
/// Whether a wake of this Claude or Codex session resumes a conversation the agent wrote (`Some(true)`) or starts the agent fresh because nobody ever prompted it (`Some(false)`): the same transcript checks `account_switch_resume_command` and `build_agent_resume_plan` use. `None` for any other agent, which has no such rule here. Account recovery asks it before typing its "Continue the unfinished task" prompt (accounts/recovery.rs): the agent publishes its session id at launch, so the id alone says nothing about a conversation.
pub(crate) fn agent_conversation_written(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Option<bool> {
    let input = to_agent_resume_input(project, session, settings);
    match input.agent_id.as_deref()? {
        "claude" => {
            let id = get_claude_session_id(get_claude_session_reference(&input).as_deref())?;
            Some(claude_transcript_written(&input, &id))
        }
        "codex" => {
            let id = get_codex_session_reference(&input)?;
            Some(codex_rollout_written(&input, &id))
        }
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct AgentResumeInput {
    agent_command: Option<String>,
    agent_id: Option<String>,
    agent_lookup_command: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    first_user_message: Option<String>,
    project_path: Option<String>,
    /// References the agent said it could not find (`agentResumeRejected`, GitHub #456).
    rejected_references: Vec<String>,
    stored_command_candidates: Vec<String>,
    title: Option<String>,
    title_source: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) struct ResumeCommandOptions {
    display: bool,
}

pub(crate) fn to_agent_resume_input(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> AgentResumeInput {
    let configured_agent_id = read_text_value(session, "agentId");
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    let agent_config = resolve_project_agent_config(
        project,
        configured_agent_id.as_deref().unwrap_or(""),
        Some(&launch_settings),
    );
    /*
    CDXC:AgentProviders 2026-09-16 WHY:
    A custom agent id names a sidebar configuration; its icon declares the CLI family that supplies resume grammar, while its command selects the launcher.
    Supersedes the 2026-08-29 note: saved commands from a different known CLI must not win after live identity adoption, which otherwise pairs the old binary with the new family's grammar and conversation id.
    Commands with no inferable CLI remain usable so custom wrappers keep working; this also repairs previously saved mismatches without modifying the stored row.
    */
    let agent_id = resume_agent_family_id(configured_agent_id, &agent_config, &launch_settings);
    let stored_agent_command = read_text_from_map(&runtime_settings, "agentCommand");
    // A custom Empryo row whose launch settings lost the command still names it here.
    let agent_id = agent_id.map(|id| {
        if is_custom_agent_id(&id) && command_runs_empryo(stored_agent_command.as_deref()) {
            "empryo".to_string()
        } else {
            id
        }
    });
    let configured_agent_command = read_text_from_map(&agent_config, "command");
    let base_command = if let Some(command) =
        read_text_from_map(&runtime_settings, "accountCommand")
            .filter(|command| stored_agent_command_matches_family(agent_id.as_deref(), command))
    {
        reusable_account_command(&command, agent_id.as_deref().unwrap_or_default()).ok()
    } else {
        stored_agent_command
            .clone()
            .filter(|command| {
                !is_one_time_agent_session_command(agent_id.as_deref(), command)
                    && stored_agent_command_matches_family(agent_id.as_deref(), command)
            })
            .or_else(|| {
                configured_agent_command.filter(|command| {
                    !is_one_time_agent_session_command(agent_id.as_deref(), command)
                        && stored_agent_command_matches_family(agent_id.as_deref(), command)
                })
            })
            .or_else(|| {
                agent_id
                    .as_deref()
                    .and_then(default_agent_command)
                    .map(str::to_string)
            })
            .or(stored_agent_command)
    };
    let base_command = base_command.map(|command| {
        hermes_profile::with_session_hermes_profile(
            agent_id.as_deref(),
            command,
            &runtime_settings,
            &launch_settings,
        )
    });
    let base_command = base_command
        .map(|command| with_resume_model_pin(agent_id.as_deref(), project, session, command))
        .map(|command| with_codex_removed_worktree_cwd(agent_id.as_deref(), session, command));
    let runtime_command = agent_id
        .as_ref()
        .and_then(|agent_id| {
            base_command.as_ref().map(|command| {
                resolve_agent_launch_command(
                    agent_id,
                    command,
                    read_text_from_map(&agent_config, "acceptAllMode")
                        .or_else(|| read_text_from_map(&launch_settings, "acceptAllMode"))
                        .as_deref(),
                    settings
                        .get("agentAcceptAllEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    read_text_from_map(&agent_config, "icon")
                        .or_else(|| read_text_from_map(&launch_settings, "icon"))
                        .as_deref(),
                )
            })
        })
        .or_else(|| base_command.clone());
    // A side chat wakes in ask-before-edits as it started (CDXC:SideChats 2026-11-02 in
    // server/src/side_chats/launch.rs).
    let runtime_command = match (agent_id.as_deref(), crate::side_chats::parent_of(session)) {
        (Some(agent_id), Some(_)) => runtime_command
            .map(|command| crate::side_chats::ask_before_edits_command(&command, agent_id)),
        _ => runtime_command,
    };
    // The workspace's own gh and gws sign-ins (crate::connectors), on this run only.
    let runtime_command = runtime_command.map(|command| {
        let command = crate::connectors::with_project_cli_env_for_resume(
            project,
            agent_id.as_deref(),
            command,
        );
        // Claude's Bypass Permissions warning, consented to for a bypass resume (bypass_consent.rs).
        let command = super::with_claude_bypass_consent(agent_id.as_deref(), &command);
        // The home folder's secret folders (home_secret_folders.rs).
        super::with_home_secret_folders_denied(
            agent_id.as_deref(),
            read_text_value(session, "cwd")
                .or_else(|| read_text_value(project, "path"))
                .as_deref(),
            &command,
        )
    });
    let runtime_command = match agent_id.as_deref() {
        Some("dsh") => runtime_command.map(|command| with_dsh_launch_env(&command)),
        _ => runtime_command,
    };
    let agent_session_id = read_text_from_map(&runtime_settings, "agentSessionId");
    let agent_session_path = read_text_from_map(&runtime_settings, "agentSessionPath");
    let (agent_session_id, agent_session_path) = if agent_id.as_deref() == Some("claude") {
        claude_identity::written_claude_identity(
            read_text_value(session, "zmxName").as_deref(),
            agent_session_id,
            agent_session_path,
        )
    } else {
        (agent_session_id, agent_session_path)
    };
    // The session-list lookup must read the same OpenCode 2 the resumed terminal runs.
    let agent_lookup_command = base_command.map(|command| match agent_id.as_deref() {
        Some(id) => crate::agent_cli::opencode::with_opencode2(id, None, &command),
        None => command,
    });
    AgentResumeInput {
        agent_command: runtime_command,
        agent_id,
        agent_lookup_command,
        agent_session_id,
        agent_session_path,
        first_user_message: read_text_from_map(&runtime_settings, "firstUserMessage")
            .or_else(|| read_text_from_map(&launch_settings, "firstUserMessage")),
        project_path: read_text_value(session, "cwd").or_else(|| read_text_value(project, "path")),
        rejected_references: crate::session_chat_queue_runtime::rejected_resume_references(
            &runtime_settings,
        ),
        stored_command_candidates: collect_stored_agent_resume_command_candidates(session),
        title: read_text_value(session, "title"),
        title_source: read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .or_else(|| Some("user".to_string())),
    }
}

/// CDXC:Worktrees 2026-10-15 WHY:
/// Codex resumes into the folder its rollout recorded last, even when that folder was deleted: every shell call then fails with "No such file or directory" while the conversation itself loads (verified with codex-cli 0.162.1). A session moved out of its removed worktree (`plan_removed_worktree_move` in worktree_sessions.rs) therefore names its new folder with `-C`, which Codex takes ahead of the recorded one without asking which folder to use.
fn with_codex_removed_worktree_cwd(
    family: Option<&str>,
    session: &Value,
    command: String,
) -> String {
    if family != Some("codex")
        || session
            .pointer(&format!(
                "/runtimeSettings/{}",
                crate::worktree_sessions::REMOVED_WORKTREE_SESSION_RUNTIME_KEY
            ))
            .is_none()
    {
        return command;
    }
    match read_text_value(session, "cwd") {
        Some(cwd) => format!("{command} -C {}", quote_shell_arg(&cwd)),
        None => command,
    }
}

/// A resumed or woken Claude, Codex or Cursor session comes back on the conversation's own model
/// and effort (CDXC:SessionChat 2026-10-08 in agent_model_pins.rs), which replace the flags its
/// saved command was launched with: a `/model` typed in its terminal, or a chat pick on a session
/// whose account command a pick never rewrote, otherwise came back on the launch model. With
/// nothing remembered, a command that names a model keeps it, and one that names none starts on
/// the agent's default (CDXC:AgentProviders 2026-10-07).
fn with_resume_model_pin(
    family: Option<&str>,
    project: &Value,
    session: &Value,
    command: String,
) -> String {
    let Some(family) = family.and_then(crate::agent_model_pins::pin_family) else {
        return command;
    };
    let agent_session_id = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str);
    let remembered = read_text_value(session, "projectId")
        .or_else(|| read_text_value(project, "projectId"))
        .zip(read_text_value(session, "sessionId"))
        .and_then(|(project_id, session_id)| {
            crate::agent_model_pins::session_choice(
                &project_id,
                &session_id,
                family,
                agent_session_id,
            )
        });
    let pin = match remembered {
        Some(pin) => pin,
        None if command_names_model(&command, family) => return command,
        None => match crate::agent_model_pins::launch_default(family) {
            Some(pin) => pin,
            None => return command,
        },
    };
    let effort = pin.effort.as_deref().filter(|_| family != "cursor");
    with_agent_model_options(&command, family, Some(&pin.model), effort).unwrap_or(command)
}

fn is_one_time_agent_session_command(agent_id: Option<&str>, command: &str) -> bool {
    /*
    CDXC:AgentProviders 2026-08-28:
    Find can create a terminal from an exact Codex history launch such as
    `codex --yolo resume <session>`. Once passive identity detection promotes that
    terminal to a Codex session, the one-time launch command remains in
    runtimeSettings.agentCommand. It is not a reusable agent base command: if
    full reload feeds it back into the ordinary resume planner, the planner
    appends another `resume <uuid>` and Codex rejects the duplicate positional
    arguments. Codex `resume` and `fork` are one-time session launch commands,
    not reusable base commands, so use the configured or built-in base command
    instead. Treat the invalid legacy `--resume` and `--fork` spellings the
    same way: they must never survive as the base command and get replayed by
    Full reload or Fork.
    Claude history launches have the same persistence shape. Its canonical
    `--resume` and `--continue` selectors, plus `--fork-session`, are also
    one-time session invocations rather than reusable agent commands.
    Existing affected rows are repaired at read time without mutating their
    saved metadata.
    */
    let tokens = command.split_whitespace();
    match agent_id {
        Some("codex") => tokens.into_iter().any(|token| {
            matches!(token, "resume" | "fork" | "--resume" | "--fork")
                || token.starts_with("--resume=")
                || token.starts_with("--fork=")
        }),
        Some("claude") => tokens.into_iter().any(|token| {
            matches!(token, "--resume" | "--continue" | "--fork-session")
                || token.starts_with("--resume=")
                || token.starts_with("--continue=")
                || token.starts_with("--fork-session=")
        }),
        _ => false,
    }
}

/// Whether a candidate base command still belongs to the resume family's CLI.
/// A command whose CLI is inferable and different is stale metadata left by an
/// earlier agent that owned the pane; resume must use the family default
/// instead of that binary. A command no known CLI claims is kept: custom
/// wrappers and unlisted launchers cannot be second-guessed. A family that is
/// not a built-in restorable id (a raw `custom-…` fallback) keeps every
/// command, because there is no family default to fall back to.
fn stored_agent_command_matches_family(family: Option<&str>, command: &str) -> bool {
    let Some(inferred) = infer_agent_id_from_command(command) else {
        return true;
    };
    match family.and_then(|family| restorable_agent_id(Some(family))) {
        Some(family) => family == inferred.as_str(),
        None => true,
    }
}

fn build_codex_resume_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} resume {shell_reference}")
}

pub(crate) fn build_codex_fork_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} fork {shell_reference}")
}

fn build_claude_resume_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} --resume {shell_reference}")
}

pub(crate) fn build_claude_fork_invocation(agent_command: &str, shell_reference: &str) -> String {
    format!("{agent_command} --resume {shell_reference} --fork-session")
}

pub(crate) fn build_agent_resume_command(
    input: &AgentResumeInput,
    options: ResumeCommandOptions,
) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let agent_lookup_command = input
        .agent_lookup_command
        .as_deref()
        .unwrap_or(agent_command);
    let resume_title = agent_resume_title(agent_id, input);
    let exact_reference = get_exact_agent_session_reference(agent_id, input);
    let codex_exact_reference = (agent_id == "codex")
        .then(|| get_codex_session_reference(input))
        .flatten();
    let codex_reference = if agent_id == "codex" {
        codex_exact_reference
            .clone()
            .or_else(|| resume_title.clone())
    } else {
        None
    };
    let claude_exact_reference = (agent_id == "claude")
        .then(|| get_claude_session_reference(input))
        .flatten();
    let cursor_reference = (agent_id == "cursor")
        .then(|| get_cursor_session_reference(input))
        .flatten();
    let opencode_reference = (agent_id == "opencode")
        .then(|| get_opencode_session_reference(input))
        .flatten();
    let pi_reference = (agent_id == "pi")
        .then(|| get_pi_session_reference(input))
        .flatten();

    match agent_id {
        "amp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} threads continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "antigravity" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --conversation {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codebuddy" | "copilot" | "droid" | "dsh" | "gemini" | "hermes-agent" | "qoder"
        | "zcode" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "grok" => exact_reference
            .map(|reference| format!("{agent_command} -r {}", quote_shell_double_arg(&reference))),
        "freebuff" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "kiro" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume-id {}",
                quote_shell_double_arg(&reference)
            )
        }),
        // CDXC:AgentProviders 2026-10-06 DECISION: "Resume. `empryo --session <id>`. The process-identity scan reads `--session` from argv." SEE-ALSO: extract_agent_process_session_id in server/src/zmx/process_identity.rs.
        // A session with no written folder: see `get_empryo_session_reference`.
        "omp" | "empryo" => match omp_wake_reference(input, exact_reference) {
            OmpWakeReference::Resume(reference) => Some(format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )),
            OmpWakeReference::Fresh => Some(agent_command.to_string()),
            OmpWakeReference::None => (agent_id == "empryo").then(|| agent_command.to_string()),
        },
        "codex" => {
            let reference = codex_reference?;
            // See `codex_rollout_written`: a thread Codex never saved starts fresh.
            if codex_exact_reference
                .as_deref()
                .is_some_and(|exact| is_uuid(exact) && !codex_rollout_written(input, exact))
            {
                return Some(agent_command.to_string());
            }
            if options.display {
                return Some(if let Some(exact) = codex_exact_reference {
                    build_codex_resume_invocation(agent_command, &quote_shell_double_arg(&exact))
                } else {
                    format!(
                        "{}  # lookup Codex session id by title",
                        build_codex_resume_invocation(
                            agent_command,
                            &quote_shell_double_arg(&reference)
                        )
                    )
                });
            }
            if let Some(exact) = codex_exact_reference {
                Some(build_codex_validated_resume_command(agent_command, &exact))
            } else {
                Some(build_codex_resume_lookup_command(agent_command, &reference))
            }
        }
        "claude" => {
            // CDXC:SessionChat 2026-10-09 WHY:
            // A session whose Claude has no conversation yet has nothing to lose, so a wake starts it fresh under the same session id instead of resuming. A wake used to resume whatever id the session held, and a Claude that quit before its first message printed "No conversation found with session ID" and fell back to the shell, so the automatic send recovery and Fix it (session_chat_queue_runtime/send_heal.rs) both failed on it. Every wake now follows the account switch's rule (CDXC:AgentProviders 2026-10-04 above).
            if let Some(id) = claude_exact_reference
                .as_deref()
                .and_then(|exact| get_claude_session_id(Some(exact)))
                .filter(|id| !claude_transcript_written(input, id))
            {
                return Some(build_claude_fresh_invocation(
                    agent_command,
                    &quote_shell_double_arg(&id),
                ));
            }
            if let Some(exact) = claude_exact_reference {
                let resume_invocation =
                    build_claude_resume_invocation(agent_command, &quote_shell_double_arg(&exact));
                #[cfg(not(windows))]
                if !options.display {
                    return Some(build_claude_attach_or_resume_command(
                        agent_command,
                        &exact,
                        resume_invocation,
                    ));
                }
                return Some(resume_invocation);
            }
            let resume_title = resume_title?;
            if options.display {
                Some(format!(
                    "{}  # lookup Claude session id by title",
                    build_claude_resume_invocation(
                        agent_command,
                        &quote_shell_double_arg(&resume_title)
                    )
                ))
            } else {
                Some(build_claude_resume_lookup_command(
                    agent_command,
                    input,
                    &resume_title,
                ))
            }
        }
        "cursor" => {
            if let Some(reference) = cursor_reference {
                return Some(format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                ));
            }
            let resume_title = resume_title?;
            let project_path = input.project_path.as_deref()?;
            if options.display {
                Some(format!(
                    "{agent_command} --resume {}  # lookup chat id in Cursor chat store",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_cursor_resume_lookup_command(
                    agent_command,
                    project_path,
                    &resume_title,
                ))
            }
        }
        "opencode" => {
            if let Some(reference) = opencode_reference {
                return Some(format!(
                    "{agent_command} --session {}",
                    quote_shell_double_arg(&reference)
                ));
            }
            let resume_title = resume_title?;
            if options.display {
                Some(format!(
                    "{agent_command} -s {}  # lookup session id in OpenCode session list",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_opencode_resume_command(
                    agent_command,
                    &resume_title,
                    agent_lookup_command,
                ))
            }
        }
        "pi" => pi_reference.and_then(|_| build_pi_resume_command(agent_command, input)),
        "rovodev" => {
            exact_reference.map(|reference| build_rovodev_resume_command(agent_command, &reference))
        }
        _ => None,
    }
}

pub(crate) fn build_agent_resume_copy_command(input: &AgentResumeInput) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let exact_reference = get_exact_agent_session_reference(agent_id, input);
    match agent_id {
        "amp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} threads continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "antigravity" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --conversation {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codebuddy" | "copilot" | "droid" | "dsh" | "gemini" | "hermes-agent" | "qoder"
        | "zcode" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "grok" => exact_reference
            .map(|reference| format!("{agent_command} -r {}", quote_shell_double_arg(&reference))),
        "freebuff" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "kiro" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume-id {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "omp" | "empryo" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codex" => get_codex_session_reference(input).map(|reference| {
            build_codex_resume_invocation(agent_command, &quote_shell_double_arg(&reference))
        }),
        "claude" => get_claude_session_reference(input).map(|reference| {
            build_claude_resume_invocation(agent_command, &quote_shell_double_arg(&reference))
        }),
        "cursor" => get_cursor_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "opencode" => get_opencode_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "pi" => build_pi_resume_command(agent_command, input),
        "rovodev" => {
            exact_reference.map(|reference| build_rovodev_resume_command(agent_command, &reference))
        }
        _ => None,
    }
}

pub(crate) fn build_agent_resume_fallback_command(input: &AgentResumeInput) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let agent_lookup_command = input
        .agent_lookup_command
        .as_deref()
        .unwrap_or(agent_command);
    let resume_title = agent_resume_title(agent_id, input)?;
    match agent_id {
        "codex" => {
            let exact = get_codex_session_reference(input)?;
            // A fresh start (no saved thread) has no conversation to fall back to.
            let fresh = is_uuid(&exact) && !codex_rollout_written(input, &exact);
            (exact != resume_title && !fresh)
                .then(|| build_codex_resume_lookup_command(agent_command, &resume_title))
        }
        // CDXC:AgentProviders 2026-10-21 WHY:
        // The title lookup only helps when the saved reference is not a conversation id. A conversation id either resumes its written transcript or starts fresh under that id (see `build_agent_resume_command`), so when that fails (an account without a saved login: cswap's "Account-3 has no stored credentials") the lookup can only find the same conversation or a wrong one. It used to run anyway, scanned every transcript on the computer for minutes (session G2nir) and held the shell, so `ghostex agents switch-account` could not take over.
        "claude" => {
            let exact = get_claude_session_reference(input)?;
            if get_claude_session_id(Some(&exact)).is_some() {
                return None;
            }
            Some(build_claude_resume_lookup_command(
                agent_command,
                input,
                &resume_title,
            ))
        }
        "opencode" => {
            let exact = get_opencode_session_reference(input)?;
            (exact != resume_title).then(|| {
                build_opencode_resume_command(agent_command, &resume_title, agent_lookup_command)
            })
        }
        "cursor" => {
            let _exact = get_cursor_session_reference(input)?;
            let project_path = input.project_path.as_deref()?;
            Some(build_cursor_resume_lookup_command(
                agent_command,
                project_path,
                &resume_title,
            ))
        }
        _ => None,
    }
}

/// The CLI-family agent id resume planning keys off. A built-in id passes
/// through; a `custom-…` configuration id resolves to the family its icon
/// declares (custom config icon first, then the session's launch icon), using
/// the same icon-to-id mapping as accept-all resolution. An unresolvable id is
/// returned unchanged so it still fails `restorable_agent_id` explicitly.
pub(crate) fn resume_agent_family_id(
    agent_id: Option<String>,
    agent_config: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
) -> Option<String> {
    let configured = agent_id?;
    if !is_custom_agent_id(&configured) {
        return Some(configured);
    }
    /*
    CDXC:AgentProviders 2026-10-06 WHY:
    Before Empryo was a built-in agent it ran as a custom agent with no icon, so those rows named no CLI family and a row that had already run woke to nothing (a session that ran gets only a resume command). A custom agent whose command runs Empryo is the Empryo family, the spec's "old Empryo rows keep opening"; other custom wrappers keep the icon rule.
    */
    let runs_empryo = [
        read_text_from_map(agent_config, "command"),
        read_text_from_map(agent_config, "agentCommand"),
        read_text_from_map(launch_settings, "agentCommand"),
    ]
    .iter()
    .any(|command| command_runs_empryo(command.as_deref()));
    read_text_from_map(agent_config, "icon")
        .or_else(|| read_text_from_map(launch_settings, "icon"))
        .and_then(|icon| {
            default_agent_icon_to_id(&icon)
                .map(str::to_string)
                .or_else(|| normalize_agent_id(Some(&icon)))
        })
        .or_else(|| runs_empryo.then(|| "empryo".to_string()))
        .or(Some(configured))
}

fn is_custom_agent_id(agent_id: &str) -> bool {
    agent_id.trim().to_ascii_lowercase().starts_with("custom-")
}

fn command_runs_empryo(command: Option<&str>) -> bool {
    command.and_then(infer_agent_id_from_command).as_deref() == Some("empryo")
}

pub(crate) fn restorable_agent_id(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    match value {
        "amp" | "antigravity" | "claude" | "codebuddy" | "codex" | "command-code" | "copilot"
        | "cursor" | "devin" | "droid" | "dsh" | "empryo" | "freebuff" | "gemini" | "grok"
        | "hermes-agent" | "kimi" | "kiro" | "omp" | "openclaude" | "opencode" | "pi" | "qoder"
        | "rovodev" | "zcode" => Some(value),
        _ => None,
    }
}

pub(crate) fn get_exact_agent_session_reference(
    agent_id: &str,
    input: &AgentResumeInput,
) -> Option<String> {
    match agent_id {
        "codex" => get_codex_session_reference(input),
        "cursor" => get_cursor_session_reference(input),
        "omp" => get_omp_session_reference(input),
        "pi" => get_pi_session_reference(input),
        "empryo" => get_empryo_session_reference(input),
        "dsh" => input
            .agent_session_id
            .clone()
            .filter(|id| crate::session_chat_dsh::is_safe_dsh_session_id(id)),
        _ => input.agent_session_id.clone(),
    }
}

/// CDXC:SessionIdentity 2026-10-06 WHY:
/// Empryo reports its session id when it mounts but writes `.empryo/sessions/<id>/` only with the first prompt, so waking a session that slept before its first message ran `empryo --session <id>` for a folder that never existed and Empryo printed "Session not found" (seen live 2026-10-06). Only a written session folder resumes; a session without one is seeded a fresh folder before it wakes (`seed_empryo_wake_session`), and a plan built without that seeding runs a fresh `empryo` (a missing resume command would leave the pane unstarted).
fn get_empryo_session_reference(input: &AgentResumeInput) -> Option<String> {
    let session_id = input.agent_session_id.as_deref()?.trim();
    if !crate::session_chat_empryo_mirror::is_safe_empryo_session_id(session_id) {
        return None;
    }
    let written = |folder: &std::path::Path| {
        folder.join("meta.json").is_file() || folder.join("session.jsonl").is_file()
    };
    let hook_folder = input
        .agent_session_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(crate::resume_lookup::expand_home)
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
    let mut candidates = hook_folder.into_iter().chain(
        [
            input.project_path.as_deref().map(std::path::PathBuf::from),
            Some(crate::resume_lookup::home_dir()),
        ]
        .into_iter()
        .flatten()
        .map(|root| root.join(".empryo").join("sessions").join(session_id)),
    );
    candidates
        .any(|folder| written(&folder))
        .then(|| session_id.to_string())
}

pub(crate) fn get_codex_session_reference(input: &AgentResumeInput) -> Option<String> {
    let session_id = input.agent_session_id.as_deref()?.trim();
    if session_id.is_empty() {
        return None;
    }
    get_uuid_from_text(session_id).or_else(|| Some(session_id.to_string()))
}

pub(crate) fn get_claude_session_reference(input: &AgentResumeInput) -> Option<String> {
    input
        .agent_session_id
        .clone()
        .or_else(|| get_claude_session_id(input.agent_session_path.as_deref()))
        .or_else(|| get_claude_session_id_from_stored_commands(&input.stored_command_candidates))
}

pub(crate) fn get_opencode_session_reference(input: &AgentResumeInput) -> Option<String> {
    input.agent_session_id.clone()
}

pub(crate) fn get_pi_session_reference(input: &AgentResumeInput) -> Option<String> {
    input
        .agent_session_path
        .clone()
        .or_else(|| input.agent_session_id.clone())
}

/// CDXC:SessionIdentity 2026-10-06 WHY:
/// Pi writes its session file only once the first message exists, while Ghostex knows the session's id from launch (`--session-id`, see agents/pi_session_id.rs) and its future path from the SessionStart hook. Waking a session that slept before its first message with `--session <path>` asked Pi for a file that was never written, so a written file resumes by path and anything else reopens the same id with `--session-id`, which Pi creates when it is missing.
fn build_pi_resume_command(agent_command: &str, input: &AgentResumeInput) -> Option<String> {
    let written = |path: &str| crate::resume_lookup::expand_home(path).is_file();
    if let Some(path) = input
        .agent_session_path
        .as_deref()
        .map(str::trim)
        .filter(|path| written(path))
    {
        return Some(format!(
            "{agent_command} --session {}",
            quote_shell_double_arg(path)
        ));
    }
    let id = input
        .agent_session_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    match id {
        Some(id) if super::pi_session_id::is_pi_session_id(id) => {
            match crate::session_chat::resolve_session_chat_transcript_path(
                crate::session_chat::SessionChatTranscriptAgent::Pi,
                Some(id),
                None,
            ) {
                Some(path) => Some(format!(
                    "{agent_command} --session {}",
                    quote_shell_double_arg(&path.to_string_lossy())
                )),
                None => Some(format!("{agent_command} --session-id {id}")),
            }
        }
        _ => get_pi_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
    }
}

enum OmpWakeReference {
    Resume(String),
    /// Every reference this session had was refused by the agent: start it fresh.
    Fresh,
    None,
}

/// What an OMP or Empryo wake resumes. A reference the agent already answered with `Session
/// "<ref>" not found.` is never run again (GitHub #456, session_chat_queue_runtime/resume_missing.rs):
/// the saved conversation file is tried by path when the id was refused, and once nothing is left
/// the agent starts fresh in the same session.
fn omp_wake_reference(input: &AgentResumeInput, exact: Option<String>) -> OmpWakeReference {
    let refused = |reference: &str| {
        input
            .rejected_references
            .iter()
            .any(|seen| seen == reference)
    };
    match exact {
        Some(reference) if !refused(&reference) => OmpWakeReference::Resume(reference),
        Some(_) => input
            .agent_session_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty() && !refused(path))
            .filter(|path| crate::resume_lookup::expand_home(path).is_file())
            .map_or(OmpWakeReference::Fresh, |path| {
                OmpWakeReference::Resume(path.to_string())
            }),
        // A session whose only reference was refused (its saved file was gone, so the id no
        // longer passes `get_omp_session_reference`) has nothing left to reopen either.
        None if input
            .agent_session_id
            .as_deref()
            .is_some_and(|id| refused(id)) =>
        {
            OmpWakeReference::Fresh
        }
        None => OmpWakeReference::None,
    }
}

/// CDXC:SessionIdentity 2026-09-14 WHY:
/// Older live scans could stamp a recycled TTY's foreign OMP transcript into a new session. Refuse that stored identity at wake time so restarting does not turn the bad observation into a real cross-project resume.
fn get_omp_session_reference(input: &AgentResumeInput) -> Option<String> {
    let id = input.agent_session_id.as_deref()?;
    if let Some(path) = input.agent_session_path.as_deref() {
        let path = crate::resume_lookup::expand_home(path);
        let head =
            crate::session_chat_paths::read_transcript_head_complete_lines(&path, 64 * 1024)?;
        let mut lines = head.lines();
        let mut header: Value = serde_json::from_str(lines.next()?).ok()?;
        // OMP can reserve its first row for an auto-generated title before the session header.
        if header.get("type").and_then(Value::as_str) == Some("title") {
            header = serde_json::from_str(lines.next()?).ok()?;
        }
        if header.get("type")?.as_str()? != "session" || header.get("id")?.as_str()? != id {
            return None;
        }
        let cwd = std::path::Path::new(header.get("cwd")?.as_str()?);
        let project = std::path::Path::new(input.project_path.as_deref()?);
        if cwd != project
            && std::fs::canonicalize(cwd).ok()? != std::fs::canonicalize(project).ok()?
        {
            return None;
        }
    }
    Some(id.to_string())
}

#[cfg(test)]
mod omp_resume_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn omp_resume_rejects_foreign_cwd_and_mismatched_id() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.jsonl");
        let project = json!({"path": temp.path(), "customAgents": [], "launchSettings": {}});
        let session = json!({"agentId": "omp", "runtimeSettings": {
            "agentCommand": "omp", "agentSessionId": "omp-id", "agentSessionPath": path,
        }});
        let settings = normalize_agent_settings(None);
        for (id, cwd, valid) in [
            ("omp-id", "/another-project", false),
            ("another-id", temp.path().to_str().unwrap(), false),
            ("omp-id", temp.path().to_str().unwrap(), true),
        ] {
            std::fs::write(
                &path,
                format!(
                    "{{\"type\":\"title\",\"title\":\"OMP test\"}}\n{}\n",
                    json!({"type": "session", "id": id, "cwd": cwd})
                ),
            )
            .unwrap();
            let plan = build_agent_resume_plan(&project, &session, &settings);
            assert_eq!(plan.get("primaryCommand").is_some(), valid);
            assert_eq!(plan.get("copyCommand").is_some(), valid);
            if valid {
                assert_eq!(plan["primaryCommand"], "omp --session \"omp-id\"");
            } else {
                assert_eq!(plan["startupTextDisposition"], "none");
            }
        }
    }

    /// GitHub #456: a reference OMP answered with "Session not found" is never resumed again.
    #[test]
    fn omp_wake_skips_refused_references_then_starts_fresh() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({"type": "session", "id": "omp-id", "cwd": temp.path()})
            ),
        )
        .unwrap();
        let path = path.to_str().unwrap().to_string();
        let project = json!({"path": temp.path(), "customAgents": [], "launchSettings": {}});
        let settings = normalize_agent_settings(None);
        let plan_for = |runtime: serde_json::Value| {
            let session = json!({"agentId": "omp", "runtimeSettings": runtime});
            build_agent_resume_plan(&project, &session, &settings)["primaryCommand"].clone()
        };
        let with_path = |rejected: serde_json::Value| {
            json!({"agentCommand": "omp", "agentSessionId": "omp-id",
                "agentSessionPath": path, "agentResumeRejected": rejected})
        };
        assert_eq!(plan_for(with_path(json!([]))), "omp --session \"omp-id\"");
        // The id was refused: the saved file is tried by path.
        assert_eq!(
            plan_for(with_path(json!(["omp-id"]))),
            format!("omp --session \"{path}\"")
        );
        // The file was refused too: OMP starts fresh, with no `--session`.
        assert_eq!(plan_for(with_path(json!(["omp-id", path]))), "omp");
        // Only an id, refused: fresh. An id nobody refused still resumes.
        assert_eq!(
            plan_for(json!({"agentCommand": "omp", "agentSessionId": "omp-id",
                "agentResumeRejected": ["omp-id"]})),
            "omp"
        );
        assert_eq!(
            plan_for(json!({"agentCommand": "omp", "agentSessionId": "new-id",
                "agentResumeRejected": ["omp-id"]})),
            "omp --session \"new-id\""
        );
    }
}

pub(crate) fn get_cursor_session_reference(input: &AgentResumeInput) -> Option<String> {
    get_cursor_chat_session_id(input.agent_session_id.as_deref())
        .or_else(|| get_cursor_chat_session_id(input.agent_session_path.as_deref()))
        .or_else(|| {
            get_cursor_chat_session_id_from_stored_commands(&input.stored_command_candidates)
        })
}

pub(crate) fn get_cursor_chat_session_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    if is_uuid(normalized) {
        return Some(normalized.to_ascii_lowercase());
    }
    let normalized_path = normalized.replace('\\', "/");
    let marker = "/agent-transcripts/";
    let index = normalized_path.to_ascii_lowercase().find(marker)?;
    let tail = &normalized_path[index + marker.len()..];
    let segment = tail.split('/').next()?.trim();
    is_uuid(segment).then(|| segment.to_ascii_lowercase())
}

pub(crate) fn get_cursor_chat_session_id_from_stored_commands(
    candidates: &[String],
) -> Option<String> {
    candidates
        .iter()
        .filter_map(|candidate| get_resume_flag_value_from_stored_command(candidate, "--resume"))
        .find_map(|value| get_cursor_chat_session_id(Some(&value)))
}

pub(crate) fn get_claude_session_id_from_stored_commands(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .filter_map(|candidate| get_resume_flag_value_from_stored_command(candidate, "--resume"))
        .find_map(|value| get_claude_session_id(Some(&value)))
}

pub(crate) fn get_claude_session_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    if let Some(uuid) = get_uuid_from_text(normalized) {
        return Some(uuid);
    }
    let cleaned = normalized.trim_end_matches(".jsonl");
    if is_claude_ses_id(cleaned) {
        return Some(cleaned.to_string());
    }
    let normalized_path = normalized.replace('\\', "/");
    normalized_path
        .split('/')
        .filter_map(|part| {
            let candidate = part.trim_end_matches(".jsonl");
            is_claude_ses_id(candidate).then(|| candidate.to_string())
        })
        .next_back()
}

pub(crate) fn is_claude_ses_id(value: &str) -> bool {
    value.strip_prefix("ses_").is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|char| char.is_ascii_alphanumeric())
    })
}

pub(crate) fn get_resume_flag_value_from_stored_command(
    command: &str,
    flag: &str,
) -> Option<String> {
    let bytes = command.as_bytes();
    let flag_bytes = flag.as_bytes();
    let mut index = 0;
    while index + flag_bytes.len() <= bytes.len() {
        if &bytes[index..index + flag_bytes.len()] != flag_bytes {
            index += 1;
            continue;
        }
        if index > 0 && !bytes[index - 1].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let mut cursor = index + flag_bytes.len();
        if bytes.get(cursor) == Some(&b'=') {
            cursor += 1;
        } else if bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
        } else {
            index += 1;
            continue;
        }
        if cursor >= bytes.len() {
            return None;
        }
        let quote = match bytes[cursor] {
            b'\'' | b'"' => {
                cursor += 1;
                Some(bytes[cursor - 1])
            }
            _ => None,
        };
        let start = cursor;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if quote == Some(byte)
                || (quote.is_none()
                    && (byte.is_ascii_whitespace() || matches!(byte, b';' | b'&' | b'|')))
            {
                break;
            }
            cursor += 1;
        }
        let value = command[start..cursor].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
        index = cursor.saturating_add(1);
    }
    None
}

pub(crate) fn collect_stored_agent_resume_command_candidates(session: &Value) -> Vec<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    let launch_plan = launch_settings
        .get("agentLaunchPlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let resume_plan = launch_settings
        .get("agentResumePlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let values = [
        read_text_from_map(&runtime_settings, "agentResumeCommand"),
        read_text_from_map(&runtime_settings, "resumeCommand"),
        read_text_from_map(&runtime_settings, "resumeFallbackCommand"),
        read_text_from_map(&runtime_settings, "copyCommand"),
        read_text_from_map(&runtime_settings, "startupText"),
        read_text_from_map(&launch_settings, "agentResumeCommand"),
        read_text_from_map(&launch_settings, "resumeCommand"),
        read_text_from_map(&launch_settings, "resumeFallbackCommand"),
        read_text_from_map(&launch_settings, "copyCommand"),
        read_text_from_map(&launch_settings, "startupText"),
        read_text_from_map(&launch_plan, "command"),
        read_text_from_map(&launch_plan, "startupText"),
        read_text_from_map(&resume_plan, "primaryCommand"),
        read_text_from_map(&resume_plan, "copyCommand"),
        read_text_from_map(&resume_plan, "displayCommand"),
        read_text_from_map(&resume_plan, "startupText"),
    ];
    let mut output = Vec::new();
    for value in values.into_iter().flatten() {
        if !output.iter().any(|candidate| candidate == &value) {
            output.push(value);
        }
    }
    output
}

/// The title a resume looks the conversation up by. Pi resumes by its session file, never by title; Codex rows stored before ingest dropped Codex's ` | <project>` title segment still carry it (see `strip_codex_project_title_suffix`).
fn agent_resume_title(agent_id: &str, input: &AgentResumeInput) -> Option<String> {
    match agent_id {
        "pi" => None,
        "codex" => trusted_resume_title_for_input(input).and_then(|title| {
            strip_codex_project_title_suffix(&title, input.project_path.as_deref())
        }),
        _ => trusted_resume_title_for_input(input),
    }
}

pub(crate) fn trusted_resume_title_for_input(input: &AgentResumeInput) -> Option<String> {
    let title = input.title.as_deref()?;
    let title_source = normalize_title_source(input.title_source.as_deref(), title);
    if title_source == "placeholder" {
        return None;
    }
    let visible = get_visible_terminal_title(title)?.trim().to_string();
    (!visible.is_empty() && !is_rejected_resume_title(&visible)).then_some(visible)
}

pub(crate) fn build_rovodev_resume_command(agent_command: &str, session_reference: &str) -> String {
    let quoted = quote_shell_double_arg(session_reference);
    if agent_command
        .split_whitespace()
        .any(|token| token == "rovodev")
    {
        format!("{agent_command} --restore {quoted}")
    } else {
        format!("{agent_command} rovodev run --restore {quoted}")
    }
}

#[cfg(not(windows))]
pub(crate) fn build_claude_resume_lookup_command(
    agent_command: &str,
    input: &AgentResumeInput,
    resume_title: &str,
) -> String {
    let args = [
        quote_shell_arg(input.project_path.as_deref().unwrap_or_default()),
        quote_shell_arg(resume_title),
        quote_shell_arg(input.first_user_message.as_deref().unwrap_or_default()),
    ]
    .join(" ");
    let resume_invocation =
        build_claude_resume_invocation(agent_command, "\"$CLAUDE_RESUME_SESSION_ID\"");
    [
        "CLAUDE_RESUME_SESSION_ID=\"$(".to_string(),
        format!("{} claude {args}", build_resume_lookup_command()),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CLAUDE_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        resume_invocation,
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to find restorable Claude session id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

#[cfg(not(windows))]
pub(crate) fn build_cursor_resume_lookup_command(
    agent_command: &str,
    project_path: &str,
    resume_title: &str,
) -> String {
    [
        "CURSOR_CHAT_ID=\"$(".to_string(),
        format!(
            "{} cursor {} {}",
            build_resume_lookup_command(),
            quote_shell_arg(project_path),
            quote_shell_arg(resume_title)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CURSOR_CHAT_ID\"".to_string(),
        "&&".to_string(),
        format!("{agent_command} --resume \"$CURSOR_CHAT_ID\""),
        "||".to_string(),
        format!(
            "printf '%s\\n' {}",
            quote_shell_arg(&format!(
                "Unable to find Cursor chat id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

pub(crate) fn build_opencode_resume_command(
    agent_command: &str,
    resume_title: &str,
    lookup_agent_command: &str,
) -> String {
    format!(
        "{agent_command} -s \"$({lookup_agent_command} session list --format json | {} opencode {})\"",
        build_resume_lookup_command(),
        quote_shell_arg(resume_title)
    )
}

#[cfg(not(windows))]
pub(crate) fn build_codex_validated_resume_command(
    agent_command: &str,
    session_reference: &str,
) -> String {
    [
        "CODEX_RESUME_SESSION_ID=\"$(".to_string(),
        format!(
            "{} codex --exact {}",
            build_resume_lookup_command(),
            quote_shell_arg(session_reference)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CODEX_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        build_codex_resume_invocation(agent_command, "\"$CODEX_RESUME_SESSION_ID\""),
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to restore Codex session \"{session_reference}\"."
            ))
        ),
    ]
    .join(" ")
}

#[cfg(not(windows))]
pub(crate) fn build_codex_resume_lookup_command(agent_command: &str, resume_title: &str) -> String {
    [
        "CODEX_RESUME_SESSION_ID=\"$(".to_string(),
        format!(
            "{} codex --title {}",
            build_resume_lookup_command(),
            quote_shell_arg(resume_title)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CODEX_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        build_codex_resume_invocation(agent_command, "\"$CODEX_RESUME_SESSION_ID\""),
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to find restorable Codex session id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

pub(crate) fn build_resume_lookup_command() -> String {
    /*
    CDXC:RemotePairing 2026-07-13:
    Resume lookups used to run as `node -e <script>` against the bundled
    code-server Node, which forced every host (including remote Linux
    packages) to carry a Node runtime for session restore. The lookups are
    now `gxserver resume-lookup <provider> ...` subcommands of this binary,
    resolved the same way agent hooks resolve their notify executable.
    */
    let executable = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "gxserver".to_string());
    #[cfg(windows)]
    return format!("& {} resume-lookup", quote_shell_arg(&executable));
    #[cfg(not(windows))]
    format!("{} resume-lookup", quote_shell_arg(&executable))
}

pub(crate) fn get_uuid_from_text(value: &str) -> Option<String> {
    let text = value.as_bytes();
    for start in 0..text.len().saturating_sub(35) {
        let end = start + 36;
        let candidate = &value[start..end];
        if is_uuid(candidate) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}
