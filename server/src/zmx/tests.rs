use std::{collections::HashMap, fs, path::Path};

use serde_json::{json, Value};

use super::*;
use crate::{
    domain::DomainRepository,
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

fn open_test_database() -> (tempfile::TempDir, rusqlite::Connection) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    (temp, db)
}

#[test]
fn wake_requires_session_provider_start_only_for_missing_zmx_provider() {
    assert!(wake_requires_session_provider_start(&json!({
        "provider": "zmx",
        "providerState": { "lifecycleState": "missing" },
    })));
    assert!(!wake_requires_session_provider_start(&json!({
        "provider": "zmx",
        "providerState": { "lifecycleState": "exists" },
    })));
    assert!(!wake_requires_session_provider_start(&json!({
        "provider": "zmx",
        "providerState": { "lifecycleState": "unknown" },
    })));
    assert!(!wake_requires_session_provider_start(&json!({
        "provider": "local",
        "providerState": { "lifecycleState": "missing" },
    })));
}

#[test]
fn ghostex_attach_command_requires_a_prestarted_provider() {
    let command = build_zmx_attach_command(ZmxAttachCommandInput {
        cwd: "/tmp/project".to_string(),
        global_session_ref: Some("S7k:P100:G100".to_string()),
        gxserver_auth_token_file: Some("/tmp/home/.ghostex/gxserver/auth/token".to_string()),
        gxserver_base_url: Some("http://127.0.0.1:58746".to_string()),
        gxserver_protocol_version: Some(1),
        prompt_editor: Some("monaco".to_string()),
        session_name: "S7k-P100-G100".to_string(),
        title: Some("Terminal".to_string()),
        zmx_executable_path: "/repo/zmx/zig-out/bin/zmx".to_string(),
    });
    assert_eq!(command.matches("attach --require-existing").count(), 2);
    assert!(command.contains("--prompt-editor=monaco"));

    let remote_command = build_zmx_attach_command(ZmxAttachCommandInput {
        prompt_editor: Some("code-server".to_string()),
        ..ZmxAttachCommandInput {
            cwd: "/srv/project".to_string(),
            global_session_ref: Some("S7k:P100:G100".to_string()),
            gxserver_auth_token_file: Some("/tmp/ghostex/token".to_string()),
            gxserver_base_url: Some("http://127.0.0.1:58744".to_string()),
            gxserver_protocol_version: Some(1),
            prompt_editor: None,
            session_name: "S7k-P100-G100".to_string(),
            title: Some("Remote terminal".to_string()),
            zmx_executable_path: "/srv/ghostex/zmx".to_string(),
        }
    });
    assert!(remote_command.contains("--prompt-editor=code-server"));
}

#[test]
fn zmx_run_command_uses_initial_command_and_ghostex_identity() {
    let command = build_zmx_run_command(ZmxRunCommandInput {
        cwd: "/tmp/project".to_string(),
        global_session_ref: Some("S7k:P100:G100".to_string()),
        gxserver_auth_token_file: Some("/tmp/home/.ghostex/gxserver/auth/token".to_string()),
        gxserver_base_url: Some("http://127.0.0.1:58746".to_string()),
        gxserver_protocol_version: Some(1),
        prompt_editor: None,
        session_name: "S7k-P100-G100".to_string(),
        startup_text: "codex --yolo\r".to_string(),
        zmx_executable_path: "/repo/zmx/zig-out/bin/zmx".to_string(),
    });
    assert!(command.contains(
        "run \"$zmx_session\" -d --initial-command /bin/zsh -lic \"$zmx_startup_command\""
    ));
    assert!(command.contains("zmx_startup_text=' codex --yolo'"));
    assert!(command.contains("export GHOSTEX_GLOBAL_SESSION_REF=\"$zmx_global_session_ref\""));
    assert!(command.contains(
        "ghostex_prompt_editor_wrapper=\"$ghostex_prompt_editor_home/state/prompt-editor\""
    ));
    assert!(command.contains("GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR"));
    assert!(!command.contains("export GHOSTEX_PROMPT_EDITOR_BACKEND=monaco"));
    assert!(!command.contains("PATH zmx"));
}

#[test]
fn zmx_child_environment_strips_only_disabling_force_color_values() {
    let mut environment = HashMap::from([
        ("FORCE_COLOR".to_string(), "0".to_string()),
        ("OTHER_KEY".to_string(), "kept".to_string()),
    ]);
    remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
    assert!(!environment.contains_key("FORCE_COLOR"));
    assert_eq!(
        environment.get("OTHER_KEY").map(String::as_str),
        Some("kept")
    );

    environment.insert("FORCE_COLOR".to_string(), "2".to_string());
    remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
    assert_eq!(
        environment.get("FORCE_COLOR").map(String::as_str),
        Some("2")
    );

    environment.insert("FORCE_COLOR".to_string(), " false ".to_string());
    remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
    assert!(!environment.contains_key("FORCE_COLOR"));
}

#[test]
fn provider_state_patches_preserve_unknown_failed_kill_route() {
    let session = json!({
        "providerState": { "lifecycleState": "exists", "provider": "zmx" },
        "zmxName": "S7k-P100-G100",
    });
    let kill = ProviderKill {
        error: Some("zmx kill failed".to_string()),
        exit_code: 42,
        killed: false,
        stderr: "zmx kill failed".to_string(),
        stdout: String::new(),
        zmx_name: "S7k-P100-G100".to_string(),
    };
    let patch = failed_kill_provider_state_patch(&session, &kill, "2026-06-15T18:06:00.000Z")
        .expect("patch");
    assert_eq!(patch.get("lifecycleState"), Some(&json!("unknown")));
    assert_eq!(patch.get("zmxName"), Some(&json!("S7k-P100-G100")));
    assert_eq!(patch.get("killError"), Some(&json!("zmx kill failed")));
}

#[test]
fn provider_metadata_does_not_create_unsupported_session_route() {
    for (session, expected_zmx_name, expected_provider) in [
        (
            json!({
                "providerState": {
                    "lifecycleState": "exists",
                    "provider": "off",
                    "zmxName": "legacy-provider-name"
                },
                "runtimeSettings": { "sessionPersistenceProvider": "off" },
                "zmxName": "S7k-P100-G100"
            }),
            "S7k-P100-G100",
            Some("off"),
        ),
        (
            json!({
                "providerState": {
                    "lifecycleState": "missing",
                    "provider": "tmux",
                    "zmxName": "legacy-tmux-name"
                },
                "runtimeSettings": { "sessionPersistenceProvider": "tmux" },
                "zmxName": "S7k-P100-G101"
            }),
            "S7k-P100-G101",
            Some("tmux"),
        ),
        (
            json!({
                "runtimeSettings": {},
                "zmxName": "S7k-P100-G102"
            }),
            "S7k-P100-G102",
            None,
        ),
    ] {
        assert_eq!(
            provider_zmx_session_name(&session).expect("zmx name"),
            expected_zmx_name
        );
        let probe = ProviderProbe {
            error: None,
            lifecycle_state: "exists".to_string(),
            probed_at: "2026-06-22T07:30:00.000Z".to_string(),
            zmx_name: provider_zmx_session_name(&session).expect("zmx name"),
        };
        let patch = provider_state_patch(&session, &probe).expect("provider patch");
        assert_eq!(patch.get("zmxName"), Some(&json!(probe.zmx_name)));
        assert_eq!(patch.get("lifecycleState"), Some(&json!("exists")));
        assert_eq!(
            patch.get("provider").and_then(Value::as_str),
            expected_provider
        );
    }
}

#[test]
fn wake_activity_suppression_resets_stale_working_state() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Wake Activity" })
                .as_object()
                .expect("project params"),
        )
        .expect("project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "codex",
                "kind": "agent",
                "lifecycleState": "running",
                "projectId": project_id,
                "providerState": { "lifecycleState": "missing", "provider": "zmx" },
                "runtimeSettings": {
                    "agentActivity": {
                        "activity": "working",
                        "agentName": "codex",
                        "hasSeenWorking": true,
                        "isAcknowledged": false,
                        "lastChangedAt": "2026-06-10T06:50:00.000Z",
                        "workingStartedAt": "2026-06-10T06:50:00.000Z"
                    },
                    "titleSource": "user"
                },
                "title": "Sleeping private session"
            })
            .as_object()
            .expect("session params"),
            true,
        )
        .expect("session");
    let before_wake_ms = chrono::Utc::now().timestamp_millis();

    let updated =
        apply_wake_session_activity_suppression(&repository, &session).expect("suppression");
    let activity = updated
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
        .and_then(Value::as_object)
        .expect("agent activity");

    assert_eq!(activity.get("activity"), Some(&json!("idle")));
    assert_eq!(activity.get("hasSeenWorking"), Some(&json!(false)));
    assert_eq!(activity.get("isAcknowledged"), Some(&json!(true)));
    let suppressed_until = activity
        .get("suppressedUntil")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .expect("suppressedUntil")
        .timestamp_millis();
    assert!(suppressed_until - before_wake_ms > 10_000);
}

#[test]
fn transition_action_errors_use_javascript_stringification() {
    assert_eq!(js_string(Some(&json!("pause"))), "pause");
    assert_eq!(
        js_string(Some(&json!({ "action": "pause" }))),
        "[object Object]"
    );
    assert_eq!(js_string(None), "undefined");
}

#[test]
fn zmx_process_identity_parser_prefers_live_codex_child() {
    let session_name = "S90-P3lv0-G0p1k".to_string();
    let identities = parse_zmx_session_process_identities(
        r#"
81395     1 /bundle/zmx run S90-P3lv0-G0p1k -d --initial-command /bin/zsh -lic gx f
81396 81395 /bin/zsh -lic gx f
81557 81396 node /Applications/Ghostex.app/Contents/Resources/CLI/ghostex-cli.mjs f
81582 81557 /Applications/Ghostex.app/Contents/Resources/Web/bin/zehn --accept-all
82148 81582 node /Users/madda/.local/bin/codex --yolo resume 019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78
82149 82148 /Users/madda/.local/lib/codex --yolo resume 019eb8d0-d27b-7f30-b6d7-7a04ab8fae78
94784 93944 /Users/madda/.local/bin/claude --resume 303d77cf-4871-48da-871f-47782e834307
"#
        .trim(),
        std::slice::from_ref(&session_name),
        &format!(
            "  name={session_name}\tpid=81396\tclients=1\tcreated=1781219985\tstart_dir=/repo"
        ),
    );
    let identity = identities.get(&session_name).expect("identity");
    assert_eq!(identity.agent_id.as_deref(), Some("codex"));
    assert_eq!(
        identity.agent_session_id.as_deref(),
        Some("019eb8d0-d27b-7f30-b6d7-7a04ab8fae78")
    );
    assert_eq!(identity.agent_session_path, None);
}

#[test]
fn codex_rollout_path_resolves_exact_session_identity() {
    let path = Path::new(
            "/home/person/.codex/sessions/2026/08/07/rollout-2026-08-07T08-15-34-019fda6e-fdbe-7570-a4fd-347e9e0bfb40.jsonl",
        );
    assert_eq!(
        codex_session_id_from_transcript_path(path).as_deref(),
        Some("019fda6e-fdbe-7570-a4fd-347e9e0bfb40")
    );
    assert_eq!(
            codex_session_id_from_transcript_path(Path::new(
                "/home/person/.codex/archive/rollout-2026-08-07T08-15-34-019fda6e-fdbe-7570-a4fd-347e9e0bfb40.jsonl"
            )),
            None
        );
}

#[test]
fn codex_process_open_rollout_resolves_exact_session_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    let sessions = temp
        .path()
        .join("sessions")
        .join("2026")
        .join("08")
        .join("11");
    fs::create_dir_all(&sessions).expect("sessions directory");
    let rollout =
        sessions.join("rollout-2026-08-11T20-15-34-019fda6e-fdbe-7570-a4fd-347e9e0bfb40.jsonl");
    let _open_rollout = fs::File::create(&rollout).expect("open rollout");
    let identity = read_codex_process_session_identity(Some(i64::from(std::process::id())))
        .expect("process rollout identity");
    assert_eq!(identity.0, "019fda6e-fdbe-7570-a4fd-347e9e0bfb40");
    assert_eq!(Path::new(&identity.1), rollout);
}

#[test]
fn zmx_process_identity_parser_keeps_omp_terminal_name() {
    let session_name = "S90-P3lv0-G0omp".to_string();
    let identities = parse_zmx_session_process_identities(
        r#"
100     1 ttys006 /bundle/zmx run S90-P3lv0-G0omp -d --initial-command /bin/zsh -lic omp
101   100 ttys006 /bin/zsh -lic omp
102   101 ttys006 bun /Users/person/.bun/bin/omp
"#
        .trim(),
        std::slice::from_ref(&session_name),
        &format!("  name={session_name}\tpid=100\tclients=1\tcreated=1781219985\tstart_dir=/repo"),
    );
    let identity = identities.get(&session_name).expect("identity");
    assert_eq!(identity.agent_id.as_deref(), Some("omp"));
    assert_eq!(identity.agent_session_id, None);
    assert_eq!(identity.terminal_name.as_deref(), Some("ttys006"));
}

#[test]
fn omp_terminal_record_resolves_exact_transcript_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    let agent_dir = temp.path().join(".omp").join("agent");
    let transcript_path = agent_dir
        .join("sessions")
        .join("repo")
        .join("2026-08-05T20-29-13-027Z_019fd39d-a9c3-7000-b87f-5ac5c34a9778.jsonl");
    fs::create_dir_all(transcript_path.parent().expect("transcript parent"))
        .expect("transcript directory");
    fs::write(&transcript_path, "{}\n").expect("transcript");
    let terminal_records = agent_dir.join("terminal-sessions");
    fs::create_dir_all(&terminal_records).expect("terminal records");
    fs::write(
        terminal_records.join("ttys006"),
        format!("/repo\n{}\nfresh\n", transcript_path.display()),
    )
    .expect("terminal record");

    let identity =
        read_omp_terminal_session_identity_from_agent_dir(&agent_dir, "ttys006", temp.path())
            .expect("OMP transcript identity");
    assert_eq!(identity.0, "019fd39d-a9c3-7000-b87f-5ac5c34a9778");
    assert_eq!(identity.1, transcript_path.to_string_lossy());
}

#[test]
fn zmx_process_identity_parser_recognizes_attached_codex_resume_without_thread_id() {
    let session_name = "S2o-P7n77-G8wrt".to_string();
    let identities = parse_zmx_session_process_identities(
            r#"
52961     1 /home/ghostex/.ghostex/gxserver/package/bin/zmx attach --prompt-editor=monaco S2o-P7n77-G8wrt
52962 52961 -bash
52966 52962 codex --yolo resume
"#
            .trim(),
            std::slice::from_ref(&session_name),
            &format!(
                "  name={session_name}\tpid=52962\tclients=1\tcreated=1782781010\tstart_dir=/home/ghostex/ghostex"
            ),
        );
    /*
    CDXC:GxserverSessionIdentity 2026-06-30-11:15:
    Attached remote zmx terminals can run Codex as a shell child without a
    resume thread id argument. The live-process scanner still needs to
    identify the agent so gxserver can project the sidebar agent icon.
    */
    let identity = identities.get(&session_name).expect("identity");
    assert_eq!(identity.agent_id.as_deref(), Some("codex"));
    assert_eq!(identity.agent_session_id, None);
    assert_eq!(identity.agent_session_path, None);
}

#[test]
fn zmx_process_identity_parser_ignores_helper_payload_agent_mentions() {
    let session_name = "S90-P3lv0-G8cl2".to_string();
    let identities = parse_zmx_session_process_identities(
            r#"
23572     1 -zsh
23754 23572 node /Users/person/.local/share/mise/installs/node/24.14.1/bin/codex --yolo
23755 23754 /Users/person/.local/share/mise/installs/node/24.14.1/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex --yolo
34225 23755 /Users/person/.codex/computer-use/Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/MacOS/SkyComputerUseClient turn-ended {"input-messages":["compare codex hotkeys with claude code"]}
"#
            .trim(),
            std::slice::from_ref(&session_name),
            &format!("  name={session_name}\tpid=23572\tclients=1\tcreated=1781317239\tstart_dir=/repo"),
        );
    /*
    CDXC:GxserverSessionIdentity 2026-06-21-18:25:
    Rust process identity parsing must copy TypeScript's helper-process guard. Agent labels come from actual executable tokens, not serialized helper payloads that may mention other CLIs in user-owned text.
    */
    let identity = identities.get(&session_name).expect("identity");
    assert_eq!(identity.agent_id.as_deref(), Some("codex"));
    assert_eq!(identity.agent_session_id, None);
    assert_eq!(identity.agent_session_path, None);
}

#[test]
fn send_payload_validation_uses_utf8_byte_cap() {
    assert!(read_interaction_text(Some(&json!("hello")), "sendSessionText").is_ok());
    let error = read_interaction_text(Some(&json!("")), "sendSessionText")
        .expect_err("empty text rejected");
    assert_eq!(error.code, "badRequest");
    let rocket = "\u{1F680}";
    assert!(read_interaction_text(
        Some(&json!(rocket.repeat(GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES / 4))),
        "sendSessionText",
    )
    .is_ok());
    let oversized = "x".repeat(GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES + 1);
    let error = read_interaction_text(Some(&json!(oversized)), "sendSessionText")
        .expect_err("oversized text rejected");
    assert!(error.message.contains("zmx send limit"));
}

#[test]
fn send_result_text_length_matches_javascript_string_length() {
    let result = send_result(
        0,
        json!({ "projectId": "P7abc", "sessionId": "G8def" }),
        "go \u{1F680}",
        false,
        "S7k-P7abc-G8def".to_string(),
    );
    assert_eq!(result["textBytes"], json!(7));
    assert_eq!(result["textLength"], json!(5));
}

#[test]
fn queued_launch_startup_text_is_explicit_and_consumable() {
    let session = json!({
        "launchSettings": {
            "agentLaunchPlan": {
                "startupText": " cursor-agent --yolo\r"
            },
            "runtimeRelevant": {
                "queueProviderStartupText": true
            }
        }
    });
    assert_eq!(
        get_queued_agent_launch_startup_text_for_session(&session),
        Some(" cursor-agent --yolo\r".to_string())
    );
    let consumed =
        launch_settings_with_consumed_agent_launch_startup_text(&session).expect("consumed");
    assert_eq!(
        consumed
            .get("runtimeRelevant")
            .and_then(Value::as_object)
            .and_then(|runtime| runtime.get("queueProviderStartupText")),
        Some(&Value::Bool(false))
    );
}

#[test]
fn startup_text_disposition_never_replays_live_provider_text() {
    assert_eq!(
        decide_startup_text_disposition("exists", Some(" codex --yolo")),
        "discardExistingProvider"
    );
    assert_eq!(
        decide_startup_text_disposition("unknown", Some(" codex --yolo")),
        "discardUnknownProvider"
    );
    assert_eq!(
        decide_startup_text_disposition("missing", Some(" codex --yolo")),
        "queueAfterTerminalReady"
    );
    assert_eq!(decide_startup_text_disposition("missing", None), "none");
}
