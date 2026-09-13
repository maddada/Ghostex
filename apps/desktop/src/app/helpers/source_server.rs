mod code_server;
pub(crate) use code_server::*;

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;
use futures::{StreamExt as _, channel::mpsc};
use gpui::{AppContext as _, Asset, ParentElement as _, prelude::FluentBuilder as _};

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiPortlessProtocol {
    Https,
    Http,
}

impl GpuiPortlessProtocol {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiPortlessAdminAction {
    Install,
    Reconfigure,
    Retry,
    Remove,
}

impl GpuiPortlessAdminAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Reconfigure => "reconfigure",
            Self::Retry => "retry",
            Self::Remove => "remove",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiPortlessStateUpdate {
    SetEnabled {
        enabled: bool,
    },
    SetProtocol {
        protocol: GpuiPortlessProtocol,
    },
    RecordAdminResult {
        action: GpuiPortlessAdminAction,
        ok: bool,
        protocol: Option<GpuiPortlessProtocol>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiPortlessSetupPromptMode {
    FirstSetup,
    StandaloneReconfigure,
}

impl GpuiPortlessSetupPromptMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FirstSetup => "firstSetup",
            Self::StandaloneReconfigure => "standaloneReconfigure",
        }
    }
}

/// Mirrors macOS `resolvePortlessSetupPrompt` (native-sidebar.tsx): the guided
/// setup prompt appears only when the global setting and gxserver status say
/// Portless is enabled, this app can run the privileged admin helper, a live
/// listener exists, and setup is missing or needs reconfigure/takeover.
pub(crate) fn gpui_resolve_portless_setup_prompt(
    settings_portless_enabled: bool,
    portless_state: &serde_json::Value,
) -> Option<(GpuiPortlessSetupPromptMode, GpuiPortlessProtocol)> {
    let health = portless_state.get("health")?;
    if !settings_portless_enabled
        || health.get("enabled").and_then(serde_json::Value::as_bool) != Some(true)
    {
        return None;
    }
    if portless_state
        .get("nativeAdmin")
        .and_then(|admin| admin.get("available"))
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return None;
    }
    if portless_state
        .get("presentation")
        .and_then(|presentation| presentation.get("liveListenerCount"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        == 0
    {
        return None;
    }
    if health
        .get("setupStatus")
        .and_then(serde_json::Value::as_str)
        != Some("needed")
    {
        return None;
    }
    let protocol = health
        .get("protocol")
        .and_then(serde_json::Value::as_str)
        .and_then(gpui_portless_protocol)?;
    match health
        .get("setupOwnership")
        .and_then(serde_json::Value::as_str)
    {
        Some("missing") => Some((GpuiPortlessSetupPromptMode::FirstSetup, protocol)),
        Some("standalone") | Some("ghostex") => {
            Some((GpuiPortlessSetupPromptMode::StandaloneReconfigure, protocol))
        }
        _ => None,
    }
}

impl GpuiPortlessStateUpdate {
    pub(crate) fn to_rpc_params(self) -> serde_json::Value {
        /*
        CDXC:Portless 2026-06-24-11:48:
        `/api/updatePortlessState` accepts only the shared Portless state-update contract: `setEnabled`, `setProtocol`, or `recordAdminResult` with booleans and bounded action/protocol enums. Keep request construction centralized here so GPUI never forwards request ids, paths, URLs, commands, output, environment, tokens, or arbitrary React payload fields to gxserver.
        */
        match self {
            Self::SetEnabled { enabled } => serde_json::json!({
                "enabled": enabled,
                "kind": "setEnabled",
            }),
            Self::SetProtocol { protocol } => serde_json::json!({
                "kind": "setProtocol",
                "protocol": protocol.as_str(),
            }),
            Self::RecordAdminResult {
                action,
                ok,
                protocol,
            } => {
                let mut params = serde_json::Map::new();
                params.insert(
                    "action".to_string(),
                    serde_json::Value::String(action.as_str().to_string()),
                );
                params.insert(
                    "kind".to_string(),
                    serde_json::Value::String("recordAdminResult".to_string()),
                );
                params.insert("ok".to_string(), serde_json::Value::Bool(ok));
                if let Some(protocol) = protocol {
                    params.insert(
                        "protocol".to_string(),
                        serde_json::Value::String(protocol.as_str().to_string()),
                    );
                }
                serde_json::Value::Object(params)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiPortlessAdminResult {
    pub(crate) request_id: String,
    pub(crate) action: GpuiPortlessAdminAction,
    pub(crate) protocol: Option<GpuiPortlessProtocol>,
    pub(crate) ok: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) status: &'static str,
    pub(crate) error_code: Option<&'static str>,
}

impl GpuiPortlessAdminResult {
    pub(crate) fn message(&self) -> serde_json::Value {
        let mut result = serde_json::Map::new();
        result.insert(
            "action".to_string(),
            serde_json::Value::String(self.action.as_str().to_string()),
        );
        if let Some(error_code) = self.error_code {
            result.insert(
                "errorCode".to_string(),
                serde_json::Value::String(error_code.to_string()),
            );
        }
        if let Some(exit_code) = self.exit_code {
            result.insert(
                "exitCode".to_string(),
                serde_json::Value::Number(serde_json::Number::from(i64::from(exit_code))),
            );
        }
        result.insert("ok".to_string(), serde_json::Value::Bool(self.ok));
        if let Some(protocol) = self.protocol {
            result.insert(
                "protocol".to_string(),
                serde_json::Value::String(protocol.as_str().to_string()),
            );
        }
        result.insert(
            "requestId".to_string(),
            serde_json::Value::String(self.request_id.clone()),
        );
        result.insert(
            "status".to_string(),
            serde_json::Value::String(self.status.to_string()),
        );
        result.insert(
            "type".to_string(),
            serde_json::Value::String("portlessAdminResult".to_string()),
        );
        serde_json::Value::Object(result)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiPortlessAdminRuntime {
    pub(crate) node_path: PathBuf,
    pub(crate) portless_cli_path: PathBuf,
}

pub(crate) fn gpui_portless_admin_request_id(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        })
        .map(ToOwned::to_owned)
}

pub(crate) fn gpui_run_portless_admin_action(
    action: GpuiPortlessAdminAction,
    protocol: Option<GpuiPortlessProtocol>,
    request_id: String,
) -> GpuiPortlessAdminResult {
    let service_protocol = match action {
        GpuiPortlessAdminAction::Install
        | GpuiPortlessAdminAction::Reconfigure
        | GpuiPortlessAdminAction::Retry => protocol,
        GpuiPortlessAdminAction::Remove => Some(protocol.unwrap_or(GpuiPortlessProtocol::Https)),
    };
    let result_protocol = if action == GpuiPortlessAdminAction::Remove {
        None
    } else {
        service_protocol
    };
    let Some(service_protocol) = service_protocol else {
        return gpui_portless_admin_result(
            request_id,
            action,
            None,
            false,
            None,
            "missing-protocol",
            Some("missing-protocol"),
        );
    };
    let Some(runtime) = gpui_resolve_portless_admin_runtime() else {
        return gpui_portless_admin_result(
            request_id,
            action,
            result_protocol,
            false,
            None,
            "missing-bundled-portless-runtime",
            Some("missing-bundled-portless-runtime"),
        );
    };
    gpui_run_portless_admin_action_with_runtime(
        action,
        service_protocol,
        result_protocol,
        request_id,
        runtime,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_portless_admin_action_with_runtime(
    action: GpuiPortlessAdminAction,
    service_protocol: GpuiPortlessProtocol,
    result_protocol: Option<GpuiPortlessProtocol>,
    request_id: String,
    runtime: GpuiPortlessAdminRuntime,
) -> GpuiPortlessAdminResult {
    let Some(script_path) = gpui_write_portless_admin_script(action, service_protocol, &runtime)
    else {
        return gpui_portless_admin_result(
            request_id,
            action,
            result_protocol,
            false,
            None,
            "admin-script-unavailable",
            Some("admin-script-unavailable"),
        );
    };
    let script_result = gpui_run_privileged_portless_admin_script(&script_path);
    let _ = fs::remove_file(&script_path);
    gpui_portless_admin_result(
        request_id,
        action,
        result_protocol,
        script_result.ok,
        Some(script_result.exit_code),
        script_result.status,
        (!script_result.ok).then_some(script_result.status),
    )
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_run_portless_admin_action_with_runtime(
    action: GpuiPortlessAdminAction,
    _service_protocol: GpuiPortlessProtocol,
    result_protocol: Option<GpuiPortlessProtocol>,
    request_id: String,
    _runtime: GpuiPortlessAdminRuntime,
) -> GpuiPortlessAdminResult {
    gpui_portless_admin_result(
        request_id,
        action,
        result_protocol,
        false,
        None,
        "missing-bundled-portless-runtime",
        Some("missing-bundled-portless-runtime"),
    )
}

pub(crate) fn gpui_portless_admin_result(
    request_id: String,
    action: GpuiPortlessAdminAction,
    protocol: Option<GpuiPortlessProtocol>,
    ok: bool,
    exit_code: Option<i32>,
    status: &'static str,
    error_code: Option<&'static str>,
) -> GpuiPortlessAdminResult {
    GpuiPortlessAdminResult {
        request_id,
        action,
        protocol,
        ok,
        exit_code,
        status,
        error_code,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_resolve_portless_admin_runtime() -> Option<GpuiPortlessAdminRuntime> {
    /*
    CDXC:Portless 2026-06-24-14:28:
    The GPUI privileged Portless helper is production-capable only from a packaged macOS app with Contents/Resources/Web/code-server/lib/node and Contents/Resources/Web/portless/dist/cli.js. Development binaries and incomplete bundles must report `missing-bundled-portless-runtime` rather than using PATH, global npm, server, or repository-local commands.

    Portless integration is currently disabled. If it returns, its Node runtime must resolve from the installed code-server component instead of restoring a Node binary to the base app bundle.
    */
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    let web_root = bundle_root.join("Contents/Resources/Web");
    let node_path = web_root.join("code-server/lib/node");
    let portless_cli_path = web_root.join("portless/dist/cli.js");
    if !gpui_is_executable_file(&node_path) || !gpui_is_file(&portless_cli_path) {
        return None;
    }
    Some(GpuiPortlessAdminRuntime {
        node_path,
        portless_cli_path,
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_resolve_portless_admin_runtime() -> Option<GpuiPortlessAdminRuntime> {
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_write_portless_admin_script(
    action: GpuiPortlessAdminAction,
    service_protocol: GpuiPortlessProtocol,
    runtime: &GpuiPortlessAdminRuntime,
) -> Option<PathBuf> {
    let home_directory = env::var("HOME").ok().filter(|value| !value.is_empty())?;
    let user_name = gpui_portless_current_user_field("-un")?;
    let user_id = gpui_portless_current_user_field("-u")?;
    let group_id = gpui_portless_current_user_field("-g")?;
    let storage_paths = shared_settings::ghostex_storage_paths();
    let ghostex_state_directory = gpui_path_string(&storage_paths.state_dir);
    let gxserver_directory = gpui_path_string(&storage_paths.gxserver_state_dir());
    let portless_state_directory =
        gpui_path_string(&storage_paths.gxserver_state_dir().join("portless"));
    let admin_commands = gpui_portless_admin_commands(
        action,
        service_protocol,
        runtime,
        &home_directory,
        &portless_state_directory,
        &user_id,
        &group_id,
    );
    let script = format!(
        r#"#!/bin/sh
set -eu

USER_HOME={home}
USER_NAME={user_name}
USER_UID={user_id}
USER_GID={group_id}
HOME="$USER_HOME"
GHOSTEX_STATE_DIR={ghostex_state_directory}
GXSERVER_DIR={gxserver_directory}
PORTLESS_STATE_DIR={portless_state_directory}
NODE_PATH={node_path}
PORTLESS_CLI_PATH={portless_cli_path}
SERVICE_LABEL="sh.portless.proxy"
PLIST_PATH="/Library/LaunchDaemons/$SERVICE_LABEL.plist"

if [ "$(/usr/bin/id -u)" -ne 0 ]; then
  exit 91
fi

/bin/mkdir -p "$PORTLESS_STATE_DIR"
/usr/sbin/chown "$USER_UID:$USER_GID" "$GHOSTEX_STATE_DIR" "$GXSERVER_DIR" "$PORTLESS_STATE_DIR" 2>/dev/null || true
/usr/sbin/chown -R "$USER_UID:$USER_GID" "$PORTLESS_STATE_DIR" 2>/dev/null || true

run_portless_cli() {{
  /usr/bin/env -i \
    HOME="$HOME" \
    USER="$USER_NAME" \
    LOGNAME="$USER_NAME" \
    SUDO_USER="$USER_NAME" \
    SUDO_UID="$USER_UID" \
    SUDO_GID="$USER_GID" \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    PORTLESS_STATE_DIR="$PORTLESS_STATE_DIR" \
    PORTLESS_SYNC_HOSTS=0 \
    "$NODE_PATH" "$PORTLESS_CLI_PATH" "$@"
}}

{admin_commands}
"#,
        home = gpui_shell_quote(&home_directory),
        user_name = gpui_shell_quote(&user_name),
        user_id = gpui_shell_quote(&user_id),
        group_id = gpui_shell_quote(&group_id),
        ghostex_state_directory = gpui_shell_quote(&ghostex_state_directory),
        gxserver_directory = gpui_shell_quote(&gxserver_directory),
        portless_state_directory = gpui_shell_quote(&portless_state_directory),
        node_path = gpui_shell_quote(&gpui_path_string(&runtime.node_path)),
        portless_cli_path = gpui_shell_quote(&gpui_path_string(&runtime.portless_cli_path)),
    );
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let script_path = env::temp_dir().join(format!(
        "ghostex-portless-admin-{}-{unique_suffix}.sh",
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&script_path)
        .ok()?;
    if file.write_all(script.as_bytes()).is_err() {
        let _ = fs::remove_file(&script_path);
        return None;
    }
    if fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700)).is_err() {
        let _ = fs::remove_file(&script_path);
        return None;
    }
    Some(script_path)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_portless_admin_commands(
    action: GpuiPortlessAdminAction,
    service_protocol: GpuiPortlessProtocol,
    runtime: &GpuiPortlessAdminRuntime,
    home_directory: &str,
    portless_state_directory: &str,
    user_id: &str,
    group_id: &str,
) -> String {
    match action {
        GpuiPortlessAdminAction::Install
        | GpuiPortlessAdminAction::Reconfigure
        | GpuiPortlessAdminAction::Retry => {
            let plist = gpui_portless_launchd_plist(
                service_protocol,
                runtime,
                home_directory,
                portless_state_directory,
                user_id,
                group_id,
            );
            let (proxy_port, trust_command) = match service_protocol {
                GpuiPortlessProtocol::Https => {
                    ("443", "run_portless_cli trust >/dev/null 2>&1 || true\n")
                }
                GpuiPortlessProtocol::Http => ("80", ""),
            };
            format!(
                r#"# CDXC:Portless 2026-06-24-14:28:
# GPUI installs the Portless macOS LaunchDaemon with the same fixed root commands as the reviewed Swift helper. The daemon writes stdout/stderr to /dev/null and runs with PORTLESS_SYNC_HOSTS=0 so support bundles do not persist paths, hostnames, command/env values, or proxy output.
{trust_command}/bin/launchctl bootout system "$PLIST_PATH" >/dev/null 2>&1 || true
run_portless_cli proxy stop --port {proxy_port} >/dev/null 2>&1 || true
/bin/cat > "$PLIST_PATH" <<'EOF_PLIST'
{plist}
EOF_PLIST
/usr/sbin/chown root:wheel "$PLIST_PATH"
/bin/chmod 644 "$PLIST_PATH"
/bin/launchctl bootstrap system "$PLIST_PATH"
/bin/launchctl enable "system/$SERVICE_LABEL"
/bin/launchctl kickstart -k "system/$SERVICE_LABEL"
"#
            )
        }
        GpuiPortlessAdminAction::Remove => {
            r#"/bin/launchctl bootout system "$PLIST_PATH" >/dev/null 2>&1 || true
/bin/rm -f "$PLIST_PATH"
"#
            .to_string()
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_portless_launchd_plist(
    service_protocol: GpuiPortlessProtocol,
    runtime: &GpuiPortlessAdminRuntime,
    home_directory: &str,
    portless_state_directory: &str,
    user_id: &str,
    group_id: &str,
) -> String {
    let (port, https, protocol_flag) = match service_protocol {
        GpuiPortlessProtocol::Https => ("443", "1", "--https"),
        GpuiPortlessProtocol::Http => ("80", "0", "--no-tls"),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>sh.portless.proxy</string>
  <key>ProgramArguments</key>
  <array>
    <string>{node_path}</string>
    <string>{portless_cli_path}</string>
    <string>proxy</string>
    <string>start</string>
    <string>--foreground</string>
    <string>--port</string>
    <string>{port}</string>
    <string>{protocol_flag}</string>
    <string>--tld</string>
    <string>localhost</string>
    <string>--skip-trust</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HOME</key>
    <string>{home}</string>
    <key>SUDO_UID</key>
    <string>{user_id}</string>
    <key>SUDO_GID</key>
    <string>{group_id}</string>
    <key>PORTLESS_STATE_DIR</key>
    <string>{state_dir}</string>
    <key>PORTLESS_PORT</key>
    <string>{port}</string>
    <key>PORTLESS_HTTPS</key>
    <string>{https}</string>
    <key>PORTLESS_TLD</key>
    <string>localhost</string>
    <key>PORTLESS_LAN</key>
    <string>0</string>
    <key>PORTLESS_WILDCARD</key>
    <string>0</string>
    <key>PORTLESS_SYNC_HOSTS</key>
    <string>0</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/dev/null</string>
  <key>StandardErrorPath</key>
  <string>/dev/null</string>
</dict>
</plist>"#,
        node_path = gpui_xml_escape(&gpui_path_string(&runtime.node_path)),
        portless_cli_path = gpui_xml_escape(&gpui_path_string(&runtime.portless_cli_path)),
        home = gpui_xml_escape(home_directory),
        user_id = gpui_xml_escape(user_id),
        group_id = gpui_xml_escape(group_id),
        state_dir = gpui_xml_escape(portless_state_directory),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_privileged_portless_admin_script(
    script_path: &Path,
) -> GpuiPrivilegedScriptResult {
    let command = format!(
        "/bin/sh {}",
        gpui_shell_quote(&gpui_path_string(script_path))
    );
    let apple_script = format!(
        "do shell script {} with administrator privileges",
        gpui_applescript_string(&command)
    );
    let output = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(apple_script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    let Ok(output) = output else {
        return GpuiPrivilegedScriptResult {
            ok: false,
            exit_code: 127,
            status: "admin-launch-failed",
        };
    };
    let exit_code = output.status.code().unwrap_or(127);
    if output.status.success() {
        return GpuiPrivilegedScriptResult {
            ok: true,
            exit_code,
            status: "completed",
        };
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = stdout
        .chars()
        .chain(stderr.chars())
        .take(4096)
        .collect::<String>();
    GpuiPrivilegedScriptResult {
        ok: false,
        exit_code,
        status: gpui_portless_admin_failure_status(exit_code, &combined_output),
    }
}

pub(crate) struct GpuiPrivilegedScriptResult {
    pub(crate) ok: bool,
    pub(crate) exit_code: i32,
    pub(crate) status: &'static str,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_portless_admin_failure_status(exit_code: i32, output: &str) -> &'static str {
    let normalized_output = output.to_lowercase();
    if normalized_output.contains("user canceled") || normalized_output.contains("user cancelled") {
        return "authorization-cancelled";
    }
    if exit_code == 91 {
        return "admin-required";
    }
    "portless-cli-failed"
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_portless_current_user_field(argument: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/id")
        .arg(argument)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_applescript_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn gpui_portless_state_with_admin_result(
    mut portless_state: serde_json::Value,
    result: &GpuiPortlessAdminResult,
) -> serde_json::Value {
    if let Some(native_admin) = portless_state
        .get_mut("nativeAdmin")
        .and_then(serde_json::Value::as_object_mut)
    {
        native_admin.insert("lastResult".to_string(), result.message());
    }
    portless_state
}

pub(crate) fn gpui_portless_native_admin_available() -> bool {
    gpui_resolve_portless_admin_runtime().is_some()
}

pub(crate) fn gpui_portless_native_admin_actions(health: &serde_json::Value) -> serde_json::Value {
    let is_local_mac = gpui_portless_native_admin_available();
    let recommended_action = gpui_recommended_portless_native_admin_action(health);
    let setup_ownership = health
        .get("setupOwnership")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut actions = serde_json::Map::new();
    for action in [
        GpuiPortlessAdminAction::Install,
        GpuiPortlessAdminAction::Reconfigure,
        GpuiPortlessAdminAction::Retry,
        GpuiPortlessAdminAction::Remove,
    ] {
        let available = is_local_mac
            && (Some(action) == recommended_action
                || (action == GpuiPortlessAdminAction::Remove && setup_ownership == "ghostex"));
        let unavailable_reason = if available {
            None
        } else if !is_local_mac {
            Some("localMacOnly")
        } else if action == GpuiPortlessAdminAction::Remove && setup_ownership != "ghostex" {
            Some("setupNotGhostexOwned")
        } else {
            Some("notRecommended")
        };
        let mut action_state = serde_json::Map::new();
        action_state.insert(
            "action".to_string(),
            serde_json::Value::String(action.as_str().to_string()),
        );
        action_state.insert("available".to_string(), serde_json::Value::Bool(available));
        if let Some(unavailable_reason) = unavailable_reason {
            action_state.insert(
                "unavailableReason".to_string(),
                serde_json::Value::String(unavailable_reason.to_string()),
            );
        }
        actions.insert(
            action.as_str().to_string(),
            serde_json::Value::Object(action_state),
        );
    }
    serde_json::Value::Object(actions)
}

pub(crate) fn gpui_recommended_portless_native_admin_action(
    health: &serde_json::Value,
) -> Option<GpuiPortlessAdminAction> {
    let enabled = health
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let setup_status = health
        .get("setupStatus")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let setup_ownership = health
        .get("setupOwnership")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if !enabled || setup_status == "disabled" {
        return None;
    }
    if setup_ownership == "missing" && setup_status == "needed" {
        return Some(GpuiPortlessAdminAction::Install);
    }
    if (setup_ownership == "ghostex" || setup_ownership == "standalone") && setup_status == "needed"
    {
        return Some(GpuiPortlessAdminAction::Reconfigure);
    }
    if setup_ownership == "ghostex" && setup_status == "failed" {
        return Some(GpuiPortlessAdminAction::Retry);
    }
    None
}

pub(crate) fn gpui_settings_portless_enabled(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    GPUI_PORTLESS_APP_INTEGRATION_ENABLED
        && settings
            .get("portlessEnabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
}

pub(crate) fn gpui_settings_portless_protocol(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> GpuiPortlessProtocol {
    settings
        .get("portlessProtocol")
        .and_then(serde_json::Value::as_str)
        .and_then(gpui_portless_protocol)
        .unwrap_or(GpuiPortlessProtocol::Https)
}

pub(crate) fn gpui_portless_protocol(value: &str) -> Option<GpuiPortlessProtocol> {
    match value {
        "http" => Some(GpuiPortlessProtocol::Http),
        "https" => Some(GpuiPortlessProtocol::Https),
        _ => None,
    }
}

pub(crate) fn gpui_portless_admin_action(value: &str) -> Option<GpuiPortlessAdminAction> {
    match value {
        "install" => Some(GpuiPortlessAdminAction::Install),
        "reconfigure" => Some(GpuiPortlessAdminAction::Reconfigure),
        "retry" => Some(GpuiPortlessAdminAction::Retry),
        "remove" => Some(GpuiPortlessAdminAction::Remove),
        _ => None,
    }
}
