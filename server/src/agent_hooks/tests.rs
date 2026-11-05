use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::paths::get_gxserver_paths;

use super::api::{
    install_agent_hooks, read_agent_hook_status, repair_installed_agent_hook_paths,
    uninstall_agent_hooks,
};
use super::config::{
    all_hook_events, HookDefinition, HookPaths, AMP_PLUGIN_MARKER, NOTIFY_HOOK_MARKER,
    NOTIFY_HOOK_VERSION, OPENCODE_PLUGIN_MARKER, PI_EXTENSION_MARKER,
};
use super::event_mapping::activity_for_hook_event;
use super::install::{
    ghostex_theme_file, inspect_agent_hook_installation, install_ghostex_opencode_theme,
    install_notify_hook, is_notify_hook_current, json_contains_hook_command,
    migrate_hook_session_sidecars, read_json_object, remove_json_hook,
    remove_retired_opencode_plugin, set_executable_permissions, uninstall_marked_yaml_hook,
    uninstall_plugin_file_hook,
};
use super::notify_runtime::{read_hook_state, read_state_string, run_notify_hook};
use super::plugin_sources::{command_for_agent, shell_quote};
use super::probing::{command_exists, decode_base64_text, path_string, read_file_text};
use super::resolution::provider_hook_paths;
use super::statusline::{install_statusline_hook, statusline_command};

#[test]
fn hook_status_uses_home_scoped_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let status = read_agent_hook_status(
        &paths,
        json!({ "agentIds": ["qoder"], "autoUpgradeInstalled": false })
            .as_object()
            .expect("params"),
    )
    .expect("status");
    assert_eq!(status.get("type"), Some(&json!("agentHookStatus")));
    assert!(status
        .get("notifyHookPath")
        .and_then(Value::as_str)
        .expect("notify path")
        .starts_with(temp.path().to_str().expect("temp path")));
}

#[test]
fn hook_status_reports_profile_only_provider_hook_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::from_paths(&paths);
    seed_real_cli(&hook_paths.home_dir, "claude");
    write_test_executable(&temp.path().join(".local").join("bin").join("claude"));
    install_notify_hook(&hook_paths).expect("notify hook");
    let profile_path = temp
        .path()
        .join(".claude-profiles")
        .join("work")
        .join("settings.json");
    let claude = HookDefinition {
        agent_id: "claude",
        cli_command: "claude",
    };
    let command = command_for_agent(&claude, &hook_paths.notify_hook_path);
    // A current install must carry the whole shipped event catalog, so the
    // fixture registers every event Ghostex writes for Claude today.
    let hooks = all_hook_events("claude")
        .into_iter()
        .map(|event_name| {
            (
                event_name.to_string(),
                json!([{ "hooks": [{ "type": "command", "command": command,
                    "timeout": super::config::nested_event_timeout("claude", event_name) }] }]),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    install_statusline_hook(&hook_paths).expect("statusline hook");
    write_test_file(
        &profile_path,
        &format!(
            "{}\n",
            json!({
                "hooks": Value::Object(hooks),
                "statusLine": {
                    "type": "command",
                    "command": statusline_command(&hook_paths, None),
                    "padding": 0,
                },
            })
        ),
    );
    let expected_paths = provider_hook_paths("claude", &hook_paths)
        .iter()
        .map(|path| path_string(path))
        .collect::<Vec<_>>();
    assert!(json_contains_hook_command(
        &read_json_object(&read_file_text(&profile_path)),
        &command
    ));
    let claude_paths = provider_hook_paths("claude", &hook_paths);
    let inspection = inspect_agent_hook_installation(&claude, &hook_paths, &claude_paths);
    assert!(is_notify_hook_current(
        &hook_paths,
        &read_file_text(&hook_paths.notify_hook_path)
    ));
    assert!(inspection.current_hook_installed);

    let status = read_agent_hook_status(
        &paths,
        json!({ "agentIds": ["claude"], "autoUpgradeInstalled": false })
            .as_object()
            .expect("params"),
    )
    .expect("status");
    let row = status
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| agents.first())
        .expect("claude row");
    assert_eq!(row.get("status"), Some(&json!("installed")));
    assert_eq!(row.get("hookInstalled"), Some(&json!(true)));
    assert_eq!(row.get("paths"), Some(&json!(expected_paths)));
    assert!(row
        .get("paths")
        .and_then(Value::as_array)
        .expect("paths")
        .contains(&json!(path_string(&profile_path))));
}

#[test]
fn hook_status_detects_stale_profile_only_provider_hook() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    write_test_executable(&temp.path().join(".local").join("bin").join("codex"));
    let profile_path = temp
        .path()
        .join(".codex-profiles")
        .join("work")
        .join("hooks.json");
    write_test_file(
        &profile_path,
        r#"{"ghostex":{"command":"legacy ~/.ghostexterm/agent-shell-notify.sh","agent":"codex"}}"#,
    );

    let status = read_agent_hook_status(
        &paths,
        json!({ "agentIds": ["codex"], "autoUpgradeInstalled": false })
            .as_object()
            .expect("params"),
    )
    .expect("status");
    let row = status
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| agents.first())
        .expect("codex row");
    assert_eq!(row.get("status"), Some(&json!("updateRequired")));
    assert_eq!(row.get("hookInstalled"), Some(&json!(false)));
    assert!(row
        .get("paths")
        .and_then(Value::as_array)
        .expect("paths")
        .contains(&json!(path_string(&profile_path))));
}

#[test]
fn hook_status_uses_pi_agent_extension_before_legacy_root_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::from_paths(&paths);
    seed_real_cli(&hook_paths.home_dir, "pi");
    write_test_executable(&temp.path().join(".local").join("bin").join("pi"));
    install_notify_hook(&hook_paths).expect("notify hook");
    let pi = HookDefinition {
        agent_id: "pi",
        cli_command: "pi",
    };
    let legacy_root_extension_path = temp
        .path()
        .join(".pi")
        .join("extensions")
        .join("ghostex-session.ts");
    let agent_extension_path = temp
        .path()
        .join(".pi")
        .join("agent")
        .join("extensions")
        .join("ghostex-session.ts");
    write_test_file(
        &agent_extension_path,
        &format!(
            "// {PI_EXTENSION_MARKER} v6\nconst hook = \"{}\";\n",
            path_string(&hook_paths.notify_hook_path)
        ),
    );
    write_test_file(
        &legacy_root_extension_path,
        &format!("// {PI_EXTENSION_MARKER} v6\n"),
    );

    let provider_paths = provider_hook_paths("pi", &hook_paths);
    let inspection = inspect_agent_hook_installation(&pi, &hook_paths, &provider_paths);
    assert!(inspection.current_hook_installed);
    assert_eq!(
        provider_paths.first().map(|path| path_string(path)),
        Some(path_string(&agent_extension_path))
    );
    assert!(provider_paths.contains(&legacy_root_extension_path));

    let status = read_agent_hook_status(
        &paths,
        json!({ "agentIds": ["pi"], "autoUpgradeInstalled": false })
            .as_object()
            .expect("params"),
    )
    .expect("status");
    let row = status
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| agents.first())
        .expect("pi row");
    assert_eq!(row.get("status"), Some(&json!("installed")));
    assert_eq!(row.get("hookInstalled"), Some(&json!(true)));
    assert!(row
        .get("paths")
        .and_then(Value::as_array)
        .expect("paths")
        .contains(&json!(path_string(&legacy_root_extension_path))));
}

#[test]
fn hook_status_reports_legacy_pi_agent_extension_as_update_required() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    seed_real_cli(&hook_paths.home_dir, "pi");
    write_test_executable(&temp.path().join(".local").join("bin").join("pi"));
    install_notify_hook(&hook_paths).expect("notify hook");
    let legacy_agent_extension_path = temp
        .path()
        .join(".pi")
        .join("agent")
        .join("extensions")
        .join("ghostex-session")
        .join("index.ts");
    write_test_file(
        &legacy_agent_extension_path,
        &format!("// {PI_EXTENSION_MARKER} v2\n"),
    );

    let status = read_agent_hook_status(
        &paths,
        json!({ "agentIds": ["pi"], "autoUpgradeInstalled": false })
            .as_object()
            .expect("params"),
    )
    .expect("status");
    let row = status
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| agents.first())
        .expect("pi row");
    assert_eq!(row.get("status"), Some(&json!("updateRequired")));
    assert_eq!(row.get("hookInstalled"), Some(&json!(false)));
}

#[test]
fn notify_hook_current_requires_marker_and_resolved_state_directory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let hook_paths = HookPaths::new(temp.path().to_path_buf());
    let hook_path = hook_paths.notify_hook_path.clone();
    write_test_file(
        &hook_path,
        &format!(
            "#!/bin/zsh\n# {NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}\nDEFAULT_HOOK_STATE_DIR={}\n",
            shell_quote(&path_string(&hook_paths.hook_state_directory))
        ),
    );
    assert!(is_notify_hook_current(
        &hook_paths,
        &read_file_text(&hook_path)
    ));

    write_test_file(
        &hook_path,
        &format!(
            "#!/bin/zsh\n# {NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}\nDEFAULT_HOOK_STATE_DIR='/stale/ghostex/state'\n"
        ),
    );
    assert!(!is_notify_hook_current(
        &hook_paths,
        &read_file_text(&hook_path)
    ));
}

#[test]
fn codex_stop_persists_attention_in_the_hook_sidecar() {
    assert_eq!(
        activity_for_hook_event("codex", "Stop", &json!({})),
        Some("attention".to_string())
    );
    assert_eq!(
        activity_for_hook_event("codex", "SessionEnd", &json!({})),
        Some("idle".to_string())
    );
}

#[test]
fn hook_session_sidecar_migration_keeps_the_newest_identity_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_directory = temp.path().join("legacy").join("agent-hooks");
    let destination_directory = temp.path().join("current").join("agent-hooks");
    let file_name = "codex-hook-sessions.json";
    write_test_file(
        &source_directory.join(file_name),
        &format!(
            "{}\n",
            json!({
                "sessions": {
                    "incoming": { "updatedAt": 20.0, "surfaceId": "surface-a" },
                    "shared": { "updatedAt": 10.0, "surfaceId": "legacy" }
                }
            })
        ),
    );
    write_test_file(
        &destination_directory.join(file_name),
        &format!(
            "{}\n",
            json!({
                "sessions": {
                    "shared": { "updatedAt": 30.0, "surfaceId": "current" }
                }
            })
        ),
    );

    let migrated = migrate_hook_session_sidecars(&source_directory, &destination_directory)
        .expect("migrate sidecars");
    assert_eq!(
        migrated,
        vec![path_string(&destination_directory.join(file_name))]
    );
    let result = read_json_object(&read_file_text(&destination_directory.join(file_name)));
    assert_eq!(
        result["sessions"]["incoming"]["surfaceId"],
        json!("surface-a")
    );
    assert_eq!(result["sessions"]["shared"]["surfaceId"], json!("current"));
}

#[test]
fn install_writes_notify_hook_without_payload_content() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let result = install_agent_hooks(
        &paths,
        json!({ "agentIds": ["qoder"] })
            .as_object()
            .expect("params"),
    )
    .expect("install");
    let installed = result
        .get("installedPaths")
        .and_then(Value::as_array)
        .expect("installed paths");
    // The shared notify hook and the Claude and Cursor statusline scripts.
    assert_eq!(installed.len(), 3);
    let hook_text = fs::read_to_string(installed[0].as_str().expect("path")).expect("hook");
    assert!(hook_text.contains(NOTIFY_HOOK_MARKER));
    assert!(!hook_text.contains("firstUserMessage"));
    assert!(!hook_text.contains("rawTitle"));
}

/// A `claude -p` that a program started from a Ghostex terminal spawned inherits the pane's
/// session ref, so its SessionStart named its own conversation as the session's (user report
/// 2026-10-30). The session's own interactive Claude is not headless and keeps reporting.
#[test]
fn a_headless_claude_hook_never_names_the_sessions_conversation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state_path = temp.path().join("session.state");
    let hook_store = temp.path().join("hook-store");
    let saved = [
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_PID",
        "CLAUDE_JOB_DIR",
        "CLAUDE_CODE_SESSION_KIND",
        "GHOSTEX_GXSERVER_BASE_URL",
        "GHOSTEX_GLOBAL_SESSION_REF",
    ]
    .map(|key| (key, std::env::var_os(key)));
    for (key, _) in &saved {
        std::env::remove_var(key);
    }
    std::env::set_var("CLAUDE_CODE_ENTRYPOINT", "sdk-cli");
    let answer = run_notify_hook(vec![
        path_string(&state_path),
        json!({
            "agent": "claude",
            "hook_event_name": "SessionStart",
            "source": "startup",
            "session_id": "0a1a3b9b-12c2-45cb-8eb9-398303ade53f",
            "transcript_path": "/tmp/0a1a3b9b-12c2-45cb-8eb9-398303ade53f.jsonl"
        })
        .to_string(),
        path_string(&hook_store),
    ]);
    for (key, value) in saved {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
    assert!(answer.expect("notify helper").is_none());
    assert!(
        !state_path.exists(),
        "a headless Claude's hook must not record a conversation for the session"
    );
    assert!(!hook_store.join("claude-hook-sessions.json").exists());
}

#[test]
fn notify_hook_helper_records_working_status_and_first_prompt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state_path = temp.path().join("session.state");
    let hook_store = temp.path().join("hook-store");
    assert!(
        run_notify_hook(vec![
            path_string(&state_path),
            json!({
                "agent": "codex",
                "event": "UserPromptSubmit",
                "hook_event_name": "UserPromptSubmit",
                "prompt": "Please fix flaky tests",
                "session_id": "codex-session-1"
            })
            .to_string(),
            path_string(&hook_store),
        ])
        .expect("notify helper")
        .is_none(),
        "a codex event never answers with coordinator context"
    );
    let state = read_hook_state(&state_path);
    assert_eq!(
        read_state_string(&state, "status").as_deref(),
        Some("working")
    );
    assert_eq!(read_state_string(&state, "agent").as_deref(), Some("codex"));
    assert_eq!(
        read_state_string(&state, "agentSessionId").as_deref(),
        Some("codex-session-1")
    );
    assert_eq!(
        decode_base64_text(
            read_state_string(&state, "firstUserMessageBase64")
                .as_deref()
                .expect("first prompt")
        ),
        "Please fix flaky tests"
    );
    assert_eq!(
        read_state_string(&state, "pendingFirstPromptAutoRenamePrompt").as_deref(),
        Some("Please fix flaky tests")
    );
}

#[test]
fn uninstall_agent_hooks_removes_notify_and_flat_json_without_autoupgrade() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    write_test_file(&hook_paths.notify_hook_path, "#!/bin/zsh\n# old notify\n");
    let cursor_path = temp.path().join(".cursor").join("hooks.json");
    write_test_file(
        &cursor_path,
        &format!(
            "{}\n",
            json!({
                "hooks": {
                    "beforeSubmitPrompt": [
                        { "command": format!("node {}/.ghostexterm/agent-shell-notify.sh", temp.path().display()) },
                        { "command": "user-managed-cursor-hook" }
                    ]
                },
                "version": 1
            })
        ),
    );

    let result = uninstall_agent_hooks(
        &paths,
        json!({ "agentIds": ["cursor"] })
            .as_object()
            .expect("params"),
    )
    .expect("uninstall");
    let removed = result
        .get("removedPaths")
        .and_then(Value::as_array)
        .expect("removed paths");
    assert!(removed.contains(&json!(path_string(&cursor_path))));
    assert!(removed.contains(&json!(path_string(&hook_paths.notify_hook_path))));
    assert!(result.get("autoUpgradedPaths").is_none());
    assert_eq!(result.get("type"), Some(&json!("agentHookStatus")));
    assert!(!hook_paths.notify_hook_path.exists());
    let cursor_text = fs::read_to_string(cursor_path).expect("cursor config");
    assert!(!cursor_text.contains("agent-shell-notify"));
    assert!(cursor_text.contains("user-managed-cursor-hook"));
}

#[test]
fn uninstall_marked_yaml_matches_typescript_missing_end_marker_behavior() {
    let temp = tempfile::tempdir().expect("tempdir");
    let rovodev = HookDefinition {
        agent_id: "rovodev",
        cli_command: "acli",
    };
    let yaml_path = temp.path().join("config.yml");
    let yaml_text = "user_before: true\n# ghostex hooks rovodev begin\nnotify: ~/.ghostex/hooks/agent-shell-notify.sh\nuser_after: true\n";
    write_test_file(&yaml_path, yaml_text);

    let removed =
        uninstall_marked_yaml_hook(&rovodev, vec![yaml_path.clone()]).expect("yaml uninstall");
    assert_eq!(removed, vec![path_string(&yaml_path)]);
    assert_eq!(
        fs::read_to_string(&yaml_path).expect("yaml text"),
        "user_before: true\n"
    );
}

#[test]
fn remove_json_hook_preserves_user_entries_for_supported_json_formats() {
    let temp = tempfile::tempdir().expect("tempdir");
    let notify_hook_path = temp
        .path()
        .join(".ghostex")
        .join("hooks")
        .join("agent-shell-notify.sh");
    let notify_hook = path_string(&notify_hook_path);

    let nested_path = temp.path().join("nested.json");
    write_test_file(
        &nested_path,
        &format!(
            "{}\n",
            json!({
                "hooks": {
                    "SessionStart": [
                        {
                            "matcher": "*",
                            "hooks": [
                                { "type": "command", "command": notify_hook },
                                { "type": "command", "command": "user-nested-hook" }
                            ]
                        },
                        {
                            "hooks": [
                                { "type": "command", "command": "legacy ~/.ghostexterm/agent-shell-notify.sh" }
                            ]
                        }
                    ]
                },
                "other": true
            })
        ),
    );
    let codex = HookDefinition {
        agent_id: "codex",
        cli_command: "codex",
    };
    assert!(remove_json_hook(&nested_path, &codex, &notify_hook).expect("nested remove"));
    let nested_text = fs::read_to_string(&nested_path).expect("nested text");
    assert!(!nested_text.contains("agent-shell-notify"));
    assert!(nested_text.contains("user-nested-hook"));

    let flat_path = temp.path().join("flat.json");
    let cursor = HookDefinition {
        agent_id: "cursor",
        cli_command: "cursor-agent",
    };
    let cursor_command = command_for_agent(&cursor, &notify_hook_path);
    write_test_file(
        &flat_path,
        &format!(
            "{}\n",
            json!({
                "hooks": {
                    "beforeSubmitPrompt": [
                        { "command": cursor_command },
                        { "command": "user-flat-hook" }
                    ],
                    "beforeShellExecution": [
                        { "command": "legacy ~/.ghostex/hooks/agent-shell-notify.sh" }
                    ]
                }
            })
        ),
    );
    assert!(remove_json_hook(&flat_path, &cursor, &cursor_command).expect("flat remove"));
    let flat_text = fs::read_to_string(&flat_path).expect("flat text");
    assert!(!flat_text.contains("agent-shell-notify"));
    assert!(flat_text.contains("user-flat-hook"));

    let kiro_path = temp.path().join("kiro.json");
    let kiro = HookDefinition {
        agent_id: "kiro",
        cli_command: "kiro-cli",
    };
    let kiro_command = command_for_agent(&kiro, &notify_hook_path);
    write_test_file(
        &kiro_path,
        &format!(
            "{}\n",
            json!({
                "hooks": {
                    "agentSpawn": [
                        { "command": kiro_command, "timeout_ms": 5000 },
                        { "command": "user-kiro-hook" }
                    ]
                },
                "name": "ghostex"
            })
        ),
    );
    assert!(remove_json_hook(&kiro_path, &kiro, &kiro_command).expect("kiro remove"));
    let kiro_text = fs::read_to_string(&kiro_path).expect("kiro text");
    assert!(!kiro_text.contains("agent-shell-notify"));
    assert!(kiro_text.contains("user-kiro-hook"));

    let antigravity_path = temp.path().join("antigravity.json");
    let antigravity = HookDefinition {
        agent_id: "antigravity",
        cli_command: "agy",
    };
    let antigravity_command = command_for_agent(&antigravity, &notify_hook_path);
    write_test_file(
        &antigravity_path,
        &format!(
            "{}\n",
            json!({
                "ghostex": {
                    "SessionStart": [
                        { "type": "command", "command": antigravity_command },
                        { "type": "command", "command": "user-antigravity-hook" }
                    ],
                    "PreToolUse": [
                        {
                            "matcher": "*",
                            "hooks": [
                                { "type": "command", "command": "legacy ~/.ghostex/hooks/agent-shell-notify.sh" },
                                { "type": "command", "command": "user-antigravity-feed-hook" }
                            ]
                        }
                    ]
                }
            })
        ),
    );
    assert!(
        remove_json_hook(&antigravity_path, &antigravity, &antigravity_command)
            .expect("antigravity remove")
    );
    let antigravity_text = fs::read_to_string(&antigravity_path).expect("antigravity text");
    assert!(!antigravity_text.contains("agent-shell-notify"));
    assert!(antigravity_text.contains("user-antigravity-hook"));
    assert!(antigravity_text.contains("user-antigravity-feed-hook"));
}

#[test]
fn uninstall_removes_plugin_yaml_and_opencode_ghostex_content_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let amp = HookDefinition {
        agent_id: "amp",
        cli_command: "amp",
    };
    let amp_path = temp.path().join("ghostex-session.ts");
    write_test_file(
        &amp_path,
        &format!("// {AMP_PLUGIN_MARKER} v3\nexport default {{}};\n"),
    );
    let removed_amp =
        uninstall_plugin_file_hook(&amp, vec![amp_path.clone()]).expect("amp uninstall");
    assert_eq!(removed_amp, vec![path_string(&amp_path)]);
    assert!(!amp_path.exists());

    let pi = HookDefinition {
        agent_id: "pi",
        cli_command: "pi",
    };
    let pi_path = temp.path().join("user-owned-ghostex-session.ts");
    write_test_file(&pi_path, "export default function userPlugin() {}\n");
    let removed_pi = uninstall_plugin_file_hook(&pi, vec![pi_path.clone()]).expect("pi uninstall");
    assert!(removed_pi.is_empty());
    assert!(pi_path.exists());

    let rovodev = HookDefinition {
        agent_id: "rovodev",
        cli_command: "acli",
    };
    let yaml_path = temp.path().join("config.yml");
    write_test_file(
        &yaml_path,
        "user_before: true\n# ghostex hooks rovodev begin\nnotify: ~/.ghostex/hooks/agent-shell-notify.sh\n# ghostex hooks rovodev end\nuser_after: true\n",
    );
    let removed_yaml =
        uninstall_marked_yaml_hook(&rovodev, vec![yaml_path.clone()]).expect("yaml uninstall");
    assert_eq!(removed_yaml, vec![path_string(&yaml_path)]);
    let yaml_text = fs::read_to_string(&yaml_path).expect("yaml text");
    assert!(!yaml_text.contains("ghostex hooks rovodev"));
    assert!(yaml_text.contains("user_before"));
    assert!(yaml_text.contains("user_after"));

    let opencode_config_path = temp.path().join("opencode.json");
    let opencode_plugin_path = temp.path().join("plugins").join("ghostex-session.js");
    write_test_file(
        &opencode_config_path,
        &format!(
            "{}\n",
            json!({
                "other": true,
                "plugin": [
                    "./plugins/other.js",
                    "./plugins/ghostex-session.js",
                    ["ghostex-session", { "enabled": true }],
                    "/tmp/plugins/ghostex-session.js",
                    "/tmp/ghostex-session.js",
                    "not-ghostex"
                ]
            })
        ),
    );
    write_test_file(
        &opencode_plugin_path,
        &format!("// {OPENCODE_PLUGIN_MARKER} v3\nexport default {{}};\n"),
    );
    let removed_opencode = remove_retired_opencode_plugin(temp.path()).expect("opencode uninstall");
    assert!(removed_opencode.contains(&path_string(&opencode_config_path)));
    assert!(removed_opencode.contains(&path_string(&opencode_plugin_path)));
    assert!(!opencode_plugin_path.exists());
    let opencode_config = serde_json::from_str::<Value>(
        &fs::read_to_string(opencode_config_path).expect("opencode config"),
    )
    .expect("opencode json");
    assert_eq!(
        opencode_config.get("plugin"),
        Some(&json!(["./plugins/other.js", "not-ghostex"]))
    );
}

#[test]
fn opencode_theme_is_named_only_when_no_theme_is_chosen() {
    let temp = tempfile::tempdir().expect("tempdir");

    // No cli.json yet: the theme file is written and named.
    let fresh = temp.path().join("fresh");
    install_ghostex_opencode_theme(&fresh).expect("fresh install");
    let theme = serde_json::from_str::<Value>(
        &fs::read_to_string(ghostex_theme_file(&fresh)).expect("theme file"),
    )
    .expect("theme json");
    assert_eq!(theme["base"]["background"]["base"], "transparent");
    let config = serde_json::from_str::<Value>(
        &fs::read_to_string(fresh.join("cli.json")).expect("fresh cli.json"),
    )
    .expect("fresh json");
    assert_eq!(config["theme"], json!({ "name": "ghostex" }));

    // A cli.json without a theme name keeps its other settings, comments and theme mode.
    let unnamed = temp.path().join("unnamed");
    write_test_file(
        &unnamed.join("cli.json"),
        "{\n  // keep this note\n  \"plugins\": [\"file:///plugins/other/\"],\n  \"theme\": { \"mode\": \"light\" },\n  \"keybinds\": { \"editor_open\": \"ctrl+g\" }\n}\n",
    );
    install_ghostex_opencode_theme(&unnamed).expect("unnamed install");
    let text = fs::read_to_string(unnamed.join("cli.json")).expect("unnamed cli.json");
    assert!(text.contains("// keep this note"));
    let config = crate::session_chat_opencode::read_config(&text).expect("unnamed json");
    assert_eq!(
        config["theme"],
        json!({ "mode": "light", "name": "ghostex" })
    );
    assert_eq!(config["plugins"], json!(["file:///plugins/other/"]));
    assert_eq!(config["keybinds"]["editor_open"], "ctrl+g");

    // A theme the user picked (Ctrl+P saves it as theme.name) is never replaced.
    let chosen = temp.path().join("chosen");
    let chosen_text = "{\n  \"theme\": { \"name\": \"github\", \"mode\": \"dark\" }\n}\n";
    write_test_file(&chosen.join("cli.json"), chosen_text);
    install_ghostex_opencode_theme(&chosen).expect("chosen install");
    assert_eq!(
        fs::read_to_string(chosen.join("cli.json")).expect("chosen cli.json"),
        chosen_text
    );
    assert!(ghostex_theme_file(&chosen).is_file());
}

#[test]
fn command_exists_uses_typescript_default_tool_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let command = format!("ghostex-test-cli-{}", std::process::id());
    write_test_executable(
        &temp
            .path()
            .join(".local")
            .join("share")
            .join("mise")
            .join("shims")
            .join(&command),
    );

    assert!(command_exists(&command, temp.path()));
}

#[test]
fn hermes_hooks_cover_every_profile_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::from_paths(&paths);
    let hermes = HookDefinition {
        agent_id: "hermes-agent",
        cli_command: "hermes",
    };
    let hermes_home = temp.path().join(".hermes");
    let root = hermes_home.join("config.yaml");
    let content = hermes_home.join("profiles/content/config.yaml");
    let dobby = hermes_home.join("profiles/dobby/config.yaml");
    write_test_file(&root, "model:\n  provider: openai-codex\n");
    write_test_file(&content, "model:\n  provider: openai-codex\n");
    fs::create_dir_all(hermes_home.join("profiles/no-config")).expect("profile without config");
    // A hand-copied, unmarked hook as the agent's own YAML writer leaves it:
    // the plain scalar folded onto a second line, beside the user's own hook.
    let shell_command = format!(
        "sh -c {}",
        shell_quote(&command_for_agent(&hermes, &hook_paths.notify_hook_path))
    );
    let (head, tail) = shell_command.split_at(shell_command.rfind(' ').expect("space"));
    write_test_file(
        &dobby,
        &format!(
            "model:\n  provider: openai-codex\nhooks:\n  on_session_start:\n    - command: {head} \n        {}\n      timeout: 5\n  pre_tool_call:\n    - command: my-audit-hook\n      timeout: 3\n",
            tail.trim_start()
        ),
    );

    let hermes_paths = provider_hook_paths("hermes-agent", &hook_paths);
    assert_eq!(
        hermes_paths,
        vec![root.clone(), content.clone(), dobby.clone()]
    );

    let notify_path = path_string(&hook_paths.notify_hook_path);
    let repaired = repair_installed_agent_hook_paths(&paths).expect("repair");
    for config in &hermes_paths {
        assert!(
            repaired.contains(&path_string(config)),
            "{config:?} repaired"
        );
        let text = read_file_text(config);
        assert!(text.contains("# ghostex hooks hermes-agent begin"));
        assert_eq!(
            text.matches(&notify_path).count(),
            10,
            "{config:?}: one hook per event"
        );
        let allowlist = read_json_object(&read_file_text(
            &config.with_file_name("shell-hooks-allowlist.json"),
        ));
        assert_eq!(allowlist["approvals"].as_array().map(Vec::len), Some(10));
    }
    assert!(read_file_text(&dobby).contains("my-audit-hook"));
    assert!(
        inspect_agent_hook_installation(&hermes, &hook_paths, &hermes_paths).current_hook_installed
    );
    assert!(repair_installed_agent_hook_paths(&paths)
        .expect("repair again")
        .is_empty());

    uninstall_agent_hooks(
        &paths,
        json!({ "agentIds": ["hermes-agent"] })
            .as_object()
            .expect("params"),
    )
    .expect("uninstall");
    for config in &hermes_paths {
        assert!(
            !read_file_text(config).contains(&notify_path),
            "{config:?} uninstalled"
        );
    }
    assert!(read_file_text(&dobby).contains("my-audit-hook"));
}

/// A real (not install-on-first-run) `command` in the test's own home, so hook status reads it
/// as installed whatever CLIs the machine running the tests has.
fn seed_real_cli(home: &Path, command: &str) {
    let path = home.join(".local/bin").join(command);
    write_test_file(&path, "#!/bin/sh\nexit 0\n");
    super::probe_cache::seed_command_path_for_test(command, home, Some(path_string(&path)));
}

fn write_test_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, text).expect("write test file");
}

fn write_test_executable(path: &Path) {
    write_test_file(path, "#!/bin/sh\nexit 0\n");
    set_executable_permissions(path).expect("chmod test executable");
}

#[test]
fn an_install_on_first_run_launcher_is_not_an_installed_cli() {
    use super::on_demand::{classify, Kind};
    use super::probing::{cli_presence_with, CliPresence};
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path();
    // Omarchy's ~/.local/bin/gemini: installs the agent with mise the first time it runs.
    let launcher = home.join(".local/bin/gemini");
    write_test_file(
        &launcher,
        "#!/bin/bash\nmise use -g npm:@google/gemini-cli >/dev/null 2>&1\nexec mise x -- gemini \"$@\"\n",
    );
    let real = home.join(".local/bin/claude");
    write_test_file(&real, &"\0binary".repeat(4096));
    // The probe rule with no mise install behind the launcher.
    let probe =
        |path: &str| (classify(Path::new(path), home) == Kind::Real).then(|| path.to_string());
    assert_eq!(
        cli_presence_with(Some(&launcher.to_string_lossy()), probe),
        CliPresence::InstallsOnDemand
    );
    assert_eq!(
        cli_presence_with(Some(&real.to_string_lossy()), probe),
        CliPresence::Installed
    );
    assert_eq!(cli_presence_with(None, probe), CliPresence::Missing);
    // Once mise has installed it, the launcher resolves to the real install: installed.
    assert_eq!(
        cli_presence_with(Some(&launcher.to_string_lossy()), |_| Some(
            real.to_string_lossy().into_owned()
        )),
        CliPresence::Installed
    );
}
