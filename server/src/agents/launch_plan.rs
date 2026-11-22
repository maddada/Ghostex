use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;
use crate::session_status::normalize_agent_activity_value;
use rusqlite::Connection;

/*
CDXC:ServerApi 2026-06-22-05:39:
`createAgentSession` is a CRUD endpoint, but its durable row is shaped by the same project agent config and persisted agent settings as TypeScript gxserver. Build the launch plan before repository insertion so listSessions returns the same launchSettings and runtimeSettings immediately after creation.
*/
/// The `account` a create names, checked before anything is made for it (a worktree thread's
/// checkout comes first): the same agent and command `create_agent_session_params_for_project` reads.
pub(crate) fn check_requested_account(
    db: &Connection,
    project: &Value,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let Some(wanted) = read_text(params, "account") else {
        return Ok(());
    };
    let global_agents_project = project_with_global_agent_list(db, project);
    let project = global_agents_project.as_ref().unwrap_or(project);
    let agent_id =
        read_text(params, "agentId").unwrap_or_else(|| DEFAULT_PROMPT_AGENT_ID.to_string());
    let launch_settings = params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let agent_config = resolve_project_agent_config(project, &agent_id, Some(&launch_settings));
    let icon = read_text_from_map(&agent_config, "icon")
        .or_else(|| read_text_from_map(&launch_settings, "icon"));
    let command = read_text_from_map(&agent_config, "command")
        .or_else(|| read_text_from_map(&launch_settings, "agentCommand"));
    crate::accounts::launch::requested_account(
        db,
        &agent_id,
        icon.as_deref(),
        command.as_deref(),
        &wanted,
        account_user_named(params),
    )
    .map(|_| ())
}

/// `accountUserNamed`: the user named the create's `account` (`start-thread --user-named`), so a
/// manual-only account is allowed (accounts/launch.rs `requested_account`).
pub(crate) fn account_user_named(params: &Map<String, Value>) -> bool {
    params.get("accountUserNamed").and_then(Value::as_bool) == Some(true)
}

/// The CLI family (`claude`, `codex`…) of the agent a create starts, read from the same agent
/// config `create_agent_session_params_for_project` launches it with.
pub(crate) fn requested_agent_family_id(
    db: &Connection,
    project: &Value,
    params: &Map<String, Value>,
) -> Option<String> {
    let global_agents_project = project_with_global_agent_list(db, project);
    let project = global_agents_project.as_ref().unwrap_or(project);
    let agent_id =
        read_text(params, "agentId").unwrap_or_else(|| DEFAULT_PROMPT_AGENT_ID.to_string());
    let launch_settings = params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let agent_config = resolve_project_agent_config(project, &agent_id, Some(&launch_settings));
    resume_agent_family_id(Some(agent_id), &agent_config, &launch_settings)
}

pub(crate) fn create_agent_session_params_for_project(
    db: &Connection,
    project: &Value,
    params: &Map<String, Value>,
) -> Result<Map<String, Value>, DomainStateError> {
    let global_agents_project = project_with_global_agent_list(db, project);
    let project = global_agents_project.as_ref().unwrap_or(project);
    let settings = read_agent_settings(db)?;
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::corrupt_state("Project missing projectId."))?;
    let agent_id =
        read_text(params, "agentId").unwrap_or_else(|| DEFAULT_PROMPT_AGENT_ID.to_string());
    let mut launch_settings = params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut runtime_settings = params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let agent_config = resolve_project_agent_config(project, &agent_id, Some(&launch_settings));
    let agent_icon = read_text_from_map(&agent_config, "icon")
        .or_else(|| read_text_from_map(&launch_settings, "icon"));
    let configured_command = read_text_from_map(&agent_config, "command")
        .or_else(|| read_text_from_map(&launch_settings, "agentCommand"));
    let (configured_command, empryo_session_id) = apply_requested_agent_model(
        project,
        &agent_id,
        &agent_config,
        &launch_settings,
        params,
        configured_command,
        &mut runtime_settings,
    )?;
    // A Claude cloud session runs Ghostex's cloud client, not a local agent: its record and account
    // are settled here (server/src/claude_cloud/create.rs); no other create may carry a record.
    if agent_id == crate::claude_cloud::CLAUDE_CLOUD_AGENT_ID {
        crate::claude_cloud::prepare_cloud_runtime(db, project, &mut runtime_settings)?;
    } else {
        crate::claude_cloud::strip_client_record(&mut runtime_settings);
    }
    let agentbox_provider = crate::agentbox::requested_agentbox_provider(params)?;
    // Only a box create writes a box record; a client cannot make a session a box session.
    if agentbox_provider.is_none() {
        runtime_settings.remove("agentbox");
    }
    let agentbox_family = match agentbox_provider {
        Some(_) => {
            if crate::coordinators::coordinator_create_request(params)?.is_some() {
                return Err(DomainStateError::bad_request(
                    "A coordinator runs on this computer. Start it without a box.",
                ));
            }
            Some(crate::agentbox::box_agent_family(
                resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings)
                    .as_deref(),
            )?)
        }
        None => None,
    };
    // A local agent whose CLI isn't installed is refused before any session exists
    // (CDXC:AgentLauncher 2026-11-21 in cli_missing.rs); boxes and custom agents (`custom-...`)
    // are not refused here.
    if agentbox_provider.is_none()
        && !agent_id.starts_with("custom-")
        && super::should_check_launch_cli()
    {
        let family =
            resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings);
        if let Some(family) = family.as_deref() {
            let home_dir = super::agent_probe_home();
            if super::launch_cli_missing(family, configured_command.as_deref(), &home_dir) {
                let name = read_text_from_map(&agent_config, "name")
                    .or_else(|| crate::sidebar_hud::default_agent_name(family).map(str::to_string))
                    .unwrap_or_else(|| family.to_string());
                return Err(super::agent_not_installed_error(&name));
            }
        }
    }
    let configured_command = if agentbox_provider.is_none() {
        pin_remembered_agent_model(
            &agent_id,
            &agent_config,
            &launch_settings,
            params,
            configured_command,
            &mut runtime_settings,
        )?
    } else {
        configured_command
    };
    // `start-thread --read-only` (CDXC:Coordinators 2026-11-13 in coordinators/read_only.rs): an
    // agent with no read-only mode is refused here, before any session exists; a box keeps its
    // own launch, so it is refused too. The marker keeps every later launch read-only, a copy of
    // the session (fork) included.
    let read_only_family = if crate::coordinators::read_only_thread_requested(params)
        || crate::coordinators::is_read_only_thread(Some(&runtime_settings))
    {
        if agentbox_provider.is_some() {
            return Err(DomainStateError::bad_request(
                "A read-only thread runs on this computer's agent, not in a box. Leave out --read-only or the box.",
            ));
        }
        let family =
            resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings);
        crate::coordinators::check_read_only_thread_family(family.as_deref())?;
        runtime_settings.insert(
            crate::coordinators::READ_ONLY_THREAD_KEY.to_string(),
            json!(true),
        );
        family
    } else {
        None
    };
    // `coordinator start-thread` marks its thread, for Lean Codex workers (codex_lean.rs); only
    // that param sets the marker, so a copy of a thread is the user's own session. A lead's
    // session gets its own marker from its link, never from a create (lead_links `link`), so a
    // copy of one starts without it.
    let coordinator_thread = params
        .get(super::codex_lean::COORDINATOR_THREAD_KEY)
        .and_then(Value::as_bool)
        == Some(true);
    runtime_settings.remove(super::codex_lean::COORDINATOR_THREAD_KEY);
    runtime_settings.remove(super::codex_lean::LEAD_WORKER_KEY);
    if coordinator_thread {
        runtime_settings.insert(
            super::codex_lean::COORDINATOR_THREAD_KEY.to_string(),
            json!(true),
        );
    }
    let configured_command = if agentbox_provider.is_none() {
        apply_thread_marked_handovers(
            &agent_id,
            &agent_config,
            &launch_settings,
            params,
            configured_command,
        )
    } else {
        configured_command
    };
    // `coordinator start-thread --account`: the account is checked here, where the agent and its command are known, and stored as the picked account `apply_new_session` honours (or, for a hub, the hub below starts on).
    let account_named = read_text(params, "account").is_some();
    if let Some(wanted) = read_text(params, "account") {
        if agentbox_provider.is_some() {
            return Err(DomainStateError::bad_request(
                "A session that runs in a box takes no account. Leave out --account.",
            ));
        }
        let id = crate::accounts::launch::requested_account(
            db,
            &agent_id,
            agent_icon.as_deref(),
            configured_command.as_deref(),
            &wanted,
            account_user_named(params),
        )?;
        runtime_settings.insert("accountId".to_string(), json!(id));
    } else if agentbox_provider.is_none() && !runtime_settings.contains_key("accountId") {
        // New sessions start on a hub (CDXC:AgentProviders 2026-11-19 in accounts/hub/launch.rs),
        // unless the workspace picks a Claude account or the command chooses its own login.
        if let Some(provider) =
            crate::accounts::launch::agent_provider(&agent_id, agent_icon.as_deref())
        {
            let base = configured_command
                .clone()
                .or_else(|| default_agent_command(&agent_id).map(str::to_string))
                .unwrap_or_else(|| provider.id().to_string());
            if !crate::accounts::launch::uses_own_login(&base, provider) {
                let registry = crate::accounts::store::read(db)?;
                let workspace_account = match provider {
                    crate::accounts::model::Provider::Claude => {
                        crate::workspaces::project_claude_account_id(db, project)?.filter(|id| {
                            registry
                                .accounts
                                .iter()
                                .any(|a| a.id == *id && a.provider == provider && !a.user_disabled)
                        })
                    }
                    crate::accounts::model::Provider::Codex => None,
                };
                if workspace_account.is_none() {
                    if let Some(hub) =
                        crate::accounts::hub::launch::default_hub(&registry, provider)
                    {
                        runtime_settings.insert(
                            "accountId".to_string(),
                            json!(format!(
                                "{}{hub}",
                                crate::accounts::hub::launch::ACCOUNT_PREFIX
                            )),
                        );
                    }
                }
            }
        }
    }
    // On the hub (CDXC:AgentProviders 2026-11-18 in accounts/hub/launch.rs): a launcher that picked
    // a CLIProxyAPI hub as the account starts the session in that hub's profile folder, which makes
    // it a session on its own login that `apply_new_session` leaves alone.
    let configured_command = match crate::accounts::hub::launch::picked_hub(&runtime_settings) {
        Some(_) if agentbox_provider.is_some() => {
            return Err(DomainStateError::bad_request(
                "A session that runs in a box can't start on the hub. Leave out the box.",
            ));
        }
        Some(hub_id) => {
            let provider =
                crate::accounts::launch::agent_provider(&agent_id, agent_icon.as_deref())
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "Only Claude and Codex sessions can start on the hub.",
                        )
                    })?;
            let base = configured_command
                .clone()
                .or_else(|| default_agent_command(&agent_id).map(str::to_string));
            // CDXC:AgentProviders 2026-11-19 WHY: a hub that is down, or refuses the key, is said at the start (security review of the hub work), not as a failed first message the user has to decode. A launch that only follows New sessions start on (the launchers send the rule's hub as the account; phones, automations, Chief, GBots and threads without --account send none) starts on the account Auto would pick and says so; only a hub named on purpose (--account, or a hub other than the rule's) refuses.
            let paths = crate::paths::get_gxserver_paths(None);
            match crate::accounts::hub::launch::check_reachable(&paths, &hub_id) {
                Ok(()) => Some(crate::accounts::hub::launch::apply(
                    &paths,
                    provider,
                    &hub_id,
                    base.as_deref(),
                    &mut runtime_settings,
                )?),
                Err(error) => {
                    let registry = crate::accounts::store::read(db)?;
                    let from_rule = !account_named
                        && crate::accounts::hub::launch::default_hub(&registry, provider)
                            .as_deref()
                            == Some(hub_id.as_str());
                    if !from_rule {
                        return Err(error);
                    }
                    crate::accounts::hub::launch::start_elsewhere(
                        &paths,
                        &registry,
                        provider,
                        &hub_id,
                        &mut runtime_settings,
                    );
                    configured_command
                }
            }
        }
        None => configured_command,
    };
    let account_command = if agentbox_provider.is_some() {
        None
    } else {
        if let Some(command) = configured_command.as_ref() {
            runtime_settings
                .entry("accountBaseCommand")
                .or_insert(json!(command));
        }
        let workspace_account_id = crate::workspaces::project_claude_account_id(db, project)?;
        crate::accounts::launch::apply_new_session(
            db,
            &agent_id,
            agent_icon.as_deref(),
            workspace_account_id.as_deref(),
            &mut runtime_settings,
        )?
    };
    /*
    CDXC:SessionIdentity 2026-09-30 WHY: A create that names no agent Ghostex knows and only carries a command (Find's resume sends `os-integration-terminal` with `claude --resume <id>`) locked the row to that placeholder id, so `launch_agent_mismatch` refused every hook the agent sent and only the ~20s live-process scan could name it. Chat View cannot open before that, so a session resumed from Find sat in the terminal (observed 2026-09-30, session S90:P3lv0:G41ci: its SessionStart hook arrived 1.5s after launch and was dropped). The agent the command starts is the session's agent from creation; the launch command and account are still resolved under the requested id, so the command runs exactly as given.
    */
    // A new local Pi session runs on an id Ghostex chose (CDXC:SessionIdentity in pi_session_id.rs).
    let pi_session_id = (agentbox_provider.is_none()
        && read_text_from_map(&runtime_settings, "agentSessionId").is_none()
        && resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings)
            .as_deref()
            == Some("pi"))
    .then(|| {
        let base = configured_command
            .clone()
            .or_else(|| default_agent_command("pi").map(str::to_string))
            .unwrap_or_else(|| "pi".to_string());
        super::pi_session_id::mint_pi_session_id(&base)
    })
    .flatten();
    if let Some(id) = pi_session_id.as_deref() {
        runtime_settings.insert("agentSessionId".to_string(), json!(id));
    }
    // A new local OpenCode session too (CDXC:SessionIdentity in opencode_session_id.rs).
    let opencode_session_id = (agentbox_provider.is_none()
        && read_text_from_map(&runtime_settings, "agentSessionId").is_none()
        && resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings)
            .as_deref()
            == Some("opencode"))
    .then(|| {
        let base = configured_command
            .clone()
            .or_else(|| default_agent_command("opencode").map(str::to_string))
            .unwrap_or_else(|| "opencode".to_string());
        let directory = read_text(params, "cwd").or_else(|| read_text_value(project, "path"));
        super::opencode_session_id::mint_opencode_session_id(&base, directory.as_deref())
    })
    .flatten();
    if let Some(id) = opencode_session_id.as_deref() {
        runtime_settings.insert("agentSessionId".to_string(), json!(id));
    }
    /*
    CDXC:SessionIdentity 2026-10-07 WHY:
    A plain Empryo launch ran bare `empryo`, which reopens the folder's latest session: the row never learned an agent session id (its chat had no transcript), and a second launch in the same repo shared the first one's session (seen live on 3.9.1-beta). Every new local Empryo session gets a seeded folder of its own, as a launch model's does, on the folder's current default model. Resume and fork arrive with their id and keep it.
    */
    let empryo_session_id = match empryo_session_id {
        None if agentbox_provider.is_none()
            && read_text_from_map(&runtime_settings, "agentSessionId").is_none()
            && resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings)
                .as_deref()
                == Some("empryo") =>
        {
            let id = crate::session_chat_empryo_launch_selection::seed_empryo_launch_session(
                &empryo_cwd(params, project)?,
                None,
                &mut runtime_settings,
            )?;
            Some(id)
        }
        seeded => seeded,
    };
    let session_agent_id = (agent_icon.is_none()
        && !agent_id.starts_with("custom-")
        && default_agent_command(&agent_id).is_none()
        && resolve_project_agent_config(project, &agent_id, None).is_empty())
    .then(|| {
        configured_command
            .as_deref()
            .and_then(infer_agent_id_from_command_executable)
    })
    .flatten()
    .unwrap_or_else(|| agent_id.clone());
    let launch_plan = match (agentbox_provider.as_deref(), agentbox_family.as_deref()) {
        (Some(provider), Some(family)) => {
            let project_path = read_text(params, "cwd")
                .or_else(|| read_text_value(project, "path"))
                .unwrap_or_default();
            let claude_sign_in_first = crate::accounts::launch::home()
                .is_ok_and(|home| !crate::agentbox::claude_signed_in_for_boxes(&home));
            crate::agentbox::prepare_box_launch(
                provider,
                family,
                &project_path,
                configured_command.as_deref(),
                claude_sign_in_first,
                &mut runtime_settings,
            )
            .launch_plan
        }
        _ => {
            let coordinator = session_is_coordinator(&runtime_settings);
            let mut plan = build_session_launch_plan(
                AgentLaunchInput {
                    accept_all_mode: read_text_from_map(&agent_config, "acceptAllMode")
                        .or_else(|| read_text_from_map(&launch_settings, "acceptAllMode")),
                    agent_id: agent_id.clone(),
                    agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
                    command: account_command.or(configured_command),
                    delayed_send_deadline_at: read_text_from_map(
                        &launch_settings,
                        "delayedSendDeadlineAt",
                    ),
                    first_user_message: read_text_from_map(&runtime_settings, "firstUserMessage"),
                    global_accept_all_enabled: settings
                        .get("agentAcceptAllEnabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    icon: agent_icon.clone(),
                },
                coordinator,
            );
            // Only the command this launch runs names the id; `agentCommand` stays the base that
            // resume, fork and account wrapping rebuild from. Pi's id is minted here; Empryo's names
            // the session folder seeded for a launch model (session_chat_empryo_launch_selection.rs).
            // A Claude cloud session brought to this computer teleports on this one launch; its
            // later resumes use the local conversation teleport made (claude_cloud/bring_back.rs).
            let teleport = runtime_settings
                .remove(crate::claude_cloud::TELEPORT_RUNTIME_KEY)
                .and_then(|id| id.as_str().map(str::to_string))
                .filter(|id| crate::claude_cloud::is_cloud_session_id(id))
                .filter(|_| {
                    resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings)
                        .as_deref()
                        == Some("claude")
                });
            let session_command =
                |command: &str| match (&pi_session_id, &empryo_session_id, &teleport) {
                    (Some(id), _, _) => Some(super::pi_session_id::with_pi_session_id(command, id)),
                    (None, Some(id), _) => Some(format!("{} --session {id}", command.trim_end())),
                    (None, None, Some(id)) => {
                        Some(format!("{} --teleport {id}", command.trim_end()))
                    }
                    (None, None, None) => opencode_session_id.as_deref().map(|id| {
                        super::opencode_session_id::with_opencode_session_id(command, id)
                    }),
                };
            if let Some(plan) = plan.as_object_mut() {
                // A read-only thread starts in its agent's read-only mode, accept-all taken off.
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .filter(|command| !command.trim().is_empty())
                    .filter(|_| read_only_family.is_some())
                    .map(|command| {
                        crate::coordinators::read_only_thread_command(
                            command,
                            read_only_family.as_deref(),
                        )
                    })
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .filter(|command| !command.trim().is_empty())
                    .and_then(session_command)
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                // The workspace's own gh and gws sign-ins (crate::connectors), on this run only.
                let family =
                    resume_agent_family_id(Some(agent_id.clone()), &agent_config, &launch_settings);
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .filter(|command| !command.trim().is_empty())
                    .map(|command| {
                        (
                            command.to_string(),
                            crate::connectors::with_project_cli_env(
                                db,
                                project,
                                family.as_deref(),
                                command,
                            ),
                        )
                    })
                    .and_then(|(before, after)| (after != before).then_some(after))
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                // Claude's Bypass Permissions warning, consented to for a bypass launch (bypass_consent.rs).
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .map(|command| {
                        (
                            command.to_string(),
                            with_claude_bypass_consent(family.as_deref(), command),
                        )
                    })
                    .and_then(|(before, after)| (after != before).then_some(after))
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                // Claude's idle compaction and cache length from Settings (claude_idle_compaction.rs),
                // in the same one `--settings` JSON.
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .map(|command| {
                        (
                            command.to_string(),
                            // Ghostex's skills, slash-only for a Claude Ghostex didn't start,
                            // back on for the model here (agent_skills_session.rs).
                            // and a named effort level's Ultracode switch off
                            // (claude_launch_ultracode.rs).
                            crate::agent_skills_session::with_ghostex_skills_on(
                                family.as_deref(),
                                &super::with_claude_launch_ultracode(
                                    family.as_deref(),
                                    &super::with_claude_compaction_settings(
                                        family.as_deref(),
                                        command,
                                    ),
                                ),
                            ),
                        )
                    })
                    .and_then(|(before, after)| (after != before).then_some(after))
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                // The home folder's secret folders, denied through the agent's own settings on
                // this run (home_secret_folders.rs).
                let launch_cwd =
                    read_text(params, "cwd").or_else(|| read_text_value(project, "path"));
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .map(|command| {
                        (
                            command.to_string(),
                            // A GBot that may not use the screen is started without its
                            // tools (gbot_computer_use.rs), and a coordinator's Codex thread
                            // lean while Lean Codex workers is on (codex_lean.rs).
                            super::with_lean_codex_worker(
                                family.as_deref(),
                                Some(&runtime_settings),
                                &super::with_gbot_computer_use(
                                    family.as_deref(),
                                    Some(&runtime_settings),
                                    &with_home_secret_folders_denied(
                                        family.as_deref(),
                                        launch_cwd.as_deref(),
                                        is_gbot_chat(Some(&runtime_settings)),
                                        command,
                                    ),
                                ),
                            ),
                        )
                    })
                    .and_then(|(before, after)| (after != before).then_some(after))
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                // A GBot API chat is always held to the secret folders and gxserver's own files
                // (home_secret_folders.rs `with_api_chat_sandbox`).
                if let Some(command) = plan
                    .get("command")
                    .and_then(Value::as_str)
                    .filter(|_| is_gbot_api_chat(Some(&runtime_settings)))
                    .and_then(|command| {
                        with_api_chat_sandbox(family.as_deref(), launch_cwd.as_deref(), command)
                    })
                {
                    plan.insert(
                        "startupText".to_string(),
                        Value::String(as_atuin_ignored_shell_input(&command)),
                    );
                    plan.insert("command".to_string(), Value::String(command));
                }
                if family.as_deref() == Some("dsh") {
                    if let Some(command) = plan
                        .get("command")
                        .and_then(Value::as_str)
                        .map(with_dsh_launch_env)
                    {
                        plan.insert(
                            "startupText".to_string(),
                            Value::String(as_atuin_ignored_shell_input(&command)),
                        );
                        plan.insert("command".to_string(), Value::String(command));
                    }
                }
                // Opt-in per-agent ACP transport (CDXC:AgentProviders 2026-11-22 in acp/mod.rs):
                // wrap the local launch inside `ghostex acp-host` so the agent stays alive inside
                // `zmx` across `gxserver` reloads and can switch in-place to the CLI terminal.
                let explicit_acp = params
                    .get(crate::acp::SESSION_NON_CLI_ACTIVE_KEY)
                    .and_then(Value::as_bool)
                    .or_else(|| {
                        params
                            .get(crate::acp::SESSION_ACP_ACTIVE_KEY)
                            .and_then(Value::as_bool)
                    });
                let runtime_acp_off = runtime_settings
                    .get(crate::acp::SESSION_NON_CLI_ACTIVE_KEY)
                    .and_then(Value::as_bool)
                    == Some(false)
                    || runtime_settings
                        .get(crate::acp::SESSION_ACP_ACTIVE_KEY)
                        .and_then(Value::as_bool)
                        == Some(false);
                let use_acp = agentbox_provider.is_none()
                    && !runtime_acp_off
                    && family.as_deref().is_some_and(|fam| {
                        crate::acp::supports_acp(fam)
                            && match explicit_acp {
                                Some(flag) => flag,
                                None => crate::acp::agent_acp_setting(&settings, fam)
                                    .unwrap_or_else(|| {
                                        crate::acp::agent_acp_enabled_in_shared_settings(fam)
                                    }),
                            }
                    });
                if use_acp {
                    if let Some(fam) = family.as_deref() {
                        let base_cli_cmd = plan
                            .get("command")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let resume_id = read_text_from_map(&runtime_settings, "agentSessionId");
                        let accept_all_effective =
                            match read_text_from_map(&agent_config, "acceptAllMode")
                                .or_else(|| read_text_from_map(&launch_settings, "acceptAllMode"))
                                .as_deref()
                            {
                                Some("enabled" | "acceptAll") => true,
                                Some("disabled") => false,
                                _ => settings
                                    .get("agentAcceptAllEnabled")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            };
                        let req_model = read_text(params, "agentModel");
                        let req_effort = read_text(params, "agentEffort");
                        let acp_cmd = crate::acp::spec::build_acp_host_launch_command(
                            fam,
                            base_cli_cmd.as_deref(),
                            resume_id.as_deref(),
                            accept_all_effective,
                            req_model.as_deref(),
                            req_effort.as_deref(),
                        );
                        runtime_settings.insert(
                            crate::acp::SESSION_NON_CLI_ACTIVE_KEY.to_string(),
                            json!(true),
                        );
                        runtime_settings
                            .insert(crate::acp::SESSION_ACP_ACTIVE_KEY.to_string(), json!(true));
                        runtime_settings
                            .insert(crate::acp::SESSION_ACP_FAMILY_KEY.to_string(), json!(fam));
                        plan.insert(
                            "startupText".to_string(),
                            Value::String(as_atuin_ignored_shell_input(&acp_cmd)),
                        );
                        plan.insert("command".to_string(), Value::String(acp_cmd));
                    }
                }
            }
            plan
        }
    };
    let launch_plan_object = launch_plan.as_object().cloned().unwrap_or_default();
    let has_launch_command = launch_plan_object
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    /*
    CDXC:AgentLauncher 2026-09-25 WHY: Every agent session gxserver creates must have a command to launch, whether or not the client sends `requireLaunchCommand`. A commandless row can never start: `startSessionProvider` declines it, so every attach surfaced as "gxserver did not confirm the zmx provider exists" (`ghostex create-agent hermes` resolved no command because the built-in id is `hermes-agent`). This supersedes the 2026-06-24 opt-in guard, which only remote GPUI starts, drafts, automations and board work set.
    */
    if !has_launch_command {
        return Err(DomainStateError::bad_request(format!(
            "Ghostex has no launch command for agent \"{agent_id}\", so it did not create the session. Use a built-in agent id (for example claude, codex or hermes-agent) or one configured for this project."
        )));
    }
    /*
    CDXC:SessionStatus 2026-10-18 WHY: Every new session starts idle, including one whose runtime settings carry a `firstUserMessage`: no local launch puts that prompt on its command line, the chat queue's startup send types it once the input box is up, and the agent's own hooks report working when the turn starts. Starting such a session "working" (the 2026-09-27 rule this supersedes) held that same startup send forever on an agent that reports nothing before its first turn, so Send Bug / Feedback's Start Agent never delivered its brief to ZCode (GitHub #254). A box session starts idle too: agentbox passes the prompt on its own command line and its activity poller reports working once the agent's screen shows it (see agentbox/activity.rs).
    */
    let agent_activity = if runtime_settings.get("agentActivity").is_some() {
        normalize_agent_activity_value(runtime_settings.get("agentActivity"), "idle")
    } else {
        default_activity(Some(&session_agent_id), None)
    };
    runtime_settings.insert("agentActivity".to_string(), agent_activity);
    runtime_settings.insert(
        "agentCommand".to_string(),
        Value::String(
            launch_plan_object
                .get("agentCommand")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    );
    runtime_settings.insert(
        "launchAgentId".to_string(),
        Value::String(session_agent_id.clone()),
    );
    if let Some(first_user_message) = launch_plan_object
        .get("firstUserMessage")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        runtime_settings.insert(
            "firstUserMessage".to_string(),
            Value::String(first_user_message.to_string()),
        );
    }
    /*
    CDXC:Drafts 2026-08-20:
    Arm the draft here, at creation, so only sessions that were created with one
    can ever consume it. A draft that is missing or blank leaves no marker and
    no key behind.
    */
    match runtime_settings
        .get(FIRST_USER_INPUT_DRAFT_KEY)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
    {
        Some(draft) => {
            runtime_settings.insert(FIRST_USER_INPUT_DRAFT_KEY.to_string(), Value::String(draft));
            runtime_settings.insert(
                FIRST_USER_INPUT_DRAFT_STATUS_KEY.to_string(),
                json!("pending"),
            );
        }
        None => {
            runtime_settings.remove(FIRST_USER_INPUT_DRAFT_KEY);
            runtime_settings.remove(FIRST_USER_INPUT_DRAFT_STATUS_KEY);
        }
    }
    /*
    CDXC:Drafts 2026-08-28:
    Arm the draft marker in the same place the first-input draft above is armed,
    and for the same reason: only a session created as a draft can ever be one.
    See `agents/drafts.rs` for what the marker means and where it is removed.
    */
    apply_draft_session_create_param(params, &mut runtime_settings);
    crate::empty_session_cleanup::apply_new_session_marker(params, &mut runtime_settings);

    let mut runtime_relevant = Map::new();
    if let Some(deadline_at) = launch_plan_object
        .get("delayedSend")
        .and_then(Value::as_object)
        .and_then(|delayed| delayed.get("deadlineAt"))
        .and_then(Value::as_str)
    {
        runtime_relevant.insert(
            "delayedSendDeadlineAt".to_string(),
            Value::String(deadline_at.to_string()),
        );
    }
    runtime_relevant.insert(
        "queueProviderStartupText".to_string(),
        Value::Bool(
            launch_plan_object
                .get("startupTextDisposition")
                .and_then(Value::as_str)
                == Some("queueAfterTerminalReady"),
        ),
    );
    if let Some(agent_icon) = agent_icon {
        launch_settings.insert("icon".to_string(), Value::String(agent_icon));
    }
    launch_settings.insert("agentLaunchPlan".to_string(), launch_plan);
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        Value::Object(runtime_relevant),
    );

    let mut normalized = params.clone();
    normalized.insert("agentId".to_string(), Value::String(session_agent_id));
    normalized.insert("kind".to_string(), Value::String("agent".to_string()));
    normalized.insert("launchSettings".to_string(), Value::Object(launch_settings));
    normalized.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    normalized
        .entry("lifecycleState".to_string())
        .or_insert_with(|| Value::String("running".to_string()));
    if read_text(&normalized, "title").is_none() {
        let default_title =
            project_agent_session_default_title(project, &Value::Object(normalized.clone()));
        normalized.insert("title".to_string(), Value::String(default_title));
        /*
        CDXC:SessionTitles 2026-09-09 WHY:
        Custom-agent defaults previously used configured names such as "Claude 71 Session", which the first-prompt auto-title gates mistook for user-chosen titles.
        Stamp every launcher default as a placeholder so auto-naming depends on its source rather than a list of display names.
        */
        if runtime_settings
            .get("titleSource")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            runtime_settings.insert(
                "titleSource".to_string(),
                Value::String("placeholder".to_string()),
            );
        }
    }
    normalized.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    Ok(normalized)
}

/// CDXC:SessionTitles 2026-09-09 DECISION:
/// User: empty Claude/Codex sessions show "∗ Claude Session" / "∗ Codex Session", never the custom agent name or ID created for an account.
/// Identity updates used the custom configuration ID as a display name and overwrote the launcher's placeholder title.
/// SEE-ALSO: agents/identity/, agents/session_state_ingest.rs, agents/drafts.rs, presentation/session_attributes.rs.
pub(crate) fn project_agent_session_default_title(project: &Value, session: &Value) -> String {
    let agent_id = read_text_value(session, "agentId");
    let family = session_agent_family_id(project, session);
    if matches!(family.as_deref(), Some("claude" | "codex")) {
        return create_agent_session_default_title(None, family.as_deref());
    }
    let agent_config = resolve_project_agent_config(
        project,
        agent_id.as_deref().unwrap_or_default(),
        Some(&object_field(session, "launchSettings")),
    );
    // A custom agent this project has no copy of is named by its CLI family, never by its raw id.
    let name = read_text_from_map(&agent_config, "name").or_else(|| {
        family
            .as_deref()
            .and_then(default_agent_session_title_name)
            .map(str::to_string)
    });
    create_agent_session_default_title(name.as_deref(), agent_id.as_deref())
}

/// The folder an Empryo create starts in: the requested `cwd`, else the project's.
fn empryo_cwd(
    params: &Map<String, Value>,
    project: &Value,
) -> Result<std::path::PathBuf, DomainStateError> {
    read_text(params, "cwd")
        .or_else(|| read_text_value(project, "path"))
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            DomainStateError::bad_request("The project has no folder to start Empryo in.")
        })
}

/// CDXC:AgentProviders 2026-10-06 DECISION:
/// User: "yes, Ghostex chooses Pi's model and thinking level at launch, like Claude and Codex." This extends the 2026-09-17 decision (an agent spawning another agent sets that worker's model and effort for the session only, and a resumed worker keeps them) from Claude and Codex to Pi, whose model is `provider/id` (`--model`) and whose effort is its thinking level (`--thinking`).
/// Typing `/model` or `/effort` into Claude Code saves the choice as the default for every new session, so the choice travels as launch flags instead.
/// The flags live in the session's saved base command, which resume, fork and account wrapping all rebuild from.
/// Sven extended it to Empryo on 2026-10-06 (Empryo harness spec). Empryo's terminal app ignores launch flags, so the model goes into a session folder Ghostex seeds and launches with `--session`, and the effort is typed with `/effort` once Empryo is up; neither changes Empryo's default. A resume keeps the model (it reopens that tab), but Empryo 3.9.0-beta drops a tab's effort when it resumes it (seen 2026-10-06), so the effort lasts until the session restarts (session_chat_empryo_launch_selection.rs).
/// Antigravity CLI joined on 2026-10-13 (user: port the launch flags after checking agy still accepts `--effort`): agy 1.2.14 takes `--model <id>` and `--effort low|medium|high|max` for that CLI session only.
/// On 2026-11-01 the user approved every other agent whose installed CLI has the flags, so a coordinator on any agent can start threads on its own model and effort (server/src/coordinators/playbook.md): OMP 18.13.2 (`--model`, `--thinking`), Copilot 1.0.103 (`--model`, `--reasoning-effort`), Grok Build 1.0.70 (`--model`, `--reasoning-effort`) and Hermes Agent 0.22.5 (`--model`, `--reasoning`) take both; Cursor CLI 2026.10.22 (effort lives with each model) takes a model only. DeepSeek Harness 0.17.0 and OpenCode 2.1.4 (the build `with_opencode2` launches; 1.19.4's `-m` is gone from its terminal app, only `opencode run` takes one) have no interactive model flag and stay refused.
/// SEE-ALSO: server/src/ghostex_cli/actions/create.rs (create-agent), server/src/ghostex_cli/board.rs and server/src/board_start_work.rs (board start-work).
fn apply_requested_agent_model(
    project: &Value,
    agent_id: &str,
    agent_config: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
    params: &Map<String, Value>,
    command: Option<String>,
    runtime_settings: &mut Map<String, Value>,
) -> Result<(Option<String>, Option<String>), DomainStateError> {
    let model = requested_agent_model_option(params, "agentModel")?;
    let effort = requested_agent_model_option(params, "agentEffort")?;
    if model.is_none() && effort.is_none() {
        return Ok((command, None));
    }
    let family = resume_agent_family_id(Some(agent_id.to_string()), agent_config, launch_settings)
        .filter(|family| {
            matches!(
                family.as_str(),
                "claude"
                    | "codex"
                    | "pi"
                    | "omp"
                    | "copilot"
                    | "grok"
                    | "hermes-agent"
                    | "cursor"
                    | "zcode"
                    | "empryo"
                    | "antigravity"
                    | "opencode"
            )
        })
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "A launch model or effort can only be set for Claude, Codex, Pi, OMP, Copilot, Grok Build, Hermes Agent, Empryo and Antigravity CLI agents; Cursor CLI takes a model only, ZCode only a coordinator's model, and OpenCode only a coordinator's model and variant.",
            )
        })?;
    if effort.is_some() {
        if let Some(refusal) = model_only_effort_refusal(&family) {
            return Err(DomainStateError::bad_request(refusal));
        }
    }
    if family == "empryo" {
        use crate::session_chat_empryo_launch_selection as launch;
        let model = launch::empryo_launch_model(model.as_deref())?;
        let session_id = launch::seed_empryo_launch_session(
            &empryo_cwd(params, project)?,
            Some(model),
            runtime_settings,
        )?;
        if let Some(effort) = effort.as_deref() {
            launch::record_empryo_launch_effort(runtime_settings, model, effort);
        }
        // Only the launch plan's command names the seeded session (as Pi's `--session-id`), so
        // the saved base command that resume and fork rebuild from stays plain.
        return Ok((command, Some(session_id)));
    }
    if family == "zcode" {
        // CDXC:Coordinators 2026-10-04 WHY:
        // ZCode has no launch model flag, so a coordinator create's chosen model reaches the
        // session as a `/model` line the create flow queues before the first request. There is
        // no such delivery for a spawned thread yet, so a thread's model request stays refused
        // rather than silently dropped, and ZCode takes no effort choice at all.
        if effort.is_some() {
            return Err(DomainStateError::bad_request(
                "ZCode agents take no effort choice.",
            ));
        }
        if crate::coordinators::coordinator_create_request(params)?.is_none() {
            return Err(DomainStateError::bad_request(
                "A ZCode thread keeps its configured model; only a coordinator's own model can be set.",
            ));
        }
        return Ok((command, None));
    }
    if family == "opencode" {
        // CDXC:Coordinators 2026-11-02 WHY:
        // OpenCode 2's terminal app has no launch model flag, so a coordinator create's model and
        // variant reach the session as a `/model provider/model [variant]` line the create flow
        // queues before the first message (`start_orchestrator_role`), which the OpenCode send
        // path applies through OpenCode's server like the chat's model pill. A thread has no such
        // delivery yet, so its model request stays refused rather than silently dropped.
        if crate::coordinators::coordinator_create_request(params)?.is_none() {
            return Err(DomainStateError::bad_request(
                "An OpenCode thread keeps its configured model; only a coordinator's own model and variant can be set.",
            ));
        }
        let Some(model) = model.as_deref() else {
            return Err(DomainStateError::bad_request(
                "An OpenCode effort is a variant of the chosen model, so pick the model too.",
            ));
        };
        if !model
            .split_once('/')
            .is_some_and(|(provider, name)| !provider.is_empty() && !name.is_empty())
            || model.split_whitespace().nth(1).is_some()
            || effort
                .as_deref()
                .is_some_and(|effort| effort.split_whitespace().nth(1).is_some())
        {
            return Err(DomainStateError::bad_request(
                "An OpenCode model is provider/model, such as openai/gpt-6.1-terra, and its effort one of that model's variants.",
            ));
        }
        return Ok((command, None));
    }
    if family == "pi"
        && effort
            .as_deref()
            .is_some_and(|effort| !crate::session_chat_pi_models::is_pi_thinking_level(effort))
    {
        return Err(DomainStateError::bad_request(
            "A Pi effort is its thinking level: off, minimal, low, medium, high, xhigh or max.",
        ));
    }
    if let (Some(effort), Some((name, levels))) = (effort.as_deref(), launch_effort_levels(&family))
    {
        if !levels.contains(&effort) {
            return Err(DomainStateError::bad_request(format!(
                "A {name} effort is one of: {}.",
                levels.join(", ")
            )));
        }
    }
    let base = command
        .or_else(|| default_agent_command(&family).map(str::to_string))
        .unwrap_or_else(|| family.clone());
    with_agent_model_options(&base, &family, model.as_deref(), effort.as_deref())
        .map(|command| (Some(command), None))
}

/// Checks a launch model and effort for `agent_id` the way a create that names them would, without
/// starting or seeding anything, so a saved choice (an automation's, GitHub #662) is refused when it
/// is saved rather than first when it runs unattended.
pub(crate) fn check_requested_agent_model(
    db: &Connection,
    project: &Value,
    agent_id: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    for (key, value) in [("agentModel", model), ("agentEffort", effort)] {
        if let Some(value) = value {
            params.insert(key.to_string(), Value::String(value.to_string()));
        }
    }
    let model = requested_agent_model_option(&params, "agentModel")?;
    let effort = requested_agent_model_option(&params, "agentEffort")?;
    if model.is_none() && effort.is_none() {
        return Ok(());
    }
    let global_agents_project = project_with_global_agent_list(db, project);
    let project = global_agents_project.as_ref().unwrap_or(project);
    let agent_config = resolve_project_agent_config(project, agent_id, None);
    let family = resume_agent_family_id(Some(agent_id.to_string()), &agent_config, &Map::new());
    // Empryo's launch seeds a session folder, so only its model's shape is checked here.
    if family.as_deref() == Some("empryo") {
        return crate::session_chat_empryo_launch_selection::empryo_launch_model(model.as_deref())
            .map(|_| ());
    }
    if let (Some(model), Some(family)) = (model.as_deref(), family.as_deref()) {
        refuse_model_outside_picker(family, model)?;
    }
    let command = read_text_from_map(&agent_config, "command");
    apply_requested_agent_model(
        project,
        agent_id,
        &agent_config,
        &Map::new(),
        &params,
        command,
        &mut Map::new(),
    )
    .map(|_| ())
}

/// CDXC:Automations 2026-11-18 WHY:
/// GitHub #669: `ghostex automation-save` stored a model the agent does not have, so the automation
/// failed first when it ran unattended. A saved model must be one the chat's model picker offers
/// for the agent (the effective catalog: the published lineup, or the one the agent's CLI printed);
/// a Claude alias whose 1M row the picker lists (`opus` for `opus[1m]`) is its 200K twin and passes.
/// An agent the catalog lists no models for (Pi, OMP, Copilot, Hermes) keeps its CLI's own check.
fn refuse_model_outside_picker(family: &str, model: &str) -> Result<(), DomainStateError> {
    let Some(agent) = crate::agent_model_catalog::catalog_agent(family) else {
        return Ok(());
    };
    let rows = agent
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let value = |row: &Value| row.get("value").and_then(Value::as_str).map(str::to_string);
    let values: Vec<String> = rows.iter().filter_map(value).collect();
    if values.is_empty()
        || values
            .iter()
            .any(|value| value == model || (family == "claude" && *value == format!("{model}[1m]")))
    {
        return Ok(());
    }
    let name = agent.get("name").and_then(Value::as_str).unwrap_or(family);
    let offered = rows
        .iter()
        .filter_map(|row| {
            let value = value(row)?;
            Some(match row.get("label").and_then(Value::as_str) {
                Some(label) if label != value => format!("{value} ({label})"),
                _ => value,
            })
        })
        .collect::<Vec<_>>()
        .join(", ");
    Err(DomainStateError::bad_request(format!(
        "{name} has no model \"{model}\", so nothing was saved. It takes: {offered}."
    )))
}

/// Why a family whose CLI takes a launch model but has no effort flag refuses a launch effort.
fn model_only_effort_refusal(family: &str) -> Option<&'static str> {
    match family {
        "cursor" => Some(
            "Cursor CLI takes a launch model but no effort: Cursor keeps the effort with each model, so pick a model id that names it, such as gpt-5.3-codex-high.",
        ),
        _ => None,
    }
}

/// The effort levels a family's CLI lists for its launch effort flag, so a typo is refused here
/// instead of the CLI exiting to the shell. Grok Build lists none, so its value goes through.
fn launch_effort_levels(family: &str) -> Option<(&'static str, &'static [&'static str])> {
    match family {
        "omp" => Some((
            "OMP",
            &[
                "off", "minimal", "low", "medium", "high", "xhigh", "max", "auto",
            ],
        )),
        "copilot" => Some((
            "Copilot",
            &["none", "minimal", "low", "medium", "high", "xhigh", "max"],
        )),
        "hermes-agent" => Some((
            "Hermes Agent",
            &[
                "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
            ],
        )),
        _ => None,
    }
}

/// A new Claude, Codex or Cursor session starts on the model the user last chose as the agent's
/// default (CDXC:AgentProviders 2026-10-07 in agent_model_pins.rs). A model the create names, or
/// one the configured command already pins, is this session's own choice and is never taught to
/// the default; with no default known, or after the agent's settings changed outside Ghostex, the
/// session runs the agent's own default and its first report becomes the remembered one.
fn pin_remembered_agent_model(
    agent_id: &str,
    agent_config: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
    params: &Map<String, Value>,
    command: Option<String>,
    runtime_settings: &mut Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    use crate::agent_model_pins::{
        launch_default, pin_family, session_marker, ModelPin, ORIGIN_DEFAULT, ORIGIN_LEARN,
        ORIGIN_SESSION, SESSION_PIN_KEY,
    };
    let Some(family) =
        resume_agent_family_id(Some(agent_id.to_string()), agent_config, launch_settings)
            .as_deref()
            .and_then(pin_family)
    else {
        return Ok(command);
    };
    let requested_model = requested_agent_model_option(params, "agentModel")?;
    let requested_effort = requested_agent_model_option(params, "agentEffort")?;
    let base = command
        .clone()
        .or_else(|| default_agent_command(family).map(str::to_string))
        .unwrap_or_else(|| family.to_string());
    if requested_model.is_some() || requested_effort.is_some() || command_names_model(&base, family)
    {
        let requested = requested_model.map(|model| ModelPin {
            model,
            effort: requested_effort,
            ..ModelPin::default()
        });
        runtime_settings.insert(
            SESSION_PIN_KEY.to_string(),
            session_marker(ORIGIN_SESSION, requested.as_ref()),
        );
        return Ok(command);
    }
    let pinned = launch_default(family).and_then(|pin| {
        let effort = pin.effort.as_deref().filter(|_| family != "cursor");
        with_agent_model_options(&base, family, Some(&pin.model), effort)
            .ok()
            .map(|command| (command, pin))
    });
    Ok(match pinned {
        Some((pinned_command, pin)) => {
            runtime_settings.insert(
                SESSION_PIN_KEY.to_string(),
                session_marker(ORIGIN_DEFAULT, Some(&pin)),
            );
            Some(pinned_command)
        }
        None => {
            runtime_settings.insert(
                SESSION_PIN_KEY.to_string(),
                session_marker(ORIGIN_LEARN, None),
            );
            command
        }
    })
}

/// CDXC:MarkedHandovers 2026-10-11 WHY:
/// A thread always marks its handovers, so `ghostex orchestrator start-thread` creates it with `markedHandovers`, and the note is saved in its base command beside the model flags, like an orchestrator's role, so resume, fork and account wrapping keep it whatever the setting says. ZCode threads get it from the SessionStart hook instead (activity.rs).
fn apply_thread_marked_handovers(
    agent_id: &str,
    agent_config: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
    params: &Map<String, Value>,
    command: Option<String>,
) -> Option<String> {
    if params.get("markedHandovers").and_then(Value::as_bool) != Some(true) {
        return command;
    }
    let Some(family) =
        resume_agent_family_id(Some(agent_id.to_string()), agent_config, launch_settings)
            .filter(|family| matches!(family.as_str(), "claude" | "codex"))
    else {
        return command;
    };
    let base = command
        .or_else(|| default_agent_command(&family).map(str::to_string))
        .unwrap_or_else(|| family.clone());
    Some(with_thread_marked_handovers(&family, &base))
}

/// CDXC:AgentLauncher 2026-10-17 WHY:
/// The agent list is one global list (the sidebar reads it from the first project that has one, and an agent edit writes it to every project), but a project added after the last agent edit keeps an empty copy. The New session menu offered a custom agent there and worked only because it sends the command along; an id-only start (`ghostex create-agent`, board work, automations) failed with "no launch command", and the row was titled with the raw custom id. A project with no agent list of its own launches from the global one.
/// SEE-ALSO: server/src/sidebar_hud/buttons.rs (`sidebar_agent_state_from_projects`), server/src/sidebar_hud/mutations.rs.
fn project_with_global_agent_list(db: &Connection, project: &Value) -> Option<Value> {
    let non_empty = |value: &Value| value.as_array().is_some_and(|items| !items.is_empty());
    if ["customAgents", "customAgentOrder"]
        .iter()
        .any(|key| project.get(*key).is_some_and(non_empty))
    {
        return None;
    }
    let mut statement = db
        .prepare(
            "SELECT customAgentsJson, customAgentOrderJson FROM projects ORDER BY updatedAt DESC, projectId ASC",
        )
        .ok()?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?;
    for (agents, order) in rows.flatten() {
        let agents = serde_json::from_str::<Value>(&agents).unwrap_or_else(|_| json!([]));
        let order = serde_json::from_str::<Value>(&order).unwrap_or_else(|_| json!([]));
        if non_empty(&agents) || non_empty(&order) {
            let mut project = project.clone();
            let fields = project.as_object_mut()?;
            fields.insert("customAgents".to_string(), agents);
            fields.insert("customAgentOrder".to_string(), order);
            return Some(project);
        }
    }
    None
}

pub(crate) fn create_agent_session_default_title(
    agent_name: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let title_name = normalize_agent_session_title_name(agent_name)
        .or_else(|| {
            default_agent_session_title_name(agent_id.unwrap_or_default()).map(str::to_string)
        })
        .or_else(|| normalize_agent_session_title_name(agent_id));
    title_name
        .map(|name| format!("{name} Session"))
        .unwrap_or_else(|| "Terminal Session".to_string())
}

pub(crate) fn normalize_agent_session_title_name(value: Option<&str>) -> Option<String> {
    let normalized = value?
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn default_agent_session_title_name(agent_id: &str) -> Option<&'static str> {
    match agent_id.trim().to_ascii_lowercase().as_str() {
        "amp" => Some("Amp CLI"),
        "antigravity" => Some("Antigravity CLI"),
        "claude" => Some("Claude"),
        "codebuddy" => Some("CodeBuddy"),
        "codex" => Some("Codex"),
        "command-code" => Some("Command Code"),
        "copilot" => Some("Copilot"),
        "cursor" => Some("Cursor CLI"),
        "mastra" => Some("Mastra Code"),
        "devin" => Some("Devin"),
        "droid" => Some("Factory Droid"),
        "dsh" => Some("DeepSeek Harness"),
        "empryo" => Some("Empryo"),
        "freebuff" => Some("Freebuff"),
        "grok" => Some("Grok Build"),
        "hermes-agent" => Some("Hermes Agent"),
        "kimi" => Some("Kimi Code"),
        "kiro" => Some("Kiro"),
        "omp" => Some("OMP"),
        "openclaude" => Some("OpenClaude"),
        "opencode" => Some("OpenCode"),
        "pi" => Some("Pi"),
        "qoder" => Some("Qoder"),
        "rovodev" => Some("Rovo Dev"),
        _ => None,
    }
}

pub(crate) struct AgentLaunchInput {
    pub(crate) accept_all_mode: Option<String>,
    pub(crate) agent_id: String,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) delayed_send_deadline_at: Option<String>,
    pub(crate) first_user_message: Option<String>,
    pub(crate) global_accept_all_enabled: bool,
    pub(crate) icon: Option<String>,
}

pub(crate) fn build_agent_launch_plan(input: AgentLaunchInput) -> Value {
    build_session_launch_plan(input, false)
}

/// [`build_agent_launch_plan`] for a session that may be a `coordinator`
/// ([`session_is_coordinator`]), which takes no extension lines in its launch note.
pub(crate) fn build_session_launch_plan(input: AgentLaunchInput, coordinator: bool) -> Value {
    let base_command = input
        .command
        .or_else(|| default_agent_command(&input.agent_id).map(str::to_string))
        .unwrap_or_default();
    let launch_command = resolve_session_launch_command(
        &input.agent_id,
        &base_command,
        input.accept_all_mode.as_deref(),
        input.global_accept_all_enabled,
        input.icon.as_deref(),
        coordinator,
    );
    let command = if input.agent_id == "cursor" {
        input
            .agent_session_id
            .filter(|value| !value.trim().is_empty())
            .and_then(|session_id| get_cursor_chat_session_id(Some(&session_id)))
            .map(|chat_id| {
                format!(
                    "{launch_command} --resume {}",
                    quote_shell_double_arg(&chat_id)
                )
            })
            .unwrap_or(launch_command)
    } else {
        launch_command
    };
    let mut plan = Map::new();
    plan.insert("agentCommand".to_string(), Value::String(base_command));
    plan.insert("command".to_string(), Value::String(command.clone()));
    if let Some(deadline) = input.delayed_send_deadline_at {
        plan.insert(
            "delayedSend".to_string(),
            json!({ "deadlineAt": deadline, "disposition": "scheduled" }),
        );
    }
    if let Some(message) = input.first_user_message {
        plan.insert("firstUserMessage".to_string(), Value::String(message));
    }
    plan.insert(
        "startupText".to_string(),
        Value::String(if command.is_empty() {
            String::new()
        } else {
            as_atuin_ignored_shell_input(&command)
        }),
    );
    plan.insert(
        "startupTextDisposition".to_string(),
        Value::String(if command.is_empty() {
            "none".to_string()
        } else {
            "queueAfterTerminalReady".to_string()
        }),
    );
    Value::Object(plan)
}

pub(crate) fn resolve_project_agent_config(
    project: &Value,
    agent_id: &str,
    launch_settings: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let normalized_agent_id = agent_id.trim().to_ascii_lowercase();
    if let Some(bot) = crate::bot_projects::bot_agent_config(project, agent_id) {
        return bot;
    }
    if let Some(agent) = project
        .get("customAgents")
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents.iter().find(|candidate| {
                candidate
                    .as_object()
                    .and_then(|agent| agent.get("agentId").and_then(Value::as_str))
                    .map(|id| id.trim().eq_ignore_ascii_case(&normalized_agent_id))
                    .unwrap_or(false)
            })
        })
        .and_then(Value::as_object)
    {
        return agent.clone();
    }
    launch_settings
        .filter(|settings| read_text_from_map(settings, "agentCommand").is_some())
        .cloned()
        .unwrap_or_default()
}

/// CDXC:AgentProviders 2026-09-04 DECISION:
/// User: Agent approvals defaults to Ask first, so interactive Claude and Codex launches must not force permission-bypass flags unless the global or per-agent policy explicitly selects Run without asking.
/// SEE-ALSO: packages/shared/ghostex-settings/defaults.ts, packages/find/src/agent.rs, apps/history-cli/src/ui/resume.rs.
pub(crate) fn resolve_agent_launch_command(
    agent_id: &str,
    command: &str,
    accept_all_mode: Option<&str>,
    global_accept_all_enabled: bool,
    icon: Option<&str>,
) -> String {
    resolve_session_launch_command(
        agent_id,
        command,
        accept_all_mode,
        global_accept_all_enabled,
        icon,
        false,
    )
}

/// Whether a session's runtime settings mark it as a coordinator (`coordinatorRole`), whose launch
/// note takes no extension lines (marked_handovers.rs `with_launch_notes`).
pub(crate) fn session_is_coordinator(runtime_settings: &Map<String, Value>) -> bool {
    runtime_settings
        .get(crate::coordinators::COORDINATOR_ROLE_SETTING)
        .and_then(Value::as_str)
        .is_some_and(|role| !role.is_empty())
}

/// [`resolve_agent_launch_command`] for a session that may be a `coordinator`.
pub(crate) fn resolve_session_launch_command(
    agent_id: &str,
    command: &str,
    accept_all_mode: Option<&str>,
    global_accept_all_enabled: bool,
    icon: Option<&str>,
    coordinator: bool,
) -> String {
    let enabled = match accept_all_mode {
        Some("enabled") => true,
        Some("disabled") => false,
        _ => global_accept_all_enabled,
    };
    let command = crate::agent_cli::opencode::with_opencode2(agent_id, icon, command);
    let command = apply_accept_all_spec(
        &command,
        agent_id,
        enabled,
        icon,
        accept_all_mode == Some("disabled"),
    );
    let command = super::mode_picker_off::with_auto_mode(agent_id, icon, enabled, &command);
    let command = with_codex_no_daemon(agent_id, icon, &command);
    let command = with_launch_notes(agent_id, icon, &command, coordinator);
    let command = crate::connectors::with_connector_tools(agent_id, icon, &command);
    // Last, so the steps above still read the program's own name (agent_cli/launch_path.rs).
    let command = crate::agent_cli::launch_path::with_reachable_program(&command);
    #[cfg(windows)]
    let command = native_cli_command(&command);
    command
}

/// CDXC:AgentProviders 2026-09-28 WHY:
/// PowerShell picks an agent's .ps1 shim before its vendor-supplied .cmd launcher, which fails under the default Restricted policy. Resolve known bare agent commands before adding launch or resume wrappers so both paths use the native launcher selected by CLI discovery, preserving custom shell commands and arguments.
#[cfg(windows)]
fn native_cli_command(command: &str) -> String {
    // Unit tests assert exact commands, which must not depend on the CLIs installed on the machine.
    if cfg!(test) {
        return command.to_string();
    }
    let trimmed = command.trim_start();
    let binary = trimmed.split_whitespace().next().unwrap_or_default();
    if crate::agent_cli::catalog::CATALOG
        .iter()
        .any(|agent| agent.binary == binary)
    {
        if let Some(path) = crate::platform::live_path::find(binary, &[]) {
            if let Some(extension @ ("cmd" | "bat")) =
                path.extension().and_then(|extension| extension.to_str())
            {
                return format!(
                    "{}{binary}.{extension}{}",
                    &command[..command.len() - trimmed.len()],
                    &trimmed[binary.len()..]
                );
            }
        }
    }
    command.to_string()
}

/// Validate supplied launch options before an empty or non-string value can be mistaken for an omitted option.
pub(crate) fn requested_agent_model_option(
    params: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, DomainStateError> {
    let value = match params.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => value.trim(),
        _ => {
            return Err(DomainStateError::bad_request(format!(
                "{key} needs a non-empty string value."
            )));
        }
    };
    // `@` names a pinned model version in some Pi providers (`vertex/<model>@<date>`); a value
    // with it is shell-quoted when the launch command is built.
    if value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._[]():/@".contains(&byte))
    {
        return Err(DomainStateError::bad_request(format!(
            "\"{value}\" is not a valid model or effort."
        )));
    }
    Ok(Some(value.to_string()))
}

#[cfg(test)]
mod launch_choice_tests {
    use super::*;

    fn launch(
        agent_id: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<Option<String>, String> {
        let mut params = Map::new();
        if let Some(model) = model {
            params.insert("agentModel".to_string(), Value::String(model.to_string()));
        }
        if let Some(effort) = effort {
            params.insert("agentEffort".to_string(), Value::String(effort.to_string()));
        }
        apply_requested_agent_model(
            &Value::Null,
            agent_id,
            &Map::new(),
            &Map::new(),
            &params,
            None,
            &mut Map::new(),
        )
        .map(|(command, _)| command)
        .map_err(|error| error.to_string())
    }

    #[test]
    fn every_agent_with_launch_flags_takes_the_model_and_effort() {
        assert_eq!(
            launch("omp", Some("anthropic/claude-opus-5-5"), Some("high"))
                .unwrap()
                .as_deref(),
            Some("omp --model anthropic/claude-opus-5-5 --thinking high")
        );
        assert_eq!(
            launch("copilot", Some("claude-opus-5.5"), Some("high"))
                .unwrap()
                .as_deref(),
            Some("copilot --model claude-opus-5.5 --reasoning-effort high")
        );
        assert_eq!(
            launch("grok", Some("grok-build"), Some("medium"))
                .unwrap()
                .as_deref(),
            Some("grok --model grok-build --reasoning-effort medium")
        );
        assert_eq!(
            launch(
                "hermes-agent",
                Some("anthropic/claude-sonnet-4.6"),
                Some("high")
            )
            .unwrap()
            .as_deref(),
            Some("hermes --model anthropic/claude-sonnet-4.6 --reasoning high")
        );
        assert_eq!(
            launch("cursor", Some("grok-4.7"), None).unwrap().as_deref(),
            Some("cursor-agent --model grok-4.7")
        );
    }

    #[test]
    fn an_agent_without_an_effort_flag_refuses_only_the_effort() {
        let refusal = launch("cursor", Some("m"), Some("high")).unwrap_err();
        assert!(
            refusal.contains("takes a launch model but no effort"),
            "{refusal}"
        );
        assert!(launch("opencode", Some("opencode/big-pickle"), None)
            .unwrap_err()
            .contains("An OpenCode thread keeps its configured model"));
        assert!(launch("dsh", Some("deepseek-v4"), None)
            .unwrap_err()
            .contains("Cursor CLI takes a model only"));
        assert!(launch("copilot", None, Some("hgih"))
            .unwrap_err()
            .contains("A Copilot effort is one of"));
        assert!(launch("omp", None, Some("ultra"))
            .unwrap_err()
            .contains("A OMP effort"));
        assert!(launch("hermes-agent", None, Some("ultra")).is_ok());
    }
}
