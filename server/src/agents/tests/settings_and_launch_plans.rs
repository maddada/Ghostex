use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;

#[test]
fn agent_settings_use_current_metadata_key_and_default_prompt_agent() {
    let (_temp, db) = open_test_database();
    let initial = read_agent_settings_with_metadata(&db).expect("initial settings");
    assert_eq!(initial.get("isPersisted"), Some(&json!(false)));
    assert_eq!(
        initial
            .get("settings")
            .and_then(|settings| settings.get("agentAcceptAllEnabled")),
        Some(&json!(false))
    );
    assert_eq!(
        initial
            .get("settings")
            .and_then(|settings| settings.get("defaultPromptAgentId")),
        Some(&json!("codex"))
    );

    let updated = update_agent_settings(
        &db,
        json!({ "agentAcceptAllEnabled": false, "defaultPromptAgentId": " claude " })
            .as_object()
            .expect("params"),
    )
    .expect("update settings");
    assert_eq!(updated.get("agentAcceptAllEnabled"), Some(&json!(false)));
    assert_eq!(updated.get("defaultPromptAgentId"), Some(&json!("claude")));
    let persisted: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [AGENT_SETTINGS_METADATA_KEY],
            |row| row.get(0),
        )
        .expect("persisted settings");
    let persisted_value = parse_json_object(&persisted);
    assert_eq!(
        persisted_value
            .get("defaultPromptAgentId")
            .and_then(Value::as_str),
        Some("claude")
    );
    let legacy_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM metadata WHERE key = 'gxserverAgentSettings'",
            [],
            |row| row.get(0),
        )
        .expect("legacy count");
    assert_eq!(legacy_count, 0);
}

#[test]
fn agent_settings_ignore_legacy_metadata_key() {
    let (_temp, db) = open_test_database();
    let legacy_value = json!({ "agentAcceptAllEnabled": false, "defaultPromptAgentId": "claude" });
    write_metadata_value(&db, "gxserverAgentSettings", legacy_value.clone());

    let settings = read_agent_settings_with_metadata(&db).expect("legacy ignored");
    assert_eq!(settings.get("isPersisted"), Some(&json!(false)));
    assert_eq!(
        settings
            .get("settings")
            .and_then(|settings| settings.get("agentAcceptAllEnabled")),
        Some(&json!(false))
    );
    assert_eq!(
        settings
            .get("settings")
            .and_then(|settings| settings.get("defaultPromptAgentId")),
        Some(&json!("codex"))
    );

    let updated = update_agent_settings(
        &db,
        json!({ "defaultPromptAgentId": " claude " })
            .as_object()
            .expect("params"),
    )
    .expect("update settings with legacy row present");
    assert_eq!(updated.get("agentAcceptAllEnabled"), Some(&json!(false)));
    assert_eq!(updated.get("defaultPromptAgentId"), Some(&json!("claude")));

    let current: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [AGENT_SETTINGS_METADATA_KEY],
            |row| row.get(0),
        )
        .expect("current metadata");
    let current_value = parse_json_object(&current);
    assert_eq!(
        current_value
            .get("agentAcceptAllEnabled")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        current_value
            .get("defaultPromptAgentId")
            .and_then(Value::as_str),
        Some("claude")
    );

    let legacy: String = db
        .query_row(
            "SELECT value FROM metadata WHERE key = 'gxserverAgentSettings'",
            [],
            |row| row.get(0),
        )
        .expect("legacy metadata remains unrelated");
    assert_eq!(parse_json_object(&legacy), legacy_value);
}

#[test]
fn agent_settings_normalize_default_prompt_agent_id() {
    let (_temp, db) = open_test_database();
    let blank = update_agent_settings(
        &db,
        json!({ "defaultPromptAgentId": "   " })
            .as_object()
            .expect("params"),
    )
    .expect("blank update");
    assert_eq!(blank.get("defaultPromptAgentId"), Some(&json!("codex")));

    let long_id = "x".repeat(MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH + 10);
    let capped = update_agent_settings(
        &db,
        json!({ "defaultPromptAgentId": long_id })
            .as_object()
            .expect("params"),
    )
    .expect("long update");
    let stored = capped
        .get("defaultPromptAgentId")
        .and_then(Value::as_str)
        .expect("stored id");
    assert_eq!(stored.len(), MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH);
}

#[test]
fn create_agent_session_params_use_project_agent_config_and_settings() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    update_agent_settings(
        &db,
        json!({ "agentAcceptAllEnabled": false })
            .as_object()
            .expect("settings params"),
    )
    .expect("agent settings");
    let project = repository
        .create_project(
            json!({
                "customAgents": [{
                    "acceptAllMode": "enabled",
                    "agentId": "claude",
                    "command": "claude",
                    "icon": "claude"
                }],
                "name": "Agent CRUD",
                "path": std::env::temp_dir()
            })
            .as_object()
            .expect("project params"),
        )
        .expect("project created");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id");
    let params = json!({
        "agentId": "claude",
        "launchSettings": {
            "agentCommand": "ignored-local-command",
            "delayedSendDeadlineAt": "2026-06-22T05:40:00.000Z"
        },
        "projectId": project_id,
        "runtimeSettings": {
            "firstUserMessage": "Summarize this repository."
        },
        "title": "Claude Agent"
    });
    let create_params = create_agent_session_params_for_project(
        &db,
        &project,
        params.as_object().expect("create params"),
    )
    .expect("normalized create params");

    let launch_settings = create_params
        .get("launchSettings")
        .and_then(Value::as_object)
        .expect("launch settings");
    let launch_plan = launch_settings
        .get("agentLaunchPlan")
        .and_then(Value::as_object)
        .expect("launch plan");
    assert_eq!(launch_plan.get("agentCommand"), Some(&json!("claude")));
    assert_eq!(
        launch_plan.get("command"),
        Some(&json!(
            r#"claude --dangerously-skip-permissions --settings '{"skipDangerousModePermissionPrompt":true}'"#
        ))
    );
    assert_eq!(
        launch_plan.get("firstUserMessage"),
        Some(&json!("Summarize this repository."))
    );
    assert_eq!(
        launch_plan
            .get("delayedSend")
            .and_then(|value| value.get("deadlineAt")),
        Some(&json!("2026-06-22T05:40:00.000Z"))
    );
    assert_eq!(
        launch_settings
            .get("runtimeRelevant")
            .and_then(|value| value.get("queueProviderStartupText")),
        Some(&json!(true))
    );
    let runtime_settings = create_params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .expect("runtime settings");
    assert_eq!(runtime_settings.get("agentCommand"), Some(&json!("claude")));
    assert_eq!(
        runtime_settings.get("launchAgentId"),
        Some(&json!("claude"))
    );
    assert_eq!(
        runtime_settings
            .get("agentActivity")
            .and_then(|value| value.get("activity")),
        Some(&json!("idle"))
    );
    assert_eq!(
        runtime_settings
            .get("agentActivity")
            .and_then(|value| value.get("agentName")),
        Some(&json!("claude"))
    );

    let session = repository
        .create_session(&create_params, false)
        .expect("agent session created");
    assert_eq!(session.get("kind"), Some(&json!("agent")));
    assert_eq!(session.get("agentId"), Some(&json!("claude")));
    assert_eq!(
        session
            .get("launchSettings")
            .and_then(|value| value.get("agentLaunchPlan"))
            .and_then(|value| value.get("command")),
        Some(&json!(
            r#"claude --dangerously-skip-permissions --settings '{"skipDangerousModePermissionPrompt":true}'"#
        ))
    );
}

#[test]
fn accept_all_specs_match_typescript_aliases_and_icon_mapping() {
    assert_eq!(
        resolve_agent_launch_command("cursor", "cursor-agent --allow-all", None, true, None),
        "cursor-agent --allow-all --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "cursor",
            "cursor-agent --force --yolo",
            Some("disabled"),
            true,
            None,
        ),
        "cursor-agent"
    );
    assert_eq!(
        resolve_agent_launch_command("cursor", "cursor-agent --allow-all", None, true, None),
        "cursor-agent --allow-all --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command("copilot", "copilot -y", None, true, None),
        "copilot -y --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "custom-cursor",
            "cursor-agent",
            None,
            true,
            Some("cursor-cli")
        ),
        "cursor-agent --yolo"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "grok",
            "grok --permission-mode bypassPermissions --always-approve",
            None,
            true,
            None,
        ),
        "grok --permission-mode bypassPermissions"
    );
    assert_eq!(
        resolve_agent_launch_command(
            "grok",
            "grok --permission-mode=bypassPermissions --always-approve",
            Some("disabled"),
            true,
            None,
        ),
        "grok"
    );
}

#[test]
fn cursor_launch_appends_only_normalized_resume_chat_ids() {
    let valid = build_agent_launch_plan(AgentLaunchInput {
        accept_all_mode: None,
        agent_id: "cursor".to_string(),
        agent_session_id: Some("8B16E7E6-3CE1-4D0B-9F35-78261B7F0767".to_string()),
        command: Some("cursor-agent".to_string()),
        delayed_send_deadline_at: None,
        first_user_message: None,
        global_accept_all_enabled: true,
        icon: None,
    });
    assert_eq!(
        valid.get("command"),
        Some(&json!(
            "cursor-agent --yolo --resume \"8b16e7e6-3ce1-4d0b-9f35-78261b7f0767\""
        ))
    );

    let invalid = build_agent_launch_plan(AgentLaunchInput {
        accept_all_mode: None,
        agent_id: "cursor".to_string(),
        agent_session_id: Some("not-a-chat-id".to_string()),
        command: Some("cursor-agent".to_string()),
        delayed_send_deadline_at: None,
        first_user_message: None,
        global_accept_all_enabled: true,
        icon: None,
    });
    assert_eq!(invalid.get("command"), Some(&json!("cursor-agent --yolo")));
}

#[test]
fn resume_and_fork_plans_shape_agent_commands() {
    let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
    // A written rollout: a Codex thread that was never saved starts fresh instead.
    let rollouts = tempfile::tempdir().expect("tempdir");
    let rollout = rollouts
        .path()
        .join("rollout-12345678-1234-1234-1234-123456789abc.jsonl");
    std::fs::write(&rollout, "{}\n").expect("rollout");
    let session = json!({
        "agentId": "codex",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "codex",
            "agentSessionId": "12345678-1234-1234-1234-123456789abc",
            "agentSessionPath": rollout.to_string_lossy(),
            "titleSource": "terminal-auto"
        },
        "title": "Investigate bug",
    });
    let settings = normalize_agent_settings(None);
    let resume = build_agent_resume_plan(&project, &session, &settings);
    let primary_command = resume
        .get("primaryCommand")
        .and_then(Value::as_str)
        .expect("primary command");
    assert!(primary_command.contains("CODEX_RESUME_SESSION_ID"));
    assert!(primary_command.contains("--exact"));
    assert!(primary_command.contains("codex resume \"$CODEX_RESUME_SESSION_ID\""));
    assert_eq!(
        resume.get("displayCommand"),
        Some(&json!(
            "codex resume \"12345678-1234-1234-1234-123456789abc\""
        ))
    );
    assert_eq!(
        resume.get("copyCommand"),
        Some(&json!(
            "codex resume \"12345678-1234-1234-1234-123456789abc\""
        ))
    );
    assert!(resume
        .get("fallbackCommand")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains("--title")));
    let startup_text = resume
        .get("startupText")
        .and_then(Value::as_str)
        .expect("startup text");
    assert!(startup_text.starts_with(' '));
    assert!(startup_text.contains("Restoring session..."));
    assert!(startup_text
        .contains("__ghostex_restore_resume_primary || __ghostex_restore_resume_status=$?"));
    assert!(startup_text.contains("Exact resume failed; trying saved fallback resume command."));
    assert!(startup_text.contains("codex resume \"12345678-1234-1234-1234-123456789abc\""));
    let fork = build_agent_fork_plan(&project, &session, &settings);
    assert_eq!(
        fork.get("primaryCommand"),
        Some(&json!(
            "codex fork \"12345678-1234-1234-1234-123456789abc\""
        ))
    );
}

#[test]
fn resume_plan_extracts_provider_exact_identity_hints() {
    let project = json!({ "path": "/repo/ghostex", "customAgents": [], "launchSettings": {} });
    let settings = {
        let mut settings = normalize_agent_settings(None);
        settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(false));
        settings
    };
    // A written transcript: a Claude conversation that was never written starts fresh instead.
    let transcripts = tempfile::tempdir().expect("tempdir");
    let transcript = transcripts
        .path()
        .join("9970b270-b39f-4d63-a764-fa8d88083995.jsonl");
    std::fs::write(
        &transcript,
        "{}
",
    )
    .expect("transcript");
    let claude = json!({
        "agentId": "claude",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "claude",
            "agentSessionPath": transcript.to_string_lossy(),
            "titleSource": "user"
        },
        "title": "Readable Claude title",
    });
    let claude_plan = build_agent_resume_plan(&project, &claude, &settings);
    let claude_resume = json!("claude --resume \"9970b270-b39f-4d63-a764-fa8d88083995\"");
    #[cfg(windows)]
    assert_eq!(claude_plan.get("primaryCommand"), Some(&claude_resume));
    #[cfg(not(windows))]
    assert!(claude_plan
        .get("primaryCommand")
        .and_then(Value::as_str)
        .is_some_and(|command| {
            command.starts_with("__ghostex_claude_bg_id=\"$(claude agents --json ")
                && command.contains("then claude attach \"$__ghostex_claude_bg_id\"; else claude --resume \"9970b270-b39f-4d63-a764-fa8d88083995\"; fi")
        }));
    assert_eq!(claude_plan.get("displayCommand"), Some(&claude_resume));
    assert_eq!(claude_plan.get("copyCommand"), Some(&claude_resume));
    // A conversation id needs no title lookup when its resume fails.
    assert!(claude_plan.get("fallbackCommand").is_none());

    let cursor = json!({
        "agentId": "cursor",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "cursor-agent",
            "resumeCommand": "cd '/repo/ghostex' && cursor-agent --resume \"E10971DA-CBD7-459A-9AC3-B9B0313199A3\"",
            "titleSource": "user"
        },
        "title": "∗ Cursor CLI Session",
    });
    let cursor_plan = build_agent_resume_plan(&project, &cursor, &settings);
    assert_eq!(
        cursor_plan.get("primaryCommand"),
        Some(&json!(
            "cursor-agent --resume \"e10971da-cbd7-459a-9ac3-b9b0313199a3\""
        ))
    );
    assert!(cursor_plan.get("fallbackCommand").is_none());

    let pi = json!({
        "agentId": "pi",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "pi",
            "agentSessionId": "pi-id",
            "agentSessionPath": "/tmp/pi/session/path",
            "titleSource": "user"
        },
        "title": "Pi thread",
    });
    let pi_plan = build_agent_resume_plan(&project, &pi, &settings);
    // Pi has not written that path yet, so the session reopens on its id.
    assert_eq!(
        pi_plan.get("primaryCommand"),
        Some(&json!("pi --session-id pi-id"))
    );
}

#[test]
fn opencode_resume_keeps_lookup_command_separate_from_runtime_accept_all() {
    let project = json!({ "path": "/repo/ghostex", "customAgents": [], "launchSettings": {} });
    let settings = normalize_agent_settings(None);
    let titled = json!({
        "agentId": "opencode",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "opencode",
            "titleSource": "user"
        },
        "title": "Readable thread title",
    });
    let plan = build_agent_resume_plan(&project, &titled, &settings);
    assert_eq!(plan.get("runtimeCommand"), Some(&json!("opencode")));
    assert_eq!(plan.get("lookupCommand"), Some(&json!("opencode")));
    let primary = plan
        .get("primaryCommand")
        .and_then(Value::as_str)
        .expect("primary command");
    assert!(primary.contains("opencode -s"));
    assert!(primary.contains("opencode session list --format json"));
    assert!(!primary.contains("opencode --auto session list"));
    assert!(plan.get("copyCommand").is_none());
}

#[test]
fn attach_startup_text_uses_agent_resume_plan_and_settings() {
    let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
    // A written rollout: a Codex thread that was never saved starts fresh instead.
    let rollouts = tempfile::tempdir().expect("tempdir");
    let rollout = rollouts
        .path()
        .join("rollout-12345678-1234-1234-1234-123456789abc.jsonl");
    std::fs::write(&rollout, "{}\n").expect("rollout");
    let session = json!({
        "agentId": "codex",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "codex",
            "agentSessionId": "12345678-1234-1234-1234-123456789abc",
            "agentSessionPath": rollout.to_string_lossy()
        },
        "title": "Restorable Codex",
    });
    let mut settings = normalize_agent_settings(None);
    settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(false));

    let startup_text =
        get_agent_startup_text_for_session(&project, &session, &settings).expect("startup text");
    assert!(startup_text.starts_with(' '));
    assert!(startup_text.ends_with('\r'));
    assert!(startup_text.contains("Restoring session..."));
    assert!(startup_text
        .contains("printf '> %s\\n\\n' 'codex resume \"12345678-1234-1234-1234-123456789abc\"'"));
    assert!(startup_text.contains("codex resume \"12345678-1234-1234-1234-123456789abc\""));
}

#[test]
fn resume_plan_rejects_gxserver_session_id_titles() {
    let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
    let session = json!({
        "agentId": "cursor",
        "launchSettings": {},
        "runtimeSettings": {
            "agentCommand": "cursor-agent",
            "titleSource": "user"
        },
        "title": "G3gnt",
    });
    let settings = normalize_agent_settings(None);
    let resume = build_agent_resume_plan(&project, &session, &settings);
    assert_eq!(resume.get("primaryCommand"), None);
    assert_eq!(resume.get("startupTextDisposition"), Some(&json!("none")));
}

#[test]
fn remembered_pins_respect_a_model_the_command_already_chooses() {
    assert!(command_names_model("claude --model 'opus[1m]'", "claude"));
    assert!(command_names_model("codex -m gpt-6-astra", "codex"));
    assert!(command_names_model("codex --profile fast", "codex"));
    assert!(!command_names_model(
        "claude --append-system-prompt '--model' --effort high",
        "claude"
    ));
    assert!(!command_names_model(
        "cursor-agent --workspace --model",
        "cursor"
    ));
    assert!(command_names_model(
        "cursor-agent --yolo --model grok-4.7",
        "cursor"
    ));
}

#[test]
fn cursor_pins_replace_its_model_and_take_no_effort_flag() {
    let pinned = with_agent_model_options(
        "cursor-agent --model gpt-6 --workspace /tmp",
        "cursor",
        Some("grok-4.7"),
        Some("high"),
    )
    .expect("cursor pin");
    assert_eq!(pinned, "cursor-agent --workspace /tmp --model grok-4.7");
}

#[test]
fn a_coordinator_offered_low_gets_it_in_every_agents_launch_command() {
    use crate::coordinators::{coordinator_create_params, CoordinatorLaunch};
    use ghostex_gx_chat_core::menus::orchestrator_effort::orchestrator_efforts;

    let claude = ["low", "medium", "high", "xhigh", "max", "ultracode"];
    let codex = ["low", "medium", "high", "xhigh", "ultra"];
    let antigravity = ["low", "medium", "high", "max"];
    for levels in [&claude[..], &codex[..], &antigravity[..]] {
        assert_eq!(orchestrator_efforts(levels, false, None)[0], "low");
    }

    let rows =
        vec![json!({"agentId": "claude", "name": "Claude", "icon": "claude", "command": "claude"})];
    let params = coordinator_create_params(CoordinatorLaunch {
        agent_rows: &rows,
        project_id: "P1",
        agent_id: "claude",
        title: None,
        goal: String::new(),
        task: String::new(),
        model: None,
        effort: Some("low".to_string()),
    });
    assert_eq!(params["agentEffort"], "low");
    assert_eq!(
        requested_agent_model_option(params.as_object().unwrap(), "agentEffort")
            .unwrap()
            .as_deref(),
        Some("low")
    );

    assert_eq!(
        with_agent_model_options("claude", "claude", Some("opus[1m]"), Some("low")).unwrap(),
        "claude --model 'opus[1m]' --effort low"
    );
    assert_eq!(
        with_agent_model_options("codex", "codex", None, Some("low")).unwrap(),
        "codex -c model_reasoning_effort=low"
    );
    assert_eq!(
        with_agent_model_options("agy", "antigravity", None, Some("low")).unwrap(),
        "agy --effort low"
    );

    // A configured command's own pin is replaced, never repeated.
    assert_eq!(
        with_agent_model_options(
            "grok -m old --effort low --rules '--model x'",
            "grok",
            Some("new"),
            Some("high")
        )
        .unwrap(),
        "grok --rules '--model x' --model new --reasoning-effort high"
    );
    assert_eq!(
        with_agent_model_options(
            "hermes -m old --reasoning=low --provider anthropic",
            "hermes-agent",
            Some("new"),
            Some("high")
        )
        .unwrap(),
        "hermes --provider anthropic --model new --reasoning high"
    );
    assert_eq!(
        with_agent_model_options(
            "omp --provider openai --model gpt --thinking low",
            "omp",
            Some("openai/gpt-5.5"),
            Some("high")
        )
        .unwrap(),
        "omp --model openai/gpt-5.5 --thinking high"
    );
    assert_eq!(
        with_agent_model_options(
            "copilot --reasoning-effort low",
            "copilot",
            None,
            Some("xhigh")
        )
        .unwrap(),
        "copilot --reasoning-effort xhigh"
    );
    assert_eq!(
        with_agent_model_options("grok -m a", "grok", Some("b"), None).unwrap(),
        "grok --model b"
    );
}
