use std::{
    collections::HashMap,
    io::Read,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{js_number, parse_json_value, Flags};

pub const GXSERVER_PRODUCT: &str = "gxserver";
pub const GXSERVER_PROTOCOL_VERSION: u64 = 1;
pub const GXSERVER_PROTOCOL_HEADER: &str = "x-gxserver-protocol-version";
pub const GXSERVER_LOCAL_API_HOST: &str = "127.0.0.1";
pub const GXSERVER_LOCAL_API_PORT: u16 = 58744;
const GXSERVER_SSH_FORWARD_DEFAULT_PORT: u16 = GXSERVER_LOCAL_API_PORT + 1;
const GXSERVER_SSH_FORWARD_PORT_SCAN_LIMIT: u16 = 25;
const GXSERVER_SSH_TUNNEL_READY_TIMEOUT_MS: u64 = 8_000;
const GXSERVER_SSH_COMMAND_TIMEOUT_MS: u64 = 12_000;
const GXSERVER_SSH_TUNNEL_IDLE_KILL_MS: u64 = 500;

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI's gxserver RPC layer: local target via
the resolved Ghostex state directory, named connection profiles from
the resolved Ghostex config directory (direct/tailscale URL or ssh:// with a
check→start→forward plan), global S:P:G refs routed by serverId, and
CLI-owned `ssh -N -L` tunnel children that are torn down after the request
burst. Error message text matches the Node CLI so scripts and humans see the
same guidance after the cutover.
*/

pub fn ghostex_home() -> PathBuf {
    storage_paths().state_dir
}

pub fn ghostex_config_home() -> PathBuf {
    storage_paths().config_dir
}

pub fn ghostex_data_home() -> PathBuf {
    storage_paths().data_dir
}

pub fn ghostex_logs_home() -> PathBuf {
    storage_paths().logs_dir
}

pub fn ghostex_runtime_home() -> PathBuf {
    storage_paths().runtime_dir
}

pub fn storage_paths() -> ghostex_paths::GhostexPaths {
    ghostex_paths::GhostexPaths::resolve()
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn gxserver_root() -> PathBuf {
    storage_paths().gxserver_state_dir()
}

pub fn gxserver_auth_token_path() -> PathBuf {
    gxserver_root().join("auth").join("token")
}

pub fn gxserver_connections_path() -> PathBuf {
    storage_paths().clients_dir().join("connections.json")
}

#[derive(Debug)]
pub enum CliError {
    /// GxserverCliConnectionError — connection/setup guidance.
    Connection(String),
    /// GxserverCliRpcError — failed RPC response body preserved.
    Rpc { message: String, response: Value },
    /// GxserverCliUnsupportedError and every other CLI error.
    Other(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Connection(message) => write!(formatter, "{message}"),
            CliError::Rpc { message, .. } => write!(formatter, "{message}"),
            CliError::Other(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<anyhow::Error> for CliError {
    fn from(error: anyhow::Error) -> Self {
        CliError::Other(error.to_string())
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        CliError::Other(error.to_string())
    }
}

pub type CliResult<T> = Result<T, CliError>;

pub fn rpc_error(response: Value) -> CliError {
    let message = if response.get("error").and_then(Value::as_str) == Some("notImplemented") {
        format!(
            "{} Update Ghostex/gxserver when that endpoint lands; the macOS app bridge is no longer a fallback.",
            response
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("gxserver endpoint is not implemented.")
        )
    } else {
        response
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| response.get("error").and_then(Value::as_str))
            .unwrap_or("gxserver request failed.")
            .to_string()
    };
    CliError::Rpc { message, response }
}

pub fn unsupported_action_error(action: &str) -> CliError {
    CliError::Other(format!(
        "Ghostex CLI action \"{action}\" requires a gxserver API endpoint that is not available in this cutover. Update Ghostex/gxserver when that endpoint lands; the macOS app bridge is no longer a fallback."
    ))
}

#[derive(Clone, Debug)]
pub struct SshForwardPlan {
    pub base_url: String,
    pub check_command: Vec<String>,
    pub install_guidance: String,
    pub local_port: u16,
    pub port_forward_command: Vec<String>,
    pub remote_local_port: u16,
    pub start_command: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub base_url: String,
    pub forward_plan: Option<SshForwardPlan>,
    pub kind: String,
    pub profile_id: Option<String>,
    pub server_id: Option<String>,
    pub token: String,
}

impl Target {
    fn profile_suffix(&self) -> String {
        self.profile_id
            .as_ref()
            .map(|id| format!(" \"{id}\""))
            .unwrap_or_default()
    }
}

pub fn is_local_gxserver_target(target: &Target) -> bool {
    if target.kind == "local" {
        return true;
    }
    let Some(rest) = target
        .base_url
        .strip_prefix("http://")
        .or_else(|| target.base_url.strip_prefix("https://"))
    else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

pub fn is_gxserver_global_session_ref(value: &str) -> bool {
    let mut parts = value.split(':');
    let (Some(s), Some(p), Some(g), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    fn segment_ok(segment: &str, prefix: char) -> bool {
        let mut chars = segment.chars();
        chars.next() == Some(prefix)
            && matches!(chars.next(), Some(c) if c.is_ascii_digit())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }
    segment_ok(s, 'S') && segment_ok(p, 'P') && segment_ok(g, 'G')
}

pub fn project_id_from_global_ref(value: &str) -> Option<String> {
    if is_gxserver_global_session_ref(value) {
        value.split(':').nth(1).map(str::to_string)
    } else {
        None
    }
}

fn find_global_ref_candidate(params: &Value) -> Option<String> {
    if let Some(text) = params.as_str() {
        if is_gxserver_global_session_ref(text) {
            return Some(text.to_string());
        }
    }
    if let Some(object) = params.as_object() {
        for key in ["globalRef", "sessionId", "target", "selector"] {
            if let Some(value) = object.get(key).and_then(Value::as_str) {
                if is_gxserver_global_session_ref(value) {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

pub fn call_gxserver_rpc(pathname: &str, params: &Value, flags: &Flags) -> CliResult<Value> {
    let target = resolve_gxserver_server_target(flags, params)?;
    request_gxserver_rpc(&target, pathname, params, flags)
}

pub fn request_gxserver_rpc(
    target: &Target,
    pathname: &str,
    params: &Value,
    flags: &Flags,
) -> CliResult<Value> {
    let _tunnel = ensure_gxserver_ssh_tunnel_for_rpc(target, flags)?;
    let timeout_ms = flags
        .number("timeout")
        .or_else(|| flags.number("timeoutMs"))
        .unwrap_or(15_000.0);
    let body = json!({
        "params": params,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
    });
    let request = ureq::post(&format!("{}{}", target.base_url, pathname))
        .set("authorization", &format!("Bearer {}", target.token))
        .set("content-type", "application/json")
        .set(
            GXSERVER_PROTOCOL_HEADER,
            &GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .timeout(Duration::from_millis(timeout_ms.max(0.0) as u64));
    let outcome = request.send_string(&body.to_string());
    let (status_ok, status, response_body) = match outcome {
        Ok(response) => {
            let status = response.status();
            (true, status, read_json_body(response))
        }
        Err(ureq::Error::Status(status, response)) => (false, status, read_json_body(response)),
        Err(error) => {
            return Err(connection_error_for_target(target, &error.to_string()));
        }
    };
    if let Some(body) = &response_body {
        if body.get("product").and_then(Value::as_str) == Some(GXSERVER_PRODUCT) {
            if let Some(protocol_version) = body.get("protocolVersion") {
                if protocol_version.as_u64() != Some(GXSERVER_PROTOCOL_VERSION) {
                    return Err(rpc_error(json!({
                        "error": "protocolMismatch",
                        "message": format!(
                            "gxserver protocol mismatch. Expected protocol {GXSERVER_PROTOCOL_VERSION}, got {}. Update Ghostex and gxserver so their protocol versions match.",
                            js_value_to_string(protocol_version)
                        ),
                        "ok": false,
                    })));
                }
            }
        }
    }
    let body_not_ok = response_body
        .as_ref()
        .map(|body| body.get("ok") == Some(&Value::Bool(false)))
        .unwrap_or(false);
    if !status_ok || body_not_ok {
        return Err(rpc_error(response_body.unwrap_or_else(|| {
            json!({
                "error": format!("http-{status}"),
                "message": format!("gxserver HTTP {status}"),
                "ok": false,
            })
        })));
    }
    let body = response_body.unwrap_or_else(|| json!({}));
    let mut merged = Map::new();
    merged.insert("ok".to_string(), Value::Bool(true));
    if let Some(result) = body.get("result").and_then(Value::as_object) {
        for (key, value) in result {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged.insert(
        "requestId".to_string(),
        body.get("requestId").cloned().unwrap_or(Value::Null),
    );
    Ok(Value::Object(merged))
}

fn read_json_body(response: ureq::Response) -> Option<Value> {
    let mut text = String::new();
    response
        .into_reader()
        .take(32 * 1024 * 1024)
        .read_to_string(&mut text)
        .ok()?;
    parse_json_value(&text)
}

fn connection_error_for_target(target: &Target, _detail: &str) -> CliError {
    let message = if target.kind == "ssh" {
        format!(
            "Could not connect to SSH gxserver profile{} at {}. Check SSH access, remote gxserver status, and the local tunnel, then retry.",
            target.profile_suffix(),
            target.base_url
        )
    } else if is_local_gxserver_target(target) {
        format!(
            "Could not connect to local gxserver at {}. Start it with \"gx server start\" and retry.",
            target.base_url
        )
    } else {
        format!(
            "Could not connect to remote gxserver at {}. Start gxserver on that host and retry.",
            target.base_url
        )
    };
    CliError::Connection(message)
}

fn js_value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub fn resolve_gxserver_server_target(flags: &Flags, params: &Value) -> CliResult<Target> {
    let server = flags
        .text("server")
        .or_else(|| std::env::var("GHOSTEX_GXSERVER_SERVER").ok())
        .unwrap_or_else(|| "local".to_string());
    let server = {
        let trimmed = server.trim().to_string();
        if trimmed.is_empty() {
            "local".to_string()
        } else {
            trimmed
        }
    };
    if let Some(global_ref) = find_global_ref_candidate(params) {
        let server_id = global_ref.split(':').next().unwrap_or_default().to_string();
        let local = resolve_local_gxserver_target()?;
        if let Ok(Some(health)) = fetch_gxserver_health(&local, 1_000) {
            if health.get("serverId").and_then(Value::as_str) == Some(server_id.as_str()) {
                return Ok(local);
            }
        }
        let Some(profile) = find_gxserver_connection_profile_by_server_id(&server_id) else {
            return Err(CliError::Other(format!(
                "Global session ref {global_ref} targets {server_id}, but no gxserver connection profile exists for that server."
            )));
        };
        return resolve_gxserver_profile_target(&profile, flags);
    }
    if server == "local" {
        return resolve_local_gxserver_target();
    }
    if server.starts_with("ssh://") {
        let profile = json!({
            "id": server,
            "name": server,
            "sshUrl": server,
            "transport": "ssh",
        });
        return resolve_gxserver_profile_target(&profile, flags);
    }
    if server.starts_with("http://") || server.starts_with("https://") {
        return Ok(Target {
            base_url: server.trim_end_matches('/').to_string(),
            forward_plan: None,
            kind: "direct".to_string(),
            profile_id: None,
            server_id: None,
            token: read_gxserver_credential_secret_from_flags(flags)?,
        });
    }
    let Some(profile) = read_gxserver_connection_profile(&server) else {
        return Err(CliError::Other(format!(
            "gxserver profile \"{server}\" was not found in {}.",
            gxserver_connections_path().display()
        )));
    };
    resolve_gxserver_profile_target(&profile, flags)
}

/*
CDXC:GxserverDevPort 2026-08-20:
The daemon already lets a compat/dev run move its loopback listener with
GHOSTEX_GXSERVER_DEV_PORT (see `config::read_selected_local_api_port`). The CLI
hardcoded 58744, so `gx` against such a daemon silently talked to the packaged
one on the default port and failed on its auth token. Read the same variable so
one setting describes one daemon; unset or unparseable keeps the product port.
*/
pub fn local_gxserver_api_port() -> u16 {
    std::env::var(crate::constants::GXSERVER_DEV_LOCAL_API_PORT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .unwrap_or(GXSERVER_LOCAL_API_PORT)
}

pub fn resolve_local_gxserver_target() -> CliResult<Target> {
    let token = read_local_gxserver_auth_token()?;
    Ok(Target {
        base_url: format!(
            "http://{GXSERVER_LOCAL_API_HOST}:{}",
            local_gxserver_api_port()
        ),
        forward_plan: None,
        kind: "local".to_string(),
        profile_id: None,
        server_id: None,
        token,
    })
}

pub fn read_local_gxserver_auth_token() -> CliResult<String> {
    let token = std::fs::read_to_string(gxserver_auth_token_path())
        .unwrap_or_default()
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(CliError::Connection(format!(
            "Could not read local gxserver auth token at {}. Start gxserver with \"gx server start\" and retry.",
            gxserver_auth_token_path().display()
        )));
    }
    Ok(token)
}

pub fn fetch_gxserver_health(target: &Target, timeout_ms: u64) -> CliResult<Option<Value>> {
    let request = ureq::get(&format!(
        "{}/api/health/server?protocolVersion={GXSERVER_PROTOCOL_VERSION}",
        target.base_url
    ))
    .set("authorization", &format!("Bearer {}", target.token))
    .set(
        GXSERVER_PROTOCOL_HEADER,
        &GXSERVER_PROTOCOL_VERSION.to_string(),
    )
    .timeout(Duration::from_millis(timeout_ms));
    match request.call() {
        Ok(response) => Ok(read_json_body(response)),
        Err(ureq::Error::Status(_, _)) => Ok(None),
        Err(error) => Err(CliError::Other(error.to_string())),
    }
}

fn read_connections_file() -> Option<Value> {
    let text = std::fs::read_to_string(gxserver_connections_path()).ok()?;
    let parsed = parse_json_value(&text)?;
    parsed.is_object().then_some(parsed)
}

pub fn read_gxserver_connection_profile(name: &str) -> Option<Value> {
    let parsed = read_connections_file()?;
    if let Some(profiles) = parsed.get("profiles").and_then(Value::as_array) {
        return profiles
            .iter()
            .find(|profile| {
                profile.get("name").and_then(Value::as_str) == Some(name)
                    || profile.get("id").and_then(Value::as_str) == Some(name)
            })
            .cloned();
    }
    parsed.get(name).cloned()
}

pub fn find_gxserver_connection_profile_by_server_id(server_id: &str) -> Option<Value> {
    let parsed = read_connections_file()?;
    let profiles: Vec<Value> = match parsed.get("profiles").and_then(Value::as_array) {
        Some(profiles) => profiles.clone(),
        None => parsed
            .as_object()
            .map(|object| object.values().cloned().collect())
            .unwrap_or_default(),
    };
    profiles
        .into_iter()
        .find(|profile| profile.get("serverId").and_then(Value::as_str) == Some(server_id))
}

pub fn resolve_gxserver_profile_target(profile: &Value, flags: &Flags) -> CliResult<Target> {
    let transport = profile
        .get("transport")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if profile.get("sshUrl").and_then(Value::as_str).is_some() {
                "ssh".to_string()
            } else {
                "direct".to_string()
            }
        });
    let token = read_gxserver_connection_profile_token(profile, flags)?;
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let server_id = profile
        .get("serverId")
        .and_then(Value::as_str)
        .map(str::to_string);
    if transport == "ssh" {
        let explicit_local_port = flags
            .number("localPort")
            .or_else(|| flags.number("forwardPort"))
            .or_else(|| {
                flags
                    .text("baseUrl")
                    .and_then(|base_url| local_port_from_base_url(&base_url))
            })
            .map(|port| port as u16)
            .filter(|port| *port != 0);
        let local_port = select_cli_ssh_forward_local_port(explicit_local_port)?;
        let remote_local_port = flags
            .number("remotePort")
            .map(|port| port as u16)
            .unwrap_or(GXSERVER_LOCAL_API_PORT);
        let forward_plan = create_cli_ssh_forward_plan(profile, local_port, remote_local_port)?;
        return Ok(Target {
            base_url: flags
                .text("baseUrl")
                .unwrap_or_else(|| forward_plan.base_url.clone()),
            forward_plan: Some(forward_plan),
            kind: "ssh".to_string(),
            profile_id,
            server_id,
            token,
        });
    }
    let base_url = profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        let label = profile
            .get("name")
            .or_else(|| profile.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        return Err(CliError::Other(format!(
            "gxserver profile \"{label}\" is missing baseUrl."
        )));
    }
    Ok(Target {
        base_url,
        forward_plan: None,
        kind: if transport == "tailscale" {
            "tailscale".to_string()
        } else {
            "direct".to_string()
        },
        profile_id,
        server_id,
        token,
    })
}

fn local_port_from_base_url(base_url: &str) -> Option<f64> {
    let rest = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))?;
    let host_port = rest.split('/').next()?;
    let (host, port) = host_port.rsplit_once(':')?;
    if host == "127.0.0.1" || host == "localhost" {
        js_number(port)
    } else {
        None
    }
}

fn select_cli_ssh_forward_local_port(explicit_local_port: Option<u16>) -> CliResult<u16> {
    if let Some(port) = explicit_local_port {
        return Ok(port);
    }
    for port in GXSERVER_SSH_FORWARD_DEFAULT_PORT
        ..GXSERVER_SSH_FORWARD_DEFAULT_PORT + GXSERVER_SSH_FORWARD_PORT_SCAN_LIMIT
    {
        if TcpListener::bind((GXSERVER_LOCAL_API_HOST, port)).is_ok() {
            return Ok(port);
        }
    }
    let listener = TcpListener::bind((GXSERVER_LOCAL_API_HOST, 0)).map_err(|_| {
        CliError::Other(
            "Could not reserve an ephemeral local port for the SSH gxserver tunnel.".to_string(),
        )
    })?;
    let port = listener
        .local_addr()
        .map_err(|_| {
            CliError::Other(
                "Could not reserve an ephemeral local port for the SSH gxserver tunnel."
                    .to_string(),
            )
        })?
        .port();
    Ok(port)
}

pub fn read_gxserver_connection_profile_token(profile: &Value, flags: &Flags) -> CliResult<String> {
    if flags.truthy("token") || flags.truthy("tokenStdin") || flags.truthy("tokenFromStdin") {
        return read_gxserver_credential_secret_from_flags(flags);
    }
    if profile.get("token").and_then(Value::as_str).is_some() {
        let label = profile
            .get("name")
            .or_else(|| profile.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("undefined");
        return Err(CliError::Other(format!(
            "gxserver profile \"{label}\" contains a plaintext token. Move it to the OS credential store and keep only tokenSecretRef in {}.",
            gxserver_connections_path().display()
        )));
    }
    let Some(token_secret_ref) = profile.get("tokenSecretRef") else {
        return read_gxserver_credential_secret_from_flags(flags);
    };
    read_gxserver_credential_secret(token_secret_ref)
}

pub fn read_gxserver_credential_secret_from_flags(flags: &Flags) -> CliResult<String> {
    if flags.truthy("tokenStdin") || flags.truthy("tokenFromStdin") {
        let token = read_gxserver_one_shot_token_from_stdin()?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(CliError::Other(
                "Remote gxserver --token-stdin did not receive a token.".to_string(),
            ));
        }
        return Ok(token);
    }
    if let Some(token) = flags.text("token").filter(|token| !token.is_empty()) {
        return Ok(token);
    }
    Err(CliError::Other(
        "Remote gxserver profiles require an auth token stored in the OS credential store. Add tokenSecretRef to the profile. For temporary one-shot use, pass --token-stdin; --token remains available for legacy scripts but can expose the token in shell history and process listings.".to_string(),
    ))
}

fn read_gxserver_one_shot_token_from_stdin() -> CliResult<String> {
    #[cfg(unix)]
    {
        let is_tty = unsafe { libc::isatty(0) == 1 };
        if is_tty {
            return Err(CliError::Other(
                "Remote gxserver --token-stdin requires a token piped on stdin.".to_string(),
            ));
        }
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| CliError::Other(error.to_string()))?;
    Ok(text)
}

pub fn read_gxserver_credential_secret(secret_ref: &Value) -> CliResult<String> {
    let service = secret_ref
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or("ghostex.gxserver");
    let account = secret_ref
        .get("account")
        .and_then(Value::as_str)
        .unwrap_or("");
    if account.is_empty() {
        return Err(CliError::Other(
            "gxserver credential secret ref is missing account.".to_string(),
        ));
    }
    let output = if cfg!(target_os = "macos") {
        Command::new("security")
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
    } else if cfg!(target_os = "linux") {
        Command::new("secret-tool")
            .args(["lookup", "service", service, "account", account])
            .output()
    } else {
        return Err(CliError::Other(
            "This platform needs a gxserver OS credential-store integration before remote tokens can be read.".to_string(),
        ));
    };
    let output = output.map_err(|error| CliError::Other(error.to_string()))?;
    if !output.status.success() {
        return Err(CliError::Other(format!(
            "Could not read gxserver credential secret for service \"{service}\" account \"{account}\"."
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn create_cli_ssh_forward_plan(
    profile: &Value,
    local_port: u16,
    remote_local_port: u16,
) -> CliResult<SshForwardPlan> {
    let ssh_url = profile
        .get("sshUrl")
        .or_else(|| profile.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = parse_cli_ssh_url(ssh_url)?;
    let target_args = cli_ssh_target_args(&target);
    let mut check_command = vec!["ssh".to_string()];
    check_command.extend(target_args.clone());
    check_command.push("command -v gxserver >/dev/null && gxserver status --json".to_string());
    let mut start_command = vec!["ssh".to_string()];
    start_command.extend(target_args.clone());
    start_command.push("gxserver start --background".to_string());
    let mut port_forward_command = vec![
        "ssh".to_string(),
        "-N".to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-L".to_string(),
        format!("{local_port}:127.0.0.1:{remote_local_port}"),
    ];
    port_forward_command.extend(target_args);
    Ok(SshForwardPlan {
        base_url: format!("http://127.0.0.1:{local_port}"),
        check_command,
        install_guidance: "gxserver is not installed on the remote host. Install the Ghostex server package there, then retry; the SSH helper does not install software silently.".to_string(),
        local_port,
        port_forward_command,
        remote_local_port,
        start_command,
    })
}

struct CliSshUrlTarget {
    host: String,
    port: Option<u16>,
    user: Option<String>,
}

fn parse_cli_ssh_url(value: &str) -> CliResult<CliSshUrlTarget> {
    let error = || CliError::Other("SSH gxserver profiles must use ssh://user@host.".to_string());
    let rest = value.strip_prefix("ssh://").ok_or_else(error)?;
    let rest = rest.split('/').next().unwrap_or_default();
    let (user, host_port) = match rest.rsplit_once('@') {
        Some((user, host_port)) => (Some(user.to_string()), host_port),
        None => (None, rest),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            (host.to_string(), port.parse::<u16>().ok())
        }
        _ => (host_port.to_string(), None),
    };
    if host.is_empty() {
        return Err(error());
    }
    Ok(CliSshUrlTarget {
        host,
        port,
        user: user.map(|user| percent_decode(&user)),
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn cli_ssh_target_args(target: &CliSshUrlTarget) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(port) = target.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }
    args.push(match &target.user {
        Some(user) => format!("{user}@{}", target.host),
        None => target.host.clone(),
    });
    args
}

/* --------------------------------------------------------- ssh tunnels */

struct TunnelRecord {
    child: Option<Child>,
}

fn active_tunnels() -> &'static Mutex<HashMap<String, TunnelRecord>> {
    static TUNNELS: OnceLock<Mutex<HashMap<String, TunnelRecord>>> = OnceLock::new();
    TUNNELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Guard released after the RPC completes; the last release schedules the
/// idle kill exactly like the Node CLI's 500ms idle timer.
pub struct TunnelGuard {
    key: Option<String>,
    idle_kill_ms: u64,
}

impl Drop for TunnelGuard {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else {
            return;
        };
        let idle_kill_ms = self.idle_kill_ms;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(idle_kill_ms));
            if let Ok(mut tunnels) = active_tunnels().lock() {
                if let Some(mut record) = tunnels.remove(&key) {
                    stop_tunnel_child(record.child.as_mut());
                }
            }
        });
    }
}

pub fn stop_all_gxserver_ssh_tunnels() {
    if let Ok(mut tunnels) = active_tunnels().lock() {
        for (_, mut record) in tunnels.drain() {
            stop_tunnel_child(record.child.as_mut());
        }
    }
}

fn stop_tunnel_child(child: Option<&mut Child>) {
    if let Some(child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn gxserver_ssh_tunnel_cache_key(target: &Target, plan: &SshForwardPlan) -> String {
    let mut parts = vec![
        target.profile_id.clone().unwrap_or_default(),
        target.server_id.clone().unwrap_or_default(),
        target.base_url.clone(),
        plan.local_port.to_string(),
        plan.remote_local_port.to_string(),
    ];
    parts.extend(plan.port_forward_command.clone());
    parts.join("\0")
}

pub fn ensure_gxserver_ssh_tunnel_for_rpc(
    target: &Target,
    flags: &Flags,
) -> CliResult<Option<TunnelGuard>> {
    let Some(plan) = &target.forward_plan else {
        return Ok(None);
    };
    if target.kind != "ssh" {
        return Ok(None);
    }
    if let Ok(Some(health)) = fetch_gxserver_health(target, 600) {
        if is_expected_gxserver_health(Some(&health), target) {
            return Ok(None);
        }
    }
    let key = gxserver_ssh_tunnel_cache_key(target, plan);
    let already_running = active_tunnels()
        .lock()
        .map(|tunnels| tunnels.contains_key(&key))
        .unwrap_or(false);
    if !already_running {
        let child = start_gxserver_ssh_tunnel(target, plan, flags)?;
        if let Ok(mut tunnels) = active_tunnels().lock() {
            tunnels.insert(key.clone(), TunnelRecord { child: Some(child) });
        }
    }
    let idle_kill_ms = flags
        .number("sshTunnelIdleKillMs")
        .map(|value| value as u64)
        .unwrap_or(GXSERVER_SSH_TUNNEL_IDLE_KILL_MS);
    Ok(Some(TunnelGuard {
        key: Some(key),
        idle_kill_ms,
    }))
}

fn start_gxserver_ssh_tunnel(
    target: &Target,
    plan: &SshForwardPlan,
    flags: &Flags,
) -> CliResult<Child> {
    let before = run_gxserver_ssh_status_command(target, plan, flags)?;
    let running = before
        .as_ref()
        .and_then(|status| status.get("state"))
        .and_then(Value::as_str)
        == Some("running");
    if !running {
        run_gxserver_ssh_command(&plan.start_command, flags)
            .map_err(|error| format_ssh_setup_error("start", target, plan, &error))?;
        let after = run_gxserver_ssh_status_command(target, plan, flags)?;
        let after_state = after
            .as_ref()
            .and_then(|status| status.get("state"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        if after_state != "running" {
            return Err(CliError::Connection(format!(
                "SSH gxserver profile{} started remote gxserver, but status is still {after_state}. Run {} on the remote profile to inspect it.",
                target.profile_suffix(),
                format_command(&plan.check_command)
            )));
        }
    }
    let mut command = Command::new(&plan.port_forward_command[0]);
    command
        .args(&plan.port_forward_command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ssh_missing_error(target)
        } else {
            CliError::Other(error.to_string())
        }
    })?;
    let timeout_ms = flags
        .number("sshTunnelReadyTimeoutMs")
        .map(|value| value as u64)
        .unwrap_or(GXSERVER_SSH_TUNNEL_READY_TIMEOUT_MS);
    let poll_ms = flags
        .number("sshTunnelPollMs")
        .map(|value| value as u64)
        .unwrap_or(100);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(CliError::Connection(format!(
                "SSH tunnel for gxserver profile{} exited before it became ready (code {}, signal null). Check SSH forwarding permissions and port {}.",
                target.profile_suffix(),
                status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                plan.local_port
            )));
        }
        if let Ok(Some(health)) = fetch_gxserver_health(target, 600) {
            if is_expected_gxserver_health(Some(&health), target) {
                return Ok(child);
            }
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(CliError::Connection(format!(
        "SSH tunnel for gxserver profile{} did not become healthy on {}. Check that {} can connect and that the remote gxserver token is valid.",
        target.profile_suffix(),
        target.base_url,
        format_command(&plan.port_forward_command)
    )))
}

fn run_gxserver_ssh_status_command(
    target: &Target,
    plan: &SshForwardPlan,
    flags: &Flags,
) -> CliResult<Option<Value>> {
    let result = run_gxserver_ssh_command(&plan.check_command, flags)
        .map_err(|error| format_ssh_setup_error("check", target, plan, &error))?;
    let parsed = parse_json_value(&result.0);
    Ok(parsed
        .filter(|value| value.get("product").and_then(Value::as_str) == Some(GXSERVER_PRODUCT)))
}

struct SshCommandFailure {
    not_found: bool,
    stderr: String,
    stdout: String,
}

fn run_gxserver_ssh_command(
    command: &[String],
    flags: &Flags,
) -> Result<(String, String), SshCommandFailure> {
    let timeout_ms = flags
        .number("sshCommandTimeoutMs")
        .map(|value| value as u64)
        .unwrap_or(GXSERVER_SSH_COMMAND_TIMEOUT_MS);
    let mut child = match Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Err(SshCommandFailure {
                not_found: error.kind() == std::io::ErrorKind::NotFound,
                stderr: String::new(),
                stdout: String::new(),
            });
        }
    };
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut reader) = child.stdout.take() {
                    let _ = reader.read_to_string(&mut stdout);
                }
                if let Some(mut reader) = child.stderr.take() {
                    let _ = reader.read_to_string(&mut stderr);
                }
                if status.success() {
                    return Ok((stdout, stderr));
                }
                return Err(SshCommandFailure {
                    not_found: false,
                    stderr,
                    stdout,
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SshCommandFailure {
                        not_found: false,
                        stderr: String::new(),
                        stdout: String::new(),
                    });
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                return Err(SshCommandFailure {
                    not_found: false,
                    stderr: String::new(),
                    stdout: String::new(),
                });
            }
        }
    }
}

fn ssh_missing_error(target: &Target) -> CliError {
    CliError::Connection(format!(
        "Could not set up SSH gxserver profile{} because the \"ssh\" executable was not found on PATH. Install OpenSSH, or use a direct/Tailscale gxserver profile instead.",
        target.profile_suffix()
    ))
}

fn format_ssh_setup_error(
    phase: &str,
    target: &Target,
    plan: &SshForwardPlan,
    failure: &SshCommandFailure,
) -> CliError {
    if failure.not_found {
        return ssh_missing_error(target);
    }
    let stderr = failure.stderr.trim();
    let stdout = failure.stdout.trim();
    let output: String = [stderr, stdout]
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(1200)
        .collect();
    let command = if phase == "start" {
        &plan.start_command
    } else {
        &plan.check_command
    };
    let guidance = if phase == "check" {
        format!(" {}", plan.install_guidance)
    } else {
        String::new()
    };
    let output_section = if output.is_empty() {
        String::new()
    } else {
        format!("\nSSH output:\n{output}")
    };
    CliError::Connection(format!(
        "Could not {phase} remote gxserver for SSH profile{} with {}.{guidance}{output_section}",
        target.profile_suffix(),
        format_command(command)
    ))
}

pub fn is_expected_gxserver_health(health: Option<&Value>, target: &Target) -> bool {
    let Some(health) = health else {
        return false;
    };
    health.get("product").and_then(Value::as_str) == Some(GXSERVER_PRODUCT)
        && health.get("protocolVersion").and_then(Value::as_u64) == Some(GXSERVER_PROTOCOL_VERSION)
        && (target.server_id.is_none()
            || health.get("serverId").and_then(Value::as_str) == target.server_id.as_deref())
}

pub fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '=' | '+')
        })
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn format_command(command: &[String]) -> String {
    command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn compact_object(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, entry)| !entry.is_null())
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_session_ref_pattern_matches_js_regex() {
        assert!(is_gxserver_global_session_ref("S1abc:P2def:G3xyz"));
        assert!(is_gxserver_global_session_ref("S0:P0:G0"));
        assert!(!is_gxserver_global_session_ref("S1:P2"));
        assert!(!is_gxserver_global_session_ref("s1:P2:G3"));
        assert!(!is_gxserver_global_session_ref("S1:P2:G3:X4"));
        assert!(!is_gxserver_global_session_ref("SA:P2:G3"));
        assert_eq!(
            project_id_from_global_ref("S1abc:P2def:G3xyz"),
            Some("P2def".to_string())
        );
    }

    #[test]
    fn ssh_url_parsing_matches_node_url_semantics() {
        let target = parse_cli_ssh_url("ssh://madda@example.com:2222").expect("parse");
        assert_eq!(target.host, "example.com");
        assert_eq!(target.port, Some(2222));
        assert_eq!(target.user, Some("madda".to_string()));
        assert_eq!(
            cli_ssh_target_args(&target),
            vec!["-p", "2222", "madda@example.com"]
        );
        assert!(parse_cli_ssh_url("https://example.com").is_err());
    }

    #[test]
    fn forward_plan_matches_node_command_shapes() {
        let profile = json!({ "sshUrl": "ssh://madda@host" });
        let plan = create_cli_ssh_forward_plan(&profile, 58745, 58744).expect("plan");
        assert_eq!(plan.base_url, "http://127.0.0.1:58745");
        assert_eq!(
            plan.check_command,
            vec![
                "ssh",
                "madda@host",
                "command -v gxserver >/dev/null && gxserver status --json"
            ]
        );
        assert_eq!(
            plan.port_forward_command,
            vec![
                "ssh",
                "-N",
                "-o",
                "ExitOnForwardFailure=yes",
                "-L",
                "58745:127.0.0.1:58744",
                "madda@host"
            ]
        );
    }
}
