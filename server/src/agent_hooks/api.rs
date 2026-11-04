use std::fs;

use serde_json::{json, Map, Value};

use crate::{domain::DomainStateError, paths::GxserverPaths};

use super::codex_trust::{codex_hook_trust_status_for_paths, CodexHookTrustStatus};
use super::config::{hook_format, HookDefinition, HookFormat, HookPaths, HOOK_DEFINITIONS};
use super::foreign_owner::hooks_belong_to_another_ghostex;
use super::install::{
    antigravity_ghostex_hook_disabled, claude_all_hooks_disabled, inspect_agent_hook_installation,
    install_agent_hook, install_notify_hook, is_notify_hook_current, migrate_hook_session_sidecars,
    notify_hook_state_directory, repair_agent_hook_paths, uninstall_agent_hook,
};
use super::plugin_sources::command_for_agent;
use super::probing::{
    cli_presence, command_exists_uncached, display_path, io_error, now_iso, path_string,
    push_unique_path, read_file_text, CliPresence,
};
use super::resolution::{normalize_agent_ids, provider_hook_paths};
use super::statusline::{
    are_statusline_hooks_current, install_statusline_hook, is_statusline_hook_current,
};

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
    let read_rows = || -> Result<Vec<Value>, DomainStateError> {
        let mut rows = Vec::new();
        for agent_id in &agent_ids {
            if let Some(definition) = HOOK_DEFINITIONS
                .iter()
                .find(|definition| definition.agent_id == agent_id.as_str())
            {
                rows.push(read_hook_status(definition, &hook_paths)?);
            }
        }
        Ok(rows)
    };
    let mut rows = read_rows()?;
    /*
    CDXC:AgentHooks 2026-10-04 WHY:
    The desktop awaits this read before every agent launch, and the full repair scan (every agent's config files) took 170–600 ms on Windows while the status read takes about 3 ms, all of it in front of the agent starting. A repair can only turn a present-but-stale Ghostex hook (`updateRequired`) into a current one, so it runs, and the rows are read again, only when a requested agent reports that; daemon startup still repairs every installed hook.
    */
    let auto_upgraded_paths =
        if auto_upgrade && rows.iter().any(|row| row["status"] == "updateRequired") {
            let upgraded = repair_installed_agent_hook_paths(paths)?;
            rows = read_rows()?;
            upgraded
        } else {
            Vec::new()
        };
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
    install_statusline_hook(&hook_paths)?;
    installed_paths.push(path_string(&hook_paths.statusline_hook_path));
    installed_paths.push(path_string(&hook_paths.cursor_statusline_hook_path));
    for agent_id in agent_ids {
        let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        else {
            continue;
        };
        /*
        CDXC:AgentHooks 2026-09-01:
        Status reads accept a 60 second CLI-presence cache, but install skips a
        provider outright when its CLI is missing. A user who installs the CLI
        and immediately presses Install Hooks must not be told the provider is
        absent, so this call re-probes and refreshes the cache entry.
        */
        if !command_exists_uncached(definition.cli_command, &hook_paths.home_dir) {
            continue;
        }
        // Another Ghostex's hooks already serve this portable copy's sessions (foreign_owner.rs).
        if hooks_belong_to_another_ghostex(
            definition,
            &provider_hook_paths(definition.agent_id, &hook_paths),
        ) {
            continue;
        }
        installed_paths.extend(install_agent_hook(definition, &hook_paths)?);
    }
    /*
    CDXC:AgentHooks 2026-09-02:
    Settings replaces its whole hook-status snapshot with this response, so a
    scoped install (one row's Install/Update hook button) must still report
    the complete catalog. Answering with only the requested agents made every
    other row fall back to "not checked" until the user pressed Refresh.
    */
    let mut status_params = params.clone();
    status_params.remove("agentIds");
    let mut status = read_agent_hook_status(paths, &status_params)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    status.insert("installedPaths".to_string(), json!(installed_paths));
    Ok(Value::Object(status))
}

/*
CDXC:AgentHooks 2026-08-05-03:35:
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

        /*
        CDXC:AgentHooks 2026-09-27 WHY:
        A marked-block install writes every file its provider lists (each Hermes profile's config), so hooks in any of them are consent for all: a profile without them (created later, or never copied) is repaired like a stale one.
        JSON providers' profile candidates stay opt-in per file.
        */
        let inspections = provider_paths
            .into_iter()
            .map(|provider_path| {
                let inspection = inspect_agent_hook_installation(
                    definition,
                    &hook_paths,
                    std::slice::from_ref(&provider_path),
                );
                (provider_path, inspection)
            })
            .collect::<Vec<_>>();
        let installed_for_every_path = matches!(
            hook_format(definition.agent_id),
            HookFormat::MarkedYaml | HookFormat::TomlMarked
        ) && inspections
            .iter()
            .any(|(_, inspection)| inspection.ghostex_hook_present);
        let mut stale_paths = Vec::new();
        for (provider_path, inspection) in inspections {
            if !inspection.ghostex_hook_present && !installed_for_every_path {
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
    let statusline_stale = !are_statusline_hooks_current(&hook_paths);
    if !notify_stale && !statusline_stale && stale_targets.is_empty() {
        return Ok(Vec::new());
    }

    let mut repaired_paths = Vec::new();
    if statusline_stale {
        install_statusline_hook(&hook_paths)?;
        push_unique_path(
            &mut repaired_paths,
            path_string(&hook_paths.statusline_hook_path),
        );
        push_unique_path(
            &mut repaired_paths,
            path_string(&hook_paths.cursor_statusline_hook_path),
        );
    }
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
    Advanced Settings uninstall must remove only Ghostex-owned hook commands, marked YAML blocks, plugin registrations, and Ghostex extension files while leaving user-managed provider hooks intact. The notify executable is shared by every provider, so remove it only after the last Ghostex-owned provider hook is gone. Status is reread for the complete catalog with auto-upgrade disabled so a scoped uninstall returns an authoritative UI snapshot without recreating hooks it just removed.
    */
    for agent_id in &agent_ids {
        let Some(definition) = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent_id)
        else {
            continue;
        };
        // A portable copy never removes another Ghostex's hooks (foreign_owner.rs).
        if hooks_belong_to_another_ghostex(
            definition,
            &provider_hook_paths(definition.agent_id, &hook_paths),
        ) {
            continue;
        }
        for removed_path in uninstall_agent_hook(definition, &hook_paths)? {
            push_unique_path(&mut removed_paths, removed_path);
        }
    }
    let has_remaining_provider_hook = HOOK_DEFINITIONS.iter().any(|definition| {
        let provider_paths = provider_hook_paths(definition.agent_id, &hook_paths);
        inspect_agent_hook_installation(definition, &hook_paths, &provider_paths)
            .ghostex_hook_present
    });
    if !has_remaining_provider_hook {
        for shared_path in [
            &hook_paths.notify_hook_path,
            &hook_paths.statusline_hook_path,
            &hook_paths.cursor_statusline_hook_path,
        ] {
            match fs::remove_file(shared_path) {
                Ok(()) => push_unique_path(&mut removed_paths, path_string(shared_path)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
        }
    }
    let mut status_params = params.clone();
    status_params.remove("agentIds");
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
    let presence = cli_presence(definition.cli_command, &hook_paths.home_dir);
    let cli_installed = presence == CliPresence::Installed;
    let installs_on_demand = presence == CliPresence::InstallsOnDemand;
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
    if cli_installed && hooks_belong_to_another_ghostex(definition, &provider_paths) {
        return Ok(json!({
            "agentId": definition.agent_id,
            "cliCommand": definition.cli_command,
            "cliInstalled": true,
            "detail": format!(
                "Installed by another Ghostex on this computer in {}; its hooks serve this copy's sessions too.",
                display_path(paths.first().map(String::as_str).unwrap_or_default(), &hook_paths.home_dir)
            ),
            "hookInstalled": true,
            "paths": paths,
            "status": "installed",
        }));
    }
    let notify_hook_contents = read_file_text(&hook_paths.notify_hook_path);
    let notify_current = is_notify_hook_current(hook_paths, &notify_hook_contents)
        // CDXC:AgentHooks 2026-09-03 WHY: Claude and Cursor also depend on
        // their statusline script being current; every other provider ignores it.
        && match definition.agent_id {
            "claude" => is_statusline_hook_current(
                hook_paths,
                &read_file_text(&hook_paths.statusline_hook_path),
            ),
            "cursor" => is_statusline_hook_current(
                hook_paths,
                &read_file_text(&hook_paths.cursor_statusline_hook_path),
            ),
            _ => true,
        };
    let inspection = inspect_agent_hook_installation(definition, hook_paths, &provider_paths);
    let provider_current = inspection.current_hook_installed;
    let ghostex_hook_present = inspection.ghostex_hook_present;
    /*
    CDXC:AgentHooks 2026-09-02 / CDXC:AgentHooks 2026-09-03:
    A hook is only "installed" once its CLI will actually RUN it. Codex needs
    its trust record next to the file, Antigravity and Claude carry an off
    switch outside the hook entries. Any closed gate reads as updateRequired
    so the Update Hooks button repairs what it can, and the detail says why
    instead of the generic "repair" wording.
    */
    let run_gate = ghostex_hook_present
        .then(|| hook_run_gate(definition, hook_paths, &provider_paths))
        .flatten();
    let hook_installed = notify_current && provider_current && run_gate.is_none();
    let status = if !cli_installed {
        "cliMissing"
    } else if hook_installed {
        "installed"
    } else if ghostex_hook_present {
        "updateRequired"
    } else {
        "missing"
    };
    let detail = if installs_on_demand {
        format!(
            "Not installed yet: the {} on your PATH is a launcher that installs it the first time you start it.",
            definition.cli_command
        )
    } else {
        hook_detail(
            definition,
            hook_paths,
            status,
            notify_current,
            run_gate,
            paths.first().map(String::as_str),
        )
    };
    Ok(json!({
        "agentId": definition.agent_id,
        "cliCommand": definition.cli_command,
        "cliInstalled": cli_installed,
        "cliInstallsOnDemand": installs_on_demand,
        "detail": detail,
        "hookInstalled": hook_installed,
        "paths": paths,
        "status": status,
    }))
}

/// A provider-side switch that stops an otherwise current Ghostex hook from
/// running. `None` means the provider will run the hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookRunGate {
    CodexUntrusted,
    CodexDisabled,
    AntigravityDisabled,
    ClaudeAllHooksDisabled,
}

fn hook_run_gate(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    provider_paths: &[std::path::PathBuf],
) -> Option<HookRunGate> {
    match definition.agent_id {
        "codex" => match codex_hook_trust_status_for_paths(
            provider_paths,
            hook_paths,
            &command_for_agent(definition, &hook_paths.notify_hook_path),
        ) {
            CodexHookTrustStatus::Trusted => None,
            CodexHookTrustStatus::Untrusted => Some(HookRunGate::CodexUntrusted),
            CodexHookTrustStatus::Disabled => Some(HookRunGate::CodexDisabled),
        },
        "antigravity" => provider_paths
            .iter()
            .any(|path| antigravity_ghostex_hook_disabled(&read_file_text(path)))
            .then_some(HookRunGate::AntigravityDisabled),
        "claude" | "openclaude" => provider_paths
            .iter()
            .any(|path| claude_all_hooks_disabled(&read_file_text(path)))
            .then_some(HookRunGate::ClaudeAllHooksDisabled),
        _ => None,
    }
}

fn hook_detail(
    definition: &HookDefinition,
    hook_paths: &HookPaths,
    status: &str,
    notify_current: bool,
    run_gate: Option<HookRunGate>,
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
        "updateRequired" if run_gate == Some(HookRunGate::CodexUntrusted) => {
            "Codex has not trusted the Ghostex hooks yet, so it skips them. Run Update Hooks to approve them."
                .to_string()
        }
        "updateRequired" if run_gate == Some(HookRunGate::CodexDisabled) => {
            "The Ghostex hooks are disabled in Codex (/hooks). Run Update Hooks to enable them."
                .to_string()
        }
        "updateRequired" if run_gate == Some(HookRunGate::AntigravityDisabled) => {
            "The Ghostex hook is disabled in Antigravity (\"enabled\": false in hooks.json). Run Update Hooks to enable it."
                .to_string()
        }
        "updateRequired" if run_gate == Some(HookRunGate::ClaudeAllHooksDisabled) => format!(
            "All hooks are switched off in {display} (disableAllHooks), so {} never runs the Ghostex hooks. Remove that setting to enable them.",
            if definition.agent_id == "openclaude" { "OpenClaude" } else { "Claude" }
        ),
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
