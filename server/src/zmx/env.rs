use std::{collections::HashMap, path::Path};

/// The session variable naming the folder that holds the prompt-editor handshake's marker and answer folders (`prompt-stash-requests/`, `prompt-handoffs/`).
pub(crate) const PROMPT_HANDSHAKE_DIR_ENV: &str = "GHOSTEX_PROMPT_HANDSHAKE_DIR";

/// CDXC:PromptEditor 2026-11-03 WHY:
/// A draft move between the terminal and Chat drops a marker in gxserver's state folder and presses the agent's Ctrl+G; the `ghostex prompt-editor` the agent then runs looked for it in the state folder its own environment resolves. A session shell whose profile sets another XDG_STATE_HOME (or GHOSTEX_HOME) looked in the wrong folder, found nothing, and opened the machine's editor (`vi`) over Claude, which then held every send (GitHub #400). Every session now names gxserver's own state folder, exported after the user's profile ran, and the CLI uses it; a session started before this has no such variable and keeps resolving the folder as before.
/// SEE-ALSO: `prompt_handshake_dir_with` in server/src/ghostex_cli/editors.rs (the reader), server/src/session_chat_send/draft_capture.rs (the writer), `zmx_provider_prompt_editor_setup_shell_command` in scripts.rs and `start` in scripts_windows.rs (the exports).
pub(crate) fn prompt_handshake_dir() -> std::path::PathBuf {
    crate::paths::get_gxserver_paths(None).app_state_dir
}

/// CDXC:AgentHooks 2026-10-17 DECISION:
/// The user chose that "hook state follows each session's own Ghostex (through its session env)". A hook command in the user's agent config names one Ghostex's gxserver and notify script, but every Ghostex on the computer (an installed one and portable copies) runs its sessions under it. Its events already go to the session's own gxserver (`GHOSTEX_GXSERVER_BASE_URL`); its sidecars (Claude's hook sessions, the status line payloads) went to the folder of whichever copy wrote the hook command, so the other copy's sessions lost their transcript links and status lines. Every session now names its own Ghostex's hook state folder, which the notify and status line hooks prefer over the folder their script names.
/// SEE-ALSO: server/src/agent_hooks/windows.rs (`notify`), `run_native_statusline_hook` in server/src/agent_hooks/statusline.rs, the bash notify and status line scripts (plugin_sources.rs, statusline.rs), server/src/agent_hooks/foreign_owner.rs.
pub(crate) fn build_gxserver_zmx_child_environment() -> HashMap<String, String> {
    let mut environment = std::env::vars().collect::<HashMap<_, _>>();
    for key in environment_keys_to_strip() {
        environment.remove(key);
    }
    environment.insert(
        "GHOSTEX_AGENT_HOOK_STATE_DIR".to_string(),
        crate::agent_hooks::config::HookPaths::from_paths(&crate::paths::get_gxserver_paths(None))
            .hook_state_directory
            .to_string_lossy()
            .into_owned(),
    );
    // Claude's task list while the Session details panel asks for it (session_details/gate.rs).
    if let Some((key, value)) = crate::session_details::claude_task_tools_environment(
        &crate::paths::get_gxserver_paths(None),
    ) {
        environment.insert(key.to_string(), value.to_string());
    }
    // On a Cloud Box: the session's injected login and the ticked secret variables.
    environment.extend(crate::cloud_boxes::box_mode::logins::session_environment());
    remove_gxserver_zmx_color_disabling_environment_values(&mut environment);
    environment.insert("COLORTERM".to_string(), "truecolor".to_string());
    environment.insert("TERM_PROGRAM".to_string(), "ghostty".to_string());
    if let Some(resources_dir) = environment
        .get("GHOSTTY_RESOURCES_DIR")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    {
        environment.insert("TERM".to_string(), "xterm-ghostty".to_string());
        if let Some(parent) = Path::new(&resources_dir).parent() {
            environment.insert(
                "TERMINFO".to_string(),
                parent.join("terminfo").to_string_lossy().to_string(),
            );
        }
    } else {
        environment.insert("TERM".to_string(), "xterm-256color".to_string());
    }
    environment
}

pub(crate) fn remove_gxserver_zmx_color_disabling_environment_values(
    environment: &mut HashMap<String, String>,
) {
    /*
    CDXC:ServerDaemon 2026-06-30-22:56:
    Factory-created Droid sessions run inside gxserver-owned zmx provider children and may honor FORCE_COLOR=0 from the Ghostex launch environment. Strip only disabling FORCE_COLOR values here so interactive Ghostty sessions keep color while positive FORCE_COLOR overrides remain intact.
    */
    if environment
        .get("FORCE_COLOR")
        .is_some_and(|value| environment_value_disables_color(value))
    {
        environment.remove("FORCE_COLOR");
    }
}

fn environment_value_disables_color(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false")
}

fn environment_keys_to_strip() -> Vec<&'static str> {
    let mut keys = Vec::new();
    keys.extend([
        "ANSI_COLORS_DISABLED",
        "NO_COLOR",
        "NODE_DISABLE_COLORS",
        "COLORTERM",
        "TERM",
        "TERMINFO",
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        "LaunchInstanceID",
        "XPC_FLAGS",
        "XPC_SERVICE_NAME",
        "__CFBundleIdentifier",
    ]);
    keys.extend(session_identity_environment_keys());
    keys
}

pub(crate) fn session_identity_environment_keys() -> Vec<&'static str> {
    let mut keys = vec![
        "GHOSTEX_AGENT",
        "GHOSTEX_AGENT_HOOK_STATE_DIR",
        "GHOSTEX_GLOBAL_SESSION_REF",
        "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
        "GHOSTEX_GXSERVER_BASE_URL",
        "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
        "GHOSTEX_NATIVE_SESSION_ID",
        "GHOSTEX_SESSION_ID",
        "GHOSTEX_SESSION_STATE_FILE",
        "GHOSTEX_WORKSPACE_ID",
        "GHOSTEX_WORKSPACE_ROOT",
        "VSMUX_AGENT",
        "VSMUX_SESSION_ID",
        "VSMUX_SESSION_STATE_FILE",
        "VSMUX_WORKSPACE_ID",
        "VSMUX_WORKSPACE_ROOT",
        "ZMX_SESSION",
        "ZMX_SESSION_PREFIX",
        "ghostex_AGENT",
        "ghostex_SESSION_ID",
        "ghostex_SESSION_STATE_FILE",
        "ghostex_WORKSPACE_ID",
        "ghostex_WORKSPACE_ROOT",
    ];
    keys.extend(agent_cli_session_identity_environment_keys());
    keys
}

/*
CDXC:SessionIdentity 2026-09-20 WHY:
gxserver is usually started from inside an agent CLI session (`ghostex start` run by Claude Code, for example), so its environment carries that CLI's per-session markers. Every zmx/wmx provider we spawned inherited them, and the agent launched in the new session then believed it was a nested child of the session that had started the daemon: Claude Code reads CLAUDE_CODE_CHILD_SESSION and turns transcript persistence off for the whole session. Strip the agent side of the session identity exactly like we strip our own, so a Ghostex session always starts as a top-level agent run. Only per-session markers belong here; durable configuration such as CLAUDE_CODE_OAUTH_TOKEN or CLAUDE_CODE_USE_BEDROCK must keep flowing through.
Empryo's EMPRYO_PROJECT_DIR is one of those markers (2026-10-07): the notify hook drops every Claude and Codex event that sees it, so a gxserver started with it set would have silenced the hooks of every Claude and Codex session. EMPRYO_HOME is configuration and stays.
*/
pub(crate) fn agent_cli_session_identity_environment_keys() -> Vec<&'static str> {
    vec![
        "AI_AGENT",
        "CLAUDECODE",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_EXECPATH",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_CODE_MESSAGING_TOKEN",
        "CLAUDE_CODE_SESSION_ATTENDED",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_SSE_PORT",
        "CLAUDE_EFFORT",
        "CLAUDE_PID",
        "EMPRYO_PROJECT_DIR",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripped_environment_drops_inherited_agent_cli_session_markers() {
        let stripped = environment_keys_to_strip();
        for key in agent_cli_session_identity_environment_keys() {
            assert!(
                stripped.contains(&key),
                "{key} must be stripped from zmx child environments"
            );
        }
    }

    #[test]
    fn stripped_environment_keeps_durable_agent_configuration() {
        let stripped = environment_keys_to_strip();
        for key in [
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_USE_BEDROCK",
        ] {
            assert!(
                !stripped.contains(&key),
                "{key} is durable configuration and must reach the session"
            );
        }
    }

    #[test]
    fn session_identity_reset_covers_agent_cli_markers() {
        let keys = session_identity_environment_keys();
        assert!(keys.contains(&"GHOSTEX_SESSION_ID"));
        assert!(keys.contains(&"CLAUDE_CODE_CHILD_SESSION"));
    }
}
