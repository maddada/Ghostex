//! Removes the hooks Ghostex wrote for the agents it follows without them.
//!
//! CDXC:AgentHooks 2026-11-12 DECISION:
//! The user's "Stop installing the hooks and remove Ghostex's own" (hook_free.rs): on every gxserver start while Use agent hooks is off (the first start after the update does the real work, later ones find nothing), and when the setting is turned off, Ghostex takes out of every config it installs into (`~/.claude`, `~/.codex`, OpenCode's folder, every cswap profile, the Windows folders, through `provider_hook_paths`) only what it wrote: its hook commands and Codex's trust records for them, its OpenCode plugin file and registration. The user's own hooks and every other setting stay, and so does Ghostex's Claude `statusLine` and OpenCode theme, which are not hooks (the statusLine names the model, effort and rate limits in the chat). An agent whose hook the safety net put back keeps it, a portable copy never touches another Ghostex's hooks, and a Claude before 2.1.172 keeps its hook. The installer keeps no backups of the files it changes, so neither does this.
//! SEE-ALSO: server/src/agent_hooks/install/uninstall.rs (the same removal Uninstall all runs), server/src/hookless_activity/task.rs (when it runs).

use serde_json::{json, Value};

use super::config::{HookPaths, HOOK_DEFINITIONS};
use super::foreign_owner::hooks_belong_to_another_ghostex;
use super::hook_free;
use super::install::{inspect_agent_hook_installation, uninstall_agent_hook_entries};
use super::probing::push_unique_path;
use super::resolution::provider_hook_paths;
use crate::paths::GxserverPaths;

/// What one cleanup pass did.
#[derive(Debug, Default)]
pub(crate) struct HookFreeCleanup {
    /// The agents whose Ghostex hook was removed.
    pub agents: Vec<String>,
    pub removed_paths: Vec<String>,
    /// Agents whose hook could not be removed, with why.
    pub errors: Vec<(String, String)>,
}

impl HookFreeCleanup {
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "agents": self.agents,
            "removedPaths": self.removed_paths,
            "errors": self
                .errors
                .iter()
                .map(|(agent, error)| json!({ "agentId": agent, "error": error }))
                .collect::<Vec<_>>(),
        })
    }
}

/// Removes Ghostex's own hook entries for every agent it follows without hooks here, except the
/// ones a fallback installed.
pub(crate) fn remove_hook_free_agent_hooks(paths: &GxserverPaths) -> HookFreeCleanup {
    let hook_paths = HookPaths::from_paths(paths);
    let kept = hook_free::fallback_agents(paths);
    let mut cleanup = HookFreeCleanup::default();
    for definition in HOOK_DEFINITIONS {
        let agent = definition.agent_id;
        if !hook_free::hook_free(paths, agent) || kept.contains(agent) {
            continue;
        }
        if let Err(error) = remove_one(definition, &hook_paths, &mut cleanup) {
            cleanup.errors.push((agent.to_string(), error));
        }
    }
    cleanup
}

fn remove_one(
    definition: &super::config::HookDefinition,
    hook_paths: &HookPaths,
    cleanup: &mut HookFreeCleanup,
) -> Result<(), String> {
    let provider_paths = provider_hook_paths(definition.agent_id, hook_paths);
    if hooks_belong_to_another_ghostex(definition, &provider_paths) {
        return Ok(());
    }
    if !inspect_agent_hook_installation(definition, hook_paths, &provider_paths)
        .ghostex_hook_present
    {
        return Ok(());
    }
    let removed =
        uninstall_agent_hook_entries(definition, hook_paths).map_err(|error| error.message)?;
    if !removed.is_empty() {
        cleanup.agents.push(definition.agent_id.to_string());
    }
    for path in removed {
        push_unique_path(&mut cleanup.removed_paths, path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hook_free_paths(temp: &std::path::Path) -> GxserverPaths {
        let paths = crate::paths::get_gxserver_paths(Some(temp.to_path_buf()));
        std::fs::create_dir_all(&paths.app_config_dir).expect("config dir");
        std::fs::write(
            crate::shared_settings_file::shared_settings_path(&paths),
            r#"{"useAgentHooks": false}"#,
        )
        .expect("settings");
        hook_free::forget_cached_setting();
        paths
    }

    fn ghostex_command(paths: &GxserverPaths, agent: &str) -> String {
        let hook_paths = HookPaths::from_paths(paths);
        let definition = HOOK_DEFINITIONS
            .iter()
            .find(|definition| definition.agent_id == agent)
            .expect("definition");
        super::super::plugin_sources::command_for_agent(definition, &hook_paths.notify_hook_path)
    }

    /// Claude's settings with Ghostex's hook beside one of the user's own, Ghostex's statusLine
    /// and an unrelated setting: only Ghostex's hook leaves.
    #[test]
    fn the_start_cleanup_removes_only_ghostexs_claude_hook() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = hook_free_paths(temp.path());
        let ghostex = ghostex_command(&paths, "claude");
        let settings_path = temp.path().join(".claude").join("settings.json");
        std::fs::create_dir_all(settings_path.parent().unwrap()).expect("dir");
        let user_hook = json!({ "type": "command", "command": "~/bin/my-own-stop-hook.sh" });
        let statusline = json!({
            "type": "command",
            "command": format!("{} --wrap", HookPaths::from_paths(&paths).statusline_hook_path.display()),
        });
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "model": "opus",
                "hooks": {
                    "Stop": [
                        { "hooks": [user_hook.clone()] },
                        { "hooks": [{ "type": "command", "command": ghostex }] },
                    ],
                    "SessionStart": [
                        { "matcher": "startup", "hooks": [{ "type": "command", "command": ghostex }] },
                    ],
                },
                "statusLine": statusline.clone(),
            }))
            .unwrap(),
        )
        .expect("settings");

        let cleanup = remove_hook_free_agent_hooks(&paths);
        assert_eq!(cleanup.agents, vec!["claude".to_string()]);
        assert!(cleanup.errors.is_empty(), "{:?}", cleanup.errors);
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(after["model"], "opus");
        assert_eq!(after["hooks"]["Stop"], json!([{ "hooks": [user_hook] }]));
        assert!(after["hooks"].get("SessionStart").is_none(), "{after}");
        assert_eq!(after["statusLine"], statusline);
        assert!(!after.to_string().contains("agent-shell-notify"));

        // A second start finds nothing left to remove.
        let again = remove_hook_free_agent_hooks(&paths);
        assert!(again.removed_paths.is_empty());
    }

    /// Codex's hooks.json keeps the user's hook; OpenCode loses only Ghostex's plugin file and
    /// registration, not the user's own plugin.
    #[test]
    fn the_start_cleanup_keeps_the_users_codex_and_opencode_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = hook_free_paths(temp.path());
        let hook_paths = HookPaths::from_paths(&paths);
        let codex_hooks = temp.path().join(".codex").join("hooks.json");
        std::fs::create_dir_all(codex_hooks.parent().unwrap()).expect("dir");
        let ghostex = ghostex_command(&paths, "codex");
        let user = json!({ "type": "command", "command": "say done" });
        std::fs::write(
            &codex_hooks,
            serde_json::to_string_pretty(&json!({
                "hooks": {
                    "Stop": [
                        { "hooks": [user.clone()] },
                        { "hooks": [{ "type": "command", "command": ghostex }] },
                    ],
                },
            }))
            .unwrap(),
        )
        .expect("codex hooks");

        let opencode_paths = provider_hook_paths("opencode", &hook_paths);
        let plugin = opencode_paths[0].clone();
        let cli_json = opencode_paths[1].clone();
        std::fs::create_dir_all(plugin.parent().unwrap()).expect("plugin dir");
        std::fs::write(
            &plugin,
            format!("// {}\n", super::super::config::OPENCODE_PLUGIN_MARKER),
        )
        .expect("plugin");
        let spec = url::Url::from_directory_path(plugin.parent().unwrap())
            .unwrap()
            .to_string();
        std::fs::write(
            &cli_json,
            serde_json::to_string_pretty(
                &json!({ "plugins": [spec, "my-plugin"], "theme": { "name": "mine" } }),
            )
            .unwrap(),
        )
        .expect("cli.json");

        let cleanup = remove_hook_free_agent_hooks(&paths);
        assert!(cleanup.errors.is_empty(), "{:?}", cleanup.errors);
        let mut agents = cleanup.agents.clone();
        agents.sort();
        assert_eq!(agents, vec!["codex".to_string(), "opencode".to_string()]);
        let codex: Value =
            serde_json::from_str(&std::fs::read_to_string(&codex_hooks).unwrap()).unwrap();
        assert_eq!(codex["hooks"]["Stop"], json!([{ "hooks": [user] }]));
        assert!(!plugin.exists());
        let cli: Value =
            crate::session_chat_opencode::read_config(&std::fs::read_to_string(&cli_json).unwrap())
                .unwrap();
        assert_eq!(cli["plugins"], json!(["my-plugin"]));
        assert_eq!(cli["theme"]["name"], "mine");
    }

    /// An agent whose hook a fallback installed keeps it, and nothing is removed while Use agent
    /// hooks is on.
    #[test]
    fn a_fallback_hook_and_the_setting_on_keep_the_hooks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = hook_free_paths(temp.path());
        let ghostex = ghostex_command(&paths, "claude");
        let settings_path = temp.path().join(".claude").join("settings.json");
        std::fs::create_dir_all(settings_path.parent().unwrap()).expect("dir");
        let text = serde_json::to_string_pretty(&json!({
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": ghostex }] }] },
        }))
        .unwrap();
        std::fs::write(&settings_path, &text).expect("settings");
        hook_free::record_fallback(&paths, "claude", "P1", "G1", "test");
        assert!(remove_hook_free_agent_hooks(&paths)
            .removed_paths
            .is_empty());
        assert_eq!(std::fs::read_to_string(&settings_path).unwrap(), text);

        hook_free::clear_fallbacks(&paths);
        std::fs::write(
            crate::shared_settings_file::shared_settings_path(&paths),
            r#"{"useAgentHooks": true}"#,
        )
        .expect("settings");
        hook_free::forget_cached_setting();
        assert!(remove_hook_free_agent_hooks(&paths)
            .removed_paths
            .is_empty());
        assert_eq!(std::fs::read_to_string(&settings_path).unwrap(), text);
    }
}
