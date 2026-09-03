use std::{collections::HashMap, path::Path};

pub(crate) fn build_gxserver_zmx_child_environment() -> HashMap<String, String> {
    let mut environment = std::env::vars().collect::<HashMap<_, _>>();
    for key in environment_keys_to_strip() {
        environment.remove(key);
    }
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
    vec![
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
    ]
}
