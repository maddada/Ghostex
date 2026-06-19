use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};

use crate::{domain::DomainStateError, paths::GxserverPaths};

const NOTIFY_HOOK_MARKER: &str = "ghostex-gxserver-agent-notify-hook-marker";
const NOTIFY_HOOK_VERSION: usize = 6;
const OPENCODE_PLUGIN_MARKER: &str = "ghostex-opencode-session-plugin-marker";
const OPENCODE_PLUGIN_SPEC: &str = "./plugins/ghostex-session.js";
const AMP_PLUGIN_MARKER: &str = "ghostex-amp-session-extension-marker";
const PI_EXTENSION_MARKER: &str = "ghostex-pi-session-extension-marker";
const OMP_EXTENSION_MARKER: &str = "ghostex-omp-session-extension-marker";

struct HookDefinition {
    agent_id: &'static str,
    cli_command: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookFormat {
    Antigravity,
    FlatJson,
    KiroJson,
    MarkedYaml,
    NestedJson,
    Opencode,
    PluginFile,
}

const HOOK_DEFINITIONS: &[HookDefinition] = &[
    HookDefinition {
        agent_id: "codex",
        cli_command: "codex",
    },
    HookDefinition {
        agent_id: "claude",
        cli_command: "claude",
    },
    HookDefinition {
        agent_id: "cursor",
        cli_command: "cursor-agent",
    },
    HookDefinition {
        agent_id: "gemini",
        cli_command: "gemini",
    },
    HookDefinition {
        agent_id: "kiro",
        cli_command: "kiro-cli",
    },
    HookDefinition {
        agent_id: "copilot",
        cli_command: "copilot",
    },
    HookDefinition {
        agent_id: "droid",
        cli_command: "droid",
    },
    HookDefinition {
        agent_id: "grok",
        cli_command: "grok",
    },
    HookDefinition {
        agent_id: "antigravity",
        cli_command: "agy",
    },
    HookDefinition {
        agent_id: "amp",
        cli_command: "amp",
    },
    HookDefinition {
        agent_id: "omp",
        cli_command: "omp",
    },
    HookDefinition {
        agent_id: "pi",
        cli_command: "pi",
    },
    HookDefinition {
        agent_id: "rovodev",
        cli_command: "acli",
    },
    HookDefinition {
        agent_id: "hermes-agent",
        cli_command: "hermes",
    },
    HookDefinition {
        agent_id: "codebuddy",
        cli_command: "codebuddy",
    },
    HookDefinition {
        agent_id: "qoder",
        cli_command: "qodercli",
    },
    HookDefinition {
        agent_id: "opencode",
        cli_command: "opencode",
    },
];

/*
CDXC:AgentHooks 2026-06-16-10:00:
Rust Phase 6 exposes the same local-only hook status and install RPCs without putting raw hook payloads, terminal titles, paths, or command output into persistent logs. Status reports deterministic metadata, while explicit install writes only Ghostex-owned hook artifacts under the selected HOME.
*/
pub fn read_agent_hook_status(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let auto_upgrade = params.get("autoUpgradeInstalled").and_then(Value::as_bool) != Some(false);
    let mut auto_upgraded_paths = Vec::new();
    let mut rows = Vec::new();
    for agent_id in agent_ids {
        if let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        {
            let mut row = read_hook_status(definition, &hook_paths)?;
            if auto_upgrade
                && row.get("status").and_then(Value::as_str) == Some("updateRequired")
                && row.get("cliInstalled").and_then(Value::as_bool) == Some(true)
            {
                install_notify_hook(&hook_paths)?;
                auto_upgraded_paths.push(path_string(&hook_paths.notify_hook_path));
                row = read_hook_status(definition, &hook_paths)?;
            }
            rows.push(row);
        }
    }
    let mut result = Map::new();
    result.insert("agents".to_string(), Value::Array(rows));
    if !auto_upgraded_paths.is_empty() {
        result.insert("autoUpgradedPaths".to_string(), json!(auto_upgraded_paths));
    }
    result.insert("generatedAt".to_string(), json!(now_iso()));
    result.insert(
        "hookStateDirectory".to_string(),
        json!(path_string(&hook_paths.hook_state_directory)),
    );
    result.insert(
        "notifyHookPath".to_string(),
        json!(path_string(&hook_paths.notify_hook_path)),
    );
    result.insert("type".to_string(), json!("agentHookStatus"));
    Ok(Value::Object(result))
}

pub fn install_agent_hooks(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let mut installed_paths = Vec::new();
    install_notify_hook(&hook_paths)?;
    installed_paths.push(path_string(&hook_paths.notify_hook_path));
    for agent_id in agent_ids {
        let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        else {
            continue;
        };
        if !command_exists(definition.cli_command, &hook_paths.home_dir) {
            continue;
        }
        if let Some(path) = provider_hook_path(definition.agent_id, &hook_paths) {
            write_provider_hook(definition, &hook_paths, &path)?;
            installed_paths.push(path_string(&path));
        }
    }
    let mut status = read_agent_hook_status(paths, params)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installedPaths".to_string(), json!(installed_paths));
    Ok(Value::Object(status))
}

pub fn uninstall_agent_hooks(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::new(paths.home_dir.clone());
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let mut removed_paths = Vec::new();
    /*
    CDXC:AgentHooks 2026-06-19-14:15:
    Advanced Settings uninstall must remove only Ghostex-owned hook commands, marked YAML blocks, plugin registrations, and Ghostex extension files while leaving user-managed provider hooks intact. The shared notify hook is removed after provider cleanup and status is reread with auto-upgrade disabled so uninstall never recreates hooks it just removed.
    */
    for agent_id in &agent_ids {
        let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        else {
            continue;
        };
        for removed_path in uninstall_agent_hook(definition, &hook_paths)? {
            push_unique_path(&mut removed_paths, removed_path);
        }
    }
    match fs::remove_file(&hook_paths.notify_hook_path) {
        Ok(()) => push_unique_path(
            &mut removed_paths,
            path_string(&hook_paths.notify_hook_path),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    let mut status_params = params.clone();
    status_params.insert("agentIds".to_string(), json!(agent_ids));
    status_params.insert("autoUpgradeInstalled".to_string(), json!(false));
    let mut status = read_agent_hook_status(paths, &status_params)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("removedPaths".to_string(), json!(removed_paths));
    Ok(Value::Object(status))
}

struct HookPaths {
    home_dir: PathBuf,
    hook_state_directory: PathBuf,
    notify_hook_path: PathBuf,
}

impl HookPaths {
    fn new(home_dir: PathBuf) -> Self {
        Self {
            hook_state_directory: home_dir.join(".ghostexterm"),
            notify_hook_path: home_dir
                .join(".ghostex")
                .join("hooks")
                .join("agent-shell-notify.sh"),
            home_dir,
        }
    }
}

fn read_hook_status(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Value, DomainStateError> {
    let cli_installed = command_exists(definition.cli_command, &hook_paths.home_dir);
    let paths = provider_hook_path(definition.agent_id, hook_paths)
        .map(|path| vec![path_string(&path)])
        .unwrap_or_default();
    let notify_current = is_notify_hook_current(&hook_paths.notify_hook_path);
    let provider_current = provider_hook_path(definition.agent_id, hook_paths)
        .map(|path| provider_hook_current(&path, &hook_paths.notify_hook_path))
        .unwrap_or(false);
    let ghostex_hook_present = provider_hook_path(definition.agent_id, hook_paths)
        .map(|path| read_file_text(&path).contains("ghostex"))
        .unwrap_or(false)
        || read_file_text(&hook_paths.notify_hook_path).contains(NOTIFY_HOOK_MARKER);
    let hook_installed = notify_current && provider_current;
    let status = if !cli_installed {
        "cliMissing"
    } else if hook_installed {
        "installed"
    } else if ghostex_hook_present {
        "updateRequired"
    } else {
        "missing"
    };
    Ok(json!({
        "agentId": definition.agent_id,
        "cliCommand": definition.cli_command,
        "cliInstalled": cli_installed,
        "detail": hook_detail(definition, hook_paths, status, paths.first().map(String::as_str)),
        "hookInstalled": hook_installed,
        "paths": paths,
        "status": status,
    }))
}

fn hook_detail(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    status: &str,
    first_path: Option<&str>,
) -> String {
    let display = display_path(
        first_path.unwrap_or_else(|| {
            hook_paths
                .notify_hook_path
                .to_str()
                .unwrap_or("~/.ghostex/hooks/agent-shell-notify.sh")
        }),
        &hook_paths.home_dir,
    );
    match status {
        "cliMissing" => format!("{} was not found on PATH.", definition.cli_command),
        "installed" => format!("Installed in {display}"),
        "updateRequired" => format!("Run Update Hooks to update {display}"),
        _ => format!("Run Install Hooks to write {display}"),
    }
}

fn provider_hook_path(agent_id: &str, hook_paths: &HookPaths) -> Option<PathBuf> {
    provider_hook_paths(agent_id, hook_paths).into_iter().next()
}

fn provider_hook_paths(agent_id: &str, hook_paths: &HookPaths) -> Vec<PathBuf> {
    match agent_id {
        "codex" => {
            let mut paths =
                vec![
                    resolve_config_directory(&hook_paths.home_dir, "CODEX_HOME", ".codex", None)
                        .join("hooks.json"),
                ];
            paths.extend(list_profile_hook_paths(
                &hook_paths.home_dir,
                ".codex-profiles",
                "hooks.json",
            ));
            paths
        }
        "claude" => {
            let mut paths = vec![hook_paths.home_dir.join(".claude").join("settings.json")];
            paths.extend(list_profile_hook_paths(
                &hook_paths.home_dir,
                ".claude-profiles",
                "settings.json",
            ));
            paths
        }
        "cursor" => vec![hook_paths.home_dir.join(".cursor").join("hooks.json")],
        "gemini" => vec![hook_paths.home_dir.join(".gemini").join("settings.json")],
        "opencode" => {
            let config_dir = resolve_config_directory(
                &hook_paths.home_dir,
                "OPENCODE_CONFIG_DIR",
                ".config/opencode",
                None,
            );
            vec![
                config_dir.join("plugins").join("ghostex-session.js"),
                config_dir.join("opencode.json"),
            ]
        }
        "amp" => vec![hook_paths
            .home_dir
            .join(".config")
            .join("amp")
            .join("plugins")
            .join("ghostex-session.ts")],
        "pi" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            "PI_CODING_AGENT_DIR",
            ".pi/agent",
            None,
        )
        .join("extensions")
        .join("ghostex-session.ts")],
        "omp" => vec![resolve_omp_agent_directory(&hook_paths.home_dir)
            .join("extensions")
            .join("ghostex-omp-session.ts")],
        "grok" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "GROK_HOME", ".grok/hooks", None)
                    .join("ghostex-session.json"),
            ]
        }
        "antigravity" => vec![hook_paths
            .home_dir
            .join(".gemini")
            .join("config")
            .join("hooks.json")],
        "kiro" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            "KIRO_HOME",
            ".kiro/agents",
            Some("agents"),
        )
        .join("ghostex.json")],
        "copilot" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "COPILOT_HOME", ".copilot", None)
                    .join("config.json"),
            ]
        }
        "droid" => vec![hook_paths.home_dir.join(".factory").join("settings.json")],
        "rovodev" => vec![hook_paths.home_dir.join(".rovodev").join("config.yml")],
        "hermes-agent" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "HERMES_HOME", ".hermes", None)
                    .join("config.yaml"),
            ]
        }
        "codebuddy" => vec![resolve_config_directory(
            &hook_paths.home_dir,
            "CODEBUDDY_CONFIG_DIR",
            ".codebuddy",
            None,
        )
        .join("settings.json")],
        "qoder" => {
            vec![
                resolve_config_directory(&hook_paths.home_dir, "QODER_CONFIG_DIR", ".qoder", None)
                    .join("settings.json"),
            ]
        }
        _ => Vec::new(),
    }
}

fn uninstall_agent_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Vec<String>, DomainStateError> {
    if hook_format(definition.agent_id) == HookFormat::Opencode {
        return uninstall_opencode_hook(hook_paths);
    }
    let config_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let command = command_for_agent(definition, &hook_paths.notify_hook_path);
    match hook_format(definition.agent_id) {
        HookFormat::PluginFile => uninstall_plugin_file_hook(definition, config_paths),
        HookFormat::MarkedYaml => uninstall_marked_yaml_hook(definition, config_paths),
        HookFormat::Antigravity
        | HookFormat::FlatJson
        | HookFormat::KiroJson
        | HookFormat::NestedJson => {
            let mut removed_paths = Vec::new();
            for config_path in config_paths {
                if remove_json_hook(&config_path, definition, &command)? {
                    removed_paths.push(path_string(&config_path));
                }
            }
            Ok(removed_paths)
        }
        HookFormat::Opencode => Ok(Vec::new()),
    }
}

fn uninstall_plugin_file_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_path) = config_paths.into_iter().next() else {
        return Ok(Vec::new());
    };
    let text = read_file_text(&config_path);
    if !text_contains_ghostex_owned_hook_command(&text)
        && hook_marker(definition.agent_id)
            .map(|marker| !text.contains(marker))
            .unwrap_or(true)
    {
        return Ok(Vec::new());
    }
    remove_file_if_exists(&config_path).map(|removed| {
        if removed {
            vec![path_string(&config_path)]
        } else {
            Vec::new()
        }
    })
}

fn uninstall_marked_yaml_hook(
    definition: &HookDefinition,
    config_paths: Vec<PathBuf>,
) -> Result<Vec<String>, DomainStateError> {
    let Some(config_path) = config_paths.into_iter().next() else {
        return Ok(Vec::new());
    };
    let begin_marker = format!("# ghostex hooks {} begin", definition.agent_id);
    let end_marker = format!("# ghostex hooks {} end", definition.agent_id);
    let current_text = read_file_text(&config_path);
    if !current_text.lines().any(|line| line.trim() == begin_marker) {
        return Ok(Vec::new());
    }
    let normalized_text = current_text.replace("\r\n", "\n");
    let lines = normalized_text
        .split('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let next_lines = without_marked_block(&lines, &begin_marker, &end_marker);
    let next_text = format!("{}\n", next_lines.join("\n").trim_end_matches('\n'));
    if current_text == next_text {
        return Ok(Vec::new());
    }
    fs::write(&config_path, next_text).map_err(io_error)?;
    Ok(vec![path_string(&config_path)])
}

fn uninstall_opencode_hook(hook_paths: &HookPaths) -> Result<Vec<String>, DomainStateError> {
    let paths = provider_hook_paths("opencode", hook_paths);
    let Some(plugin_path) = paths.first() else {
        return Ok(Vec::new());
    };
    let Some(config_path) = paths.get(1) else {
        return Ok(Vec::new());
    };
    uninstall_opencode_hook_paths(plugin_path, config_path)
}

fn uninstall_opencode_hook_paths(
    plugin_path: &Path,
    config_path: &Path,
) -> Result<Vec<String>, DomainStateError> {
    let mut removed_paths = Vec::new();
    let config_text = read_file_text(config_path);
    if !config_text.trim().is_empty() {
        let mut data = read_json_object(&config_text);
        if let Some(object) = data.as_object_mut() {
            if let Some(plugins) = object.get_mut("plugin").and_then(Value::as_array_mut) {
                let next_plugins = plugins
                    .iter()
                    .filter(|plugin| !is_opencode_session_plugin_registration(plugin))
                    .cloned()
                    .collect::<Vec<_>>();
                if next_plugins.len() != plugins.len() {
                    *plugins = next_plugins;
                    write_json_file(config_path, &data)?;
                    push_unique_path(&mut removed_paths, path_string(config_path));
                }
            }
        }
    }
    let plugin_text = read_file_text(plugin_path);
    if plugin_text.contains(OPENCODE_PLUGIN_MARKER)
        || text_contains_ghostex_owned_hook_command(&plugin_text)
    {
        if remove_file_if_exists(plugin_path)? {
            push_unique_path(&mut removed_paths, path_string(plugin_path));
        }
    }
    Ok(removed_paths)
}

fn remove_json_hook(
    config_path: &Path,
    definition: &HookDefinition,
    command: &str,
) -> Result<bool, DomainStateError> {
    let current_text = read_file_text(config_path);
    if current_text.trim().is_empty() {
        return Ok(false);
    }
    let mut data = read_json_object(&current_text);
    let events = all_hook_events(definition.agent_id);
    let mut changed = false;
    if hook_format(definition.agent_id) == HookFormat::Antigravity {
        if let Some(ghostex) = data.get_mut("ghostex").and_then(Value::as_object_mut) {
            for event_name in events {
                let Some(entries) = ghostex.get_mut(event_name).and_then(Value::as_array_mut)
                else {
                    continue;
                };
                let next_entries = remove_antigravity_entries(entries, command);
                changed = changed || next_entries != *entries;
                *entries = next_entries;
            }
        }
    } else if matches!(
        hook_format(definition.agent_id),
        HookFormat::FlatJson | HookFormat::KiroJson
    ) {
        if let Some(hooks) = data.get_mut("hooks").and_then(Value::as_object_mut) {
            for event_name in events {
                let Some(entries) = hooks.get_mut(event_name).and_then(Value::as_array_mut) else {
                    continue;
                };
                let next_entries = entries
                    .iter()
                    .filter(|entry| !is_ghostex_owned_hook_command(entry, command))
                    .cloned()
                    .collect::<Vec<_>>();
                changed = changed || next_entries.len() != entries.len();
                *entries = next_entries;
            }
        }
    } else if let Some(hooks) = data.get_mut("hooks").and_then(Value::as_object_mut) {
        for event_name in events {
            let Some(groups) = hooks.get_mut(event_name).and_then(Value::as_array_mut) else {
                continue;
            };
            let next_groups = remove_nested_hook_groups(groups, command);
            changed = changed || next_groups != *groups;
            *groups = next_groups;
        }
    }
    if !changed {
        return Ok(false);
    }
    write_json_file(config_path, &data)?;
    Ok(true)
}

fn remove_antigravity_entries(entries: &[Value], command: &str) -> Vec<Value> {
    let mut next_entries = Vec::new();
    for entry in entries {
        if is_ghostex_owned_hook_command(entry, command) {
            continue;
        }
        let Some(object) = entry.as_object() else {
            next_entries.push(entry.clone());
            continue;
        };
        let Some(hooks) = object.get("hooks").and_then(Value::as_array) else {
            next_entries.push(entry.clone());
            continue;
        };
        let next_hooks = hooks
            .iter()
            .filter(|hook| !is_ghostex_owned_hook_command(hook, command))
            .cloned()
            .collect::<Vec<_>>();
        if !next_hooks.is_empty() {
            let mut next_entry = object.clone();
            next_entry.insert("hooks".to_string(), Value::Array(next_hooks));
            next_entries.push(Value::Object(next_entry));
        }
    }
    next_entries
}

fn remove_nested_hook_groups(groups: &[Value], command: &str) -> Vec<Value> {
    let mut next_groups = Vec::new();
    for group in groups {
        let Some(object) = group.as_object() else {
            next_groups.push(group.clone());
            continue;
        };
        let Some(hooks) = object.get("hooks").and_then(Value::as_array) else {
            next_groups.push(group.clone());
            continue;
        };
        let next_hooks = hooks
            .iter()
            .filter(|hook| !is_ghostex_owned_hook_command(hook, command))
            .cloned()
            .collect::<Vec<_>>();
        if !next_hooks.is_empty() {
            let mut next_group = object.clone();
            next_group.insert("hooks".to_string(), Value::Array(next_hooks));
            next_groups.push(Value::Object(next_group));
        }
    }
    next_groups
}

fn without_marked_block(lines: &[String], begin_marker: &str, end_marker: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != begin_marker {
            result.push(lines[index].clone());
            index += 1;
            continue;
        }
        while index < lines.len() && lines[index].trim() != end_marker {
            index += 1;
        }
        if index < lines.len() {
            index += 1;
        }
    }
    while result.last().map(|line| line.trim().is_empty()) == Some(true) {
        result.pop();
    }
    result
}

fn read_json_object(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({});
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => json!({}),
    }
}

fn write_json_file(path: &Path, data: &Value) -> Result<(), DomainStateError> {
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(data).map_err(json_error)?
    );
    fs::write(path, text).map_err(io_error)
}

fn remove_file_if_exists(path: &Path) -> Result<bool, DomainStateError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn write_provider_hook(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    path: &Path,
) -> Result<(), DomainStateError> {
    let parent = path.parent().ok_or_else(|| {
        DomainStateError::bad_request("Agent hook path must have a parent directory.")
    })?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let source = if matches!(definition.agent_id, "amp" | "opencode" | "pi" | "omp") {
        format!(
            "// ghostex-{}-session-extension-marker\n// {}\n",
            definition.agent_id,
            path_string(&hook_paths.notify_hook_path)
        )
    } else if matches!(definition.agent_id, "rovodev" | "hermes-agent") {
        format!(
            "# ghostex hooks {} begin\nnotify: {}\n# ghostex hooks {} end\n",
            definition.agent_id,
            path_string(&hook_paths.notify_hook_path),
            definition.agent_id
        )
    } else {
        json!({
            "ghostex": {
                "command": path_string(&hook_paths.notify_hook_path),
                "agent": definition.agent_id,
            }
        })
        .to_string()
    };
    fs::write(path, source).map_err(io_error)
}

fn provider_hook_current(path: &Path, notify_hook_path: &Path) -> bool {
    let text = read_file_text(path);
    !text.is_empty() && text.contains(&path_string(notify_hook_path))
}

fn install_notify_hook(hook_paths: &HookPaths) -> Result<(), DomainStateError> {
    if let Some(parent) = hook_paths.notify_hook_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::create_dir_all(&hook_paths.hook_state_directory).map_err(io_error)?;
    let script = format!(
        "#!/bin/zsh\n# {NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}\n# Sends agent hook events to gxserver without persisting hook payloads.\n"
    );
    fs::write(&hook_paths.notify_hook_path, script).map_err(io_error)?;
    fs::set_permissions(
        &hook_paths.notify_hook_path,
        fs::Permissions::from_mode(0o755),
    )
    .map_err(io_error)?;
    Ok(())
}

fn is_notify_hook_current(path: &Path) -> bool {
    read_file_text(path).contains(&format!("{NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}"))
}

fn hook_format(agent_id: &str) -> HookFormat {
    match agent_id {
        "antigravity" => HookFormat::Antigravity,
        "cursor" => HookFormat::FlatJson,
        "kiro" => HookFormat::KiroJson,
        "rovodev" | "hermes-agent" => HookFormat::MarkedYaml,
        "opencode" => HookFormat::Opencode,
        "amp" | "omp" | "pi" => HookFormat::PluginFile,
        _ => HookFormat::NestedJson,
    }
}

fn hook_marker(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some(AMP_PLUGIN_MARKER),
        "omp" => Some(OMP_EXTENSION_MARKER),
        "pi" => Some(PI_EXTENSION_MARKER),
        "opencode" => Some(OPENCODE_PLUGIN_MARKER),
        _ => None,
    }
}

fn command_agent(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude" => Some("claude"),
        "cursor" => Some("cursor"),
        "gemini" => Some("gemini"),
        "kiro" => Some("kiro"),
        "copilot" => Some("copilot"),
        "droid" => Some("factory"),
        "grok" => Some("grok"),
        "antigravity" => Some("antigravity"),
        "amp" => Some("amp"),
        "omp" => Some("omp"),
        "pi" => Some("pi"),
        "rovodev" => Some("rovodev"),
        "hermes-agent" => Some("hermes-agent"),
        "codebuddy" => Some("codebuddy"),
        "qoder" => Some("qoder"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

fn all_hook_events(agent_id: &str) -> Vec<&'static str> {
    let events: &[&str] = match agent_id {
        "codex" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "PreToolUse",
            "PermissionRequest",
        ],
        "claude" => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Stop",
            "Notification",
            "SessionEnd",
        ],
        "cursor" => &[
            "beforeSubmitPrompt",
            "stop",
            "afterAgentResponse",
            "beforeShellExecution",
            "afterShellExecution",
        ],
        "gemini" => &[
            "SessionStart",
            "BeforeAgent",
            "AfterAgent",
            "SessionEnd",
            "PreToolUse",
        ],
        "kiro" => &[
            "agentSpawn",
            "userPromptSubmit",
            "stop",
            "preToolUse",
            "postToolUse",
        ],
        "copilot" | "droid" | "codebuddy" => &[
            "SessionStart",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
        ],
        "grok" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "Notification",
            "SessionEnd",
            "PreToolUse",
        ],
        "antigravity" => &[
            "SessionStart",
            "PreInvocation",
            "Stop",
            "turn-completion",
            "Notification",
            "SessionEnd",
            "PreToolUse",
            "PostToolUse",
        ],
        "qoder" => &["SessionStart", "Stop", "SessionEnd", "PreToolUse"],
        _ => &[],
    };
    let mut output = Vec::new();
    for event in events {
        if !output.contains(event) {
            output.push(*event);
        }
    }
    output
}

fn command_for_agent(definition: &HookDefinition, notify_hook_path: &Path) -> String {
    let notify_hook_path = path_string(notify_hook_path);
    match command_agent(definition.agent_id) {
        Some(agent) => format!(
            "GHOSTEX_AGENT={} {}",
            shell_quote(agent),
            shell_quote(&notify_hook_path)
        ),
        None => notify_hook_path,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn is_opencode_session_plugin_registration(value: &Value) -> bool {
    let plugin = value
        .as_str()
        .or_else(|| value.as_array().and_then(|items| items.first()?.as_str()))
        .unwrap_or_default();
    plugin == OPENCODE_PLUGIN_SPEC
        || plugin == "ghostex-session"
        || plugin == "./plugins/ghostex-session.js"
        || plugin.ends_with("/plugins/ghostex-session.js")
        || plugin.ends_with("/ghostex-session.js")
}

fn is_ghostex_owned_hook_command(value: &Value, command: &str) -> bool {
    let Some(command_value) = value.get("command").and_then(Value::as_str) else {
        return false;
    };
    command_value == command || text_contains_ghostex_owned_hook_command(command_value)
}

fn text_contains_ghostex_owned_hook_command(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("agent-shell-notify")
        || normalized.contains(".ghostex/hooks")
        || normalized.contains(".ghostexterm")
        || normalized.contains("ghostex_notify_hook")
        || normalized.contains("ghostex-agent-notify")
        || normalized.contains(AMP_PLUGIN_MARKER)
        || normalized.contains(OMP_EXTENSION_MARKER)
        || normalized.contains(PI_EXTENSION_MARKER)
        || normalized.contains(OPENCODE_PLUGIN_MARKER)
        || normalized.contains("ghostex-opencode-session-extension-marker")
        || normalized.contains("ghostex-session-plugin-marker")
        || normalized.contains("ghostex-session-extension-marker")
}

fn normalize_agent_ids(value: Option<&Value>) -> Vec<String> {
    let requested = value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(normalize_requested_agent_id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            HOOK_DEFINITIONS
                .iter()
                .map(|definition| definition.agent_id.to_string())
                .collect()
        });
    let mut output = Vec::new();
    for agent_id in requested {
        if HOOK_DEFINITIONS
            .iter()
            .any(|definition| definition.agent_id == agent_id)
            && !output.contains(&agent_id)
        {
            output.push(agent_id);
        }
    }
    if output.is_empty() {
        HOOK_DEFINITIONS
            .iter()
            .map(|definition| definition.agent_id.to_string())
            .collect()
    } else {
        output
    }
}

fn normalize_requested_agent_id(value: &Value) -> Option<String> {
    let normalized = value
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mapped = match normalized.as_str() {
        "agy" | "antigravity cli" => "antigravity",
        "claude code" => "claude",
        "code buddy" => "codebuddy",
        "codex cli" => "codex",
        "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "factory" | "factory droid" => "droid",
        "gemini cli" => "gemini",
        "github copilot" => "copilot",
        "kiro cli" | "kiro-cli" => "kiro",
        "open code" => "opencode",
        "qodercli" => "qoder",
        "rovo" | "rovo dev" => "rovodev",
        other => other,
    };
    (!mapped.is_empty()).then_some(mapped.to_string())
}

fn resolve_config_directory(
    home_dir: &Path,
    env_key: &str,
    fallback_relative_path: &str,
    env_subpath: Option<&str>,
) -> PathBuf {
    match normalize_environment_path(std::env::var(env_key).ok().as_deref(), home_dir) {
        Some(path) => env_subpath
            .map(|subpath| path.join(subpath))
            .unwrap_or(path),
        None => home_dir.join(fallback_relative_path),
    }
}

fn resolve_omp_agent_directory(home_dir: &Path) -> PathBuf {
    if let Some(pi_agent_root) = normalize_environment_path(
        std::env::var("PI_CODING_AGENT_DIR").ok().as_deref(),
        home_dir,
    ) {
        return pi_agent_root;
    }
    let config_dir =
        normalize_environment_path(std::env::var("PI_CONFIG_DIR").ok().as_deref(), home_dir)
            .unwrap_or_else(|| home_dir.join(".omp"));
    config_dir.join("agent")
}

fn normalize_environment_path(value: Option<&str>, home_dir: &Path) -> Option<PathBuf> {
    let trimmed = value.map(str::trim).filter(|value| !value.is_empty())?;
    if trimmed == "~" {
        return Some(home_dir.to_path_buf());
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return Some(home_dir.join(relative));
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(home_dir.join(path))
    }
}

fn list_profile_hook_paths(home_dir: &Path, profile_dir: &str, file_name: &str) -> Vec<PathBuf> {
    let profiles_path = home_dir.join(profile_dir);
    let Ok(entries) = fs::read_dir(&profiles_path) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|file_type| file_type.is_dir())
                .map(|_| entry.path().join(file_name))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn command_exists(command: &str, home_dir: &Path) -> bool {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let mut entries = path_env.split(':').map(PathBuf::from).collect::<Vec<_>>();
    entries.extend([
        home_dir.join(".opencode").join("bin"),
        home_dir.join(".local").join("bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    entries
        .into_iter()
        .map(|entry| entry.join(command))
        .any(|candidate| candidate.is_file() && is_executable(&candidate))
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn read_file_text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn display_path(path: &str, home_dir: &Path) -> String {
    let home = path_string(home_dir);
    path.strip_prefix(&format!("{home}/"))
        .map(|relative| format!("~/{relative}"))
        .unwrap_or_else(|| path.to_string())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn push_unique_path(paths: &mut Vec<String>, path: String) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn json_error(error: serde_json::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Agent hook JSON operation failed: {error}"),
    }
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Agent hook file operation failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;

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
        let removed_pi =
            uninstall_plugin_file_hook(&pi, vec![pi_path.clone()]).expect("pi uninstall");
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

    fn write_test_file(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, text).expect("write test file");
    }
}
