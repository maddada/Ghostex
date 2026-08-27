use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::paths::get_gxserver_paths;

use super::api::{install_agent_hooks, read_agent_hook_status, uninstall_agent_hooks};
use super::config::{
    all_hook_events, HookDefinition, HookPaths, AMP_PLUGIN_MARKER, NOTIFY_HOOK_MARKER,
    NOTIFY_HOOK_VERSION, OPENCODE_PLUGIN_MARKER, PI_EXTENSION_MARKER,
};
use super::event_mapping::activity_for_hook_event;
use super::install::{
    inspect_agent_hook_installation, install_notify_hook, is_notify_hook_current,
    json_contains_hook_command, migrate_hook_session_sidecars, read_json_object, remove_json_hook,
    set_executable_permissions, uninstall_marked_yaml_hook, uninstall_opencode_hook_paths,
    uninstall_plugin_file_hook,
};
use super::notify_runtime::{read_hook_state, read_state_string, run_notify_hook};
use super::plugin_sources::{command_for_agent, shell_quote};
use super::probing::{command_exists, decode_base64_text, path_string, read_file_text};
use super::resolution::provider_hook_paths;

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
    let hook_paths = HookPaths::new(paths.home_dir.clone());
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
                json!([{ "hooks": [{ "type": "command", "command": command }] }]),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    write_test_file(
        &profile_path,
        &format!("{}\n", json!({ "hooks": Value::Object(hooks) })),
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
fn hook_status_uses_pi_root_extension_before_legacy_agent_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    write_test_executable(&temp.path().join(".local").join("bin").join("pi"));
    install_notify_hook(&hook_paths).expect("notify hook");
    let pi = HookDefinition {
        agent_id: "pi",
        cli_command: "pi",
    };
    let root_extension_path = temp
        .path()
        .join(".pi")
        .join("extensions")
        .join("ghostex-session.ts");
    let legacy_agent_extension_path = temp
        .path()
        .join(".pi")
        .join("agent")
        .join("extensions")
        .join("ghostex-session")
        .join("index.ts");
    write_test_file(
        &root_extension_path,
        &format!(
            "// {PI_EXTENSION_MARKER} v4\nconst hook = \"{}\";\n",
            path_string(&hook_paths.notify_hook_path)
        ),
    );
    write_test_file(
        &legacy_agent_extension_path,
        &format!("// {PI_EXTENSION_MARKER} v2\n"),
    );

    let provider_paths = provider_hook_paths("pi", &hook_paths);
    let inspection = inspect_agent_hook_installation(&pi, &hook_paths, &provider_paths);
    assert!(inspection.current_hook_installed);
    assert_eq!(
        provider_paths.first().map(|path| path_string(path)),
        Some(path_string(&root_extension_path))
    );
    assert!(provider_paths.contains(&legacy_agent_extension_path));

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
        .contains(&json!(path_string(&legacy_agent_extension_path))));
}

#[test]
fn hook_status_reports_legacy_pi_agent_extension_as_update_required() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let hook_paths = HookPaths::new(paths.home_dir.clone());
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
    assert_eq!(installed.len(), 1);
    let hook_text = fs::read_to_string(installed[0].as_str().expect("path")).expect("hook");
    assert!(hook_text.contains(NOTIFY_HOOK_MARKER));
    assert!(!hook_text.contains("firstUserMessage"));
    assert!(!hook_text.contains("rawTitle"));
}

#[test]
fn notify_hook_helper_records_working_status_and_first_prompt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state_path = temp.path().join("session.state");
    let hook_store = temp.path().join("hook-store");
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
    .expect("notify helper");
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
    let removed_opencode =
        uninstall_opencode_hook_paths(&opencode_plugin_path, &opencode_config_path)
            .expect("opencode uninstall");
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
