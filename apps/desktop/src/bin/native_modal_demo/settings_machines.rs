//! Machine picker preview (Settings > Agents and Integrations): three saved machines (Kubuntu VM
//! connected, M5 whose SSH failed, Old box turned off) and a scripted remote gxserver for Kubuntu VM.
//!
//! States: `machines-agents` (Agents, Kubuntu VM picked), `machines-agents-open` (Agents on this
//! computer with the picker open), `machines-integrations` (Integrations, Kubuntu VM picked),
//! `machines-dictation` and `machines-dictation-openai` (Dictation, Kubuntu VM picked, its Hex not running
//! or its engine OpenAI without a key), `machines-dictation-open` (Dictation on this computer with
//! the picker open: only Kubuntu VM is connected, so M5 and Old box are left out),
//! `machines-old` (Agents, Kubuntu VM picked, its gxserver too old for the requests),
//! `machines-windows` and `machines-windows-integrations` (Agents and Integrations, Kubuntu VM
//! picked and answering as Windows: the Developer Mode notice; `GHOSTEX_NATIVE_MODAL_DEMO_SETTINGS=
//! '{"settingsModalNavigation":{"windowsLinkNoticeCollapsed":true}}'` shows it folded).
//! `machines-accounts` (Accounts, Kubuntu VM picked, as a remote session's Reconnect opens it) and
//! `machines-accounts-unreachable` (Accounts opened on M5, whose SSH failed: the can't-reach line)
//! answer Accounts from two real test gxservers instead of a script: `/api/agentAccounts` on this
//! computer goes to the one `GHOSTEX_NATIVE_MODAL_DEMO_LOCAL_ENV` names, and Kubuntu VM's to the one
//! `GHOSTEX_NATIVE_MODAL_DEMO_REMOTE_ENV` names (each a `cargo xtask test-server` env.sh), through
//! `ghostex session-chat-rpc agentAccounts`.
use serde_json::{Map, Value, json};

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("machines-")
}

pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    if !owns(state) {
        return;
    }
    settings.insert(
        "remoteMachines".into(),
        json!([
            { "id": "remote-1", "name": "Kubuntu VM", "sshHost": "100.98.229.105", "sshUser": "madda" },
            { "id": "remote-2", "name": "M5", "sshHost": "100.77.81.4", "sshUser": "madda" },
            { "id": "remote-3", "name": "Old box", "sshHost": "10.0.0.9", "sshUser": "madda", "disabled": true }
        ]),
    );
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    Some(match state {
        "machines-agents" | "machines-old" | "machines-windows" => {
            json!({ "initialTab": "agents", "initialMachineId": "remote-1" })
        }
        "machines-agents-open" => json!({ "initialTab": "agents" }),
        "machines-dictation-open" => json!({ "initialTab": "dictation" }),
        "machines-dictation" | "machines-dictation-openai" => {
            json!({ "initialTab": "dictation", "initialMachineId": "remote-1" })
        }
        "machines-accounts" => json!({ "initialTab": "accounts", "initialMachineId": "remote-1" }),
        "machines-accounts-unreachable" => {
            json!({ "initialTab": "accounts", "initialMachineId": "remote-2" })
        }
        "machines-integrations" | "machines-windows-integrations" => {
            json!({ "initialTab": "integrations", "initialMachineId": "remote-1" })
        }
        _ => return None,
    })
}

/// The app's answer to the picker asking for connection states.
pub(super) fn answers(message: &Value) -> Vec<Value> {
    if message.get("type").and_then(Value::as_str) != Some("requestRemoteMachineConnectStates") {
        return Vec::new();
    }
    vec![json!({
        "type": "remoteMachineConnectStates",
        "states": { "remote-1": "connected", "remote-2": "sshFailed" },
    })]
}

/// Kubuntu VM's scripted gxserver; the `machines-windows*` states answer as a Windows machine.
pub(super) fn remote_rpc(state: &str, path: &str, params: &Value) -> Result<Value, String> {
    if state.starts_with("machines-dictation") {
        let story = if state == "machines-dictation-openai" {
            "dictation-openai"
        } else {
            "dictation-hex-stopped"
        };
        if let Some(result) = super::settings_voice::rpc(story, path, params) {
            return result;
        }
    }
    let answer = linux_rpc(state, path, params)?;
    if !state.starts_with("machines-windows") {
        return Ok(answer);
    }
    serde_json::from_str(
        &answer
            .to_string()
            .replace("\"platform\":\"linux\"", "\"platform\":\"windows\""),
    )
    .map_err(|error| error.to_string())
}

fn linux_rpc(state: &str, path: &str, params: &Value) -> Result<Value, String> {
    if state == "machines-old" {
        return Err("Kubuntu VM runs an older Ghostex that can't do this yet. Update Ghostex on Kubuntu VM (Settings > Remote), then try again.".to_string());
    }
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match (path, action) {
        ("/api/agentCliMaintenance", "list") => Ok(json!({ "agents": [
            { "agentId": "codex", "platform": "linux", "executablePath": "/home/madda/.local/share/mise/shims/codex", "version": "0.40.0", "latestVersion": "0.41.0", "updateAvailable": true, "detectedMethodId": "mise", "methods": [{ "id": "mise", "label": "mise", "command": "mise use -g npm:@openai/codex@latest" }] },
            { "agentId": "claude", "platform": "linux", "executablePath": "/home/madda/.local/bin/claude", "version": "2.1.309", "detectedMethodId": "native", "methods": [{ "id": "native", "label": "Official installer", "command": "curl -fsSL https://claude.ai/install.sh | bash" }] },
            { "agentId": "qoder", "platform": "linux", "methods": [{ "id": "npm", "label": "npm", "command": "npm install -g @qoder-ai/qodercli", "prerequisite": "node" }] },
            { "agentId": "opencode", "platform": "linux", "methods": [{ "id": "script", "label": "Official installer", "command": "curl -fsSL https://opencode.ai/install | bash" }] }
        ] })),
        ("/api/agentCliMaintenance", _) => Ok(json!({
            "platform": "linux",
            "executablePath": "/home/madda/.local/share/mise/shims/codex",
            "version": "0.40.0",
            "latestVersion": "0.41.0",
            "updateAvailable": true,
            "detectedMethodId": "mise",
            "methods": [{ "id": "mise", "label": "mise", "command": "mise use -g npm:@openai/codex@latest" }],
        })),
        ("/api/managedTools", _) => Ok(json!({ "tools": [
            { "id": "node", "supported": true, "label": "Node.js and npm", "installed": true, "version": "22.11.0", "managed": true, "actions": ["update", "reinstall", "uninstall"], "updateAvailable": false },
            { "id": "uv", "supported": true, "label": "uv", "installed": false, "actions": ["install"], "installPlan": "Installs uv with its official installer." },
            { "id": "gh", "supported": true, "label": "GitHub CLI", "installed": true, "version": "2.60.0", "managed": true, "actions": ["update", "reinstall", "uninstall"], "updateAvailable": true, "latestVersion": "2.62.0" },
            { "id": "glab", "supported": true, "label": "GitLab CLI", "installed": true, "source": "system", "actions": [], "notAnswering": "`glab --version` didn't answer within 5 seconds, so Ghostex stopped it." },
            { "id": "trycua", "label": "Fast Computer & Browser Use", "supported": true, "installed": true, "version": "0.6.2", "source": "system", "actions": ["update", "reinstall", "uninstall"], "updateAvailable": false, "permissionNote": "Installed, but it can't control that Mac yet. Only someone at that Mac can allow Accessibility and Screen Recording: in Ghostex's Settings > Integrations there, or System Settings > Privacy & Security." },
            { "id": "spaceo", "label": "SpaceO", "supported": true, "installed": false, "actions": ["install"], "installPlan": "Runs SpaceO's official installer from GitHub in the background." }
        ] })),
        ("/api/machineIntegrations", _) => Ok(json!({
            "platform": "linux",
            "ghostexCli": { "installed": true, "path": "/home/madda/.local/bin/ghostex" },
            "skills": [
                { "skillName": "ghostex-cli", "installed": true, "unavailableReason": null },
                { "skillName": "ghostex-help", "installed": true, "unavailableReason": null },
                { "skillName": "ghostex-agents", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-browser-use", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-computer-use", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-embedded-browser-use", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-manage-beads", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-auto-rename-session", "installed": true, "unavailableReason": null },
                { "skillName": "ghostex-move-codex-session", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-visuals", "installed": false, "unavailableReason": null },
                { "skillName": "ghostex-spaceo", "installed": false, "unavailableReason": "SpaceO is only available on macOS." }
            ],
        })),
        // The Session resume hooks card (tabs/agents/remote_hooks.rs): Claude's hook is missing
        // until Install all, which installs it.
        ("/api/readAgentHookStatus" | "/api/installAgentHooks", _) => {
            let claude = if path == "/api/installAgentHooks" {
                "installed"
            } else {
                "missing"
            };
            let row = |agent: &str, status: &str| json!({ "agentId": agent, "status": status, "hookInstalled": status == "installed" });
            Ok(json!({
                "agents": [
                    row("codex", "installed"),
                    row("claude", claude),
                    row("qoder", "cliMissing"),
                    row("opencode", "cliMissing"),
                ],
                "hookStateDirectory": "/home/madda/.local/state/ghostex/agent-hooks",
                "type": "agentHookStatus",
            }))
        }
        _ => Err(format!(
            "Kubuntu VM's story gxserver has no handler for {path}."
        )),
    }
}

/// Whether `state` answers Accounts from the test gxservers (see the header).
pub(super) fn forwards_accounts(state: &str, path: &str) -> bool {
    state.starts_with("machines-accounts") && path == "/api/agentAccounts"
}

/// `/api/agentAccounts` on the test gxserver whose env.sh `env_var` names: this computer's
/// (`remote: false`) or Kubuntu VM's. Runs `ghostex session-chat-rpc` there; blocks, so call it off
/// the main thread.
pub(super) fn forward_accounts(remote: bool, params: &Value) -> Result<Value, String> {
    use base64::Engine as _;
    let env_var = if remote {
        "GHOSTEX_NATIVE_MODAL_DEMO_REMOTE_ENV"
    } else {
        "GHOSTEX_NATIVE_MODAL_DEMO_LOCAL_ENV"
    };
    let env_file =
        std::env::var(env_var).map_err(|_| format!("Set {env_var} to a test server's env.sh."))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(params.to_string());
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#". "$1" && exec ghostex session-chat-rpc agentAccounts --params-base64 "$2" --timeout 120000"#)
        .arg("sh")
        .arg(&env_file)
        .arg(&encoded)
        .output()
        .map_err(|error| error.to_string())?;
    let answer: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| String::from_utf8_lossy(&output.stderr).trim().to_string())?;
    if answer["ok"] == true {
        Ok(answer["result"].clone())
    } else {
        Err(answer["message"]
            .as_str()
            .unwrap_or("The test gxserver refused the request.")
            .to_string())
    }
}
