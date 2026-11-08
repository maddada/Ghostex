//! Agents page preview: the Storybook sidebar store's default agent roster
//! (`createDefaultSidebarAgentButtons`), the Settings story's hook status (hooks installed for the
//! first four default agents, missing for the next six, CLI missing for the rest), and a scripted
//! `/api/agentCliMaintenance` shaped like the "Modals/Settings Agent CLIs" story (ZCode not
//! installed, Claude installed through mise, Pi installed without a known method, Grok's update
//! failed).
//!
//! States: `agents` (the Agents story), `agents-hooks` (the `agentHooks` deep link),
//! `agents-expanded` (Codex and Claude open), `agents-clis` (ZCode, Claude, Pi and Grok open),
//! `agents-cli-running` (Gemini installing), `agents-cli-failed` (Grok's failed update),
//! `agents-editor-new`, `agents-editor-edit` (Claude), `agents-skip-confirm`, `agents-loading`
//! (hook status never answers), `agents-empty` (no launchers), `agents-search` ("approval"),
//! `agents-no-match` ("tailscale"), `agents-home` (scrolled to the Home folder card),
//! `agents-home-open` (its folders row opened by a search). The page reads the page states from `preview_state`.
use crate::settings_modal::catalog::{module, settings_catalog};
use serde_json::{Map, Value, json};
use std::cell::RefCell;

thread_local! {
    static STATE: RefCell<String> = const { RefCell::new(String::new()) };
}

fn state() -> String {
    STATE.with(|state| state.borrow().clone())
}

fn is_agents_state(state: &str) -> bool {
    state == "agents" || state.starts_with("agents-")
}

pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    STATE.with(|current| *current.borrow_mut() = state.to_string());
    // The page opens scrolled like the Storybook captures (`scrollTop = 420`).
    if matches!(state, "agents-expanded") {
        settings.insert(
            "settingsModalNavigation".into(),
            json!({ "activeTab": "agents", "scrollTopByTab": { "agents": 420 }, "version": 1 }),
        );
    }
}

/// `createDefaultSidebarAgentButtons()`.
fn default_agent_buttons() -> Value {
    json!([
        { "agentId": "codex", "command": "codex", "icon": "codex", "isDefault": true, "name": "Codex" },
        { "agentId": "claude", "command": "claude", "icon": "claude", "isDefault": true, "name": "Claude" },
        { "agentId": "cursor", "command": "cursor-agent", "icon": "cursor-cli", "isDefault": true, "name": "Cursor CLI" },
        { "agentId": "pi", "command": "pi", "icon": "pi", "isDefault": true, "name": "Pi Agent" },
        { "agentId": "opencode", "command": "opencode", "icon": "opencode", "isDefault": true, "name": "OpenCode" },
        { "agentId": "gemini", "command": "gemini", "icon": "gemini", "isDefault": true, "name": "Gemini" },
        { "agentId": "copilot", "command": "copilot", "icon": "copilot", "isDefault": true, "name": "Copilot" },
        { "agentId": "droid", "command": "droid", "icon": "factory-droid", "isDefault": true, "name": "Factory Droid" },
        { "agentId": "grok", "command": "grok", "icon": "grok-build", "isDefault": true, "name": "Grok Build" },
        { "agentId": "antigravity", "command": "agy", "icon": "antigravity-cli", "isDefault": true, "name": "Antigravity CLI" },
        { "agentId": "amp", "command": "amp", "icon": "amp-cli", "isDefault": true, "name": "Amp CLI" },
        { "agentId": "hermes-agent", "command": "hermes", "icon": "hermes-agent", "isDefault": true, "name": "Hermes Agent" },
        { "agentId": "mastra", "command": "mastracode", "icon": "mastra", "isDefault": true, "name": "Mastra Code" },
        { "agentId": "zcode", "command": "zcode", "icon": "zcode", "isDefault": true, "name": "ZCode" }
    ])
}

pub(super) fn extend_sidebar_state(message: &mut Value) {
    message["hud"]["agents"] = if state() == "agents-empty" {
        json!([])
    } else {
        default_agent_buttons()
    };
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    if !is_agents_state(state) {
        return None;
    }
    let mut message = json!({ "initialTab": "agents" });
    match state {
        "agents-hooks" => message["initialAgentsSection"] = json!("agentHooks"),
        "agents-search" => message["initialSearchQuery"] = json!("approval"),
        // The Home folder card (on with `GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS='{"mainHubHidden":false}'`),
        // its folders row folded, or opened by a search that finds it.
        "agents-home" => message["initialSection"] = json!("homeFolder"),
        "agents-home-open" => message["initialSearchQuery"] = json!("can't read"),
        "agents-no-match" => message["initialSearchQuery"] = json!("tailscale"),
        _ => {}
    }
    Some(message)
}

/// The Storybook Agents story has no gxserver connection (no CLI actions, "Connect to a computer
/// to manage its agent CLIs."); the CLI states have one.
pub(super) fn gxserver_rpc_available(state: &str) -> bool {
    !matches!(
        state,
        "agents" | "agents-hooks" | "agents-expanded" | "agents-loading"
    )
}

/// The Settings story's `agentHookStatus`.
fn hook_status(installed_all: bool) -> Value {
    let agents: Vec<Value> = settings_catalog()
        .module_value(module::SIDEBAR_AGENTS, "DEFAULT_SIDEBAR_AGENTS")
        .and_then(Value::as_array)
        .map(|agents| {
            agents
                .iter()
                .enumerate()
                .map(|(index, agent)| {
                    let agent_id = agent["agentId"].as_str().unwrap_or_default();
                    let command = agent["command"].as_str().unwrap_or_default();
                    let cli = command.split(' ').next().unwrap_or(command);
                    let cli_installed = index < 10;
                    let installed = index < 4 || (installed_all && cli_installed);
                    json!({
                        "agentId": agent_id,
                        "cliCommand": cli,
                        "cliInstalled": cli_installed,
                        "detail": if installed { "Hook config is installed." } else { "Hook config is not installed." },
                        "hookInstalled": installed,
                        "paths": [format!("~/.ghostex/mock-hooks/{agent_id}.json")],
                        "status": if installed { "installed" } else if cli_installed { "missing" } else { "cliMissing" },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "agents": agents,
        "generatedAt": "2026-05-27T04:17:00.000Z",
        "hookStateDirectory": "~/.ghostexterm",
        "notifyHookPath": "~/.ghostexterm/notify-agent-status.js",
        "type": "agentHookStatus",
    })
}

pub(super) fn answers(message: &Value) -> Vec<Value> {
    if state() == "agents-loading" {
        return Vec::new();
    }
    match message.get("type").and_then(Value::as_str) {
        Some("requestAgentHookStatus") | Some("uninstallAgentHooks") => vec![hook_status(false)],
        Some("installAgentHooks") => vec![hook_status(true)],
        _ => Vec::new(),
    }
}

/// One agent's CLI state in the preview.
fn cli_state(agent_id: &str) -> Value {
    let npm = |package: &str| json!({ "id": "npm", "label": "npm", "command": format!("npm install -g {package}@latest") });
    match agent_id {
        "codex" => json!({
            "agentId": "codex", "platform": "windows",
            "executablePath": "C:\\Users\\you\\AppData\\Roaming\\npm\\codex.cmd",
            "version": "codex-cli 0.50.0", "latestVersion": "0.50.0", "updateAvailable": false,
            "detectedMethodId": "npm",
            "methods": [npm("@openai/codex")],
        }),
        "claude" => json!({
            "agentId": "claude", "platform": "macos",
            "executablePath": "/home/you/.local/share/mise/installs/claude/2.1.267/claude",
            "version": "Claude Code 2.1.267", "latestVersion": "2.1.300", "updateAvailable": true,
            "detectedMethodId": "mise",
            "methods": [{ "id": "mise", "label": "mise", "command": "mise upgrade --bump --no-prune --yes 'claude'" }],
        }),
        "pi" => json!({
            "agentId": "pi", "platform": "linux",
            "executablePath": "/home/you/.local/bin/pi", "version": "0.84.1",
            "methods": [
                npm("--ignore-scripts @earendil-works/pi-coding-agent"),
                { "id": "bun", "label": "bun", "command": "bun install -g @earendil-works/pi-coding-agent@latest" }
            ],
        }),
        "grok" => json!({
            "agentId": "grok", "platform": "macos",
            "executablePath": "/Users/you/.local/bin/grok", "version": "grok 0.2.93",
            "detectedMethodId": "native",
            "methods": [{ "id": "native", "label": "Official installer", "command": "grok update" }],
            "job": {
                "id": "failed", "status": "failed", "operation": "update", "command": "grok update",
                "output": "Checking for updates…\nDownload failed: connection timed out.",
                "error": "Command exited with exit status: 1.",
            },
        }),
        "zcode" => json!({
            "agentId": "zcode", "platform": "macos",
            "methods": [
                { "id": "mise", "label": "mise", "command": "mise use --global --yes 'npm:zcode-app-cli[prerelease=true]@latest'" },
                npm("zcode-app-cli")
            ],
        }),
        "gemini" if state() == "agents-cli-running" => json!({
            "agentId": "gemini", "platform": "windows",
            "methods": [npm("@google/gemini-cli")],
            "job": {
                "id": "gemini-install", "status": "running", "operation": "install",
                "command": "npm install -g @google/gemini-cli@latest",
                "output": "Downloading CLI package…\n\u{1b}[32madded 312 packages\u{1b}[0m in 9s\n",
            },
        }),
        other => json!({
            "agentId": other, "platform": "windows",
            "methods": [npm(other)],
        }),
    }
}

/// A scripted `/api/agentCliMaintenance`.
pub(super) fn rpc(params: &Value) -> Result<Value, String> {
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match params.get("action").and_then(Value::as_str) {
        Some("list") => {
            let agents: Vec<Value> = [
                "codex",
                "claude",
                "cursor",
                "pi",
                "opencode",
                "gemini",
                "copilot",
                "droid",
                "grok",
                "antigravity",
                "amp",
                "hermes-agent",
                "mastra",
                "zcode",
            ]
            .iter()
            .map(|agent_id| cli_state(agent_id))
            .collect();
            Ok(json!({ "agents": agents }))
        }
        Some("read") | Some("addToPath") => Ok(cli_state(agent_id)),
        Some("start") => {
            let mut state = cli_state(agent_id);
            state["job"] = json!({
                "id": format!("job-{agent_id}"),
                "command": "npm install -g",
                "operation": params.get("operation").cloned().unwrap_or(json!("install")),
                "status": "running",
                "output": "Downloading CLI package…\n",
            });
            Ok(state)
        }
        _ => Err("The CLI request failed.".to_string()),
    }
}
