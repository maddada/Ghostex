use std::fs;

use serde_json::{json, Map, Value};

use crate::{domain::DomainStateError, paths::GxserverPaths};

use super::config::{HOOK_DEFINITIONS, HookDefinition, HookFormat, HookPaths, hook_format};
use super::install::{
    inspect_agent_hook_installation, install_agent_hook, install_notify_hook,
    is_notify_hook_current, migrate_hook_session_sidecars, notify_hook_state_directory,
    repair_agent_hook_paths, uninstall_agent_hook,
};
use super::probing::{
    command_exists, display_path, io_error, now_iso, path_string, push_unique_path, read_file_text,
};
use super::resolution::{normalize_agent_ids, provider_hook_paths};

/*
CDXC:AgentHooks 2026-06-16-10:00:
Rust Phase 6 exposes the same local-only hook status and install RPCs without putting raw hook payloads, terminal titles, paths, or command output into persistent logs. Status reports deterministic metadata, while explicit install writes only Ghostex-owned hook artifacts under the selected HOME.
*/
pub fn read_agent_hook_status(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::from_paths(paths);
    let agent_ids = normalize_agent_ids(params.get("agentIds"));
    let auto_upgrade = params.get("autoUpgradeInstalled").and_then(Value::as_bool) != Some(false);
    let auto_upgraded_paths = if auto_upgrade {
        repair_installed_agent_hook_paths(paths)?
    } else {
        Vec::new()
    };
    let mut rows = Vec::new();
    for agent_id in agent_ids {
        if let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        {
            rows.push(read_hook_status(definition, &hook_paths)?);
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
    let hook_paths = HookPaths::from_paths(paths);
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
        installed_paths.extend(install_agent_hook(definition, &hook_paths)?);
    }
    let mut status = read_agent_hook_status(paths, params)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installedPaths".to_string(), json!(installed_paths));
    Ok(Value::Object(status))
}

/*
CDXC:AgentHookRepair 2026-08-05-03:35:
Platform-native storage moved the shared notify executable out of ~/.ghostex,
and older installers could write a current path without the shell quoting it
needs. Provider configuration files live outside Ghostex storage, so repair
every already-installed Ghostex hook at daemon startup without depending on
whether that provider CLI is visible on the launch daemon's PATH. A
Ghostex-owned command that does not exactly match the current generated command
is stale. This remains idempotent: absent providers and user-owned hooks stay
untouched, while main and profile configs that already contain Ghostex hooks
are upgraded in place.
*/
pub fn repair_installed_agent_hook_paths(
    paths: &GxserverPaths,
) -> Result<Vec<String>, DomainStateError> {
    let hook_paths = HookPaths::from_paths(paths);
    let mut stale_targets = Vec::new();
    let mut has_installed_ghostex_hook = false;

    for definition in HOOK_DEFINITIONS {
        let provider_paths = provider_hook_paths(definition.agent_id, &hook_paths);
        if hook_format(definition.agent_id) == HookFormat::Opencode {
            let inspection =
                inspect_agent_hook_installation(definition, &hook_paths, &provider_paths);
            if inspection.ghostex_hook_present {
                has_installed_ghostex_hook = true;
                if !inspection.current_hook_installed {
                    stale_targets.push((definition, provider_paths));
                }
            }
            continue;
        }

        let mut stale_paths = Vec::new();
        for provider_path in provider_paths {
            let inspection = inspect_agent_hook_installation(
                definition,
                &hook_paths,
                std::slice::from_ref(&provider_path),
            );
            if !inspection.ghostex_hook_present {
                continue;
            }
            has_installed_ghostex_hook = true;
            if !inspection.current_hook_installed {
                stale_paths.push(provider_path);
            }
        }
        if !stale_paths.is_empty() {
            stale_targets.push((definition, stale_paths));
        }
    }

    if !has_installed_ghostex_hook {
        return Ok(Vec::new());
    }

    let notify_hook_contents = read_file_text(&hook_paths.notify_hook_path);
    let notify_stale = !is_notify_hook_current(&hook_paths, &notify_hook_contents);
    if !notify_stale && stale_targets.is_empty() {
        return Ok(Vec::new());
    }

    let mut repaired_paths = Vec::new();
    if notify_stale {
        if let Some(previous_state_directory) = notify_hook_state_directory(&notify_hook_contents) {
            for migrated_path in migrate_hook_session_sidecars(
                &previous_state_directory,
                &hook_paths.hook_state_directory,
            )? {
                push_unique_path(&mut repaired_paths, migrated_path);
            }
        }
        install_notify_hook(&hook_paths)?;
        push_unique_path(
            &mut repaired_paths,
            path_string(&hook_paths.notify_hook_path),
        );
    }
    for (definition, provider_paths) in stale_targets {
        for repaired_path in repair_agent_hook_paths(definition, &hook_paths, provider_paths)? {
            push_unique_path(&mut repaired_paths, repaired_path);
        }
    }
    Ok(repaired_paths)
}

pub fn uninstall_agent_hooks(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let hook_paths = HookPaths::from_paths(paths);
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

fn read_hook_status(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
) -> Result<Value, DomainStateError> {
    let cli_installed = command_exists(definition.cli_command, &hook_paths.home_dir);
    /*
    CDXC:AgentHooks 2026-06-19-18:43:
    Hook status must report every candidate provider config path and inspect all candidates for Ghostex-owned hooks so profile-only Codex and Claude installs are not misreported as missing.
    Keep first-time install conservative, but status reads should treat any current provider candidate as installed once the shared notify hook is current.
    */
    let provider_paths = provider_hook_paths(definition.agent_id, hook_paths);
    let paths = provider_paths
        .iter()
        .map(|path| path_string(path))
        .collect::<Vec<_>>();
    let notify_hook_contents = read_file_text(&hook_paths.notify_hook_path);
    let notify_current = is_notify_hook_current(hook_paths, &notify_hook_contents);
    let inspection = inspect_agent_hook_installation(definition, hook_paths, &provider_paths);
    let provider_current = inspection.current_hook_installed;
    let ghostex_hook_present = inspection.ghostex_hook_present;
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
        "detail": hook_detail(definition, hook_paths, status, notify_current, paths.first().map(String::as_str)),
        "hookInstalled": hook_installed,
        "paths": paths,
        "status": status,
    }))
}

fn hook_detail(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    status: &str,
    notify_current: bool,
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
        "updateRequired" if notify_current => format!("Run Update Hooks to repair {display}"),
        "updateRequired" => format!(
            "Run Update Hooks to update {}",
            display_path(
                &path_string(&hook_paths.notify_hook_path),
                &hook_paths.home_dir
            )
        ),
        _ => format!("Run Install Hooks to write {display}"),
    }
}
