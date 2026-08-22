#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use gxserver::{
    agent_hooks::{read_agent_hook_status, repair_installed_agent_hook_paths},
    paths::get_gxserver_paths,
};
use serde_json::{json, Value};

fn write_json(path: &Path, value: Value) {
    fs::create_dir_all(path.parent().expect("parent directory")).expect("create parent");
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value).expect("serialize JSON")
        ),
    )
    .expect("write JSON");
}

fn write_text(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent directory")).expect("create parent");
    fs::write(path, text).expect("write text");
}

#[test]
fn repairs_installed_agent_hooks_after_storage_directory_migration() {
    for variable in [
        "CODEX_HOME",
        "OPENCODE_CONFIG_DIR",
        "GROK_HOME",
        "KIRO_HOME",
        "COPILOT_HOME",
        "HERMES_HOME",
        "CODEBUDDY_CONFIG_DIR",
        "QODER_CONFIG_DIR",
        "PI_CODING_AGENT_DIR",
        "PI_CONFIG_DIR",
    ] {
        std::env::remove_var(variable);
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().to_path_buf();
    let codex_cli = home.join(".local").join("bin").join("codex");
    write_text(&codex_cli, "#!/bin/sh\nexit 0\n");
    let mut codex_cli_permissions = fs::metadata(&codex_cli)
        .expect("Codex CLI metadata")
        .permissions();
    codex_cli_permissions.set_mode(0o755);
    fs::set_permissions(&codex_cli, codex_cli_permissions).expect("make Codex CLI executable");
    let legacy_notify = home
        .join(".ghostex")
        .join("hooks")
        .join("agent-shell-notify.sh");
    let legacy_notify_text = legacy_notify.to_string_lossy().to_string();

    let codex_config = home.join(".codex").join("hooks.json");
    write_json(
        &codex_config,
        json!({
            "hooks": {
                "PreToolUse": [{
                    "hooks": [{
                        "type": "command",
                        "command": legacy_notify_text
                    }]
                }],
                "UserPromptSubmit": [{
                    "hooks": [{
                        "type": "command",
                        "command": legacy_notify_text
                    }]
                }],
                "SessionEnd": [{
                    "hooks": [
                        {
                            "type": "command",
                            "command": legacy_notify_text
                        },
                        {
                            "type": "command",
                            "command": "user-session-end-hook"
                        }
                    ]
                }],
                "Notification": [{
                    "hooks": [{
                        "type": "command",
                        "command": legacy_notify_text
                    }]
                }]
            }
        }),
    );
    let user_only_codex_profile = home
        .join(".codex-profiles")
        .join("user-only")
        .join("hooks.json");
    write_json(
        &user_only_codex_profile,
        json!({
            "hooks": {
                "PreToolUse": [{
                    "hooks": [{ "type": "command", "command": "codex-user-only-hook" }]
                }]
            }
        }),
    );
    let user_only_codex_profile_before =
        fs::read_to_string(&user_only_codex_profile).expect("user-only Codex profile");

    let claude_config = home.join(".claude").join("settings.json");
    write_json(
        &claude_config,
        json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [
                        {
                            "type": "command",
                            "command": format!("GHOSTEX_AGENT='claude' '{legacy_notify_text}'")
                        },
                        { "type": "command", "command": "user-owned-hook" }
                    ]
                }],
                "UserPromptSubmit": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": format!("GHOSTEX_AGENT='claude' '{legacy_notify_text}'")
                    }]
                }]
            }
        }),
    );
    let user_only_claude_profile = home
        .join(".claude-profiles")
        .join("user-only")
        .join("settings.json");
    write_json(
        &user_only_claude_profile,
        json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [{ "type": "command", "command": "claude-user-only-hook" }]
                }]
            }
        }),
    );
    let user_only_claude_profile_before =
        fs::read_to_string(&user_only_claude_profile).expect("user-only Claude profile");
    let cursor_config = home.join(".cursor").join("hooks.json");
    write_json(
        &cursor_config,
        json!({
            "version": 1,
            "hooks": {
                "beforeSubmitPrompt": [{
                    "command": format!("GHOSTEX_AGENT='cursor' '{legacy_notify_text}'")
                }],
                "beforeShellExecution": [{
                    "command": format!("GHOSTEX_AGENT='cursor' '{legacy_notify_text}'")
                }]
            }
        }),
    );
    let rovodev_config = home.join(".rovodev").join("config.yml");
    write_text(
        &rovodev_config,
        &format!(
            "user_before: true\n# ghostex hooks rovodev begin\neventHooks:\n  events:\n    - name: on_complete\n      commands:\n        - command: \"{legacy_notify_text}\"\n# ghostex hooks rovodev end\nuser_after: true\n"
        ),
    );
    let user_only_pi_plugin = home
        .join(".pi")
        .join("extensions")
        .join("ghostex-session.ts");
    write_text(
        &user_only_pi_plugin,
        "export default function userOwnedPlugin() {}\n",
    );
    let user_only_pi_plugin_before =
        fs::read_to_string(&user_only_pi_plugin).expect("user-only Pi plugin");
    let legacy_pi_plugin = home
        .join(".pi")
        .join("agent")
        .join("extensions")
        .join("ghostex-session")
        .join("index.ts");
    write_text(
        &legacy_pi_plugin,
        &format!(
            "// ghostex-pi-session-extension-marker v2\nconst notify = {legacy_notify_text:?};\n"
        ),
    );

    let mut paths = get_gxserver_paths(Some(home.clone()));
    paths.app_data_dir = home.join(".config").join("ghostex");
    paths.app_state_dir = home.join(".local").join("state").join("ghostex");
    let current_notify = paths
        .app_data_dir
        .join("hooks")
        .join("agent-shell-notify.sh");
    let current_notify_text = current_notify.to_string_lossy().to_string();
    let unquoted_current_codex_profile = home
        .join(".codex-profiles")
        .join("unquoted-current")
        .join("hooks.json");
    write_json(
        &unquoted_current_codex_profile,
        json!({
            "hooks": {
                "PreToolUse": [{
                    "hooks": [
                        { "type": "command", "command": current_notify_text },
                        { "type": "command", "command": "codex-profile-user-hook" }
                    ]
                }]
            }
        }),
    );

    let repaired = repair_installed_agent_hook_paths(&paths).expect("repair installed hooks");
    assert!(repaired.contains(&current_notify_text));
    assert!(fs::read_to_string(&current_notify)
        .expect("notify hook")
        .contains("ghostex-gxserver-agent-notify-hook-marker v7"));
    assert_ne!(
        fs::metadata(&current_notify)
            .expect("notify hook metadata")
            .permissions()
            .mode()
            & 0o111,
        0,
        "the provider command target must remain executable"
    );
    let hook_output = Command::new(&current_notify)
        .env("GHOSTEX_INTERNAL_PROMPT_GENERATION", "1")
        .output()
        .expect("execute repaired notify hook");
    assert!(hook_output.status.success());
    assert_eq!(hook_output.stdout, br#"{"continue":true}"#);

    for config_path in [&codex_config, &claude_config, &cursor_config] {
        let text = fs::read_to_string(config_path).expect("provider config");
        assert!(text.contains(&current_notify_text));
        assert!(!text.contains(&legacy_notify_text));
    }
    assert!(fs::read_to_string(&claude_config)
        .expect("claude config")
        .contains("user-owned-hook"));
    let repaired_codex = fs::read_to_string(&codex_config).expect("codex config");
    assert!(repaired_codex.contains("user-session-end-hook"));
    let repaired_codex_json =
        serde_json::from_str::<Value>(&repaired_codex).expect("parse repaired codex config");
    assert!(!repaired_codex_json["hooks"]["SessionEnd"]
        .to_string()
        .contains(&current_notify_text));
    assert!(repaired_codex_json["hooks"].get("Notification").is_none());
    assert_eq!(
        fs::read_to_string(&user_only_codex_profile).expect("user-only Codex profile after repair"),
        user_only_codex_profile_before
    );
    let repaired_unquoted_profile =
        fs::read_to_string(&unquoted_current_codex_profile).expect("repaired Codex profile");
    let repaired_unquoted_profile_json = serde_json::from_str::<Value>(&repaired_unquoted_profile)
        .expect("parse repaired Codex profile");
    let repaired_profile_commands = repaired_unquoted_profile_json["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse groups")
        .iter()
        .flat_map(|group| {
            group["hooks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|hook| hook["command"].as_str())
        })
        .collect::<Vec<_>>();
    assert!(repaired_profile_commands.contains(&"codex-profile-user-hook"));
    assert!(repaired_profile_commands.contains(&format!("'{current_notify_text}'").as_str()));
    assert!(!repaired_profile_commands.contains(&current_notify_text.as_str()));
    assert!(repaired.contains(&unquoted_current_codex_profile.to_string_lossy().to_string()));
    let status = read_agent_hook_status(
        &paths,
        json!({ "agentIds": ["codex"] })
            .as_object()
            .expect("status params"),
    )
    .expect("read repaired hook status");
    let codex_status = status["agents"]
        .as_array()
        .and_then(|agents| agents.first())
        .expect("Codex hook status");
    assert_eq!(codex_status["status"], json!("installed"));
    assert_eq!(
        fs::read_to_string(&user_only_codex_profile)
            .expect("user-only Codex profile after status refresh"),
        user_only_codex_profile_before
    );
    assert_eq!(
        fs::read_to_string(&user_only_claude_profile)
            .expect("user-only Claude profile after repair"),
        user_only_claude_profile_before
    );
    assert!(!repaired.contains(&user_only_codex_profile.to_string_lossy().to_string()));
    assert!(!repaired.contains(&user_only_claude_profile.to_string_lossy().to_string()));
    let rovodev_text = fs::read_to_string(&rovodev_config).expect("Rovo Dev config");
    assert!(rovodev_text.contains(&current_notify_text));
    assert!(!rovodev_text.contains(&legacy_notify_text));
    assert!(rovodev_text.contains("user_before: true"));
    assert!(rovodev_text.contains("user_after: true"));
    let legacy_pi_text = fs::read_to_string(&legacy_pi_plugin).expect("legacy Pi plugin");
    assert!(legacy_pi_text.contains("ghostex-pi-session-extension-marker v3"));
    assert!(legacy_pi_text.contains(&current_notify_text));
    assert!(!legacy_pi_text.contains(&legacy_notify_text));
    assert_eq!(
        fs::read_to_string(&user_only_pi_plugin).expect("user-only Pi plugin after repair"),
        user_only_pi_plugin_before
    );
    assert!(repaired.contains(&legacy_pi_plugin.to_string_lossy().to_string()));
    assert!(!repaired.contains(&user_only_pi_plugin.to_string_lossy().to_string()));
    assert!(repair_installed_agent_hook_paths(&paths)
        .expect("idempotent repair")
        .is_empty());
}
