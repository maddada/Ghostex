#![allow(dead_code)]

use std::{env, ffi::OsStr, process::Command};

use portable_pty::CommandBuilder;

const COLOR_DISABLING_ENVIRONMENT_KEYS: [&str; 3] =
    ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"];
const CONDITIONALLY_COLOR_DISABLING_ENVIRONMENT_KEYS: [&str; 1] = ["FORCE_COLOR"];
const SESSION_IDENTITY_ENVIRONMENT_KEYS: [&str; 22] = [
    "GHOSTEX_AGENT",
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
const CLICOLOR_KEY: &str = "CLICOLOR";

fn environment_key_matches(key: &str, expected: &str) -> bool {
    if cfg!(windows) {
        key.eq_ignore_ascii_case(expected)
    } else {
        key == expected
    }
}

fn environment_os_key_matches(key: &OsStr, expected: &str) -> bool {
    key.to_str()
        .is_some_and(|key| environment_key_matches(key, expected))
}

fn environment_value_disables_color(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized == "0" || normalized == "false"
}

fn environment_os_value_disables_color(value: &OsStr) -> bool {
    environment_value_disables_color(&value.to_string_lossy())
}

fn is_color_disabling_environment_key(key: &str) -> bool {
    COLOR_DISABLING_ENVIRONMENT_KEYS
        .iter()
        .any(|candidate| environment_key_matches(key, candidate))
}

fn is_conditionally_color_disabling_environment_key(key: &str) -> bool {
    CONDITIONALLY_COLOR_DISABLING_ENVIRONMENT_KEYS
        .iter()
        .any(|candidate| environment_key_matches(key, candidate))
}

fn environment_pair_disables_color(key: &str, value: &str) -> bool {
    is_color_disabling_environment_key(key)
        || (is_conditionally_color_disabling_environment_key(key)
            && environment_value_disables_color(value))
}

pub(crate) unsafe fn remove_color_disabling_from_current_process() {
    /*
    CDXC:GPUIProcessColorEnv 2026-07-04:
    GPUI can be launched from agent terminals that export NO_COLOR, but the app
    is a color-capable host for gxserver, zmx, provider CLIs, GhosttyKit, and
    the GPUI PTY terminal engine. Strip inherited color-disabling keys at
    process start so later process-environment snapshots are color-capable.
    Keep positive FORCE_COLOR overrides and remove only disabling values.
    */
    for key in COLOR_DISABLING_ENVIRONMENT_KEYS {
        unsafe { env::remove_var(key) };
    }
    for key in CONDITIONALLY_COLOR_DISABLING_ENVIRONMENT_KEYS {
        if env::var_os(key)
            .as_deref()
            .is_some_and(environment_os_value_disables_color)
        {
            unsafe { env::remove_var(key) };
        }
    }
}

pub(crate) unsafe fn remove_session_identity_from_current_process() {
    /*
    CDXC:GPUISessionIdentityEnv 2026-07-04:
    GPUI can itself be launched from a Ghostex pane. GhosttyKit snapshots the
    host process environment when creating embedded surfaces, so remove stale
    pane/session identity keys at startup and let explicit terminal launch
    config add the current session state later.
    */
    for key in SESSION_IDENTITY_ENVIRONMENT_KEYS {
        unsafe { env::remove_var(key) };
    }
}

pub(crate) fn color_capable_terminal_env_vars(
    env_vars: Vec<(String, String)>,
) -> Vec<(String, String)> {
    /*
    CDXC:GPUITerminalColorEnv 2026-07-04:
    GPUI terminal launch payloads use the same color policy as the macOS
    Ghostty boundary: remove NO_COLOR-style blockers, remove only disabling
    FORCE_COLOR values, preserve positive FORCE_COLOR overrides, and set the
    non-forcing CLICOLOR opt-in for interactive PTYs.
    */
    let mut sanitized = Vec::with_capacity(env_vars.len() + 1);
    for (key, value) in env_vars {
        if environment_pair_disables_color(&key, &value)
            || environment_key_matches(&key, CLICOLOR_KEY)
        {
            continue;
        }
        sanitized.push((key, value));
    }
    sanitized.push((CLICOLOR_KEY.to_string(), "1".to_string()));
    sanitized
}

pub(crate) fn apply_color_capable_terminal_command_builder(command: &mut CommandBuilder) {
    for key in COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
    for key in CONDITIONALLY_COLOR_DISABLING_ENVIRONMENT_KEYS {
        if command
            .get_env(key)
            .is_some_and(environment_os_value_disables_color)
        {
            command.env_remove(key);
        }
    }
    command.env(CLICOLOR_KEY, "1");
}

pub(crate) fn remove_session_identity_from_terminal_command_builder(command: &mut CommandBuilder) {
    /*
    CDXC:GPUITerminalSessionIdentityEnv 2026-07-04:
    portable_pty::CommandBuilder starts with a copy of the process base
    environment. Strip stale inherited Ghostex/zmx identity before callers add
    their explicit per-terminal env overlay, matching macOS Ghostty creation.
    */
    for key in SESSION_IDENTITY_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
}

pub(crate) fn remove_session_identity_from_process_command(command: &mut Command) {
    /*
    CDXC:GPUIGxserverSessionIdentityEnv 2026-07-04:
    GPUI-spawned daemon/helper processes must not inherit the pane identity of
    the terminal that launched the app; gxserver provider scripts export the
    active session identity explicitly when needed.
    */
    for key in SESSION_IDENTITY_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
}

pub(crate) fn apply_color_capable_process_command(command: &mut Command) {
    /*
    CDXC:GPUIGxserverColorEnv 2026-07-04:
    GPUI-spawned helper processes that can own future terminal/provider
    launches must not inherit NO_COLOR-style blockers from the app process.
    This mirrors the macOS gxserver bootstrap boundary while preserving
    positive FORCE_COLOR overrides.
    */
    for key in COLOR_DISABLING_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
    for key in CONDITIONALLY_COLOR_DISABLING_ENVIRONMENT_KEYS {
        let explicit_value = command
            .get_envs()
            .find(|(candidate, _)| environment_os_key_matches(candidate, key))
            .map(|(_, value)| value.map(OsStr::to_os_string));
        let should_remove = match explicit_value {
            Some(Some(value)) => environment_os_value_disables_color(&value),
            Some(None) => false,
            None => env::var_os(key)
                .as_deref()
                .is_some_and(environment_os_value_disables_color),
        };
        if should_remove {
            command.env_remove(key);
        }
    }
}
