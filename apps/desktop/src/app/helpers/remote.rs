// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use flate2::{Compression, write::GzEncoder};
use futures::{StreamExt as _, channel::mpsc};
use gpui::http_client::HttpRequestExt as _;
use gpui::{
    AnyElement, AppContext as _, Asset, Element, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::v_flex;

use crate::app::helpers::*;
use crate::*;

/// Status overlay drawn over the terminal body while a surfaced remote session's
/// machine is not connected. Same normal-layout contract as the title overlay
/// above: an absolutely-positioned child of the body it covers, occluding input
/// for exactly that rectangle, with no hidden hit region beyond it.
pub(crate) fn render_agents_remote_connect_status_overlay(
    pane_id: WorkspacePaneId,
    session_id: TerminalSessionId,
    title: &str,
    detail: Option<&str>,
) -> AnyElement {
    v_flex()
        .id(format!(
            "ghostex-gpui-terminal-remote-connect-overlay-{}-{}",
            pane_id.0, session_id.0
        ))
        .occlude()
        .absolute()
        .inset_0()
        .items_center()
        .justify_center()
        .bg(rgb(0x000000).opacity(0.58))
        .child(
            div()
                .text_size(px(17.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0xffffff).opacity(0.96))
                .child(title.to_string()),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .mt(px(5.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgb(0xffffff).opacity(0.58))
                    .child(detail.to_string()),
            )
        })
        .into_any_element()
}

/// Overlay copy for a remote machine's latest connect wire state. Mirrors the
/// sidebar's `remoteMachineBusyLabel` / `remoteMachineFailureLabel` vocabulary so
/// the terminal area and the sidebar row never disagree about one machine.
pub(crate) fn gpui_remote_connect_overlay_labels(state: Option<&str>) -> (&'static str, Option<&'static str>) {
    match state {
        Some("installing") => ("Installing gxserver…", None),
        Some("downloadingRemoteServerPackage") => ("Downloading server package…", None),
        Some("installApprovalRequired") => (
            "Remote setup needed",
            Some("Approve the gxserver install for this machine."),
        ),
        Some("installFailed") => (
            "Remote setup failed",
            Some("Reconnect the machine to try again."),
        ),
        Some("sshFailed") => ("Cannot reach machine", Some("SSH connection failed.")),
        Some("tunnelFailed") => ("Cannot reach machine", Some("SSH tunnel failed.")),
        Some("keychainFailed") => (
            "Cannot reach machine",
            Some("Saved credentials are unavailable."),
        ),
        Some("tokenUnavailable") => (
            "Cannot reach machine",
            Some("The remote gxserver token is unavailable."),
        ),
        Some("presentationSubscribeFailed") | Some("presentationStreamFailed") => (
            "Reconnecting to machine…",
            Some("The remote session stream dropped."),
        ),
        Some("unsupported") | Some("unsupportedRemotePlatform") => (
            "Machine unsupported",
            Some("This remote platform cannot host gxserver."),
        ),
        Some("invalid") => (
            "Machine unavailable",
            Some("The saved machine settings are incomplete."),
        ),
        Some("failed") => (
            "Cannot reach machine",
            Some("Reconnect the machine to try again."),
        ),
        // `connecting`, `disconnected`, and the pre-first-status cold-start
        // window all describe the same user-visible situation: the machine is
        // not reachable yet and a connect attempt is expected.
        _ => ("Connecting to machine…", None),
    }
}

pub(crate) fn browser_repository_remote_web_url(project_path: &Path) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_path)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let remote = String::from_utf8(output.stdout).ok()?;
    let remote = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    let web_url = if let Some(path) = remote.strip_prefix("git@") {
        let (host, repository) = path.split_once(':')?;
        format!("https://{host}/{repository}")
    } else if let Some(path) = remote.strip_prefix("ssh://git@") {
        let (host, repository) = path.split_once('/')?;
        format!("https://{host}/{repository}")
    } else if remote.starts_with("https://") || remote.starts_with("http://") {
        remote.to_string()
    } else {
        return None;
    };

    sanitize_browser_tab_url_for_state(&web_url)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemoteMachineConfig {
    pub(crate) remote_machine_id: String,
    pub(crate) ssh_host: String,
    pub(crate) ssh_identity_file: Option<String>,
    pub(crate) ssh_port: Option<u16>,
    pub(crate) ssh_user: Option<String>,
    pub(crate) has_saved_password: bool,
    pub(crate) wsl_distribution: Option<String>,
}

impl GpuiRemoteMachineConfig {
    pub(crate) fn ssh_target_host(&self) -> String {
        self.ssh_user
            .as_ref()
            .filter(|user| !user.is_empty())
            .map(|user| format!("{user}@{}", self.ssh_host))
            .unwrap_or_else(|| self.ssh_host.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteExecutionTarget {
    PosixHost,
    WindowsWsl { distribution: String },
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteExecutionTargetProbeError {
    Ssh(GpuiRemoteProcessResult),
    Unsupported(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GpuiRemoteAttachSessionKey {
    pub(crate) remote_machine_id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiProjectEditorCompanionRemoteAttachAttempt {
    pub(crate) connection_generation: u64,
    pub(crate) remote_key: GpuiRemoteAttachSessionKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiProjectEditorCompanionRemoteAttachState {
    Preparing(GpuiProjectEditorCompanionRemoteAttachAttempt),
    Unavailable {
        attempt: GpuiProjectEditorCompanionRemoteAttachAttempt,
        message: String,
    },
}

impl GpuiProjectEditorCompanionRemoteAttachState {
    pub(crate) fn attempt(&self) -> &GpuiProjectEditorCompanionRemoteAttachAttempt {
        match self {
            Self::Preparing(attempt) | Self::Unavailable { attempt, .. } => attempt,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemoteAttachSessionReference {
    pub(crate) remote_machine_id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemoteProjectReference {
    pub(crate) remote_machine_id: String,
    pub(crate) project_id: String,
}

impl From<&GpuiRemoteAttachSessionReference> for GpuiRemoteAttachSessionKey {
    fn from(reference: &GpuiRemoteAttachSessionReference) -> Self {
        Self {
            remote_machine_id: reference.remote_machine_id.clone(),
            project_id: reference.project_id.clone(),
            session_id: reference.session_id.clone(),
        }
    }
}

pub(crate) struct GpuiRemoteAttachTerminalPlan {
    pub(crate) agent_icon: Option<&'static str>,
    #[cfg(target_os = "macos")]
    pub(crate) askpass: Option<GpuiRemoteAskpassScript>,
    pub(crate) clipboard_command: String,
    pub(crate) terminal_command: String,
    pub(crate) title: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteGxserverConnectState {
    Connecting,
    Connected,
    DownloadingRemoteServerPackage,
    InstallApprovalRequired,
    InstallFailed,
    Invalid,
    SshFailed,
    TokenUnavailable,
    KeychainFailed,
    PresentationSubscribeFailed,
    PresentationStreamFailed,
    TunnelFailed,
    Unsupported,
    UnsupportedRemotePlatform,
}

impl GpuiRemoteGxserverConnectState {
    pub(crate) fn toast_level(self) -> &'static str {
        match self {
            Self::Connecting | Self::DownloadingRemoteServerPackage => "info",
            Self::Connected => "success",
            Self::InstallApprovalRequired
            | Self::Invalid
            | Self::Unsupported
            | Self::UnsupportedRemotePlatform => "warning",
            Self::InstallFailed
            | Self::SshFailed
            | Self::TokenUnavailable
            | Self::KeychainFailed
            | Self::PresentationSubscribeFailed
            | Self::PresentationStreamFailed
            | Self::TunnelFailed => "error",
        }
    }

    pub(crate) fn toast_title(self) -> &'static str {
        match self {
            Self::Connecting => "Remote gxserver connecting",
            Self::Connected => "Remote gxserver connected",
            Self::DownloadingRemoteServerPackage => "Remote package downloading",
            Self::InstallApprovalRequired => "Install approval required",
            Self::InstallFailed => "Remote install failed",
            Self::Invalid => "Remote connect failed",
            Self::SshFailed => "Remote SSH failed",
            Self::TokenUnavailable => "Remote token unavailable",
            Self::KeychainFailed => "Remote token not saved",
            Self::PresentationSubscribeFailed => "Remote sidebar stream failed",
            Self::PresentationStreamFailed => "Remote sidebar stream failed",
            Self::TunnelFailed => "Remote tunnel failed",
            Self::Unsupported => "Remote connect unavailable",
            Self::UnsupportedRemotePlatform => "Remote platform unsupported",
        }
    }

    pub(crate) fn wire_status_state(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::DownloadingRemoteServerPackage => "downloadingRemoteServerPackage",
            Self::InstallApprovalRequired => "installApprovalRequired",
            Self::InstallFailed => "installFailed",
            Self::Invalid => "invalid",
            Self::SshFailed => "sshFailed",
            Self::TokenUnavailable => "tokenUnavailable",
            Self::KeychainFailed => "keychainFailed",
            Self::PresentationSubscribeFailed => "presentationSubscribeFailed",
            Self::PresentationStreamFailed => "presentationStreamFailed",
            Self::TunnelFailed => "tunnelFailed",
            Self::Unsupported => "unsupported",
            Self::UnsupportedRemotePlatform => "unsupportedRemotePlatform",
        }
    }

    pub(crate) fn support_log_state(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::DownloadingRemoteServerPackage => "downloadingRemoteServerPackage",
            Self::InstallApprovalRequired => "installApprovalRequired",
            Self::InstallFailed => "installFailed",
            Self::Invalid => "invalid",
            Self::SshFailed => "sshFailed",
            Self::TokenUnavailable => "tokenUnavailable",
            Self::KeychainFailed => "keychainFailed",
            Self::PresentationSubscribeFailed => "presentationSubscribeFailed",
            Self::PresentationStreamFailed => "presentationStreamFailed",
            Self::TunnelFailed => "tunnelFailed",
            Self::Unsupported => "unsupported",
            Self::UnsupportedRemotePlatform => "unsupportedRemotePlatform",
        }
    }
}

pub(crate) fn gpui_remote_gxserver_connect_state_from_wire_status(
    state: &str,
) -> Option<GpuiRemoteGxserverConnectState> {
    match state {
        "connecting" => Some(GpuiRemoteGxserverConnectState::Connecting),
        "connected" => Some(GpuiRemoteGxserverConnectState::Connected),
        "downloadingRemoteServerPackage" => {
            Some(GpuiRemoteGxserverConnectState::DownloadingRemoteServerPackage)
        }
        "installApprovalRequired" => Some(GpuiRemoteGxserverConnectState::InstallApprovalRequired),
        "installFailed" => Some(GpuiRemoteGxserverConnectState::InstallFailed),
        "invalid" => Some(GpuiRemoteGxserverConnectState::Invalid),
        "keychainFailed" => Some(GpuiRemoteGxserverConnectState::KeychainFailed),
        "presentationSubscribeFailed" => {
            Some(GpuiRemoteGxserverConnectState::PresentationSubscribeFailed)
        }
        "presentationStreamFailed" => {
            Some(GpuiRemoteGxserverConnectState::PresentationStreamFailed)
        }
        "sshFailed" => Some(GpuiRemoteGxserverConnectState::SshFailed),
        "tokenUnavailable" => Some(GpuiRemoteGxserverConnectState::TokenUnavailable),
        "tunnelFailed" => Some(GpuiRemoteGxserverConnectState::TunnelFailed),
        "unsupported" => Some(GpuiRemoteGxserverConnectState::Unsupported),
        "unsupportedRemotePlatform" => {
            Some(GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform)
        }
        _ => None,
    }
}

pub(crate) fn gpui_remote_gxserver_status_state_is_known(state: &str) -> bool {
    gpui_remote_gxserver_connect_state_from_wire_status(state).is_some()
        || matches!(state, "disconnected" | "failed" | "installing")
}

pub(crate) fn gpui_remote_gxserver_status_state_is_broken(state: &str) -> bool {
    matches!(
        state,
        "disconnected"
            | "failed"
            | "keychainFailed"
            | "presentationStreamFailed"
            | "presentationSubscribeFailed"
            | "sshFailed"
            | "tokenUnavailable"
            | "tunnelFailed"
    )
}

pub(crate) fn gpui_remote_gxserver_status_state_is_connect_progress(state: &str) -> bool {
    matches!(
        state,
        "connecting" | "downloadingRemoteServerPackage" | "installing"
    )
}

pub(crate) struct GpuiRemoteGxserverConnectResult {
    pub(crate) state: GpuiRemoteGxserverConnectState,
    pub(crate) message: String,
    pub(crate) connection: Option<GpuiRemoteGxserverConnection>,
}

pub(crate) struct GpuiRemoteGxserverConnectProgress {
    pub(crate) state: GpuiRemoteGxserverConnectState,
}

impl GpuiRemoteGxserverConnectResult {
    pub(crate) fn without_connection(state: GpuiRemoteGxserverConnectState, message: &str) -> Self {
        Self {
            state,
            message: message.to_string(),
            connection: None,
        }
    }

    pub(crate) fn connected(connection: GpuiRemoteGxserverConnection) -> Self {
        Self {
            state: GpuiRemoteGxserverConnectState::Connected,
            message: "Remote gxserver is connected.".to_string(),
            connection: Some(connection),
        }
    }

    pub(crate) fn terminate_connection(&mut self) {
        if let Some(mut connection) = self.connection.take() {
            connection.terminate();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteTokenKeychainResult {
    Unsupported,
    Failed,
    Success,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemoteProcessResult {
    pub(crate) exit_code: i32,
    pub(crate) stderr: String,
    pub(crate) stdout: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemoteInstallTarget {
    pub(crate) arch: String,
    pub(crate) distribution: Option<String>,
    pub(crate) os: String,
}

impl GpuiRemoteInstallTarget {
    pub(crate) fn normalized_arch(&self) -> String {
        gpui_normalize_remote_install_arch(self.arch.as_str())
    }

    pub(crate) fn normalized_os(&self) -> String {
        gpui_normalize_remote_install_os(self.os.as_str())
    }

    pub(crate) fn display_label(&self) -> String {
        let os_label = self
            .distribution
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.normalized_os());
        format!("{os_label}/{}", self.normalized_arch())
    }
}

pub(crate) struct GpuiOnDemandArchiveFailure {
    pub(crate) message: String,
    pub(crate) state: GpuiRemoteGxserverConnectState,
}

pub(crate) struct GpuiRemoteAskpassScript {
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) directory: PathBuf,
    pub(crate) password_server: Option<thread::JoinHandle<()>>,
    pub(crate) script: PathBuf,
}

#[cfg(target_os = "macos")]
pub(crate) struct GpuiRemoteSpawnedTunnel {
    pub(crate) child: Child,
    pub(crate) _askpass: Option<GpuiRemoteAskpassScript>,
}

impl Drop for GpuiRemoteAskpassScript {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(password_server) = self.password_server.take() {
            let _ = password_server.join();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub(crate) fn gpui_remote_machine_config_from_settings(
    object: &serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
) -> Option<GpuiRemoteMachineConfig> {
    /*
    CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
    Remote reconnect must source SSH host/user/port/identity/password marker from the normalized shared Settings snapshot, not from the React command. This keeps the modal command bounded to an id/approval flag and prevents injected hostnames, paths, passwords, tokens, or shell text from crossing the app-modal bridge.
    */
    let machines = object.get("remoteMachines")?.as_array()?;
    let machine = machines.iter().find_map(|machine| {
        (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
            .then_some(machine.as_object())
            .flatten()
    })?;
    let ssh_host = gpui_remote_machine_string_field(machine, "sshHost")?;
    let wsl_distribution = gpui_remote_machine_string_field(machine, "wslDistribution");
    if wsl_distribution
        .as_deref()
        .is_some_and(|distribution| !gpui_remote_wsl_distribution_is_valid(distribution))
    {
        return None;
    }
    Some(GpuiRemoteMachineConfig {
        remote_machine_id: remote_machine_id.to_string(),
        ssh_host,
        ssh_identity_file: gpui_remote_machine_string_field(machine, "sshIdentityFile")
            .map(|path| gpui_expand_remote_identity_file(path.as_str())),
        ssh_port: gpui_remote_machine_ssh_port(machine.get("sshPort")),
        ssh_user: gpui_remote_machine_string_field(machine, "sshUser"),
        has_saved_password: machine
            .get("sshPasswordSaved")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        wsl_distribution,
    })
}

pub(crate) fn gpui_remote_wsl_distribution_is_valid(distribution: &str) -> bool {
    let distribution = distribution.trim();
    !distribution.is_empty()
        && distribution.chars().count() <= 120
        && !distribution.starts_with('-')
        && distribution.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '+' | '-' | '(' | ')' | ' ')
        })
}

pub(crate) fn gpui_remote_machine_name_from_settings(remote_machine_id: &str) -> Option<String> {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .and_then(|machines| {
            machines.iter().find_map(|machine| {
                let object = machine.as_object()?;
                (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
                    .then(|| gpui_remote_machine_string_field(object, "name"))
                    .flatten()
            })
        })
}

pub(crate) fn gpui_remote_machine_string_field(
    machine: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    machine
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn gpui_remote_request_id_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .map(str::to_string)
}

pub(crate) fn gpui_remote_gxserver_presentation_client_id(remote_machine_id: &str) -> String {
    format!("{GPUI_SIDEBAR_GXSERVER_CLIENT_ID}:{remote_machine_id}")
}

pub(crate) fn gpui_remote_presentation_client_id_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    command
        .get("clientId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .filter(|value| !value.chars().any(char::is_control))
        .map(str::to_string)
}

pub(crate) fn gpui_remote_path_like_string_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    allow_empty: bool,
) -> Option<String> {
    command
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| allow_empty || !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .map(str::to_string)
}

/*
CDXC:AddProject 2026-07-30:
Every server round trip the shared add-project dialog performs crosses this one
bridge command. The renderer sends a bounded operation name plus the fields that
operation is allowed to carry; Rust owns the endpoint map, the timeouts, and the
local-vs-remote routing, so the CEF modal host never learns a host, a tunnel
port, or a token — it only ever names a machine by its bounded id.
*/
pub(crate) fn gpui_remote_repository_clone_preview_params_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let repository_input =
        gpui_remote_repository_clone_text_field(command, "repositoryInput", 4_096, false)?;
    let folder_path = gpui_remote_path_like_string_from_command(command, "folderPath", false)?;
    let mut params = serde_json::Map::new();
    params.insert("folderPath".to_string(), serde_json::json!(folder_path));
    if let Some(new_folder_name) =
        gpui_remote_repository_clone_text_field(command, "newFolderName", 255, true)
            .filter(|value| !value.is_empty())
    {
        params.insert(
            "newFolderName".to_string(),
            serde_json::json!(new_folder_name),
        );
    }
    params.insert(
        "repositoryInput".to_string(),
        serde_json::json!(repository_input),
    );
    Some(serde_json::Value::Object(params))
}

pub(crate) fn gpui_remote_repository_clone_start_params_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut params = gpui_remote_repository_clone_preview_params_from_command(command)?;
    let object = params.as_object_mut()?;
    if let Some(branch_name) =
        gpui_remote_repository_clone_text_field(command, "branchName", 255, true)
            .filter(|value| !value.is_empty())
    {
        if !gpui_remote_repository_clone_branch_name_allowed(&branch_name) {
            return None;
        }
        object.insert("branchName".to_string(), serde_json::json!(branch_name));
    }
    object.insert(
        "cloneMainOnly".to_string(),
        serde_json::json!(
            command
                .get("cloneMainOnly")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        ),
    );
    object.insert(
        "shallowClone".to_string(),
        serde_json::json!(
            command
                .get("shallowClone")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        ),
    );
    Some(params)
}

pub(crate) fn gpui_remote_repository_clone_text_field(
    command: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    max_chars: usize,
    allow_empty: bool,
) -> Option<String> {
    command
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| allow_empty || !value.is_empty())
        .filter(|value| value.chars().count() <= max_chars)
        .filter(|value| !value.contains('\0'))
        .filter(|value| !value.chars().any(char::is_control))
        .map(str::to_string)
}

pub(crate) fn gpui_remote_repository_clone_branch_name_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 255
        && value != "@"
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '\u{7f}')
                || ch.is_control()
        })
        && value.split('/').all(|segment| {
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
        })
}

pub(crate) fn gpui_remote_repository_clone_job_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(crate) fn gpui_remote_repository_clone_toast_id(request_id: &str) -> String {
    /*
    CDXC:RemoteClone 2026-06-24-19:35:
    Remote clone toast identifiers may be derived from modal request ids for UI replacement, but the id must be bounded and ASCII-sanitized before it crosses back to CEF so renderer-controlled text is never used verbatim as app-modal chrome identity.
    */
    let suffix: String = request_id
        .bytes()
        .take(64)
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
                byte as char
            } else {
                '-'
            }
        })
        .collect();
    let suffix = if suffix.is_empty() {
        "request".to_string()
    } else {
        suffix
    };
    format!("gpui-remote-repository-clone-{suffix}")
}

pub(crate) fn gpui_remote_project_name_from_path(path: &str) -> String {
    path.trim()
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .next_back()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .unwrap_or("Remote Project")
        .chars()
        .take(120)
        .collect()
}

pub(crate) fn gpui_remote_sidebar_request_timeout(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Duration {
    let timeout_ms = command
        .get("timeoutMs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(15_000)
        .clamp(
            GPUI_REMOTE_GXSERVER_SIDEBAR_REQUEST_TIMEOUT_MIN_MS,
            GPUI_REMOTE_GXSERVER_SIDEBAR_REQUEST_TIMEOUT_MAX_MS,
        );
    Duration::from_millis(timeout_ms)
}

pub(crate) fn gpui_remote_sidebar_request_path_allowed(path: &str) -> bool {
    matches!(
        path,
        "/api/createSession"
            | "/api/createAgentSession"
            | "/api/forkSession"
            | "/api/scheduleDelayedSend"
            | "/api/cancelDelayedSend"
            | "/api/sleepSession"
            | "/api/wakeSession"
            | "/api/killSession"
            /*
            CDXC:GPUIRemoteAgentLaunch 2026-08-18:
            Creating a remote agent session is a two-step daemon operation:
            `/api/createAgentSession` writes the row and queues the agent's
            launch startup text, then `/api/startSessionProvider` spawns the
            zmx provider that actually runs the agent, and `/api/sendSessionMessage`
            delivers the workflow prompt. Leaving the second and third steps off
            this allowlist made every remote agent launch report "Remote agent
            failed" at the Rust boundary and silently dropped Git/worktree
            workflow prompts. Params below are reshaped to the two ids (plus a
            bounded message body) so CEF still cannot tunnel startup text,
            commands, or daemon flags to a remote machine.
            */
            | "/api/startSessionProvider"
            | "/api/sendSessionMessage"
            | "/api/updateSession"
            | "/api/requestSessionRename"
            /*
            CDXC:ExportTranscript 2026-08-20:
            A remote session's transcript only exists on the machine that runs
            the agent, so Export Transcript is an id-scoped read-and-write on
            that machine's own daemon, exactly like sleep/wake. Params are
            reshaped to the two ids below and the answer is reduced to the
            written path plus its size.
            */
            | "/api/exportSessionTranscript"
            /*
            CDXC:SidebarV2Lifecycle 2026-07-29:
            Sidebar V2's settle/snooze commands are id-scoped session mutations
            on a remote machine's own daemon, exactly like sleep/wake/kill.
            Their params are reshaped below so CEF can only ever send the two
            ids plus a bounded ISO wake time.
            */
            | "/api/settleSession"
            | "/api/unsettleSession"
            | "/api/snoozeSession"
            | "/api/unsnoozeSession"
            | "/api/listPreviousSessions"
            | "/api/removeSession"
            | "/api/updateProject"
            | "/api/listRecentProjects"
            | "/api/closeProjectToRecent"
            | "/api/restoreRecentProject"
            | "/api/removeRecentProject"
            | "/api/removeProject"
            | "/api/listProjectWorktrees"
            | "/api/createProjectWorktree"
            | "/api/openProjectWorktree"
            /*
            CDXC:SidebarV2LogicalProjects 2026-07-29:
            Sidebar V2's worktree flow, allow-listed for remote machines: only
            the daemon that holds the repository can cut or delete a checkout in
            it, so these are project-scoped mutations on that machine's own
            gxserver, exactly like the settle/snooze pair above. Params are
            reshaped below so CEF can only ever send a project id, bounded
            agent/branch/prompt strings, and one nested existing-worktree path;
            responses are reduced to the created session's ids plus the removal
            verdict.
            */
            | "/api/createWorktreeSession"
            | "/api/removeSessionWorktree"
            | "/api/mergeWorktreeIntoMain"
            | "/api/checkoutProjectNewBranch"
            | "/api/readPresentationSnapshot"
            | "/api/updateSidebarProjectCollections"
            | "/api/runGitAction"
            | "/api/runGitHubAction"
            | "/api/runBeadsAction"
            | "/api/generateCommitMessage"
            | "/api/createPullRequest"
            | "/api/deleteWorktreeProject"
    )
}

pub(crate) fn gpui_remote_sidebar_request_params(
    path: &str,
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIRemoteProjects 2026-06-24-18:22:
    The remote sidebar bridge allowlists project mutation endpoints only for id-scoped operations. Shape params at the Rust boundary so CEF cannot tunnel arbitrary updateProject fields, paths, names, commands, URLs, branch refs, tokens, stdout/stderr, or daemon response authority to a remote gxserver.
    */
    match path {
        "/api/updateProject" => gpui_remote_sidebar_update_project_params(params),
        "/api/updateSidebarProjectCollections" => {
            gpui_remote_sidebar_project_collections_params(params)
        }
        "/api/closeProjectToRecent"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/removeProject"
        | "/api/listProjectWorktrees"
        | "/api/mergeWorktreeIntoMain" => gpui_remote_sidebar_project_id_params(params),
        "/api/scheduleDelayedSend" => gpui_remote_sidebar_delayed_send_params(params, false),
        "/api/cancelDelayedSend" => gpui_remote_sidebar_delayed_send_params(params, true),
        "/api/startSessionProvider" => gpui_remote_sidebar_session_lifecycle_params(params, None),
        "/api/sendSessionMessage" => gpui_remote_sidebar_send_session_message_params(params),
        "/api/settleSession"
        | "/api/unsettleSession"
        | "/api/unsnoozeSession"
        | "/api/exportSessionTranscript" => {
            gpui_remote_sidebar_session_lifecycle_params(params, None)
        }
        "/api/snoozeSession" => {
            gpui_remote_sidebar_session_lifecycle_params(params, Some("snoozedUntil"))
        }
        "/api/requestSessionRename" => gpui_remote_sidebar_request_session_rename_params(params),
        "/api/createProjectWorktree" => gpui_remote_sidebar_create_project_worktree_params(params),
        "/api/openProjectWorktree" => gpui_remote_sidebar_open_project_worktree_params(params),
        "/api/createWorktreeSession" => gpui_remote_sidebar_create_worktree_session_params(params),
        "/api/removeSessionWorktree" => gpui_remote_sidebar_remove_session_worktree_params(params),
        "/api/checkoutProjectNewBranch" => {
            gpui_remote_sidebar_checkout_project_new_branch_params(params)
        }
        _ => Some(params),
    }
}

pub(crate) fn gpui_remote_sidebar_delayed_send_params(
    params: serde_json::Value,
    cancel: bool,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert("projectId".to_string(), serde_json::json!(project_id));
    shaped.insert("sessionId".to_string(), serde_json::json!(session_id));
    if cancel {
        return Some(serde_json::Value::Object(shaped));
    }
    let delay_ms = object.get("delayMs").and_then(serde_json::Value::as_u64);
    let send_when_agent_stops = object
        .get("sendWhenAgentStops")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let send_when_all_project_sessions_stop = object
        .get("sendWhenAllProjectSessionsStop")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    if usize::from(delay_ms.is_some())
        + usize::from(send_when_agent_stops)
        + usize::from(send_when_all_project_sessions_stop)
        != 1
    {
        return None;
    }
    if let Some(delay_ms) = delay_ms {
        gpui_command_delayed_send_duration_from_millis(delay_ms)?;
        shaped.insert("delayMs".to_string(), serde_json::json!(delay_ms));
    } else if send_when_agent_stops {
        shaped.insert("sendWhenAgentStops".to_string(), serde_json::Value::Bool(true));
    } else {
        shaped.insert(
            "sendWhenAllProjectSessionsStop".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_project_id_params(params: serde_json::Value) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    Some(serde_json::json!({ "projectId": project_id }))
}

pub(crate) fn gpui_remote_sidebar_project_collections_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let state = gpui_remote_sidebar_project_collections_state(params.get("state")?)?;
    Some(serde_json::json!({ "state": state }))
}

pub(crate) fn gpui_remote_sidebar_project_collections_state(
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_COLLECTIONS: usize = 256;
    const MAX_PROJECT_IDS_PER_COLLECTION: usize = 512;
    const MAX_ID_CHARS: usize = 256;
    const MAX_TITLE_CHARS: usize = 256;
    const MAX_NEXT_COLLECTION_NUMBER: u64 = 1_000_000;

    let source = value.as_object()?;
    let source_collections = source.get("collections")?.as_object()?;
    let source_order = source.get("order")?.as_array()?;
    let next_collection_number = source.get("nextCollectionNumber")?.as_u64()?;
    if source_collections.len() > MAX_COLLECTIONS
        || source_order.len() != source_collections.len()
        || !(1..=MAX_NEXT_COLLECTION_NUMBER).contains(&next_collection_number)
    {
        return None;
    }

    fn bounded_text(candidate: &str, max_chars: usize) -> Option<&str> {
        let trimmed = candidate.trim();
        (!trimmed.is_empty()
            && trimmed.chars().count() <= max_chars
            && !trimmed.contains('\0')
            && !trimmed.chars().any(char::is_control))
        .then_some(trimmed)
    }
    let valid_color = |candidate: &str| {
        candidate == "transparent"
            || (candidate.len() == 7
                && candidate.starts_with('#')
                && candidate[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    };

    let mut collections = serde_json::Map::new();
    for (collection_id, candidate) in source_collections {
        let normalized_collection_id = bounded_text(collection_id, MAX_ID_CHARS)?;
        let candidate = candidate.as_object()?;
        let embedded_collection_id = candidate.get("collectionId")?.as_str()?;
        if embedded_collection_id != normalized_collection_id {
            return None;
        }
        let title = bounded_text(candidate.get("title")?.as_str()?, MAX_TITLE_CHARS)?;
        let color = candidate.get("color")?.as_str()?;
        let collapsed = candidate.get("collapsed")?.as_bool()?;
        let source_project_ids = candidate.get("projectIds")?.as_array()?;
        if !valid_color(color)
            || source_project_ids.is_empty()
            || source_project_ids.len() > MAX_PROJECT_IDS_PER_COLLECTION
        {
            return None;
        }
        let mut project_ids = Vec::with_capacity(source_project_ids.len());
        for project_id in source_project_ids {
            let project_id = project_id.as_str()?.trim();
            if !gpui_remote_sidebar_project_id_allowed(project_id) {
                return None;
            }
            project_ids.push(serde_json::Value::String(project_id.to_string()));
        }
        collections.insert(
            normalized_collection_id.to_string(),
            serde_json::json!({
                "collapsed": collapsed,
                "collectionId": normalized_collection_id,
                "color": color,
                "projectIds": project_ids,
                "title": title,
            }),
        );
    }

    let mut order = Vec::with_capacity(source_order.len());
    let mut seen_order_ids = std::collections::HashSet::new();
    for collection_id in source_order {
        let collection_id = bounded_text(collection_id.as_str()?, MAX_ID_CHARS)?;
        if !collections.contains_key(collection_id)
            || !seen_order_ids.insert(collection_id.to_string())
        {
            return None;
        }
        order.push(serde_json::Value::String(collection_id.to_string()));
    }

    Some(serde_json::json!({
        "collections": collections,
        "nextCollectionNumber": next_collection_number,
        "order": order,
    }))
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29:
Settle/snooze params reduced to their id scope at the Rust boundary. The only
extra field any of them may carry is `snoozedUntil`, and it is accepted only as
a bounded RFC3339-shaped ASCII timestamp — the remote daemon validates that it
is strictly in the future, but CEF must not be able to tunnel arbitrary text
through this endpoint on the way there.
*/
pub(crate) fn gpui_remote_sidebar_session_lifecycle_params(
    params: serde_json::Value,
    extra_timestamp_key: Option<&str>,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    shaped.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    if let Some(key) = extra_timestamp_key {
        let timestamp = object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| gpui_remote_sidebar_iso_timestamp_allowed(value))?;
        shaped.insert(
            key.to_string(),
            serde_json::Value::String(timestamp.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

/*
CDXC:GPUIRemoteAgentLaunch 2026-08-18:
A remote agent prompt is user-authored message text, not a command: gxserver
types it into the session and submits it. Shape it to the two ids plus the
bounded body and pin `submit` here so this route can never become a way for CEF
to write unsubmitted terminal input or pass daemon-only send flags.
*/
pub(crate) fn gpui_remote_sidebar_send_session_message_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_MESSAGE_BYTES: usize = 32 * 1024;
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let text = object
        .get("text")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= MAX_MESSAGE_BYTES && !value.contains('\0')
        })?;
    Some(serde_json::json!({
        "projectId": project_id,
        "sessionId": session_id,
        "submit": true,
        "text": text,
    }))
}

pub(crate) fn gpui_remote_sidebar_iso_timestamp_allowed(value: &str) -> bool {
    (20..=40).contains(&value.len())
        && value.starts_with(|ch: char| ch.is_ascii_digit())
        && value
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '-' | ':' | '.' | 'T' | 'Z' | '+'))
}

pub(crate) fn gpui_remote_sidebar_request_session_rename_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIRemoteSessionRename 2026-08-12:
    A remote rename may carry only the target ids, bounded normalized title,
    and optional agent id into the selected machine's gxserver. The native
    bridge fixes the request reason/source and opts into daemon-owned command
    submission so CEF cannot turn this route into arbitrary remote terminal
    input.
    */
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let title = gxserver_workspace_terminal_rename_title_field(object, "title").ok()?;
    let mut shaped = serde_json::Map::new();
    shaped.insert("projectId".to_string(), serde_json::json!(project_id));
    shaped.insert("reason".to_string(), serde_json::json!("gpui-sidebar"));
    shaped.insert("sessionId".to_string(), serde_json::json!(session_id));
    shaped.insert(
        "submitAgentRenameCommand".to_string(),
        serde_json::Value::Bool(true),
    );
    shaped.insert("title".to_string(), serde_json::Value::String(title));
    shaped.insert("titleSource".to_string(), serde_json::json!("user"));
    if let Some(agent_name) = object.get("agentName") {
        let agent_name = agent_name
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))?;
        shaped.insert(
            "agentName".to_string(),
            serde_json::Value::String(agent_name.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_update_project_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let git_config = object
        .get("gitConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_git_preferences_update_payload)?;
    Some(serde_json::json!({
        "gitConfig": git_config,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_create_project_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let base_ref = object
        .get("baseRef")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))?;
    let name_hint = object
        .get("nameHint")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_slug_label_allowed(value))?;
    Some(serde_json::json!({
        "baseRef": base_ref,
        "nameHint": name_hint,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_open_project_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let worktree_key = object
        .get("worktreeKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_key_allowed(value))?;
    Some(serde_json::json!({
        "projectId": project_id,
        "worktreeKey": worktree_key,
    }))
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29:
Sidebar V2 worktree-create params, reduced to the P4 wire contract at the Rust
boundary. Every optional field is dropped unless it passes its own shape check,
so a malformed value can never be forwarded verbatim: the remote daemon then
sees a well-formed request missing that field rather than renderer-supplied text
it has to defend against. Only `projectId` is mandatory — everything else has a
server-side default (last agent, project default branch, no prompt).

`existingWorktree.path` is the one path this bridge accepts from the renderer,
and it is accepted only because the flow demands it: the client learned the path
from THAT machine's own presentation (`session.cwd`). It is still bounded and
absolute-only here, and the daemon re-applies its own path-safety normalization
before touching the filesystem.
*/
pub(crate) fn gpui_remote_sidebar_create_worktree_session_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    if let Some(agent_id) = object
        .get("agentId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))
    {
        shaped.insert(
            "agentId".to_string(),
            serde_json::Value::String(agent_id.to_string()),
        );
    }
    if let Some(base_branch) = object
        .get("baseBranch")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))
    {
        shaped.insert(
            "baseBranch".to_string(),
            serde_json::Value::String(base_branch.to_string()),
        );
    }
    if let Some(first_prompt) = object
        .get("firstPrompt")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_first_prompt_allowed(value))
    {
        shaped.insert(
            "firstPrompt".to_string(),
            serde_json::Value::String(first_prompt.to_string()),
        );
    }
    if object
        .get("startFromOrigin")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        shaped.insert("startFromOrigin".to_string(), serde_json::Value::Bool(true));
    }
    if let Some(path) = object
        .get("existingWorktree")
        .and_then(serde_json::Value::as_object)
        .and_then(|worktree| worktree.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    {
        shaped.insert(
            "existingWorktree".to_string(),
            serde_json::json!({ "path": path }),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_remove_session_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let worktree_path = object
        .get("worktreePath")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    shaped.insert(
        "worktreePath".to_string(),
        serde_json::Value::String(worktree_path.to_string()),
    );
    /*
    `force` is a DESTRUCTIVE escalation (delete a dirty checkout), so it is
    forwarded only as an explicit `true`. Any other value simply omits the key
    and the daemon re-applies its dirty refusal.
    */
    if object.get("force").and_then(serde_json::Value::as_bool) == Some(true) {
        shaped.insert("force".to_string(), serde_json::Value::Bool(true));
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_agent_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/*
The optional first prompt is real user prose, so newlines and tabs are legal
where every other bridged string forbids control characters. Everything else in
the control range is still rejected, and the length is bounded so this endpoint
cannot become a bulk channel into the remote daemon.
*/
pub(crate) fn gpui_remote_sidebar_first_prompt_allowed(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= 4_000
        && !value
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
}

pub(crate) fn gpui_remote_sidebar_worktree_path_allowed(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 1_024
        && !value.chars().any(char::is_control)
        && !value.contains("..")
}

pub(crate) fn gpui_remote_sidebar_checkout_project_new_branch_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let branch_label = object
        .get("branchLabel")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_bounded_text_label_allowed(value))?;
    Some(serde_json::json!({
        "branchLabel": branch_label,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_project_id_allowed(value: &str) -> bool {
    let bytes = value.as_bytes();
    (2..=32).contains(&bytes.len())
        && bytes[0] == b'P'
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

/*
CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
The Agents workspace, presentation focus state, and parked shell-state models
key projects by either a raw local gxserver project id or a machine-scoped
remote project id. Both shapes are opaque workspace keys; everything that
persists or validates a workspace project key must accept both, or remote
workspaces are silently dropped and focus snapshots swap to bogus projects.
*/
pub(crate) fn gpui_remote_sidebar_worktree_key_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn gpui_remote_sidebar_git_ref_allowed(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value.len() <= 200
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}

pub(crate) fn gpui_remote_sidebar_slug_label_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
}

pub(crate) fn gpui_remote_sidebar_bounded_text_label_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 160
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_remote_sidebar_git_preferences_update_payload(
    source: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let confirm_commit = json_bool_field(source, "confirmCommit")?;
    let generate_commit_body = json_bool_field(source, "generateCommitBody")?;
    let primary_action = json_string_field(source, "primaryAction")
        .filter(|value| gpui_remote_sidebar_git_action_allowed(*value))?;
    Some(serde_json::json!({
        "confirmCommit": confirm_commit,
        "generateCommitBody": generate_commit_body,
        "primaryAction": primary_action,
    }))
}

pub(crate) fn gpui_remote_sidebar_request_refreshes_presentation(path: &str) -> bool {
    !matches!(
        path,
        "/api/listPreviousSessions"
            | "/api/listRecentProjects"
            | "/api/listProjectWorktrees"
            | "/api/readPresentationSnapshot"
            | "/api/checkoutProjectNewBranch"
            | "/api/runGitAction"
            | "/api/runGitHubAction"
            | "/api/runBeadsAction"
            | "/api/generateCommitMessage"
            | "/api/createPullRequest"
    )
}

pub(crate) fn gpui_remote_sidebar_response_payload(
    path: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:GPUIRemoteSessions 2026-06-24-17:19:
    Response-capable remote sidebar RPCs may return only the sanitized payload shapes explicitly matched here: created-session ids, previous-session metadata, recent-project rows, presentation snapshots, project Git preference metadata, command-stripped typed Git/GitHub/Beads results, generated commit text, PR state confirmation, and delete-warning kinds. Keep path-bearing project list/add and remote native launch data out of this bridge so renderer payloads do not become side-effect authority.
    */
    match path {
        "/api/createSession" | "/api/createAgentSession" => {
            gpui_remote_sidebar_created_session_response_payload(result)
        }
        "/api/listPreviousSessions" => {
            gpui_remote_sidebar_previous_sessions_response_payload(result)
        }
        "/api/listRecentProjects" => gpui_remote_sidebar_recent_projects_response_payload(result),
        "/api/readPresentationSnapshot" => result,
        "/api/scheduleDelayedSend" => serde_json::json!({}),
        "/api/cancelDelayedSend" => serde_json::json!({
            "changed": result
                .get("changed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }),
        "/api/updateSidebarProjectCollections" => result
            .get("sidebarProjectCollections")
            .and_then(gpui_remote_sidebar_project_collections_state)
            .map(|sidebar_project_collections| {
                serde_json::json!({
                    "sidebarProjectCollections": sidebar_project_collections,
                })
            })
            .unwrap_or(serde_json::Value::Null),
        "/api/updateProject"
        | "/api/closeProjectToRecent"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/removeProject"
        | "/api/createProjectWorktree"
        | "/api/openProjectWorktree" => gpui_remote_sidebar_project_response_payload(result),
        "/api/listProjectWorktrees" => {
            gpui_remote_sidebar_project_worktrees_response_payload(result)
        }
        "/api/createWorktreeSession" => {
            gpui_remote_sidebar_create_worktree_session_response_payload(result)
        }
        "/api/removeSessionWorktree" => {
            gpui_remote_sidebar_remove_session_worktree_response_payload(result)
        }
        "/api/mergeWorktreeIntoMain" => gpui_remote_sidebar_merge_worktree_response_payload(result),
        "/api/checkoutProjectNewBranch" => {
            gpui_remote_sidebar_checkout_new_branch_response_payload(result)
        }
        "/api/runGitAction" | "/api/runGitHubAction" | "/api/runBeadsAction" => {
            gpui_remote_sidebar_typed_operation_response_payload(result)
        }
        "/api/generateCommitMessage" => {
            gpui_remote_sidebar_generate_commit_message_response_payload(result)
        }
        "/api/createPullRequest" => {
            gpui_remote_sidebar_create_pull_request_response_payload(result)
        }
        "/api/deleteWorktreeProject" => {
            gpui_remote_sidebar_delete_worktree_response_payload(result)
        }
        "/api/exportSessionTranscript" => {
            gpui_remote_sidebar_export_session_transcript_response_payload(result)
        }
        _ => serde_json::Value::Null,
    }
}

/*
CDXC:ExportTranscript 2026-08-20:
The export answer the sidebar actually consumes: where the markdown landed on
the remote machine (so the dialog can show and copy it, and the seeded prompt
can reference it) and how big it is. The remote daemon also reports its source
transcript path and parse counters; those are diagnostics, so they stop here.
*/
pub(crate) fn gpui_remote_sidebar_export_session_transcript_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let Some(path) = result
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    else {
        return serde_json::Value::Null;
    };
    let mut response = serde_json::Map::new();
    response.insert("path".to_string(), serde_json::json!(path));
    if let Some(bytes) = result.get("bytes").and_then(serde_json::Value::as_u64) {
        response.insert("bytes".to_string(), serde_json::json!(bytes));
    }
    // The agent name only picks the dialog's preselected agent, so it stays a
    // short lowercase token (`claude`, `codex`, `grok`, `pi`).
    if let Some(agent) = result
        .get("agent")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 40
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        response.insert("agent".to_string(), serde_json::json!(agent));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_created_session_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let Some(session) = result.get("session").and_then(serde_json::Value::as_object) else {
        return serde_json::json!({});
    };
    let mut sanitized_session = serde_json::Map::new();
    if let Some(project_id) = session
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        sanitized_session.insert("projectId".to_string(), serde_json::json!(project_id));
    }
    if let Some(session_id) = session
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))
    {
        sanitized_session.insert("sessionId".to_string(), serde_json::json!(session_id));
    }
    if sanitized_session.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({ "session": sanitized_session })
    }
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29:
The worktree-create answer the sidebar actually consumes: the created session's
id (so the host can focus it), the checkout it landed in (so the cleanup prompt
can name it later), and the branch (so the toast/label can state it). Anything
else the daemon returns is dropped, and each field is validated with the same
shape rule the request side uses — a daemon reply is not a reason to relax the
boundary the request had to pass.
*/
pub(crate) fn gpui_remote_sidebar_create_worktree_session_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(session_id) = result
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))
    {
        response.insert("sessionId".to_string(), serde_json::json!(session_id));
    }
    if let Some(worktree_path) = result
        .get("worktreePath")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    {
        response.insert("worktreePath".to_string(), serde_json::json!(worktree_path));
    }
    if let Some(branch) = result
        .get("branch")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))
    {
        response.insert("branch".to_string(), serde_json::json!(branch));
    }
    serde_json::Value::Object(response)
}

/*
The removal verdict. `dirty` is a REFUSAL the sidebar re-asks on, not a failure,
so it has to survive the boundary alongside `removed`. Warnings are already
user-safe prose by contract (never raw git output), but they are still bounded
in count and length here so a misbehaving daemon cannot flood the renderer.
*/
pub(crate) fn gpui_remote_sidebar_remove_session_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    response.insert(
        "removed".to_string(),
        serde_json::Value::Bool(
            result.get("removed").and_then(serde_json::Value::as_bool) == Some(true),
        ),
    );
    if result.get("dirty").and_then(serde_json::Value::as_bool) == Some(true) {
        response.insert("dirty".to_string(), serde_json::Value::Bool(true));
    }
    let warnings = result
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|warning| !warning.is_empty() && !warning.chars().any(char::is_control))
                .take(20)
                .map(|warning| {
                    serde_json::Value::String(warning.chars().take(400).collect::<String>())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !warnings.is_empty() {
        response.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_previous_sessions_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:GPUIRemotePreviousSessions 2026-06-24-17:19:
    Remote previous-session search results only need titles, stable project/session ids, timestamps, tags, and provider identity metadata for restore. Strip path-bearing fields at the Rust boundary before CEF sees the response.
    */
    let cursor = result.get("cursor").cloned();
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_object())
                .map(|row| {
                    let mut sanitized = row.clone();
                    sanitized.remove("agentSessionPath");
                    sanitized.remove("cwd");
                    serde_json::Value::Object(sanitized)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = serde_json::Map::new();
    if let Some(cursor) = cursor {
        response.insert("cursor".to_string(), cursor);
    }
    response.insert("results".to_string(), serde_json::Value::Array(results));
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_project_response_payload(result: serde_json::Value) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(project) = result
        .get("project")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_presentation_project_payload)
    {
        response.insert("project".to_string(), project);
    }
    if let Some(recent_projects) = gpui_remote_sidebar_recent_projects_value(&result) {
        response.insert("recentProjects".to_string(), recent_projects);
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_recent_projects_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    response.insert(
        "recentProjects".to_string(),
        gpui_remote_sidebar_recent_projects_value(&result)
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
    );
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_project_worktrees_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:RemoteWorktrees 2026-06-24-18:40:
    Remote Add Worktree receives display rows plus opaque worktree keys from the
    owning daemon. The bridge must not accept renderer paths for the subsequent
    open-existing mutation; it forwards only daemon-returned rows and strips all
    unrelated response fields before CEF receives them.
    */
    let mut response = serde_json::Map::new();
    if let Some(parent_project_id) = result
        .get("parentProjectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        response.insert(
            "parentProjectId".to_string(),
            serde_json::json!(parent_project_id),
        );
    }
    if let Some(source_project_id) = result
        .get("sourceProjectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        response.insert(
            "sourceProjectId".to_string(),
            serde_json::json!(source_project_id),
        );
    }
    let branches = result
        .get("branches")
        .and_then(serde_json::Value::as_array)
        .map(|branches| {
            branches
                .iter()
                .filter_map(|branch| branch.as_object())
                .filter_map(gpui_remote_sidebar_worktree_branch_payload)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    response.insert("branches".to_string(), serde_json::Value::Array(branches));
    let worktrees = result
        .get("worktrees")
        .and_then(serde_json::Value::as_array)
        .map(|worktrees| {
            worktrees
                .iter()
                .filter_map(|worktree| worktree.as_object())
                .filter_map(gpui_remote_sidebar_worktree_option_payload)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    response.insert("worktrees".to_string(), serde_json::Value::Array(worktrees));
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_worktree_branch_payload(
    branch: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let name = json_string_field(branch, "name")?;
    let mut output = serde_json::Map::new();
    output.insert(
        "current".to_string(),
        serde_json::json!(json_bool_field(branch, "current").unwrap_or(false)),
    );
    output.insert("name".to_string(), serde_json::json!(name));
    output.insert(
        "remote".to_string(),
        serde_json::json!(json_bool_field(branch, "remote").unwrap_or(false)),
    );
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_worktree_option_payload(
    worktree: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let name = json_string_field(worktree, "name")?;
    let path = json_string_field(worktree, "path")?;
    let worktree_key = json_string_field(worktree, "worktreeKey")
        .filter(|value| gpui_remote_sidebar_worktree_key_allowed(value))?;
    let mut output = serde_json::Map::new();
    if let Some(branch) = json_string_field(worktree, "branch") {
        output.insert("branch".to_string(), serde_json::json!(branch));
    }
    output.insert(
        "isCurrentProject".to_string(),
        serde_json::json!(json_bool_field(worktree, "isCurrentProject").unwrap_or(false)),
    );
    output.insert(
        "isRegistered".to_string(),
        serde_json::json!(json_bool_field(worktree, "isRegistered").unwrap_or(false)),
    );
    output.insert("name".to_string(), serde_json::json!(name));
    output.insert("path".to_string(), serde_json::json!(path));
    output.insert("worktreeKey".to_string(), serde_json::json!(worktree_key));
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_merge_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(parent_project_id) = result
        .get("parentProjectId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))
    {
        response.insert(
            "parentProjectId".to_string(),
            serde_json::json!(parent_project_id),
        );
    }
    if let Some(status) = result
        .get("status")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "conflicts" | "merged"))
    {
        response.insert("status".to_string(), serde_json::json!(status));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_checkout_new_branch_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "checkedOut": result
            .get("checkedOut")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn gpui_remote_sidebar_recent_projects_value(
    result: &serde_json::Value,
) -> Option<serde_json::Value> {
    let rows = result.get("recentProjects")?.as_array()?;
    Some(serde_json::Value::Array(
        rows.iter()
            .filter_map(|row| row.as_object())
            .filter_map(gpui_remote_sidebar_recent_project_payload)
            .collect(),
    ))
}

pub(crate) fn gpui_remote_sidebar_recent_project_payload(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let project_id = json_string_field(project, "projectId")?;
    let title = json_string_field(project, "title")?;
    let path = json_string_field(project, "path")?;
    let mut output = serde_json::Map::new();
    output.insert("path".to_string(), serde_json::json!(path));
    output.insert("projectId".to_string(), serde_json::json!(project_id));
    output.insert("title".to_string(), serde_json::json!(title));
    if let Some(icon) = project.get("icon").and_then(serde_json::Value::as_object) {
        output.insert("icon".to_string(), serde_json::Value::Object(icon.clone()));
    }
    if let Some(icon_data_url) = json_string_field(project, "iconDataUrl") {
        output.insert("iconDataUrl".to_string(), serde_json::json!(icon_data_url));
    }
    if let Some(recent_closed_at) = json_string_field(project, "recentClosedAt") {
        output.insert(
            "recentClosedAt".to_string(),
            serde_json::json!(recent_closed_at),
        );
    }
    if let Some(session_count) = json_u64_field(project, "sessionCount") {
        output.insert("sessionCount".to_string(), serde_json::json!(session_count));
    }
    if let Some(theme) = json_string_field(project, "theme") {
        output.insert("theme".to_string(), serde_json::json!(theme));
    }
    if let Some(theme_color) = json_string_field(project, "themeColor") {
        output.insert("themeColor".to_string(), serde_json::json!(theme_color));
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_presentation_project_payload(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIRemoteProjects 2026-06-24-18:22:
    Remote project mutations may return only presentation-shaped project metadata plus sanitized Git preferences. Strip raw domain-only state such as custom commands, agents, launch settings, notifications, history, and board config before CEF receives the response.
    */
    let project_id = json_string_field(project, "projectId")?;
    let title = json_string_field(project, "title")
        .or_else(|| json_string_field(project, "name"))
        .unwrap_or("Project");
    let created_at = json_string_field(project, "createdAt").unwrap_or("");
    let updated_at = json_string_field(project, "updatedAt").unwrap_or(created_at);
    let sort_key = json_string_field(project, "sortKey").unwrap_or(updated_at);
    let mut output = serde_json::Map::new();
    output.insert("createdAt".to_string(), serde_json::json!(created_at));
    output.insert(
        "groupIds".to_string(),
        serde_json::json!(
            json_array_field(project, "groupIds")
                .cloned()
                .unwrap_or_default()
        ),
    );
    if let Some(git_config) = project
        .get("gitConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_git_config_payload)
    {
        output.insert("gitConfig".to_string(), git_config);
    }
    output.insert(
        "isFavorite".to_string(),
        serde_json::json!(json_bool_field(project, "isFavorite").unwrap_or(false)),
    );
    output.insert(
        "isPinned".to_string(),
        serde_json::json!(json_bool_field(project, "isPinned").unwrap_or(false)),
    );
    if let Some(path) = json_string_field(project, "path") {
        output.insert("path".to_string(), serde_json::json!(path));
    }
    output.insert("projectId".to_string(), serde_json::json!(project_id));
    output.insert("sortKey".to_string(), serde_json::json!(sort_key));
    output.insert("title".to_string(), serde_json::json!(title));
    output.insert("updatedAt".to_string(), serde_json::json!(updated_at));
    if let Some(worktree) = project
        .get("worktree")
        .and_then(serde_json::Value::as_object)
    {
        output.insert(
            "worktree".to_string(),
            serde_json::Value::Object(worktree.clone()),
        );
    }
    Some(serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_git_config_payload(
    source: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut output = serde_json::Map::new();
    if let Some(confirm_commit) = json_bool_field(source, "confirmCommit") {
        output.insert(
            "confirmCommit".to_string(),
            serde_json::json!(confirm_commit),
        );
    }
    if let Some(generate_commit_body) = json_bool_field(source, "generateCommitBody") {
        output.insert(
            "generateCommitBody".to_string(),
            serde_json::json!(generate_commit_body),
        );
    }
    if let Some(primary_action) = json_string_field(source, "primaryAction")
        .filter(|value| gpui_remote_sidebar_git_action_allowed(*value))
    {
        output.insert(
            "primaryAction".to_string(),
            serde_json::json!(primary_action),
        );
    }
    (!output.is_empty()).then(|| serde_json::Value::Object(output))
}

pub(crate) fn gpui_remote_sidebar_git_action_allowed(value: &str) -> bool {
    matches!(
        value,
        "commit" | "push" | "pr" | "syncRemote" | "syncMain" | "multiRelease" | "release"
    )
}

pub(crate) fn gpui_remote_sidebar_typed_operation_response_payload(
    mut result: serde_json::Value,
) -> serde_json::Value {
    if let Some(object) = result.as_object_mut() {
        object.remove("command");
    }
    result
}

pub(crate) fn gpui_remote_sidebar_generate_commit_message_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(subject) = result.get("subject").and_then(serde_json::Value::as_str) {
        response.insert("subject".to_string(), serde_json::json!(subject));
    }
    if let Some(body) = result.get("body").and_then(serde_json::Value::as_str) {
        response.insert("body".to_string(), serde_json::json!(body));
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_create_pull_request_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let mut response = serde_json::Map::new();
    if let Some(ok) = result.get("ok").and_then(serde_json::Value::as_bool) {
        response.insert("ok".to_string(), serde_json::json!(ok));
    }
    if let Some(created) = result.get("created").and_then(serde_json::Value::as_bool) {
        response.insert("created".to_string(), serde_json::json!(created));
    }
    if let Some(reason) = result.get("reason").and_then(serde_json::Value::as_str) {
        response.insert("reason".to_string(), serde_json::json!(reason));
    }
    if let Some(pr) = result.get("pr").and_then(serde_json::Value::as_object) {
        let mut sanitized_pr = serde_json::Map::new();
        if let Some(state) = pr.get("state").and_then(serde_json::Value::as_str) {
            sanitized_pr.insert("state".to_string(), serde_json::json!(state));
        }
        if let Some(number) = pr.get("number").and_then(serde_json::Value::as_u64) {
            sanitized_pr.insert("number".to_string(), serde_json::json!(number));
        }
        if !sanitized_pr.is_empty() {
            response.insert("pr".to_string(), serde_json::Value::Object(sanitized_pr));
        }
    }
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_sidebar_delete_worktree_response_payload(
    result: serde_json::Value,
) -> serde_json::Value {
    let warnings = result
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(|warning| {
                    let kind = warning.get("kind").and_then(serde_json::Value::as_str)?;
                    Some(serde_json::json!({ "kind": kind }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut response = serde_json::Map::new();
    response.insert("warnings".to_string(), serde_json::Value::Array(warnings));
    serde_json::Value::Object(response)
}

pub(crate) fn gpui_remote_machine_ssh_port(value: Option<&serde_json::Value>) -> Option<u16> {
    value
        .and_then(serde_json::Value::as_u64)
        .filter(|port| (1..=u16::MAX as u64).contains(port))
        .map(|port| port as u16)
}

pub(crate) fn gpui_expand_remote_identity_file(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" {
        return gpui_path_string(&home_dir());
    }
    if let Some(relative_path) = trimmed.strip_prefix("~/") {
        return gpui_path_string(&home_dir().join(relative_path));
    }
    trimmed.to_string()
}

pub(crate) fn gpui_connect_remote_gxserver(
    config: GpuiRemoteMachineConfig,
    install_approved: bool,
    progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    gpui_connect_remote_gxserver_platform(config, install_approved, progress_tx)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_connect_remote_gxserver_platform(
    _config: GpuiRemoteMachineConfig,
    _install_approved: bool,
    _progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    GpuiRemoteGxserverConnectResult::without_connection(
        GpuiRemoteGxserverConnectState::Unsupported,
        "Remote gxserver connect from Settings is only available in the macOS GPUI build.",
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_connect_remote_gxserver_platform(
    config: GpuiRemoteMachineConfig,
    install_approved: bool,
    progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    // macOS RemoteGxserverInstallDebugLog parity: record the connect/install
    // lifecycle with bounded machine id + state enums only (no hosts, users,
    // ports, paths, tokens, or process output).
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.connectStarted",
        serde_json::json!({
            "installApproved": install_approved,
            "machineId": config.remote_machine_id,
        }),
    );
    let result = gpui_connect_remote_gxserver_platform_inner(config, install_approved, progress_tx);
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        if matches!(result.state, GpuiRemoteGxserverConnectState::Connected) {
            "gpui.remoteGxserver.connectFinished"
        } else {
            "gpui.remoteGxserver.connectFailed"
        },
        serde_json::json!({ "state": result.state.support_log_state() }),
    );
    result
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_connect_remote_gxserver_platform_inner(
    config: GpuiRemoteMachineConfig,
    install_approved: bool,
    progress_tx: Option<mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> GpuiRemoteGxserverConnectResult {
    if config.ssh_host.trim().is_empty() || config.remote_machine_id.trim().is_empty() {
        return GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::Invalid,
            "The saved remote machine is missing required SSH settings.",
        );
    }
    let mut execution_target = GpuiRemoteExecutionTarget::PosixHost;
    let mut token_result = gpui_run_remote_ssh(
        &config,
        gpui_remote_token_read_command(),
        GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
    );
    if token_result.exit_code != 0 {
        /*
        CDXC:GPUIRemoteWindowsWsl 2026-07-26:
        Keep the established Unix SSH command byte-for-byte as the first
        attempt. When that cannot run, identify the SSH host boundary and, for
        Windows OpenSSH, move every Linux runtime operation into the selected
        or default WSL distro before interpreting the gxserver exit contract.
        */
        match gpui_probe_remote_execution_target(&config) {
            Ok(target @ GpuiRemoteExecutionTarget::WindowsWsl { .. }) => {
                token_result = gpui_run_remote_ssh_in_execution_target(
                    &config,
                    &target,
                    gpui_remote_token_read_command(),
                    GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
                );
                execution_target = target;
            }
            Ok(GpuiRemoteExecutionTarget::PosixHost) => {}
            Err(GpuiRemoteExecutionTargetProbeError::Ssh(probe_result)) => {
                return GpuiRemoteGxserverConnectResult::without_connection(
                    GpuiRemoteGxserverConnectState::SshFailed,
                    gpui_remote_sanitized_process_failure(
                        "Remote gxserver SSH setup failed.",
                        &probe_result,
                    )
                    .as_str(),
                );
            }
            Err(GpuiRemoteExecutionTargetProbeError::Unsupported(message)) => {
                if gpui_remote_process_failure_is_ssh_transport(&token_result) {
                    return GpuiRemoteGxserverConnectResult::without_connection(
                        GpuiRemoteGxserverConnectState::SshFailed,
                        gpui_remote_sanitized_process_failure(
                            "Remote gxserver SSH setup failed.",
                            &token_result,
                        )
                        .as_str(),
                    );
                }
                return GpuiRemoteGxserverConnectResult::without_connection(
                    GpuiRemoteGxserverConnectState::UnsupportedRemotePlatform,
                    message.as_str(),
                );
            }
        }
    }
    let installed_managed_package_needs_update = token_result.exit_code == 0
        && gpui_remote_managed_gxserver_package_needs_update(&config, &execution_target);
    if (token_result.exit_code == 127 && install_approved) || installed_managed_package_needs_update
    {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-20:08:
        Approved GPUI Remote installs must be native-owned and packaged-only: after SSH reports gxserver missing, Rust probes the remote OS/CPU, selects a matching app-bundled gxserver package, uploads it over the saved SSH configuration, installs/starts it, and then reuses the existing token/Keychain/tunnel path. Development checkout paths and renderer-provided SSH details are not runtime fallbacks.

        An existing Ghostex-managed package is updated without another install
        approval when its sealed build identity differs from this app's bundled
        package. Leaving a protocol-compatible but stale CLI active can make
        interactive attach use storage contracts that no longer match the
        daemon that produced the token.
        */
        match gpui_install_bundled_remote_gxserver_and_read_token(
            &config,
            &execution_target,
            progress_tx.as_ref(),
        ) {
            Ok(install_result) => {
                if install_result.exit_code != 0 {
                    return GpuiRemoteGxserverConnectResult::without_connection(
                        GpuiRemoteGxserverConnectState::InstallFailed,
                        gpui_remote_sanitized_process_failure(
                            "Remote gxserver install failed.",
                            &install_result,
                        )
                        .as_str(),
                    );
                }
                token_result = install_result;
            }
            Err(result) => return result,
        }
    }
    if let Some(state) =
        gpui_remote_token_read_failure_state(token_result.exit_code, install_approved)
    {
        return GpuiRemoteGxserverConnectResult::without_connection(
            state,
            gpui_remote_token_read_failure_message(state, &token_result).as_str(),
        );
    }

    let token = gpui_extract_remote_gxserver_token(token_result.stdout.as_str());
    if !gpui_is_valid_remote_gxserver_token(token.as_deref().unwrap_or_default()) {
        return GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::TokenUnavailable,
            "Remote gxserver token was not readable after SSH start.",
        );
    }
    let token = token.unwrap_or_default();
    match gpui_save_remote_gxserver_token_to_keychain(&config.remote_machine_id, token.as_str()) {
        GpuiRemoteTokenKeychainResult::Success => {}
        GpuiRemoteTokenKeychainResult::Unsupported => {
            return GpuiRemoteGxserverConnectResult::without_connection(
                GpuiRemoteGxserverConnectState::Unsupported,
                "Remote gxserver token storage is only available on macOS.",
            );
        }
        GpuiRemoteTokenKeychainResult::Failed => {
            return GpuiRemoteGxserverConnectResult::without_connection(
                GpuiRemoteGxserverConnectState::KeychainFailed,
                "Could not store the remote gxserver token in Keychain.",
            );
        }
    }

    match gpui_open_remote_gxserver_tunnel(&config, &execution_target, token.as_str()) {
        Ok(connection) => GpuiRemoteGxserverConnectResult::connected(connection),
        Err(message) => GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::TunnelFailed,
            message.as_str(),
        ),
    }
}

pub(crate) fn gpui_remote_token_read_failure_state(
    exit_code: i32,
    install_approved: bool,
) -> Option<GpuiRemoteGxserverConnectState> {
    match (exit_code, install_approved) {
        (0, _) => None,
        (127, false) => Some(GpuiRemoteGxserverConnectState::InstallApprovalRequired),
        (127, true) => Some(GpuiRemoteGxserverConnectState::InstallFailed),
        _ => Some(GpuiRemoteGxserverConnectState::SshFailed),
    }
}

pub(crate) fn gpui_remote_token_read_failure_message(
    state: GpuiRemoteGxserverConnectState,
    result: &GpuiRemoteProcessResult,
) -> String {
    match state {
        GpuiRemoteGxserverConnectState::InstallApprovalRequired => {
            "gxserver is not installed on that machine. Ask before installing the remote gxserver package.".to_string()
        }
        GpuiRemoteGxserverConnectState::InstallFailed => {
            "Remote gxserver install failed.".to_string()
        }
        GpuiRemoteGxserverConnectState::SshFailed => gpui_remote_sanitized_process_failure(
            "Remote gxserver SSH setup failed.",
            result,
        ),
        _ => "Remote gxserver connect failed.".to_string(),
    }
}

pub(crate) fn gpui_remote_sanitized_process_failure(
    default_message: &str,
    result: &GpuiRemoteProcessResult,
) -> String {
    let stderr = result.stderr.trim().to_ascii_lowercase();
    if stderr.contains("saved ssh password") || stderr.contains("ssh password helper") {
        return "Ghostex could not read the saved SSH password from Keychain. Open Remote settings and save the password again.".to_string();
    }
    if stderr.is_empty() {
        if result.exit_code == 124 {
            return "The SSH connection to the remote machine timed out.".to_string();
        }
        return default_message.to_string();
    }
    if stderr.contains("permission denied") {
        return "SSH authentication failed for the remote machine.".to_string();
    }
    if stderr.contains("could not resolve hostname") {
        return "SSH could not resolve the remote host.".to_string();
    }
    if result.exit_code == 124
        || stderr.contains("operation timed out")
        || stderr.contains("connection timed out")
        || stderr.contains("command timed out")
    {
        return "SSH connection to the remote machine timed out.".to_string();
    }
    default_message.to_string()
}

pub(crate) fn gpui_remote_process_stderr_category(result: &GpuiRemoteProcessResult) -> &'static str {
    let stderr = result.stderr.trim().to_ascii_lowercase();
    if stderr.contains("saved ssh password") {
        return "savedPasswordUnavailable";
    }
    if stderr.contains("ssh password helper") {
        return "passwordHelperFailed";
    }
    if stderr.contains("permission denied") {
        return "authenticationFailed";
    }
    if result.exit_code == 124
        || stderr.contains("operation timed out")
        || stderr.contains("connection timed out")
        || stderr.contains("command timed out")
    {
        return "timedOut";
    }
    if stderr.contains("could not resolve hostname") {
        return "hostResolutionFailed";
    }
    if stderr.contains("connection refused") {
        return "connectionRefused";
    }
    if stderr.contains("host key verification failed") {
        return "hostKeyVerificationFailed";
    }
    if stderr.contains("no route to host") {
        return "noRouteToHost";
    }
    if stderr.is_empty() {
        return "none";
    }
    if result.exit_code == 255 {
        return "sshExit255";
    }
    "other"
}

pub(crate) fn gpui_remote_process_failure_is_ssh_transport(result: &GpuiRemoteProcessResult) -> bool {
    if matches!(result.exit_code, 124 | 255) {
        return true;
    }
    let stderr = result.stderr.trim().to_ascii_lowercase();
    [
        "saved ssh password",
        "ssh password helper",
        "permission denied",
        "could not resolve hostname",
        "connection refused",
        "connection timed out",
        "host key verification failed",
        "no route to host",
        "operation timed out",
    ]
    .iter()
    .any(|needle| stderr.contains(needle))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_probe_remote_execution_target(
    config: &GpuiRemoteMachineConfig,
) -> Result<GpuiRemoteExecutionTarget, GpuiRemoteExecutionTargetProbeError> {
    /*
    A direct SSH endpoint inside WSL can expose cmd.exe through WSL interop.
    Positively identify the SSH login environment first so that endpoint stays
    a POSIX host. Only a login environment that cannot produce the marked
    uname payload is eligible for native Windows detection.
    */
    let posix_probe = gpui_run_remote_ssh_raw(
        config,
        gpui_remote_install_target_probe_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.executionTargetProbe",
        serde_json::json!({
            "exitCode": posix_probe.exit_code,
            "markedOutput": gpui_remote_install_target_probe_is_marked(&posix_probe.stdout),
            "phase": "posix",
            "stderrCategory": gpui_remote_process_stderr_category(&posix_probe),
            "stderrPresent": !posix_probe.stderr.trim().is_empty(),
        }),
    );
    if posix_probe.exit_code == 0 && gpui_remote_install_target_probe_is_marked(&posix_probe.stdout)
    {
        if let Some(target) = gpui_extract_remote_install_target(posix_probe.stdout.as_str()) {
            if matches!(target.normalized_os().as_str(), "darwin" | "linux") {
                return Ok(GpuiRemoteExecutionTarget::PosixHost);
            }
            return Err(GpuiRemoteExecutionTargetProbeError::Unsupported(format!(
                "Remote platform {} is unsupported. Ghostex remote setup supports macOS, Linux, and Windows through WSL2.",
                target.display_label()
            )));
        }
    }
    if gpui_remote_process_failure_is_ssh_transport(&posix_probe) {
        return Err(GpuiRemoteExecutionTargetProbeError::Ssh(posix_probe));
    }

    let windows_probe = gpui_run_remote_ssh_raw(
        config,
        "cmd.exe /d /s /c \"echo __GHOSTEX_REMOTE_WINDOWS__\"",
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.executionTargetProbe",
        serde_json::json!({
            "exitCode": windows_probe.exit_code,
            "markedOutput": windows_probe
                .stdout
                .lines()
                .any(|line| line.trim() == "__GHOSTEX_REMOTE_WINDOWS__"),
            "phase": "windows",
            "stderrCategory": gpui_remote_process_stderr_category(&windows_probe),
            "stderrPresent": !windows_probe.stderr.trim().is_empty(),
        }),
    );
    if windows_probe.exit_code != 0
        || !windows_probe
            .stdout
            .lines()
            .any(|line| line.trim() == "__GHOSTEX_REMOTE_WINDOWS__")
    {
        if gpui_remote_process_failure_is_ssh_transport(&windows_probe) {
            return Err(GpuiRemoteExecutionTargetProbeError::Ssh(windows_probe));
        }
        return Err(GpuiRemoteExecutionTargetProbeError::Unsupported(
            "Ghostex could not identify the remote SSH login environment. Remote setup supports macOS, Linux, and native Windows OpenSSH through WSL2."
                .to_string(),
        ));
    }

    /*
    A blank saved distribution means "the default for a new connection", but
    no command in an active connection may consult that mutable default again.
    Enter it once, read WSL_DISTRO_NAME from that exact instance, validate the
    canonical name, and retain it in the execution target.
    */
    let wsl_probe_command = gpui_remote_wsl_target_probe_command();
    let wsl_probe = gpui_run_remote_ssh_in_windows_wsl(
        config,
        config.wsl_distribution.as_deref(),
        wsl_probe_command.as_str(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.executionTargetProbe",
        serde_json::json!({
            "exitCode": wsl_probe.exit_code,
            "markedOutput": gpui_remote_install_target_probe_is_marked(&wsl_probe.stdout),
            "phase": "wsl",
            "stderrCategory": gpui_remote_process_stderr_category(&wsl_probe),
            "stderrPresent": !wsl_probe.stderr.trim().is_empty(),
        }),
    );
    if wsl_probe.exit_code != 0
        || !gpui_remote_install_target_probe_is_marked(&wsl_probe.stdout)
        || !gpui_extract_remote_install_target(wsl_probe.stdout.as_str())
            .is_some_and(|target| target.normalized_os() == "linux")
    {
        if gpui_remote_process_failure_is_ssh_transport(&wsl_probe) {
            return Err(GpuiRemoteExecutionTargetProbeError::Ssh(wsl_probe));
        }
        return Err(GpuiRemoteExecutionTargetProbeError::Unsupported(
            gpui_remote_wsl_unavailable_message(config),
        ));
    }

    let distribution = gpui_extract_remote_wsl_distribution(wsl_probe.stdout.as_str())
        .filter(|distribution| gpui_remote_wsl_distribution_is_valid(distribution))
        .filter(|distribution| {
            config
                .wsl_distribution
                .as_deref()
                .is_none_or(|requested| requested.eq_ignore_ascii_case(distribution))
        })
        .ok_or_else(|| {
            GpuiRemoteExecutionTargetProbeError::Unsupported(gpui_remote_wsl_unavailable_message(
                config,
            ))
        })?;
    Ok(GpuiRemoteExecutionTarget::WindowsWsl { distribution })
}

pub(crate) fn gpui_remote_wsl_unavailable_message(config: &GpuiRemoteMachineConfig) -> String {
    match config.wsl_distribution.as_deref() {
        Some(distribution) => format!(
            "Windows remote setup could not start the selected WSL distribution '{distribution}'. Initialize that WSL2 distribution or choose its exact name in Remote settings, then reconnect."
        ),
        None => "Windows remote setup requires an initialized default WSL2 Linux distribution. Initialize WSL2 or select a distribution in Remote settings, then reconnect.".to_string(),
    }
}

pub(crate) fn gpui_remote_command_for_execution_target(
    target: &GpuiRemoteExecutionTarget,
    command: &str,
) -> String {
    match target {
        GpuiRemoteExecutionTarget::PosixHost => gpui_login_shell_remote_command(command),
        GpuiRemoteExecutionTarget::WindowsWsl { distribution } => {
            gpui_remote_command_for_windows_wsl(Some(distribution.as_str()), command)
        }
    }
}

pub(crate) fn gpui_remote_command_for_windows_wsl(distribution: Option<&str>, command: &str) -> String {
    /*
    Compress and encode the POSIX login-shell program so PowerShell and cmd
    only parse a short, fixed WSL argv shape. Native Windows OpenSSH rejects
    long non-interactive exec requests before WSL starts them. Decode into a
    private, no-clobber script instead of piping the program into sh: the
    latter would consume SSH stdin and make package streaming or interactive
    attach impossible. Omission of --distribution is allowed only for the
    one-time default-distro probe; retained targets always pass the validated
    canonical name.
    */
    let login_command = gpui_login_shell_remote_command(command);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(login_command.as_bytes())
        .expect("gzip encoding into memory must succeed");
    let compressed_command = encoder
        .finish()
        .expect("gzip encoding into memory must finish");
    let encoded_command = project_board_base64_encode(compressed_command.as_slice());
    let script_path = format!(
        "/tmp/ghostex-remote-{}-{}.sh",
        std::process::id(),
        gpui_remote_install_unique_id()
    );
    /*
    Windows OpenSSH preserves quote characters in the remote command passed
    to wsl.exe, so `--distribution "Name"` asks WSL for a distro whose name
    literally contains quotes. Validated ordinary distro names are already
    safe to place directly in this fixed command shape.
    */
    let distribution_argument = distribution
        .map(|value| format!(" --distribution {value}"))
        .unwrap_or_default();
    format!(
        "wsl.exe{distribution_argument} --exec /bin/sh -lc \"umask 077; set -C; echo {encoded_command} | base64 -d | gzip -dc > {script_path} || exit 126; trap '/bin/rm -f {script_path}' EXIT HUP INT TERM; /bin/sh {script_path}\""
    )
}

pub(crate) fn gpui_remote_wsl_target_probe_command() -> String {
    format!(
        "printf '__GHOSTEX_REMOTE_WSL_DISTRO_START__\\n%s\\n__GHOSTEX_REMOTE_WSL_DISTRO_END__\\n' \"${{WSL_DISTRO_NAME:-}}\"; {}",
        gpui_remote_install_target_probe_command()
    )
}

pub(crate) fn gpui_extract_remote_wsl_distribution(stdout: &str) -> Option<String> {
    let start_marker = "__GHOSTEX_REMOTE_WSL_DISTRO_START__";
    let end_marker = "__GHOSTEX_REMOTE_WSL_DISTRO_END__";
    let start = stdout.find(start_marker)? + start_marker.len();
    let end = stdout[start..].find(end_marker)?;
    let distribution = stdout[start..start + end].trim();
    (!distribution.is_empty()).then(|| distribution.to_string())
}

pub(crate) fn gpui_remote_install_target_probe_is_marked(stdout: &str) -> bool {
    stdout.contains("__GHOSTEX_REMOTE_PLATFORM_START__")
        && stdout.contains("__GHOSTEX_REMOTE_PLATFORM_END__")
}

pub(crate) fn gpui_extract_remote_gxserver_token(stdout: &str) -> Option<String> {
    if let Some(start) = stdout.find(GPUI_REMOTE_GXSERVER_TOKEN_START_MARKER) {
        let token_start = start + GPUI_REMOTE_GXSERVER_TOKEN_START_MARKER.len();
        if let Some(relative_end) =
            stdout[token_start..].find(GPUI_REMOTE_GXSERVER_TOKEN_END_MARKER)
        {
            let token = stdout[token_start..token_start + relative_end].trim();
            return (!token.is_empty()).then(|| token.to_string());
        }
    }
    gpui_first_remote_gxserver_token_like_run(stdout).or_else(|| {
        let trimmed = stdout.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub(crate) fn gpui_first_remote_gxserver_token_like_run(value: &str) -> Option<String> {
    let mut run_start: Option<usize> = None;
    for (index, ch) in value.char_indices() {
        if gpui_is_remote_gxserver_token_char(ch) {
            if run_start.is_none() {
                run_start = Some(index);
            }
            continue;
        }
        if let Some(start) = run_start.take() {
            if index - start >= 32 {
                return Some(value[start..index].to_string());
            }
        }
    }
    let start = run_start?;
    (value.len() - start >= 32).then(|| value[start..].to_string())
}

pub(crate) fn gpui_is_valid_remote_gxserver_token(token: &str) -> bool {
    token.chars().count() >= 32 && token.chars().all(gpui_is_remote_gxserver_token_char)
}

pub(crate) fn gpui_is_remote_gxserver_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

pub(crate) fn gpui_remote_token_read_command() -> &'static str {
    /*
    Remote packages released before the platform-storage migration remain
    runnable from their installed legacy root. Bind the token path to the
    binary that actually owns the remote daemon; current packages use the
    native State/Data contract, while an explicitly legacy-installed binary
    keeps its matching token contract until that remote package is upgraded.
    */
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"$GHOSTEX_HOME/state\"; GHOSTEX_REMOTE_DATA_DIR=\"$GHOSTEX_HOME\";; ",
        "*) case \"${XDG_STATE_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"${XDG_STATE_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_STATE_DIR=\"$HOME/.local/state/ghostex\";; esac; ",
        "case \"${XDG_DATA_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"${XDG_DATA_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_DATA_DIR=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_TOKEN_FILE=\"$GHOSTEX_REMOTE_STATE_DIR/gxserver/auth/token\"; ",
        "GHOSTEX_REMOTE_LEGACY_ROOT=\"$HOME/.ghostex/gxserver\"; ",
        "GXSERVER_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/gxserver\"; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\" ]; then GXSERVER_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$HOME/.local/bin/gxserver\" ]; then GXSERVER_BIN=\"$HOME/.local/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/gxserver\" ]; then GXSERVER_BIN=\"/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/gxserver\"; fi; ",
        "GHOSTEX_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/ghostex\"; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\" ]; then GHOSTEX_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\"; fi; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$HOME/.local/bin/ghostex\" ]; then GHOSTEX_BIN=\"$HOME/.local/bin/ghostex\"; fi; ",
        "GHOSTEX_REMOTE_START_FAILED=0; ",
        "if [ -x \"$GXSERVER_BIN\" ]; then ",
        "GHOSTEX_REMOTE_COMMAND_BIN=\"$GXSERVER_BIN\"; ",
        "\"$GXSERVER_BIN\" start --json >/dev/null 2>&1 || \"$GXSERVER_BIN\" start >/dev/null 2>&1 || GHOSTEX_REMOTE_START_FAILED=1; ",
        "elif [ -x \"$GHOSTEX_BIN\" ]; then ",
        "GHOSTEX_REMOTE_COMMAND_BIN=\"$GHOSTEX_BIN\"; ",
        "\"$GHOSTEX_BIN\" server start --json >/dev/null 2>&1 || \"$GHOSTEX_BIN\" server start >/dev/null 2>&1 || GHOSTEX_REMOTE_START_FAILED=1; ",
        "else exit 127; fi; ",
        "GHOSTEX_REMOTE_COMMAND_LINK=\"$(readlink \"$GHOSTEX_REMOTE_COMMAND_BIN\" 2>/dev/null || true)\"; ",
        "case \"$GHOSTEX_REMOTE_COMMAND_BIN|$GHOSTEX_REMOTE_COMMAND_LINK\" in *\"$GHOSTEX_REMOTE_LEGACY_ROOT/\"*) GHOSTEX_REMOTE_TOKEN_FILE=\"$GHOSTEX_REMOTE_LEGACY_ROOT/auth/token\";; esac; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_TOKEN_FILE\" ]; then if [ \"$GHOSTEX_REMOTE_START_FAILED\" = \"1\" ]; then exit 127; fi; exit 126; fi; ",
        "printf '__GHOSTEX_REMOTE_TOKEN_START__\\n'; ",
        "cat \"$GHOSTEX_REMOTE_TOKEN_FILE\"; ",
        "printf '\\n__GHOSTEX_REMOTE_TOKEN_END__\\n'"
    )
}

pub(crate) fn gpui_remote_windows_wsl_gxserver_owner_command() -> &'static str {
    /*
    Windows WSL2 stops a distribution after its last Windows-owned execution
    ends, even when gxserver detached successfully inside Linux. Keep the SSH
    tunnel's remote command attached to the exact gxserver pid that produced
    the saved token. The command neither restarts nor substitutes for gxserver:
    it exits when that daemon exits, so the tunnel and WSL lifetime share one
    honest owner.
    */
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"$GHOSTEX_HOME/state\";; ",
        "*) case \"${XDG_STATE_HOME:-}\" in /*) GHOSTEX_REMOTE_STATE_DIR=\"${XDG_STATE_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_STATE_DIR=\"$HOME/.local/state/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_TOKEN_FILE=\"$GHOSTEX_REMOTE_STATE_DIR/gxserver/auth/token\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_TOKEN_FILE\" ] && [ -r \"$HOME/.ghostex/gxserver/auth/token\" ]; then GHOSTEX_REMOTE_TOKEN_FILE=\"$HOME/.ghostex/gxserver/auth/token\"; fi; ",
        "GHOSTEX_REMOTE_RUNTIME_FILE=\"${GHOSTEX_REMOTE_TOKEN_FILE%/auth/token}/runtime/server.json\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_RUNTIME_FILE\" ]; then exit 1; fi; ",
        "GHOSTEX_REMOTE_GXSERVER_PID=\"$(sed -n 's/.*\"pid\"[[:space:]]*:[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p' \"$GHOSTEX_REMOTE_RUNTIME_FILE\" | head -n 1)\"; ",
        "case \"$GHOSTEX_REMOTE_GXSERVER_PID\" in ''|*[!0-9]*) exit 1;; esac; ",
        "kill -0 \"$GHOSTEX_REMOTE_GXSERVER_PID\" 2>/dev/null || exit 1; ",
        "while kill -0 \"$GHOSTEX_REMOTE_GXSERVER_PID\" 2>/dev/null; do sleep 5; done"
    )
}

pub(crate) fn gpui_remote_managed_gxserver_build_identity_command() -> &'static str {
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"$GHOSTEX_HOME\";; ",
        "*) case \"${XDG_DATA_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"${XDG_DATA_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_DATA_DIR=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_IDENTITY_FILE=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/build-identity.json\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ] && [ -r \"$HOME/.ghostex/gxserver/package/build-identity.json\" ]; then GHOSTEX_REMOTE_IDENTITY_FILE=\"$HOME/.ghostex/gxserver/package/build-identity.json\"; fi; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ]; then exit 3; fi; ",
        "printf '__GHOSTEX_REMOTE_BUILD_IDENTITY_START__\\n'; ",
        "cat \"$GHOSTEX_REMOTE_IDENTITY_FILE\"; ",
        "printf '\\n__GHOSTEX_REMOTE_BUILD_IDENTITY_END__\\n'"
    )
}

pub(crate) fn gpui_extract_remote_managed_gxserver_build_identity(stdout: &str) -> Option<String> {
    let start = stdout.find(GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_START_MARKER)?
        + GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_START_MARKER.len();
    let payload = &stdout[start..];
    let end = payload.find(GPUI_REMOTE_GXSERVER_BUILD_IDENTITY_END_MARKER)?;
    serde_json::from_str::<serde_json::Value>(payload[..end].trim())
        .ok()?
        .get("buildIdentity")?
        .as_str()
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .map(str::to_string)
}

pub(crate) fn gpui_bundled_remote_gxserver_build_identity(package_dir: &Path) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(
        &fs::read_to_string(package_dir.join("build-identity.json")).ok()?,
    )
    .ok()?
    .get("buildIdentity")?
    .as_str()
    .map(str::trim)
    .filter(|identity| !identity.is_empty())
    .map(str::to_string)
}

pub(crate) fn gpui_remote_installed_gxserver_version_command() -> &'static str {
    /*
    CDXC:RemoteMachines 2026-08-19:
    Settings only needs to know whether the saved machine already carries a
    gxserver package and which version that package is, so this probe reads the
    installed package identity (or asks an already installed binary for its
    version) and never starts, installs, or upgrades anything.

    Resolve the same binaries the token command owns, including the copy inside
    an installed macOS Ghostex.app, or a macOS remote that runs gxserver from
    its own app bundle reads as "not installed". The package identity is taken
    from the resolved binary's own package root (following a ~/.local/bin
    symlink) so the reported version always describes the gxserver that would
    actually serve this machine.
    */
    concat!(
        "case \"${GHOSTEX_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"$GHOSTEX_HOME\";; ",
        "*) case \"${XDG_DATA_HOME:-}\" in /*) GHOSTEX_REMOTE_DATA_DIR=\"${XDG_DATA_HOME%/}/ghostex\";; *) GHOSTEX_REMOTE_DATA_DIR=\"$HOME/.local/share/ghostex\";; esac;; esac; ",
        "GHOSTEX_REMOTE_LEGACY_ROOT=\"$HOME/.ghostex/gxserver\"; ",
        "GHOSTEX_REMOTE_APP_ROOT=\"/Applications/Ghostex.app/Contents/Resources/Web/gxserver\"; ",
        "GXSERVER_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/gxserver\"; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\" ]; then GXSERVER_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$HOME/.local/bin/gxserver\" ]; then GXSERVER_BIN=\"$HOME/.local/bin/gxserver\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_APP_ROOT/bin/gxserver\" ]; then GXSERVER_BIN=\"$GHOSTEX_REMOTE_APP_ROOT/bin/gxserver\"; fi; ",
        "GHOSTEX_BIN=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/bin/ghostex\"; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\" ]; then GHOSTEX_BIN=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/bin/ghostex\"; fi; ",
        "if [ ! -x \"$GHOSTEX_BIN\" ] && [ -x \"$HOME/.local/bin/ghostex\" ]; then GHOSTEX_BIN=\"$HOME/.local/bin/ghostex\"; fi; ",
        "if [ ! -x \"$GXSERVER_BIN\" ] && [ ! -x \"$GHOSTEX_BIN\" ]; then exit 3; fi; ",
        "GHOSTEX_REMOTE_RESOLVED_BIN=\"$GXSERVER_BIN\"; ",
        "if [ ! -x \"$GHOSTEX_REMOTE_RESOLVED_BIN\" ]; then GHOSTEX_REMOTE_RESOLVED_BIN=\"$GHOSTEX_BIN\"; fi; ",
        "GHOSTEX_REMOTE_BIN_LINK=\"$(readlink \"$GHOSTEX_REMOTE_RESOLVED_BIN\" 2>/dev/null || true)\"; ",
        "case \"$GHOSTEX_REMOTE_BIN_LINK\" in /*) GHOSTEX_REMOTE_RESOLVED_BIN=\"$GHOSTEX_REMOTE_BIN_LINK\";; ?*) GHOSTEX_REMOTE_RESOLVED_BIN=\"$(dirname \"$GHOSTEX_REMOTE_RESOLVED_BIN\")/$GHOSTEX_REMOTE_BIN_LINK\";; esac; ",
        "GHOSTEX_REMOTE_IDENTITY_FILE=\"$(dirname \"$(dirname \"$GHOSTEX_REMOTE_RESOLVED_BIN\")\")/build-identity.json\"; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ] && [ -r \"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/build-identity.json\" ]; then GHOSTEX_REMOTE_IDENTITY_FILE=\"$GHOSTEX_REMOTE_DATA_DIR/gxserver/package/build-identity.json\"; fi; ",
        "if [ ! -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ] && [ -r \"$GHOSTEX_REMOTE_LEGACY_ROOT/package/build-identity.json\" ]; then GHOSTEX_REMOTE_IDENTITY_FILE=\"$GHOSTEX_REMOTE_LEGACY_ROOT/package/build-identity.json\"; fi; ",
        "printf '__GHOSTEX_REMOTE_GXSERVER_VERSION_START__\\n'; ",
        "if [ -r \"$GHOSTEX_REMOTE_IDENTITY_FILE\" ]; then cat \"$GHOSTEX_REMOTE_IDENTITY_FILE\"; elif [ -x \"$GXSERVER_BIN\" ]; then \"$GXSERVER_BIN\" --version 2>/dev/null || true; fi; ",
        "printf '\\n__GHOSTEX_REMOTE_GXSERVER_VERSION_END__\\n'"
    )
}

pub(crate) fn gpui_extract_remote_installed_gxserver_version(stdout: &str) -> Option<String> {
    let start = stdout.find(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER)?
        + GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER.len();
    let payload = &stdout[start..];
    let end = payload.find(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_END_MARKER)?;
    let payload = payload[..end].trim();
    if payload.is_empty() {
        return None;
    }
    if let Ok(identity) = serde_json::from_str::<serde_json::Value>(payload) {
        /*
        Ghostex-managed packages ship build-identity.json, where the version is
        either its own field or the middle segment of `gxserver:<version>:<fingerprint>`.
        */
        if let Some(version) = identity
            .get("packageVersion")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_sanitized_remote_gxserver_version)
        {
            return Some(version);
        }
        return identity
            .get("buildIdentity")
            .and_then(serde_json::Value::as_str)
            .and_then(|identity| identity.split(':').nth(1))
            .and_then(gpui_sanitized_remote_gxserver_version);
    }
    gpui_sanitized_remote_gxserver_version(payload.lines().next().unwrap_or_default())
}

pub(crate) fn gpui_sanitized_remote_gxserver_version(raw: &str) -> Option<String> {
    let version = raw
        .trim()
        .strip_prefix("gxserver")
        .unwrap_or(raw)
        .trim()
        .to_string();
    if version.is_empty() || version.len() > GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_MAX_LENGTH {
        return None;
    }
    version
        .chars()
        .all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+')
        })
        .then_some(version)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GpuiRemoteGxserverInstallProbe {
    pub(crate) installed: bool,
    pub(crate) version: Option<String>,
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_probe_remote_gxserver_install(
    _config: GpuiRemoteMachineConfig,
) -> GpuiRemoteGxserverInstallProbe {
    GpuiRemoteGxserverInstallProbe::default()
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_probe_remote_gxserver_install(
    config: GpuiRemoteMachineConfig,
) -> GpuiRemoteGxserverInstallProbe {
    /*
    CDXC:RemoteMachines 2026-08-19:
    The Remote settings action must say Install for a machine with no gxserver
    and Update for one that already has it. Read that state over the saved SSH
    configuration only.

    The probe's own exit code decides which login environment answered: the
    marked payload and the "nothing installed" code can only come from a shell
    that actually ran the script, so any other outcome means this endpoint is
    not a POSIX host. Native Windows OpenSSH is exactly that case and keeps
    gxserver inside WSL2, so the same script is re-run in the saved (or
    default) distribution instead of reporting the machine as missing gxserver.
    */
    if config.ssh_host.trim().is_empty() {
        return GpuiRemoteGxserverInstallProbe::default();
    }
    let posix_result = gpui_run_remote_ssh(
        &config,
        gpui_remote_installed_gxserver_version_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    gpui_log_remote_gxserver_install_probe(&config, "posix", &posix_result);
    if let Some(probe) = gpui_remote_gxserver_install_probe_from_result(&posix_result) {
        return probe;
    }
    if !posix_result.stderr.trim().is_empty()
        && gpui_remote_process_failure_is_ssh_transport(&posix_result)
    {
        /*
        SSH itself reported why it could not reach the machine, so there is no
        second login environment to try. A bare non-zero exit with no SSH
        diagnosis is what a native Windows shell produces for POSIX script
        text, and that case must still reach the WSL attempt below.
        */
        return GpuiRemoteGxserverInstallProbe::default();
    }
    /*
    A WSL2 distribution that no command has entered yet has to boot before it
    can answer, so give this attempt the longer connect budget instead of the
    short probe budget used for an already-running POSIX login shell.
    */
    let wsl_result = gpui_run_remote_ssh_in_windows_wsl(
        &config,
        config.wsl_distribution.as_deref(),
        gpui_remote_installed_gxserver_version_command(),
        GPUI_REMOTE_GXSERVER_CONNECT_TIMEOUT,
    );
    gpui_log_remote_gxserver_install_probe(&config, "windowsWsl", &wsl_result);
    gpui_remote_gxserver_install_probe_from_result(&wsl_result).unwrap_or_default()
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_gxserver_install_probe_from_result(
    result: &GpuiRemoteProcessResult,
) -> Option<GpuiRemoteGxserverInstallProbe> {
    if result
        .stdout
        .contains(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER)
    {
        return Some(GpuiRemoteGxserverInstallProbe {
            installed: true,
            version: gpui_extract_remote_installed_gxserver_version(result.stdout.as_str()),
        });
    }
    (result.exit_code == GPUI_REMOTE_GXSERVER_NOT_INSTALLED_EXIT_CODE)
        .then(GpuiRemoteGxserverInstallProbe::default)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_log_remote_gxserver_install_probe(
    config: &GpuiRemoteMachineConfig,
    phase: &str,
    result: &GpuiRemoteProcessResult,
) {
    // Bounded machine id, phase, and process outcome only: no hosts, users,
    // ports, paths, tokens, or process output.
    support_logs::append(
        support_logs::GpuiSupportLog::RemoteGxserverInstall,
        "gpui.remoteGxserver.installStateProbe",
        serde_json::json!({
            "exitCode": result.exit_code,
            "machineId": config.remote_machine_id,
            "markedOutput": result
                .stdout
                .contains(GPUI_REMOTE_GXSERVER_INSTALLED_VERSION_START_MARKER),
            "phase": phase,
            "stderrCategory": gpui_remote_process_stderr_category(result),
            "stderrPresent": !result.stderr.trim().is_empty(),
        }),
    );
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_managed_gxserver_package_needs_update(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> bool {
    let target_probe = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_install_target_probe_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    let Some(target) = (target_probe.exit_code == 0)
        .then(|| gpui_extract_remote_install_target(target_probe.stdout.as_str()))
        .flatten()
    else {
        return false;
    };
    let Some(package_dir) = gpui_bundled_remote_gxserver_package_dir(&target) else {
        return false;
    };
    let Some(expected_identity) =
        gpui_bundled_remote_gxserver_build_identity(package_dir.as_path())
    else {
        return false;
    };
    let installed_identity = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_managed_gxserver_build_identity_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    if installed_identity.exit_code != 0 {
        return false;
    }
    gpui_extract_remote_managed_gxserver_build_identity(installed_identity.stdout.as_str())
        .is_some_and(|identity| identity != expected_identity)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_install_bundled_remote_gxserver_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    progress_tx: Option<&mpsc::UnboundedSender<GpuiRemoteGxserverConnectProgress>>,
) -> Result<GpuiRemoteProcessResult, GpuiRemoteGxserverConnectResult> {
    let probe_result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_install_target_probe_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    if probe_result.exit_code != 0 {
        return Err(GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::InstallFailed,
            "Could not identify the remote operating system before installing gxserver.",
        ));
    }
    let Some(target) = gpui_extract_remote_install_target(probe_result.stdout.as_str()) else {
        return Err(GpuiRemoteGxserverConnectResult::without_connection(
            GpuiRemoteGxserverConnectState::InstallFailed,
            "Could not identify the remote operating system before installing gxserver.",
        ));
    };
    if let Some(package_dir) = gpui_bundled_remote_gxserver_package_dir(&target) {
        return Ok(gpui_upload_install_bundled_remote_gxserver_and_read_token(
            config,
            execution_target,
            package_dir.as_path(),
        ));
    }
    match gpui_on_demand_gxserver_archive(&target, progress_tx) {
        Ok(archive_path) => Ok(gpui_install_gxserver_archive_and_read_token(
            config,
            execution_target,
            archive_path.as_path(),
        )),
        Err(failure) => Err(GpuiRemoteGxserverConnectResult::without_connection(
            failure.state,
            failure.message.as_str(),
        )),
    }
}

pub(crate) fn gpui_remote_install_target_probe_command() -> &'static str {
    concat!(
        "GHOSTEX_REMOTE_OS=\"$(uname -s 2>/dev/null || true)\"; ",
        "GHOSTEX_REMOTE_ARCH=\"$(uname -m 2>/dev/null || true)\"; ",
        "GHOSTEX_REMOTE_DIST=\"\"; ",
        "if [ -r /etc/os-release ]; then ",
        "GHOSTEX_REMOTE_DIST=\"$(sed -n 's/^ID=//p' /etc/os-release 2>/dev/null | head -n 1 | tr -d '\"' || true)\"; ",
        "fi; ",
        "printf '__GHOSTEX_REMOTE_PLATFORM_START__\\n'; ",
        "printf '%s\\n' \"$GHOSTEX_REMOTE_OS\"; ",
        "printf '%s\\n' \"$GHOSTEX_REMOTE_ARCH\"; ",
        "printf '%s\\n' \"$GHOSTEX_REMOTE_DIST\"; ",
        "printf '__GHOSTEX_REMOTE_PLATFORM_END__\\n'"
    )
}

pub(crate) fn gpui_extract_remote_install_target(stdout: &str) -> Option<GpuiRemoteInstallTarget> {
    let payload = if let Some(start) = stdout.find("__GHOSTEX_REMOTE_PLATFORM_START__") {
        let payload_start = start + "__GHOSTEX_REMOTE_PLATFORM_START__".len();
        stdout[payload_start..]
            .find("__GHOSTEX_REMOTE_PLATFORM_END__")
            .map(|end| &stdout[payload_start..payload_start + end])
            .unwrap_or(stdout)
    } else {
        stdout
    };
    /*
    The probe prints the start marker as its own line, so the marked slice
    begins with that line's delimiter. Remove exactly that delimiter before
    assigning OS/arch/distribution fields; broad whitespace trimming would
    hide a genuinely missing OS field. macOS intentionally leaves the third
    distribution line empty.
    */
    let payload = payload
        .strip_prefix("\r\n")
        .or_else(|| payload.strip_prefix('\n'))
        .unwrap_or(payload);
    let lines = payload
        .lines()
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() < 2 || lines[0].is_empty() || lines[1].is_empty() {
        return None;
    }
    Some(GpuiRemoteInstallTarget {
        arch: lines[1].clone(),
        distribution: lines
            .get(2)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        os: lines[0].clone(),
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_bundled_remote_gxserver_package_dir(target: &GpuiRemoteInstallTarget) -> Option<PathBuf> {
    let names = gpui_bundled_remote_gxserver_package_resource_names(target);
    if names.is_empty() {
        return None;
    }
    let resources_dir = gpui_app_bundle_resources_dir()?;
    for resource_name in names {
        let package_dir = resources_dir.join(resource_name);
        if gpui_is_dir(&package_dir)
            && gpui_bundled_remote_gxserver_package_is_compatible(&package_dir, target)
        {
            return Some(package_dir);
        }
    }
    None
}

pub(crate) fn gpui_bundled_remote_gxserver_package_resource_names(
    target: &GpuiRemoteInstallTarget,
) -> Vec<&'static str> {
    let os = target.normalized_os();
    let arch = target.normalized_arch();
    if os == "linux" && arch == "x64" {
        return vec!["Web/gxserver-linux-x64", "Web/gxserver-linux-amd64"];
    }
    if os == "linux" && arch == "arm64" {
        return vec!["Web/gxserver-linux-arm64", "Web/gxserver-linux-aarch64"];
    }
    if os == "darwin" && arch == "arm64" {
        return if gpui_bundled_host_remote_gxserver_package_arch() == "arm64" {
            vec!["Web/gxserver-darwin-arm64", "Web/gxserver"]
        } else {
            vec!["Web/gxserver-darwin-arm64"]
        };
    }
    if os == "darwin" && arch == "x64" {
        return if gpui_bundled_host_remote_gxserver_package_arch() == "x64" {
            vec!["Web/gxserver-darwin-x64", "Web/gxserver"]
        } else {
            vec!["Web/gxserver-darwin-x64"]
        };
    }
    Vec::new()
}

pub(crate) fn gpui_bundled_host_remote_gxserver_package_arch() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(target_arch = "x86_64")]
    {
        "x64"
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "unknown"
    }
}

pub(crate) fn gpui_bundled_remote_gxserver_package_is_compatible(
    package_dir: &Path,
    target: &GpuiRemoteInstallTarget,
) -> bool {
    for relative_path in ["bin/gxserver", "bin/zmx", "bin/bd"] {
        if !gpui_is_file(&package_dir.join(relative_path)) {
            return false;
        }
    }
    if target.normalized_os() != "linux" {
        return true;
    }
    // CDXC:GhostexRustCli 2026-07-13: the public CLI is the native bin/ghostex
    // built from server; packages with only the old CLI/ghostex-cli.mjs
    // Node entrypoint are stale. Linux remote packages no longer ship a Node
    // runtime at all.
    let arch = target.normalized_arch();
    for relative_path in ["bin/gxserver", "bin/ghostex", "bin/zmx", "bin/bd"] {
        let path = package_dir.join(relative_path);
        if gpui_is_macho_binary(&path) || !gpui_is_elf_binary(&path, Some(arch.as_str())) {
            return false;
        }
    }
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_app_bundle_resources_dir() -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;
    let bundle_root = find_app_bundle_root(&executable)?;
    Some(bundle_root.join("Contents/Resources"))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_unsupported_remote_package_message(target: &GpuiRemoteInstallTarget) -> String {
    format!(
        "This Ghostex app bundle does not include a gxserver package for {}. Install a Ghostex build that includes a matching remote gxserver package, then retry.",
        target.display_label()
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_install_bundled_remote_gxserver_and_read_token(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    package_dir: &Path,
) -> GpuiRemoteProcessResult {
    let temp_dir = env::temp_dir().join(format!(
        "ghostex-gpui-remote-gxserver-{}-{}",
        std::process::id(),
        gpui_remote_install_unique_id()
    ));
    let archive_path = temp_dir.join("gxserver.tar.gz");
    if fs::create_dir_all(&temp_dir).is_err() {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Could not prepare gxserver upload archive.".to_string(),
            stdout: String::new(),
        };
    }
    let result = gpui_upload_install_bundled_remote_gxserver_and_read_token_inner(
        config,
        execution_target,
        package_dir,
        archive_path.as_path(),
    );
    let _ = fs::remove_dir_all(&temp_dir);
    result
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_install_bundled_remote_gxserver_and_read_token_inner(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    package_dir: &Path,
    archive_path: &Path,
) -> GpuiRemoteProcessResult {
    let mut tar_environment = HashMap::new();
    tar_environment.insert("COPYFILE_DISABLE".to_string(), "1".to_string());
    let tar_arguments = vec![
        "-czf".to_string(),
        gpui_path_string(archive_path),
        "-C".to_string(),
        gpui_path_string(package_dir),
        ".".to_string(),
    ];
    let tar_result = gpui_run_remote_process(
        "/usr/bin/tar",
        &tar_arguments,
        Some(tar_environment),
        GPUI_REMOTE_GXSERVER_ARCHIVE_TIMEOUT,
    );
    if tar_result.exit_code != 0 {
        return GpuiRemoteProcessResult {
            exit_code: tar_result.exit_code,
            stderr: "Could not archive bundled gxserver package.".to_string(),
            stdout: String::new(),
        };
    }
    gpui_install_gxserver_archive_and_read_token(config, execution_target, archive_path)
}

pub(crate) fn gpui_remote_gxserver_install_command(release_id: &str) -> String {
    let token_read = gpui_remote_token_read_command();
    /*
    CDXC:RemoteMinimalDeps 2026-07-13:
    Package activation (stale-listener stop, package symlink swap, tool links
    into ~/.local/bin, ghostex CLI wrapper) moved into the uploaded package's
    own `gxserver setup` subcommand so every installer shares one Rust
    implementation. The shell script keeps only what must run before the new
    binary exists: extract the upload and invoke setup. The app and its
    remote packages are version-paired through the sealed asset manifest, so
    the uploaded gxserver always understands `setup`.
    */
    format!(
        r#"set -eu
case "${{GHOSTEX_HOME:-}}" in
  /*) ghostex_data_dir="$GHOSTEX_HOME" ;;
  *) case "${{XDG_DATA_HOME:-}}" in
       /*) ghostex_data_dir="${{XDG_DATA_HOME%/}}/ghostex" ;;
       *) ghostex_data_dir="$HOME/.local/share/ghostex" ;;
     esac ;;
esac
install_root="$ghostex_data_dir/gxserver"
upload_path="$install_root/gxserver-upload.tar.gz"
release_dir="$install_root/releases/{release_id}"
mkdir -p "$release_dir"
tar -xzf "$upload_path" -C "$release_dir"
chmod 755 "$release_dir/bin/gxserver"
"$release_dir/bin/gxserver" setup --install-root "$install_root" --release-dir "$release_dir" --upload-path "$upload_path"
{token_read}"#
    )
}

pub(crate) fn gpui_remote_install_unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub(crate) fn gpui_normalize_remote_install_os(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.contains("darwin") {
        return "darwin".to_string();
    }
    if normalized.contains("linux") {
        return "linux".to_string();
    }
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized
    }
}

pub(crate) fn gpui_normalize_remote_install_arch(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" => "x64".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        normalized if normalized.is_empty() => "unknown".to_string(),
        normalized => normalized.to_string(),
    }
}

pub(crate) fn gpui_is_macho_binary(path: &Path) -> bool {
    let Ok(data) = fs::read(path) else {
        return false;
    };
    if data.len() < 4 {
        return false;
    }
    let prefix = [data[0], data[1], data[2], data[3]];
    matches!(
        prefix,
        [0xfe, 0xed, 0xfa, 0xce]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
    )
}

pub(crate) fn gpui_is_elf_binary(path: &Path, arch: Option<&str>) -> bool {
    let Ok(data) = fs::read(path) else {
        return false;
    };
    if data.len() < 20 || &data[..4] != b"\x7fELF" {
        return false;
    }
    let Some(arch) = arch else {
        return true;
    };
    let Some(expected_machine) = gpui_expected_elf_machine(arch) else {
        return false;
    };
    gpui_elf_machine(data.as_slice()) == Some(expected_machine)
}

pub(crate) fn gpui_expected_elf_machine(arch: &str) -> Option<u16> {
    match gpui_normalize_remote_install_arch(arch).as_str() {
        "x64" => Some(0x3e),
        "arm64" => Some(0xb7),
        _ => None,
    }
}

pub(crate) fn gpui_elf_machine(data: &[u8]) -> Option<u16> {
    if data.len() < 20 {
        return None;
    }
    match data[5] {
        1 => Some(u16::from(data[18]) | (u16::from(data[19]) << 8)),
        2 => Some((u16::from(data[18]) << 8) | u16::from(data[19])),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh(
    config: &GpuiRemoteMachineConfig,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    gpui_run_remote_ssh_in_execution_target(
        config,
        &GpuiRemoteExecutionTarget::PosixHost,
        remote_command,
        timeout,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let command = gpui_remote_command_for_execution_target(execution_target, remote_command);
    gpui_run_remote_ssh_raw(config, command.as_str(), timeout)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_in_windows_wsl(
    config: &GpuiRemoteMachineConfig,
    distribution: Option<&str>,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let command = gpui_remote_command_for_windows_wsl(distribution, remote_command);
    gpui_run_remote_ssh_raw(config, command.as_str(), timeout)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_raw(
    config: &GpuiRemoteMachineConfig,
    remote_command: &str,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not prepare SSH password helper.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    arguments.push(remote_command.to_string());
    gpui_run_remote_process(
        "/usr/bin/ssh",
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        timeout,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_ssh_with_stdin_file_in_execution_target(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    stdin_path: &Path,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    let askpass = match gpui_remote_ssh_askpass_script(config) {
        Ok(askpass) => askpass,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not prepare SSH password helper.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let mut arguments = gpui_remote_ssh_client_options(config.has_saved_password);
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    arguments.push(gpui_remote_command_for_execution_target(
        execution_target,
        remote_command,
    ));
    gpui_run_remote_process_with_stdin_file(
        "/usr/bin/ssh",
        &arguments,
        gpui_remote_ssh_askpass_environment(askpass.as_ref()),
        stdin_path,
        timeout,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_terminal_attachment_to_remote(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    local_path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    let metadata = fs::metadata(local_path)
        .map_err(|_| "The selected file or folder is no longer available.".to_string())?;
    let kind = if metadata.is_dir() {
        GpuiTerminalAttachmentKind::Folder
    } else if metadata.is_file() && is_project_board_image_file_path(local_path) {
        GpuiTerminalAttachmentKind::Image
    } else if metadata.is_file() {
        GpuiTerminalAttachmentKind::File
    } else {
        return Err("The selected item is not a file or folder.".to_string());
    };
    let filename = gpui_terminal_attachment_sanitized_filename(local_path)?;
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let remote_path = format!(
        "/tmp/ghostex-gpui-attachments/{}-{unique_id}-{filename}",
        std::process::id()
    );
    let quoted_remote_path = gpui_shell_single_quote(remote_path.as_str());
    let remote_command = match kind {
        GpuiTerminalAttachmentKind::Folder => format!(
            "umask 077; mkdir -p /tmp/ghostex-gpui-attachments; mkdir -- {quoted_remote_path} && tar -xzf - -C {quoted_remote_path} --strip-components=1"
        ),
        GpuiTerminalAttachmentKind::Image | GpuiTerminalAttachmentKind::File => format!(
            "umask 077; mkdir -p /tmp/ghostex-gpui-attachments; cat > {quoted_remote_path} && chmod 600 {quoted_remote_path}"
        ),
    };

    let staged_archive;
    let upload_path = if kind == GpuiTerminalAttachmentKind::Folder {
        let parent = local_path
            .parent()
            .ok_or_else(|| "The selected folder has no containing directory.".to_string())?;
        let directory_name = local_path
            .file_name()
            .ok_or_else(|| "The selected folder has no usable name.".to_string())?;
        staged_archive = env::temp_dir().join(format!(
            "ghostex-gpui-terminal-attachment-{}-{unique_id}.tar.gz",
            std::process::id()
        ));
        let archive_status = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(staged_archive.as_path())
            .arg("-C")
            .arg(parent)
            .arg(directory_name)
            .status()
            .map_err(|_| "Could not prepare the selected folder for upload.".to_string())?;
        if !archive_status.success() {
            let _ = fs::remove_file(staged_archive.as_path());
            return Err("Could not prepare the selected folder for upload.".to_string());
        }
        staged_archive.as_path()
    } else {
        local_path
    };

    let upload_result = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        remote_command.as_str(),
        upload_path,
        Duration::from_secs(300),
    );
    if kind == GpuiTerminalAttachmentKind::Folder {
        let _ = fs::remove_file(upload_path);
    }
    if upload_result.exit_code != 0 {
        return Err("Could not upload the selected item to the remote machine.".to_string());
    }
    Ok(GpuiTerminalAttachmentReference {
        kind,
        path: remote_path,
    })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_upload_terminal_attachment_to_remote(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _local_path: &Path,
) -> Result<GpuiTerminalAttachmentReference, String> {
    Err("Remote attachments are unavailable in this GPUI build.".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_upload_terminal_clipboard_image_to_remote(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    payload: TerminalClipboardImagePayload,
) -> Result<Vec<GpuiTerminalAttachmentReference>, String> {
    match payload {
        TerminalClipboardImagePayload::FilePaths(paths) => paths
            .iter()
            .map(|path| {
                gpui_upload_terminal_attachment_to_remote(config, execution_target, path.as_path())
            })
            .collect(),
        TerminalClipboardImagePayload::Bytes { bytes, extension } => {
            let unique_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let staged_image = env::temp_dir().join(format!(
                "ghostex-gpui-clipboard-image-{}-{unique_id}.{extension}",
                std::process::id()
            ));
            fs::write(staged_image.as_path(), &bytes)
                .map_err(|_| "Could not stage the pasted image for upload.".to_string())?;
            let result = gpui_upload_terminal_attachment_to_remote(
                config,
                execution_target,
                staged_image.as_path(),
            );
            let _ = fs::remove_file(staged_image.as_path());
            result.map(|reference| vec![reference])
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_upload_terminal_clipboard_image_to_remote(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _payload: TerminalClipboardImagePayload,
) -> Result<Vec<GpuiTerminalAttachmentReference>, String> {
    Err("Remote image paste is unavailable in this GPUI build.".to_string())
}

pub(crate) fn gpui_terminal_attachment_sanitized_filename(path: &Path) -> Result<String, String> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The selected item has no usable filename.".to_string())?;
    let sanitized = filename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized
        .trim_matches(|character| matches!(character, '.' | '-' | '_'))
        .chars()
        .take(96)
        .collect::<String>();
    if sanitized.is_empty() {
        Ok("attachment".to_string())
    } else {
        Ok(sanitized)
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_client_options(has_saved_password: bool) -> Vec<String> {
    /*
    CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
    Password-backed GPUI Remote machines use the same SSH askpass boundary as Swift: disable BatchMode, allow exactly one password prompt, and have the helper read the saved credential from Keychain. Key-only machines keep BatchMode so SSH cannot hang waiting for interactive input.
    */
    let mut arguments = vec![
        "-o".to_string(),
        "UseKeychain=yes".to_string(),
        "-o".to_string(),
        "AddKeysToAgent=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-o".to_string(),
        "TCPKeepAlive=yes".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];
    if has_saved_password {
        arguments.extend([
            "-o".to_string(),
            "BatchMode=no".to_string(),
            "-o".to_string(),
            "NumberOfPasswordPrompts=1".to_string(),
            "-o".to_string(),
            "PreferredAuthentications=publickey,password,keyboard-interactive".to_string(),
            "-o".to_string(),
            "PasswordAuthentication=yes".to_string(),
        ]);
    } else {
        arguments.extend(["-o".to_string(), "BatchMode=yes".to_string()]);
    }
    arguments
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_remote_ssh_client_options(has_saved_password: bool) -> Vec<String> {
    let mut arguments = vec![
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-o".to_string(),
        "TCPKeepAlive=yes".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];
    if has_saved_password {
        arguments.extend(["-o".to_string(), "BatchMode=no".to_string()]);
    } else {
        arguments.extend(["-o".to_string(), "BatchMode=yes".to_string()]);
    }
    arguments
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_target_arguments(config: &GpuiRemoteMachineConfig) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(identity_file) = config.ssh_identity_file.as_ref() {
        arguments.extend(["-i".to_string(), identity_file.clone()]);
    }
    if let Some(port) = config.ssh_port {
        arguments.extend(["-p".to_string(), port.to_string()]);
    }
    arguments.push(config.ssh_target_host());
    arguments
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_remote_ssh_target_arguments(config: &GpuiRemoteMachineConfig) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(identity_file) = config.ssh_identity_file.as_ref() {
        arguments.extend(["-i".to_string(), identity_file.clone()]);
    }
    if let Some(port) = config.ssh_port {
        arguments.extend(["-p".to_string(), port.to_string()]);
    }
    arguments.push(config.ssh_target_host());
    arguments
}

pub(crate) fn gpui_login_shell_remote_command(command: &str) -> String {
    let quoted_command = gpui_shell_single_quote(command);
    format!(
        "if [ -x /bin/zsh ]; then exec /bin/zsh -lic {quoted_command}; elif command -v zsh >/dev/null 2>&1; then exec zsh -lic {quoted_command}; else exec /bin/sh -lc {quoted_command}; fi"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandActionRunFileStatus {
    Working,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandActionStatusFile {
    pub(crate) exit_code: i32,
    pub(crate) status: GpuiCommandActionRunFileStatus,
    pub(crate) run_id: String,
}

pub(crate) fn gpui_command_action_execution_text_for_current_backend(
    command: &str,
    run_id: &str,
    status_file_path: &Path,
) -> String {
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
    ) {
        return gpui_powershell_command_action_execution_text(command, run_id, status_file_path);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = status_file_path;
    gpui_command_action_execution_text(command, run_id)
}

pub(crate) fn gpui_command_action_startup_text(execution_text: &str, status_file_path: &Path) -> String {
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
    ) {
        return format!("{execution_text}\r");
    }
    format!(
        "{}\r",
        gpui_command_action_mounted_terminal_script_text(execution_text, status_file_path)
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_powershell_command_action_execution_text(
    command: &str,
    run_id: &str,
    status_file_path: &Path,
) -> String {
    /*
    PowerShell command tabs use the same bounded status-file contract as the
    Unix wrapper, but write it with native PowerShell syntax and ASCII output
    so the existing parser never sees a UTF-8 BOM on its first key. The saved
    command remains visible terminal input and runs in the interactive ConPTY
    shell; this wrapper does not start a hidden process or persist the command.
    */
    let state_file = gpui_powershell_single_quote(status_file_path.to_string_lossy().as_ref());
    let run_id = gpui_powershell_single_quote(run_id);
    format!(
        r#"$__ghostexStateFile = '{state_file}'
$__ghostexStateDir = Split-Path -Parent $__ghostexStateFile
if ($__ghostexStateDir) {{ New-Item -ItemType Directory -Force -Path $__ghostexStateDir | Out-Null }}
$__ghostexUpdatedAt = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
@('status=working', "statusUpdatedAt=$__ghostexUpdatedAt", 'commandRunId={run_id}', 'commandExitCode=0', "lastActivityAt=$__ghostexUpdatedAt") | Set-Content -LiteralPath $__ghostexStateFile -Encoding Ascii
$global:LASTEXITCODE = $null
& {{
{command}
}}
$__ghostexSucceeded = $?
$__ghostexExit = if ($null -ne $LASTEXITCODE) {{ [int]$LASTEXITCODE }} elseif ($__ghostexSucceeded) {{ 0 }} else {{ 1 }}
if ($__ghostexExit -lt 0 -or $__ghostexExit -gt 255) {{ $__ghostexExit = 1 }}
$__ghostexUpdatedAt = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
@('status=idle', "statusUpdatedAt=$__ghostexUpdatedAt", 'commandRunId={run_id}', "commandExitCode=$__ghostexExit", "lastActivityAt=$__ghostexUpdatedAt") | Set-Content -LiteralPath $__ghostexStateFile -Encoding Ascii
"#
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_powershell_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

pub(crate) fn gpui_command_action_execution_text(command: &str, run_id: &str) -> String {
    /*
    CDXC:GPUICommandPane 2026-06-24-23:36:
    GPUI command-pane Actions need the same hidden process-command shape as the macOS app: wrap the saved command in a function, preserve the user's output as the visible terminal result, then return to an interactive login shell instead of pasting the wrapper into the prompt or closing the pane.

    CDXC:GPUICommandPane 2026-06-24-23:36:
    The wrapper stamps only safe command lifecycle fields into the env-provided session-state file before and after the action. That file is how GPUI can clear a reused command-pane tab from Working back to Idle without parsing terminal output, titles, command text, cwd, env, or stdout/stderr.
    */
    let working_stamp = gpui_command_action_status_stamp_text("working", run_id, "0");
    let idle_stamp = gpui_command_action_status_stamp_text("idle", run_id, "$__ghostex_exit");
    gpui_with_atuin_ignored_shell_history_prefix(
        vec![
            "__ghostex_command_pane_action() {",
            command,
            "}",
            working_stamp.as_str(),
            "__ghostex_command_pane_action",
            "__ghostex_exit=$?",
            "unset -f __ghostex_command_pane_action",
            idle_stamp.as_str(),
            "exec /bin/zsh -l",
        ]
        .join("\n"),
    )
}

pub(crate) fn gpui_debug_command_action_initial_input(command: &str) -> String {
    gpui_with_atuin_ignored_shell_history_prefix(format!("{command}\r"))
}

pub(crate) fn gpui_with_atuin_ignored_shell_history_prefix(text: String) -> String {
    if text.starts_with(' ') {
        text
    } else {
        format!(" {text}")
    }
}

pub(crate) fn gpui_shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn gpui_command_action_status_stamp_text(status: &str, run_id: &str, exit_code: &str) -> String {
    /*
    CDXC:GPUICommandPaneActions 2026-06-26-06:18:
    Command Action status stamps must keep macOS parity by writing a replacement file beside the session-state file with shell process uniqueness before atomically moving it into place. Do not use a shared fixed temp file because concurrent command/status writers can clobber the idle stamp that drives completion sound.
    */
    let status = gpui_shell_single_quote(status);
    let run_id = gpui_shell_single_quote(run_id);
    vec![
        "__ghostex_session_state_file=\"${GHOSTEX_SESSION_STATE_FILE:-${VSMUX_SESSION_STATE_FILE:-$ghostex_SESSION_STATE_FILE}}\"".to_string(),
        "if [ -n \"$__ghostex_session_state_file\" ]; then".to_string(),
        "  __ghostex_state_dir=\"${__ghostex_session_state_file:h}\"".to_string(),
        "  [ \"$__ghostex_state_dir\" = \"$__ghostex_session_state_file\" ] && __ghostex_state_dir=\".\"".to_string(),
        "  mkdir -p -- \"$__ghostex_state_dir\"".to_string(),
        "  __ghostex_status_updated_at=\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"".to_string(),
        "  __ghostex_state_tmp=\"$__ghostex_session_state_file.$$.$RANDOM.command.tmp\"".to_string(),
        "  {".to_string(),
        format!("    printf 'status=%s\\n' {status}"),
        "    printf 'statusUpdatedAt=%s\\n' \"$__ghostex_status_updated_at\"".to_string(),
        format!("    printf 'commandRunId=%s\\n' {run_id}"),
        format!("    printf 'commandExitCode=%s\\n' {exit_code}"),
        "    printf 'lastActivityAt=%s\\n' \"$__ghostex_status_updated_at\"".to_string(),
        "  } > \"$__ghostex_state_tmp\" && /bin/mv -f -- \"$__ghostex_state_tmp\" \"$__ghostex_session_state_file\"".to_string(),
        "fi".to_string(),
    ]
    .join("\n")
}

pub(crate) fn create_gpui_command_action_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("gpui-{millis}-{}", std::process::id())
}

pub(crate) fn gpui_command_action_status_file_path(session_id: CommandSessionId) -> PathBuf {
    let directory = ghostex_state_root().join("gpui-command-actions");
    prune_stale_gpui_command_action_temp_files(&directory);
    directory.join(format!("command-session-{}.state", session_id.0))
}

pub(crate) fn prune_stale_gpui_command_action_temp_files(directory: &Path) {
    const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

    let Some(cutoff) = SystemTime::now().checked_sub(STALE_AFTER) else {
        return;
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("command-session-") || !name.ends_with(".command.tmp") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() && metadata.modified().is_ok_and(|modified| modified <= cutoff) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn gpui_command_action_state_export_text(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    if matches!(
        windows_terminal_backend::resolve_current(),
        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
    ) {
        let windows_path = gpui_shell_single_quote(path.to_string_lossy().as_ref());
        return [
            format!("__ghostex_windows_session_state_file={windows_path}"),
            "__ghostex_session_state_file=\"$(wslpath -a -u \"$__ghostex_windows_session_state_file\")\"".to_string(),
            "export GHOSTEX_SESSION_STATE_FILE=\"$__ghostex_session_state_file\"".to_string(),
            "export VSMUX_SESSION_STATE_FILE=\"$__ghostex_session_state_file\"".to_string(),
            "export ghostex_SESSION_STATE_FILE=\"$__ghostex_session_state_file\"".to_string(),
            "unset __ghostex_windows_session_state_file __ghostex_session_state_file".to_string(),
        ]
        .join("\n");
    }
    let path = gpui_shell_single_quote(path.to_string_lossy().as_ref());
    [
        format!("export GHOSTEX_SESSION_STATE_FILE={path}"),
        format!("export VSMUX_SESSION_STATE_FILE={path}"),
        format!("export ghostex_SESSION_STATE_FILE={path}"),
    ]
    .join("\n")
}

pub(crate) fn gpui_command_action_mounted_terminal_script_text(
    execution_text: &str,
    status_file_path: &Path,
) -> String {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-07:54:
    Mounted reused default Actions need the same wrapper text as a created Action launch payload, but it belongs in a private staged script so the interactive shell sees only a short source command. Keep the status-file path inside process-local script/env setup and out of sidebar bridges, shell-state JSON, logs, and command titles.
    */
    format!(
        "{}\n{execution_text}\n",
        gpui_command_action_state_export_text(status_file_path)
    )
}

// CDXC:GPUILinuxX11Backend 2026-07-05: staged mounted-Action scripts are
// plain POSIX (temp script + 0600/0700 permissions + `. path; rm path`), and
// the mounted GPUI-engine terminal branch that consumes them is
// cross-platform, so the staging helpers are unix-wide rather than
// macOS-only. main.rs already imports PermissionsExt unconditionally.
#[cfg(unix)]
pub(crate) fn gpui_command_action_staged_mounted_script_source_command(
    execution_text: &str,
    status_file_path: &Path,
) -> Option<String> {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-07:54:
    Native `writeTerminalScript` parity for mounted Action reuse stages the full wrapper in a temp script and submits `. script; rm script` through the current interactive shell. The staged command carries only the temp script path, not command text, run ids, status-file paths, cwd/env values, terminal output, or persisted shell metadata.
    */
    let script_path = gpui_command_action_staged_mounted_script_path();
    let directory = script_path.parent()?;
    fs::create_dir_all(directory).ok()?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).ok()?;
    let script_text =
        gpui_command_action_mounted_terminal_script_text(execution_text, status_file_path);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&script_path)
        .ok()?;
    if file.write_all(script_text.as_bytes()).is_err() {
        let _ = fs::remove_file(&script_path);
        return None;
    }
    drop(file);
    if fs::set_permissions(&script_path, fs::Permissions::from_mode(0o600)).is_err() {
        let _ = fs::remove_file(&script_path);
        return None;
    }
    Some(gpui_command_action_mounted_script_source_command(
        &script_path,
    ))
}

#[cfg(not(unix))]
pub(crate) fn gpui_command_action_staged_mounted_script_source_command(
    execution_text: &str,
    status_file_path: &Path,
) -> Option<String> {
    let _ = (execution_text, status_file_path);
    None
}

#[cfg(unix)]
pub(crate) fn gpui_command_action_staged_mounted_script_path() -> PathBuf {
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir()
        .join("ghostex-restore-scripts")
        .join(format!(
            "gpui-command-action-{}-{unique_id}.zsh",
            std::process::id()
        ))
}

pub(crate) fn gpui_command_action_mounted_script_source_command(script_path: &Path) -> String {
    let path = gpui_shell_single_quote(script_path.to_string_lossy().as_ref());
    format!(" . {path}; /bin/rm -f -- {path}")
}

pub(crate) fn gpui_command_action_should_insert_launch_payload(
    selection_kind: CommandPaneActionSessionSelectionKind,
    mounted_reuse_surface_available: bool,
    wrote_to_mounted_reuse: bool,
) -> bool {
    /*
    CDXC:GPUICommandPaneActions 2026-06-27-07:54:
    Native default Action routing has two mutually exclusive execution paths: mounted idle reuse writes to the current command surface and never queues startup data, while created or unmounted Action tabs receive an exact-slot launch payload for their first mount. A failed mounted write must not become a hidden later launch payload for the same live shell.
    */
    match selection_kind {
        CommandPaneActionSessionSelectionKind::Created => true,
        CommandPaneActionSessionSelectionKind::Reused => {
            !mounted_reuse_surface_available && !wrote_to_mounted_reuse
        }
        CommandPaneActionSessionSelectionKind::ReusedActive => false,
    }
}

pub(crate) fn gpui_command_action_title_key(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(crate) fn gpui_command_action_status_from_file(path: &Path) -> Option<GpuiCommandActionStatusFile> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > 8 * 1024 {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let mut exit_code = None;
    let mut status = None;
    let mut run_id = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "status" => {
                status = match value {
                    "working" => Some(GpuiCommandActionRunFileStatus::Working),
                    "idle" => Some(GpuiCommandActionRunFileStatus::Idle),
                    _ => None,
                };
            }
            "commandRunId" => {
                if !value.is_empty()
                    && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                    && !value.chars().any(char::is_control)
                {
                    run_id = Some(value.to_string());
                }
            }
            "commandExitCode" => {
                if !value.is_empty()
                    && value.len() <= 3
                    && value.chars().all(|character| character.is_ascii_digit())
                {
                    exit_code = value
                        .parse::<i32>()
                        .ok()
                        .filter(|code| (0..=255).contains(code));
                }
            }
            _ => {}
        }
    }
    Some(GpuiCommandActionStatusFile {
        exit_code: exit_code?,
        status: status?,
        run_id: run_id?,
    })
}

/*
CDXC:SidebarBrowserTabReveal 2026-08-18:
The reveal payload names the tab in Rust's own vocabulary (project id + tab id).
Turning that into the sidebar's session id belongs to the sidebar runtime, which
is the code that builds those rows in the first place.
*/
pub(crate) fn gpui_remote_attach_session_reference_from_project_id(
    project_id: &str,
) -> Option<GpuiRemoteAttachSessionReference> {
    let rest = project_id.strip_prefix("remote:")?;
    let (remote_machine_id, rest) = rest.split_once(":session:")?;
    let (project_id, session_id) = rest.split_once(':')?;
    let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
    if !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_session_id_allowed(session_id)
    {
        return None;
    }
    Some(GpuiRemoteAttachSessionReference {
        remote_machine_id,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    })
}

/*
CDXC:GPUIRemotePortsBrowser 2026-07-30:
The remote Browser default page lists the machine's listening TCP sockets so a
locally running app there is one click away. Discovery runs over the saved SSH
configuration (`ss` on Linux/WSL remotes, `netstat` elsewhere) — no gxserver
endpoint and no renderer-supplied hosts, ports, paths, or command text. The
rendered page is written under the local state directory and only ever
contains the saved ssh host plus parsed port/address/process fields.
*/
pub(crate) const GPUI_REMOTE_LISTENING_PORTS_COMMAND: &str = "if command -v ss >/dev/null 2>&1; then ss -tlnp 2>/dev/null || ss -tln; else netstat -an 2>/dev/null | grep LISTEN; fi";

pub(crate) struct GpuiRemoteListeningPort {
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) process: Option<String>,
    pub(crate) remotely_reachable: bool,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_prepare_remote_ports_browser_page(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> Result<String, String> {
    let result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        GPUI_REMOTE_LISTENING_PORTS_COMMAND,
        Duration::from_secs(12),
    );
    if result.exit_code != 0 {
        return Err("Listing the remote machine's ports over SSH failed.".to_string());
    }
    let ports = gpui_parse_remote_listening_ports(result.stdout.as_str());
    let html = gpui_remote_ports_page_html(config.ssh_host.trim(), &ports);
    let directory = shared_settings::ghostex_storage_paths()
        .state_dir
        .join("remote-ports");
    fs::create_dir_all(&directory)
        .map_err(|_| "Could not prepare the remote ports page.".to_string())?;
    let path = directory.join(format!("{}.html", config.remote_machine_id));
    fs::write(&path, html).map_err(|_| "Could not write the remote ports page.".to_string())?;
    Ok(format!("file://{}", gpui_path_string(&path)))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_prepare_remote_ports_browser_page(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
) -> Result<String, String> {
    Err("Remote port discovery is unavailable on this platform.".to_string())
}

pub(crate) fn gpui_parse_remote_listening_ports(stdout: &str) -> Vec<GpuiRemoteListeningPort> {
    let mut by_port: std::collections::BTreeMap<u16, GpuiRemoteListeningPort> =
        std::collections::BTreeMap::new();
    for line in stdout.lines() {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 4 {
            continue;
        }
        let (local, process) = if tokens[0].eq_ignore_ascii_case("LISTEN") {
            // ss: State Recv-Q Send-Q Local:Port Peer [users:(("name",…))]
            (tokens[3], gpui_remote_ss_process_name(line))
        } else if tokens[0].starts_with("tcp")
            && tokens
                .last()
                .is_some_and(|token| token.eq_ignore_ascii_case("LISTEN"))
        {
            // netstat: Proto Recv-Q Send-Q Local Foreign State
            (tokens[3], None)
        } else {
            continue;
        };
        let Some((address, port)) = gpui_remote_split_listening_local_address(local) else {
            continue;
        };
        let remotely_reachable = gpui_remote_listen_address_is_remotely_reachable(address.as_str());
        match by_port.get_mut(&port) {
            Some(existing) => {
                if remotely_reachable && !existing.remotely_reachable {
                    existing.address = address;
                    existing.remotely_reachable = true;
                }
                if existing.process.is_none() {
                    existing.process = process;
                }
            }
            None => {
                by_port.insert(
                    port,
                    GpuiRemoteListeningPort {
                        address,
                        port,
                        process,
                        remotely_reachable,
                    },
                );
            }
        }
    }
    by_port.into_values().collect()
}

pub(crate) fn gpui_remote_split_listening_local_address(value: &str) -> Option<(String, u16)> {
    // ss prints `address:port` (`[::]:80`, `*:80`); macOS netstat prints
    // `address.port` (`127.0.0.1.58744`, `*.58744`).
    let (address, port) = value.rsplit_once(':').or_else(|| value.rsplit_once('.'))?;
    let port = port.parse::<u16>().ok()?;
    let address = address.trim_matches(['[', ']']).to_string();
    Some((address, port))
}

pub(crate) fn gpui_remote_listen_address_is_remotely_reachable(address: &str) -> bool {
    let normalized = address.trim().to_ascii_lowercase();
    !(normalized.starts_with("127.") || normalized == "::1" || normalized == "localhost")
}

pub(crate) fn gpui_remote_ss_process_name(line: &str) -> Option<String> {
    let marker = "users:((\"";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    let name = rest[..end].trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn gpui_remote_ports_page_html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn gpui_remote_ports_link_host(ssh_host: &str) -> String {
    if ssh_host.contains(':') {
        format!("[{ssh_host}]")
    } else {
        ssh_host.to_string()
    }
}

pub(crate) fn gpui_remote_ports_page_html(ssh_host: &str, ports: &[GpuiRemoteListeningPort]) -> String {
    let escaped_host = gpui_remote_ports_page_html_escape(ssh_host);
    let link_host = gpui_remote_ports_page_html_escape(&gpui_remote_ports_link_host(ssh_host));
    let mut reachable_rows = String::new();
    let mut loopback_rows = String::new();
    for entry in ports {
        let process = entry
            .process
            .as_deref()
            .map(gpui_remote_ports_page_html_escape)
            .unwrap_or_default();
        let address = gpui_remote_ports_page_html_escape(entry.address.as_str());
        if entry.remotely_reachable {
            reachable_rows.push_str(&format!(
                "<a class=\"row\" href=\"http://{link_host}:{port}/\">\
                 <span class=\"port\">{port}</span>\
                 <span class=\"process\">{process}</span>\
                 <span class=\"address\">{address}</span>\
                 <span class=\"go\">http://{link_host}:{port}/ →</span></a>\n",
                port = entry.port,
            ));
        } else {
            loopback_rows.push_str(&format!(
                "<div class=\"row loopback\">\
                 <span class=\"port\">{port}</span>\
                 <span class=\"process\">{process}</span>\
                 <span class=\"address\">{address}</span>\
                 <span class=\"go\">localhost-only on the remote</span></div>\n",
                port = entry.port,
            ));
        }
    }
    if reachable_rows.is_empty() {
        reachable_rows =
            "<div class=\"empty\">No externally reachable listening TCP ports were found.</div>\n"
                .to_string();
    }
    let loopback_section = if loopback_rows.is_empty() {
        String::new()
    } else {
        format!(
            "<h2>Bound to localhost on the remote</h2>\
             <p class=\"hint\">These only answer on the remote machine itself; reach them with an SSH port forward.</p>\n{loopback_rows}"
        )
    };
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\
         <title>Ports on {escaped_host}</title>\
         <style>\
         body{{margin:0;padding:32px;background:#101418;color:#d7dde3;font:14px/1.5 -apple-system,'Segoe UI',sans-serif;}}\
         h1{{font-size:20px;margin:0 0 4px;}}\
         h2{{font-size:14px;margin:28px 0 4px;color:#9aa7b2;}}\
         p.hint{{margin:0 0 12px;color:#77828c;font-size:12px;}}\
         .row{{display:flex;align-items:baseline;gap:16px;padding:10px 14px;margin:6px 0;border-radius:8px;background:#1a2027;text-decoration:none;color:inherit;}}\
         a.row:hover{{background:#232b35;}}\
         .port{{font-size:16px;font-weight:600;min-width:64px;color:#e8eef4;}}\
         .process{{min-width:160px;color:#a9c7e8;}}\
         .address{{min-width:120px;color:#77828c;font-family:ui-monospace,monospace;font-size:12px;}}\
         .go{{margin-left:auto;color:#6fa8dc;font-family:ui-monospace,monospace;font-size:12px;}}\
         .loopback{{opacity:.55;}}\
         .empty{{padding:24px;color:#77828c;}}\
         </style></head><body>\
         <h1>Ports on {escaped_host}</h1>\
         <p class=\"hint\">Click a port to open the app running on the remote machine. Click the project's Browser button again to refresh this list.</p>\n\
         {reachable_rows}{loopback_section}</body></html>"
    )
}

pub(crate) fn gpui_remote_scoped_session_id(
    remote_machine_id: &str,
    project_id: &str,
    session_id: &str,
) -> String {
    format!("remote:{remote_machine_id}:session:{project_id}:{session_id}")
}

pub(crate) fn gpui_remote_scoped_project_id(remote_machine_id: &str, project_id: &str) -> String {
    format!("remote:{remote_machine_id}:project:{project_id}")
}

pub(crate) fn gpui_remote_project_reference_from_project_id(
    project_id: &str,
) -> Option<GpuiRemoteProjectReference> {
    let rest = project_id.strip_prefix("remote:")?;
    let (remote_machine_id, project_id) = rest.split_once(":project:")?;
    let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
    if !gpui_remote_sidebar_project_id_allowed(project_id) {
        return None;
    }
    Some(GpuiRemoteProjectReference {
        remote_machine_id,
        project_id: project_id.to_string(),
    })
}

pub(crate) fn gpui_remote_sidebar_session_id_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_prepare_remote_attach_terminal_plan(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
    wake_session: bool,
) -> Result<GpuiRemoteAttachTerminalPlan, String> {
    /*
    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    Remote attach validates session/project ownership through the Rust-owned gxserver tunnel before creating a GPUI terminal. The interactive pane runs the authoritative attach command returned by that validation over a fresh SSH connection; asking the remote CLI to resolve the same ids again would repeat its full session-inventory RPC sequence before input becomes available. The human-facing copy command remains `ghostex attach`, and renderer text, gxserver bearer tokens, remote paths, stdout/stderr, and daemon bodies are never logged or copied to CEF.
    */
    let path = if wake_session {
        "/api/wakeSession"
    } else {
        "/api/attachSessionMetadata"
    };
    let attach_rpc_started = Instant::now();
    let result = gpui_remote_gxserver_rpc_result(
        target,
        path,
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(15),
    )?;
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.remoteNewTerminal.attachRpcCompleted",
        serde_json::json!({
            "durationMs": attach_rpc_started.elapsed().as_millis() as u64,
            "operation": if wake_session { "wake" } else { "metadata" },
        }),
    );
    gpui_remote_attach_terminal_plan_from_result(config, target, reference, &result, wake_session)
}

pub(crate) fn gpui_remote_attach_terminal_plan_from_result(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
    result: &serde_json::Value,
    interactive_attach: bool,
) -> Result<GpuiRemoteAttachTerminalPlan, String> {
    gpui_validate_remote_attach_metadata(&result)?;
    let attach = result
        .get("attach")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    let attach_command = attach
        .get("attachCommand")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    let agent_icon = gpui_workspace_attach_agent_icon(attach);
    let title = gpui_workspace_attach_title(attach);
    let clipboard_command =
        gpui_remote_ghostex_attach_ssh_command(config, &target.execution_target, reference);
    let terminal_remote_command =
        format!("printf '\\033]2;{TEMP_REMOTE_SSH_READY_TITLE}\\007'; {attach_command}");
    let terminal_ssh_command = gpui_remote_ssh_shell_command(
        config,
        &target.execution_target,
        terminal_remote_command.as_str(),
        true,
        false,
    );
    let terminal_command = gpui_remote_attach_terminal_process_command(
        &terminal_ssh_command,
        config.ssh_host.as_str(),
        config.ssh_port,
    );
    #[cfg(target_os = "macos")]
    let askpass = if interactive_attach {
        gpui_remote_ssh_askpass_script(config)?
    } else {
        None
    };
    Ok(GpuiRemoteAttachTerminalPlan {
        agent_icon,
        #[cfg(target_os = "macos")]
        askpass,
        clipboard_command,
        terminal_command,
        title,
    })
}

pub(crate) fn gpui_create_remote_project_workspace_terminal(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    project: &GpuiRemoteProjectReference,
) -> Result<
    (
        GpuiRemoteAttachSessionReference,
        GpuiRemoteAttachTerminalPlan,
    ),
    String,
> {
    let create_started = Instant::now();
    /*
    CDXC:GPUIRemoteNewTerminal 2026-08-20:
    Ctrl+G in a remote pane can only reach that machine's code-server prompt
    editor when the daemon serving the session understands the selector. The
    remote runs the gxserver package this app installed for it, which can predate
    the mode, and such a daemon rejects the whole create request instead of
    ignoring an unknown editor. Ask for it only from a daemon that advertised
    `codeServerPromptEditor`; every other machine creates the terminal with its
    own shell editor, exactly like the remote attach and wake paths already do.
    */
    let mut params = serde_json::Map::new();
    params.insert(
        "projectId".to_string(),
        serde_json::Value::String(project.project_id.clone()),
    );
    if target.capabilities.code_server_prompt_editor {
        params.insert(
            "promptEditor".to_string(),
            serde_json::Value::String("code-server".to_string()),
        );
    }
    let (status_code, body) = gpui_remote_gxserver_post_typed_operation(
        target,
        "/api/createWorkspaceTerminal",
        &serde_json::Value::Object(params),
        Duration::from_secs(30),
    )?;
    if !(200..300).contains(&status_code) {
        /*
        The remote daemon is the gxserver package this app installed for that
        machine, so a rejected create is a client/daemon mismatch rather than a
        bad click. Say which side has to move, without forwarding daemon
        response text, remote paths, or request details into the toast.
        */
        return Err(if (400..500).contains(&status_code) {
            "The remote machine's Ghostex daemon refused to create the terminal. Reconnect that machine to update its gxserver, then try again.".to_string()
        } else {
            "Remote gxserver request failed.".to_string()
        });
    }
    let result = parse_gpui_gxserver_rpc_result(&body)?;
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.remoteNewTerminal.createRpcCompleted",
        serde_json::json!({
            "durationMs": create_started.elapsed().as_millis() as u64,
        }),
    );
    let session = result
        .get("session")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Remote gxserver did not create a terminal session.".to_string())?;
    let project_id = gpui_trimmed_json_string_field(session, "projectId")
        .ok_or_else(|| "Remote gxserver did not return a terminal project id.".to_string())?
        .to_string();
    let session_id = gpui_trimmed_json_string_field(session, "sessionId")
        .ok_or_else(|| "Remote gxserver did not return a terminal session id.".to_string())?
        .to_string();
    if project_id != project.project_id
        || !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return Err("Remote gxserver returned an invalid terminal session.".to_string());
    }
    let reference = GpuiRemoteAttachSessionReference {
        remote_machine_id: project.remote_machine_id.clone(),
        project_id,
        session_id,
    };
    let plan_started = Instant::now();
    let plan =
        gpui_remote_attach_terminal_plan_from_result(config, target, &reference, &result, true)?;
    support_logs::append_temporary(
        support_logs::GpuiSupportLog::TerminalFocus,
        "TEMP.remoteNewTerminal.planCompleted",
        serde_json::json!({
            "durationMs": plan_started.elapsed().as_millis() as u64,
            "totalDurationMs": create_started.elapsed().as_millis() as u64,
        }),
    );
    Ok((reference, plan))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteAttachOpenIntent {
    AttachExistingSession,
    CreatedByThisAction,
}

pub(crate) fn gpui_validate_remote_attach_metadata(result: &serde_json::Value) -> Result<(), String> {
    let attach = result
        .get("attach")
        .ok_or_else(|| "Remote attach metadata is unavailable.".to_string())?;
    if attach.get("restoreBlocked").is_some() {
        return Err("Remote session restore is blocked on the remote machine.".to_string());
    }
    let attach_command = attach
        .get("attachCommand")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if attach_command.is_empty() {
        return Err("Remote attach metadata is unavailable.".to_string());
    }
    Ok(())
}

pub(crate) fn gpui_prepare_remote_resume_clipboard_command(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    reference: &GpuiRemoteAttachSessionReference,
) -> Result<String, String> {
    /*
    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    Copy Remote Resume asks the owning remote gxserver for its agent resume plan and wraps only the returned copy command in a saved-machine SSH command. The renderer supplies no command text or cwd, and Rust must not log the plan, cwd, SSH target, stdout/stderr, token, or daemon body.
    */
    let result = gpui_remote_gxserver_rpc_result(
        target,
        "/api/readAgentResumePlan",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(15),
    )?;
    let resume_command = result
        .get("plan")
        .and_then(|plan| plan.get("copyCommand"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| "No resume command is available for that remote session.".to_string())?;
    let cwd = result
        .get("session")
        .and_then(|session| session.get("cwd"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty());
    let remote_command = cwd
        .map(|cwd| format!("cd {} && {resume_command}", gpui_shell_single_quote(cwd)))
        .unwrap_or_else(|| resume_command.to_string());
    Ok(gpui_remote_ssh_shell_command(
        config,
        &target.execution_target,
        remote_command.as_str(),
        false,
        true,
    ))
}

pub(crate) fn gpui_remote_ghostex_attach_ssh_command(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    reference: &GpuiRemoteAttachSessionReference,
) -> String {
    gpui_remote_ssh_shell_command(
        config,
        execution_target,
        gpui_remote_ghostex_attach_command(reference).as_str(),
        true,
        true,
    )
}

pub(crate) fn gpui_remote_ssh_shell_command(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    remote_command: &str,
    force_tty: bool,
    interactive_login_shell: bool,
) -> String {
    let mut arguments = vec!["ssh".to_string()];
    if force_tty {
        arguments.push("-tt".to_string());
    }
    arguments.extend(gpui_remote_ssh_client_options(config.has_saved_password));
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    let command = if interactive_login_shell {
        gpui_remote_command_for_execution_target(execution_target, remote_command)
    } else {
        match execution_target {
            GpuiRemoteExecutionTarget::PosixHost => {
                gpui_noninteractive_login_shell_remote_command(remote_command)
            }
            GpuiRemoteExecutionTarget::WindowsWsl { .. } => {
                gpui_remote_command_for_execution_target(execution_target, remote_command)
            }
        }
    };
    arguments.push(command);
    arguments
        .iter()
        .map(|argument| gpui_remote_shell_command_arg(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn gpui_noninteractive_login_shell_remote_command(command: &str) -> String {
    /*
    CDXC:GPUIRemoteAttach 2026-08-15:
    gxserver returns an authoritative attach program with absolute daemon and
    zmx paths. Starting that program must load login PATH state, but it must not
    source the user's interactive zsh configuration before zmx owns the PTY.
    On macOS, `zsh -lic` can spend hundreds of milliseconds initializing prompt
    plugins and completions for a shell that never becomes the terminal shell.
    */
    let quoted_command = gpui_shell_single_quote(command);
    format!(
        "if [ -x /bin/zsh ]; then exec /bin/zsh -lc {quoted_command}; elif command -v zsh >/dev/null 2>&1; then exec zsh -lc {quoted_command}; else exec /bin/sh -lc {quoted_command}; fi"
    )
}

pub(crate) fn gpui_remote_ghostex_attach_command(reference: &GpuiRemoteAttachSessionReference) -> String {
    let mut parts = vec![
        "attach".to_string(),
        "--session-id".to_string(),
        gpui_shell_single_quote(reference.session_id.as_str()),
        "--project-id".to_string(),
        gpui_shell_single_quote(reference.project_id.as_str()),
    ];
    /*
    Remote Ctrl+G owns a file on the authoritative remote session. Advertise
    the fixed code-server capability so the remote CLI uses h2fe's remote Code
    runtime IPC; never advertise the Mac-only GhostexEditor capability across
    SSH and never let a remote path fall through to a local editor.
    */
    parts.extend(["--prompt-editor".to_string(), "code-server".to_string()]);
    [
        "case \"${GHOSTEX_HOME:-}\" in /*) ghostex_data_dir=\"$GHOSTEX_HOME\";; *) case \"${XDG_DATA_HOME:-}\" in /*) ghostex_data_dir=\"${XDG_DATA_HOME%/}/ghostex\";; *) ghostex_data_dir=\"$HOME/.local/share/ghostex\";; esac;; esac".to_string(),
        "remote_ghostex=\"$ghostex_data_dir/gxserver/package/bin/ghostex\"".to_string(),
        "if [ ! -x \"$remote_ghostex\" ] && [ -x \"$HOME/.ghostex/gxserver/package/bin/ghostex\" ]; then remote_ghostex=\"$HOME/.ghostex/gxserver/package/bin/ghostex\"; fi".to_string(),
        "if [ ! -x \"$remote_ghostex\" ]; then if [ -x \"$HOME/.local/bin/ghostex\" ]; then remote_ghostex=\"$HOME/.local/bin/ghostex\"; else remote_ghostex=\"ghostex\"; fi; fi".to_string(),
        format!("\"$remote_ghostex\" {}", parts.join(" ")),
    ]
    .join("; ")
}

pub(crate) fn gpui_prompt_editor_backend_setting_is_monaco() -> bool {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("promptEditorBackend")
        .and_then(serde_json::Value::as_str)
        == Some("monaco")
}

pub(crate) fn gpui_current_zmx_prompt_editor_attach_mode_is_monaco() -> bool {
    gpui_prompt_editor_backend_setting_is_monaco()
        && gpui_resolved_ghostex_editor_executable().is_some()
}

pub(crate) fn gpui_resolved_ghostex_editor_executable() -> Option<PathBuf> {
    env::var_os("GHOSTEX_EDITOR_APP")
        .and_then(|value| gpui_ghostex_editor_executable_candidate(PathBuf::from(value)))
        .or_else(gpui_default_ghostex_editor_executable)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    /*
    Packaged builds ship the daemon at Contents/Resources/GhostexEditor.app, so
    prefer the app-owned copy: it is version-matched and signed with the same
    team as the app that launches it. Standalone installs and the source
    checkout stay as fallbacks for development bundles that ship no helper.
    */
    gpui_app_bundle_resources_dir()
        .and_then(|resources_dir| {
            gpui_ghostex_editor_executable_candidate(resources_dir.join("GhostexEditor.app"))
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                gpui_home_dir().join("Applications/GhostexEditor.app"),
            )
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(PathBuf::from(
                "/Applications/GhostexEditor.app",
            ))
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../editor/dist/GhostexEditor.app"),
            )
        })
}

#[cfg(target_os = "linux")]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    gpui_ghostex_editor_executable_candidate(gpui_home_dir().join(".local/bin/ghostex-editor"))
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(PathBuf::from("/usr/local/bin/ghostex-editor"))
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../editor/dist/desktop/ghostex-editor"),
            )
        })
}

#[cfg(target_os = "windows")]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .and_then(|value| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(value).join("Ghostex/GhostexEditor/GhostexEditor.exe"),
            )
        })
        .or_else(|| {
            gpui_ghostex_editor_executable_candidate(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../editor/dist/desktop/GhostexEditor.exe"),
            )
        })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn gpui_default_ghostex_editor_executable() -> Option<PathBuf> {
    None
}

pub(crate) fn gpui_ghostex_editor_executable_candidate(candidate: PathBuf) -> Option<PathBuf> {
    let executable = if gpui_is_dir(&candidate)
        && candidate
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("app")
    {
        candidate.join("Contents/MacOS/GhostexEditor")
    } else {
        candidate
    };
    gpui_is_executable_file(&executable).then_some(executable)
}

/// Socket resolution mirror of the daemon's `resolveSocketPath`
/// (apps/editor/macos DaemonSupport.swift ↔ scripts/ghostex-cli.mjs): env
/// override, then the shared Ghostex runtime directory.
#[cfg(unix)]
pub(crate) fn gpui_ghostex_editor_socket_path() -> PathBuf {
    if let Some(path) = env::var_os("GHOSTEX_EDITOR_SOCKET")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        return path;
    }
    shared_settings::ghostex_storage_paths()
        .runtime_dir
        .join("ghostex-editor.sock")
}

#[cfg(unix)]
pub(crate) fn gpui_ghostex_editor_daemon_request(request: &serde_json::Value) -> Option<serde_json::Value> {
    use std::io::{BufRead as _, BufReader, Write as _};
    let mut stream =
        std::os::unix::net::UnixStream::connect(gpui_ghostex_editor_socket_path()).ok()?;
    let timeout = Some(Duration::from_millis(750));
    stream.set_read_timeout(timeout).ok()?;
    stream.set_write_timeout(timeout).ok()?;
    let mut line = serde_json::to_string(request).ok()?;
    line.push('\n');
    stream.write_all(line.as_bytes()).ok()?;
    let mut response_line = String::new();
    BufReader::new(stream).read_line(&mut response_line).ok()?;
    serde_json::from_str(response_line.trim()).ok()
}

pub(crate) fn gpui_ghostex_editor_daemon_open_count() -> u64 {
    #[cfg(unix)]
    {
        gpui_ghostex_editor_daemon_request(
            &serde_json::json!({"v": GHOSTEX_EDITOR_PROTOCOL_VERSION, "type": "ping"}),
        )
        .and_then(|response| {
            response
                .get("openCount")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(0)
    }
    #[cfg(not(unix))]
    {
        0
    }
}

pub(crate) fn gpui_ghostex_editor_daemon_bring_to_front() {
    let _ = gpui_ghostex_editor_daemon_front(None);
}

pub(crate) fn gpui_ghostex_editor_daemon_front(originating_session_id: Option<&str>) -> bool {
    #[cfg(unix)]
    {
        let mut request = serde_json::json!({
            "v": GHOSTEX_EDITOR_PROTOCOL_VERSION,
            "type": "front",
        });
        if let Some(originating_session_id) = originating_session_id {
            request["originatingSessionId"] =
                serde_json::Value::String(originating_session_id.to_string());
        }
        return gpui_ghostex_editor_daemon_request(&request).is_some_and(|response| {
            response.get("type").and_then(serde_json::Value::as_str) == Some("fronted")
                && response
                    .get("frontedCount")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|count| count > 0)
        });
    }
    #[cfg(not(unix))]
    {
        let _ = originating_session_id;
        false
    }
}

pub(crate) fn gpui_prewarm_ghostex_editor_daemon() {
    let Some(executable) = gpui_resolved_ghostex_editor_executable() else {
        return;
    };
    let mut command = Command::new(executable);
    command
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command
            .arg("--socket")
            .arg(gpui_ghostex_editor_socket_path());
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    let _ = command.spawn();
}

pub(crate) fn gpui_remote_shell_command_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "_/:=@%+.,-".contains(ch))
    {
        value.to_string()
    } else {
        gpui_shell_single_quote(value)
    }
}

pub(crate) fn gpui_remote_attach_terminal_process_command(
    ssh_command: &str,
    ssh_host: &str,
    ssh_port: Option<u16>,
) -> String {
    /*
    CDXC:GPUIRemoteAttachReconnect 2026-08-13:
    Remote-attach terminals must never degrade to a vanilla local shell when
    the SSH transport drops (lid close, wifi loss, captive portal, transit).
    The launch wrapper keeps the pane attached to the remote session by
    reconnecting after every attach exit. Each retry is gated on a cheap
    reachability probe so a network outage is handled gracefully instead of
    burning full SSH handshakes; the probe polls at a flat cadence and never
    parks on a network outage, while closing the tab or quitting terminates the
    loop. The remote zmx session persists server-side
    independently, and re-running `ghostex attach` revives a missing provider
    before attaching, so reconnection restores the session and its scrollback.

    For Tailscale CGNAT (100.x) targets `tailscale ping` is the authoritative
    "WireGuard handshake is live" signal and cuts through captive-portal
    ambiguity; other hosts fall back to a bounded TCP connect probe, and the
    absence of either tool degrades to letting SSH's ConnectTimeout bound each
    attempt. The probe target and ssh command stay process-local; no host,
    token, command, or path crosses the CEF boundary.
    */
    let quoted_host = gpui_shell_single_quote(ssh_host);
    let port_assignment = match ssh_port {
        Some(port) => format!(
            "__gx_attach_port={}\n",
            gpui_shell_single_quote(&port.to_string())
        ),
        None => String::new(),
    };
    let probe_lines = [
        "__gx_probe() {",
        "  case \"$__gx_attach_host\" in 100.*)",
        "    __gx_ts=\"\"",
        "    if command -v tailscale >/dev/null 2>&1; then __gx_ts=tailscale",
        "    elif [ -x /Applications/Tailscale.app/Contents/MacOS/Tailscale ]; then __gx_ts=/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        "    fi",
        "    if [ -n \"$__gx_ts\" ]; then",
        "      \"$__gx_ts\" ping --c 1 --timeout 2s \"$__gx_attach_host\" >/dev/null 2>&1 && return 0",
        "      return 1",
        "    fi",
        "    ;;",
        "  esac",
        "  if command -v nc >/dev/null 2>&1; then",
        "    __gx_p=\"${__gx_attach_port:-22}\"",
        "    nc -z -w 3 \"$__gx_attach_host\" \"$__gx_p\" >/dev/null 2>&1 && return 0",
        "    return 1",
        "  fi",
        "  return 0",
        "}",
    ];
    /*
    CDXC:GPUIRemoteAttachReconnect 2026-08-14:
    Probe polling stays at a flat 2s so a returning link reconnects within
    ~2 seconds; only the probe runs while the network is down (one cheap
    process + one packet), so this cadence cannot flood anything. The backoff
    ladder applies only to fast ssh failures against a reachable host
    (2s steady, 5s cap) and auth rejections keep their own hard 30s/3-attempt
    budget. Network outages never park: a 2s probe is one packet and ~50ms of
    CPU, and a system-suspended wrapper consumes nothing (the process is
    frozen), so probing stays effectively free no matter how long the laptop
    sleeps or stays offline. A wall-clock budget is wrong here because sleep
    time would count against it and park the loop the moment the lid opens.
    Only auth rejections park, because they require a human or the app's
    recovery flow.
    */
    let backoff_lines = [
        "__gx_backoff() {",
        "  case \"$1\" in 0|1|2|3) sleep 2 ;; *) sleep 5 ;; esac",
        "}",
    ];
    /*
    CDXC:GPUIRemoteAttachReconnect 2026-08-14:
    The attach command's stderr is streamed live through tee while also being
    captured so each reconnect attempt can be classified. An SSH transport
    authentication rejection never rapid-retries: it backs off hard for three
    attempts and then parks with guidance to use the sidebar recovery (the
    app-owned re-prepare path re-arms askpass/credentials correctly), because
    a wrapper-side retry cannot re-read the Keychain. Fast non-auth failures
    (< 8s) escalate through the same backoff ladder as the probe loop so a
    reachable-but-failing path can never hammer the remote sshd; only a real
    session drop resets the failure counters.
    */
    let attach_lines = [
        "__gx_attach() {".to_string(),
        format!("  {} 2> >(tee \"$__gx_err\" >&2)", ssh_command),
        "}".to_string(),
    ];
    let loop_lines = [
        "__gx_err=\"$(mktemp \"${TMPDIR:-/tmp}/gx-reattach.XXXXXX\")\"",
        "__gx_cleanup() { rm -f \"$__gx_err\"; }",
        "trap __gx_cleanup EXIT",
        "trap 'exit 129' HUP",
        "trap 'exit 130' INT",
        "trap 'exit 143' TERM",
        "__gx_authfails=0",
        "__gx_fastfails=0",
        "while true; do",
        "  __gx_started=$(date +%s)",
        "  __gx_attach",
        "  __gx_exit=$?",
        "  __gx_dur=$(( $(date +%s) - __gx_started ))",
        "  if [ \"$__gx_exit\" -eq 255 ] && grep -q \"Permission denied (\" \"$__gx_err\" 2>/dev/null; then",
        "    __gx_authfails=$((__gx_authfails + 1))",
        "    printf '\\nRemote SSH rejected the login (attempt %s of 3).\\n' \"$__gx_authfails\"",
        "    if [ \"$__gx_authfails\" -ge 3 ]; then",
        "      printf '\\nAuto-reconnect cannot log back in - the saved login is not accepted.\\n'",
        "      printf 'Click this session in the Ghostex sidebar to recover it,\\n'",
        "      printf 'or press Enter to retry. Ctrl+C stops this terminal.\\n'",
        "      if ! read -s -r __gx_junk; then sleep 30; fi",
        "      __gx_authfails=0",
        "    else",
        "      sleep 30",
        "    fi",
        "    continue",
        "  fi",
        "  __gx_authfails=0",
        "  printf '\\nRemote attach ended (exit %s). Reconnecting...\\n' \"$__gx_exit\"",
        "  if [ \"$__gx_dur\" -ge 8 ]; then",
        "    __gx_fastfails=0",
        "  else",
        "    __gx_fastfails=$((__gx_fastfails + 1))",
        "  fi",
        "  if [ \"$__gx_fastfails\" -gt 0 ]; then __gx_backoff \"$__gx_fastfails\"; fi",
        "  while ! __gx_probe; do",
        "    sleep 2",
        "  done",
        "  sleep 1",
        "done",
    ];
    let mut body = String::new();
    body.push_str(&format!(
        "printf '\\033]2;{TEMP_REMOTE_LOCAL_READY_TITLE}\\007'\n"
    ));
    body.push_str(&format!("__gx_attach_host={}\n", quoted_host));
    body.push_str(&port_assignment);
    for line in probe_lines
        .iter()
        .copied()
        .chain(backoff_lines.iter().copied())
        .chain(attach_lines.iter().map(String::as_str))
        .chain(loop_lines.iter().copied())
    {
        body.push_str(line);
        body.push('\n');
    }
    format!("/bin/zsh -c {}", gpui_shell_single_quote(body.trim_end()))
}

pub(crate) const TEMP_REMOTE_LOCAL_READY_TITLE: &str = "TEMP_REMOTE_LOCAL_READY_20260814";
pub(crate) const TEMP_REMOTE_SSH_READY_TITLE: &str = "TEMP_REMOTE_SSH_READY_20260814";

#[cfg(target_os = "macos")]
pub(crate) fn gpui_read_remote_ssh_password_from_keychain(remote_machine_id: &str) -> Result<Vec<u8>, String> {
    const PASSWORD_CAPACITY: usize = 4_096;
    let remote_machine_id = std::ffi::CString::new(remote_machine_id)
        .map_err(|_| "Could not read the saved SSH password from Keychain.".to_string())?;
    let mut password = vec![0_u8; PASSWORD_CAPACITY];
    let mut password_length = 0_usize;
    let result = unsafe {
        GhostexGpuiCopyRemoteSshPassword(
            remote_machine_id.as_ptr(),
            password.as_mut_ptr(),
            password.len(),
            &mut password_length,
        )
    };
    if result == -1 {
        return Err("The saved SSH password is no longer available in Keychain.".to_string());
    }
    if result != 1 || password_length == 0 || password_length > password.len() {
        password.fill(0);
        return Err("Could not read the saved SSH password from Keychain.".to_string());
    }
    password.truncate(password_length);
    Ok(password)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_askpass_script(
    config: &GpuiRemoteMachineConfig,
) -> Result<Option<GpuiRemoteAskpassScript>, String> {
    if !config.has_saved_password {
        return Ok(None);
    }
    let unique_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let directory = env::temp_dir().join(format!("gxap-{}-{unique_id:x}", std::process::id()));
    let script = directory.join("a");
    let socket = directory.join("s");
    fs::create_dir_all(&directory)
        .map_err(|_| "Could not prepare SSH password helper.".to_string())?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|_| {
        let _ = fs::remove_dir_all(&directory);
        "Could not prepare SSH password helper.".to_string()
    })?;
    let contents = format!(
        concat!(
            "#!/bin/sh\n",
            "password=$(/usr/bin/nc -U {} </dev/null) || exit 1\n",
            "if [ -z \"$password\" ]; then\n",
            "  printf '%s\\n' 'Ghostex saved SSH password unavailable.' >&2\n",
            "  exit 1\n",
            "fi\n",
            "printf '%s\\n' \"$password\"\n",
            "unset password\n",
        ),
        gpui_shell_single_quote(gpui_path_string(socket.as_path()).as_str())
    );
    if fs::write(&script, contents).is_err()
        || fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).is_err()
    {
        let _ = fs::remove_dir_all(&directory);
        return Err("Could not prepare SSH password helper.".to_string());
    }

    let listener = match std::os::unix::net::UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(_) => {
            let _ = fs::remove_dir_all(&directory);
            return Err("Could not prepare SSH password helper.".to_string());
        }
    };
    if listener.set_nonblocking(true).is_err() {
        let _ = fs::remove_dir_all(&directory);
        return Err("Could not prepare SSH password helper.".to_string());
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let server_cancel = cancel.clone();
    let remote_machine_id = config.remote_machine_id.clone();
    let askpass_prepared_at = Instant::now();
    let password_server = thread::spawn(move || {
        while !server_cancel.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.remoteNewTerminal.askpassRequested",
                        serde_json::json!({
                            "durationSincePreparedMs": askpass_prepared_at.elapsed().as_millis() as u64,
                            "machineId": remote_machine_id.as_str(),
                        }),
                    );
                    let keychain_read_started = Instant::now();
                    let password_result =
                        gpui_read_remote_ssh_password_from_keychain(remote_machine_id.as_str());
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.remoteNewTerminal.keychainPasswordReadCompleted",
                        serde_json::json!({
                            "durationMs": keychain_read_started.elapsed().as_millis() as u64,
                            "machineId": remote_machine_id.as_str(),
                            "succeeded": password_result.is_ok(),
                        }),
                    );
                    if let Ok(mut password) = password_result {
                        let _ = stream.write_all(password.as_slice());
                        let _ = stream.write_all(b"\n");
                        password.fill(0);
                    }
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    Ok(Some(GpuiRemoteAskpassScript {
        cancel,
        directory,
        password_server: Some(password_server),
        script,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_ssh_askpass_environment(
    askpass: Option<&GpuiRemoteAskpassScript>,
) -> Option<HashMap<String, String>> {
    let askpass = askpass?;
    let mut environment: HashMap<String, String> = env::vars().collect();
    environment
        .entry("DISPLAY".to_string())
        .or_insert_with(|| "localhost:0".to_string());
    environment.insert(
        "SSH_ASKPASS".to_string(),
        gpui_path_string(askpass.script.as_path()),
    );
    environment.insert("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string());
    Some(environment)
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_process(
    executable: &str,
    arguments: &[String],
    environment: Option<HashMap<String, String>>,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    if !gpui_remote_process_launch_input_is_safe(executable, arguments, environment.as_ref()) {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Remote gxserver process launch input was invalid.".to_string(),
            stdout: String::new(),
        };
    }
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(environment) = environment {
        command.env_clear();
        command.envs(environment);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return GpuiRemoteProcessResult {
                exit_code: 127,
                stderr: error.to_string(),
                stdout: String::new(),
            };
        }
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                use std::os::unix::process::ExitStatusExt as _;

                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return GpuiRemoteProcessResult {
                    exit_code: status.code().or_else(|| status.signal()).unwrap_or(1),
                    stderr,
                    stdout,
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                gpui_terminate_remote_process(&mut child);
                return GpuiRemoteProcessResult {
                    exit_code: 124,
                    stderr: "Remote SSH command timed out.".to_string(),
                    stdout: String::new(),
                };
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_run_remote_process_with_stdin_file(
    executable: &str,
    arguments: &[String],
    environment: Option<HashMap<String, String>>,
    stdin_path: &Path,
    timeout: Duration,
) -> GpuiRemoteProcessResult {
    if !gpui_remote_process_launch_input_is_safe(executable, arguments, environment.as_ref()) {
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Remote gxserver process launch input was invalid.".to_string(),
            stdout: String::new(),
        };
    }
    let input = match fs::File::open(stdin_path) {
        Ok(input) => input,
        Err(_) => {
            return GpuiRemoteProcessResult {
                exit_code: 126,
                stderr: "Could not read the gxserver package for upload.".to_string(),
                stdout: String::new(),
            };
        }
    };
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(environment) = environment {
        command.env_clear();
        command.envs(environment);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return GpuiRemoteProcessResult {
                exit_code: 127,
                stderr: error.to_string(),
                stdout: String::new(),
            };
        }
    };
    let Some(mut child_stdin) = child.stdin.take() else {
        gpui_terminate_remote_process(&mut child);
        return GpuiRemoteProcessResult {
            exit_code: 126,
            stderr: "Could not open the SSH package upload stream.".to_string(),
            stdout: String::new(),
        };
    };
    let writer = thread::spawn(move || {
        let mut input = input;
        let _ = std::io::copy(&mut input, &mut child_stdin);
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                use std::os::unix::process::ExitStatusExt as _;

                let _ = writer.join();
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return GpuiRemoteProcessResult {
                    exit_code: status.code().or_else(|| status.signal()).unwrap_or(1),
                    stderr,
                    stdout,
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                gpui_terminate_remote_process(&mut child);
                let _ = child.wait();
                let _ = writer.join();
                return GpuiRemoteProcessResult {
                    exit_code: 124,
                    stderr: "Remote SSH package upload timed out.".to_string(),
                    stdout: String::new(),
                };
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_process_launch_input_is_safe(
    executable: &str,
    arguments: &[String],
    environment: Option<&HashMap<String, String>>,
) -> bool {
    if executable.contains('\0') || arguments.iter().any(|argument| argument.contains('\0')) {
        return false;
    }
    if let Some(environment) = environment {
        for (key, value) in environment {
            if key.contains('\0') || value.contains('\0') {
                return false;
            }
        }
    }
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminate_remote_process(child: &mut Child) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    unsafe {
        let _ = kill(child.id() as i32, SIGTERM);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_open_remote_gxserver_tunnel(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    token: &str,
) -> Result<GpuiRemoteGxserverConnection, String> {
    let code_server_component_platform =
        gpui_remote_code_server_component_platform(config, execution_target);
    for local_port in gpui_remote_gxserver_candidate_ports() {
        let mut tunnel =
            match gpui_spawn_remote_gxserver_tunnel(config, execution_target, local_port) {
                Ok(tunnel) => tunnel,
                Err(_) => continue,
            };
        if let Some(capabilities) = gpui_wait_for_remote_authenticated_health(local_port, token) {
            return Ok(GpuiRemoteGxserverConnection {
                _base_url: format!("http://127.0.0.1:{local_port}"),
                capabilities,
                code_server_component_platform,
                execution_target: execution_target.clone(),
                local_port,
                presentation_stream_cancel: None,
                presentation_stream_generation: None,
                token: token.to_string(),
                child: tunnel.child,
                health_check_failures: 0,
            });
        }
        let _ = tunnel.child.kill();
        let _ = tunnel.child.wait();
    }
    Err(match execution_target {
        GpuiRemoteExecutionTarget::PosixHost => {
            "Could not open an authenticated SSH tunnel to remote gxserver.".to_string()
        }
        GpuiRemoteExecutionTarget::WindowsWsl { .. } => {
            "gxserver started inside WSL2, but the SSH tunnel could not reach it through the Windows host's localhost forwarding. Enable WSL localhost forwarding, then reconnect.".to_string()
        }
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_remote_code_server_component_platform(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
) -> Option<String> {
    let probe = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        gpui_remote_install_target_probe_command(),
        GPUI_REMOTE_GXSERVER_INSTALL_PROBE_TIMEOUT,
    );
    let target = (probe.exit_code == 0)
        .then(|| gpui_extract_remote_install_target(probe.stdout.as_str()))
        .flatten()?;
    if target.normalized_os() != "linux" {
        return None;
    }
    match target.normalized_arch().as_str() {
        "x64" => Some("linux-x64".to_string()),
        "arm64" => Some("linux-arm64".to_string()),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_spawn_remote_gxserver_tunnel(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    local_port: u16,
) -> Result<GpuiRemoteSpawnedTunnel, String> {
    let askpass = gpui_remote_ssh_askpass_script(config)?;
    let mut arguments = Vec::new();
    if matches!(execution_target, GpuiRemoteExecutionTarget::PosixHost) {
        arguments.push("-N".to_string());
    }
    arguments.extend(gpui_remote_ssh_client_options(config.has_saved_password));
    arguments.extend([
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-L".to_string(),
        format!("{local_port}:127.0.0.1:{GPUI_GXSERVER_LOCAL_API_PORT}"),
    ]);
    arguments.extend(gpui_remote_ssh_target_arguments(config));
    if let GpuiRemoteExecutionTarget::WindowsWsl { distribution } = execution_target {
        arguments.push(gpui_remote_command_for_windows_wsl(
            Some(distribution.as_str()),
            gpui_remote_windows_wsl_gxserver_owner_command(),
        ));
    }
    let mut command = Command::new("/usr/bin/ssh");
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
        command.envs(environment);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start the SSH tunnel.".to_string())?;
    thread::sleep(GPUI_REMOTE_GXSERVER_TUNNEL_STARTUP_DELAY);
    if child
        .try_wait()
        .map_err(|_| "Could not check the SSH tunnel process.".to_string())?
        .is_some()
    {
        return Err("SSH tunnel exited before remote gxserver became reachable.".to_string());
    }
    Ok(GpuiRemoteSpawnedTunnel {
        child,
        _askpass: askpass,
    })
}

pub(crate) fn gpui_remote_gxserver_candidate_ports() -> Vec<u16> {
    let range =
        u64::from(GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MAX - GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MIN + 1);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id());
    let mut ports = Vec::with_capacity(GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS);
    let mut seen = HashSet::new();
    for attempt in 0..(GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS * 2) {
        let offset = seed
            .wrapping_add((attempt as u64).wrapping_mul(7_919))
            .wrapping_rem(range);
        let port = GPUI_REMOTE_GXSERVER_TUNNEL_PORT_MIN + offset as u16;
        if seen.insert(port) {
            ports.push(port);
        }
        if ports.len() == GPUI_REMOTE_GXSERVER_TUNNEL_ATTEMPTS {
            break;
        }
    }
    ports
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_wait_for_remote_authenticated_health(
    local_port: u16,
    token: &str,
) -> Option<GpuiRemoteGxserverCapabilities> {
    let deadline = Instant::now() + GPUI_REMOTE_GXSERVER_HEALTH_DEADLINE;
    while Instant::now() < deadline {
        if let Some(capabilities) = gpui_remote_authenticated_health(local_port, token) {
            return Some(capabilities);
        }
        thread::sleep(GPUI_REMOTE_GXSERVER_TUNNEL_RETRY_DELAY);
    }
    None
}

/// Authenticated remote-daemon liveness probe. A healthy answer also carries
/// that daemon's advertised capability inventory, which callers keep for the
/// lifetime of the connection so remote requests can pick selectors this daemon
/// implements. `None` means the probe did not get a healthy answer.
pub(crate) fn gpui_remote_authenticated_health(
    local_port: u16,
    token: &str,
) -> Option<GpuiRemoteGxserverCapabilities> {
    let address = format!("127.0.0.1:{local_port}");
    let mut stream = TcpStream::connect(address.as_str()).ok()?;
    stream
        .set_read_timeout(Some(GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT))
        .ok()?;
    stream
        .set_write_timeout(Some(GPUI_REMOTE_GXSERVER_HEALTH_TIMEOUT))
        .ok()?;
    let request = format!(
        "GET /api/health/server HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\n\r\n",
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let healthy = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .is_some_and(|status| (200..300).contains(&status));
    if !healthy {
        return None;
    }
    Some(gpui_remote_gxserver_capabilities_from_health_response(
        response.as_str(),
    ))
}

/// Reads the fixed capability names GPUI selects remote operations with out of a
/// healthy `/api/health/server` body. A daemon that predates a capability simply
/// omits it, so an unparsable or capability-less answer means "not supported"
/// rather than a failed connection.
pub(crate) fn gpui_remote_gxserver_capabilities_from_health_response(
    response: &str,
) -> GpuiRemoteGxserverCapabilities {
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return GpuiRemoteGxserverCapabilities::default();
    };
    let Ok(body) = gxserver_http_response_body(headers, body) else {
        return GpuiRemoteGxserverCapabilities::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body.as_str()) else {
        return GpuiRemoteGxserverCapabilities::default();
    };
    let advertises = |capability: &str| {
        value
            .get("capabilities")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|capabilities| {
                capabilities
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .any(|advertised| advertised == capability)
            })
    };
    GpuiRemoteGxserverCapabilities {
        code_server_prompt_editor: advertises(GPUI_GXSERVER_CODE_SERVER_PROMPT_EDITOR_CAPABILITY),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteSshPasswordKeychainResult {
    Unsupported,
    Failed,
    Success,
}

pub(crate) fn gpui_remote_machine_password_failure_title(has_password: bool) -> &'static str {
    if has_password {
        "SSH password not saved"
    } else {
        "SSH password not removed"
    }
}

pub(crate) fn gpui_normalize_remote_machine_id(input: &str) -> Option<String> {
    let id = input.trim();
    if id.is_empty() || id.chars().count() > GPUI_REMOTE_MACHINE_ID_MAX_CHARS {
        return None;
    }
    let prefix_len = "remote-".len();
    if id.len() <= prefix_len
        || !id
            .get(..prefix_len)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("remote-"))
    {
        return None;
    }
    let Some(suffix) = id.get(prefix_len..) else {
        return None;
    };
    if suffix
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Some(id.to_string());
    }
    None
}

pub(crate) fn gpui_remote_machine_id_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_object()
        .and_then(|machine| machine.get("id"))
        .and_then(serde_json::Value::as_str)
        .and_then(gpui_normalize_remote_machine_id)
}

pub(crate) fn gpui_settings_object_has_remote_machine_id(
    object: &serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
) -> bool {
    object
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|machines| {
            machines.iter().any(|machine| {
                gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id)
            })
        })
}

pub(crate) fn gpui_set_remote_machine_password_marker(
    object: &mut serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
    has_password: bool,
) -> bool {
    let Some(machines) = object
        .get_mut("remoteMachines")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    for machine in machines {
        let Some(machine_object) = machine.as_object_mut() else {
            continue;
        };
        if machine_object
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
            .as_deref()
            != Some(remote_machine_id)
        {
            continue;
        }
        if has_password {
            machine_object.insert(
                "sshPasswordSaved".to_string(),
                serde_json::Value::Bool(true),
            );
        } else {
            machine_object.remove("sshPasswordSaved");
        }
        return true;
    }
    false
}

pub(crate) fn gpui_removed_remote_machine_password_ids(
    previous_settings: &serde_json::Map<String, serde_json::Value>,
    next_settings: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let next_ids: HashSet<String> = next_settings
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .map(|machines| {
            machines
                .iter()
                .filter_map(gpui_remote_machine_id_from_value)
                .collect()
        })
        .unwrap_or_default();
    let Some(previous_machines) = previous_settings
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut removed = Vec::new();
    for machine in previous_machines {
        let Some(machine_object) = machine.as_object() else {
            continue;
        };
        if machine_object
            .get("sshPasswordSaved")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            continue;
        }
        let Some(remote_machine_id) = gpui_remote_machine_id_from_value(machine) else {
            continue;
        };
        if !next_ids.contains(&remote_machine_id) && seen.insert(remote_machine_id.clone()) {
            removed.push(remote_machine_id);
        }
    }
    removed
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_save_remote_machine_password_to_keychain(
    remote_machine_id: &str,
    password: &str,
) -> GpuiRemoteSshPasswordKeychainResult {
    let Ok(remote_machine_id) = std::ffi::CString::new(remote_machine_id) else {
        return GpuiRemoteSshPasswordKeychainResult::Failed;
    };
    let password_bytes = password.as_bytes();
    match unsafe {
        GhostexGpuiSaveRemoteSshPassword(
            remote_machine_id.as_ptr(),
            password_bytes.as_ptr(),
            password_bytes.len(),
        )
    } {
        1 => GpuiRemoteSshPasswordKeychainResult::Success,
        -1 => GpuiRemoteSshPasswordKeychainResult::Unsupported,
        _ => GpuiRemoteSshPasswordKeychainResult::Failed,
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_save_remote_machine_password_to_keychain(
    _remote_machine_id: &str,
    _password: &str,
) -> GpuiRemoteSshPasswordKeychainResult {
    GpuiRemoteSshPasswordKeychainResult::Unsupported
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_save_remote_gxserver_token_to_keychain(
    remote_machine_id: &str,
    token: &str,
) -> GpuiRemoteTokenKeychainResult {
    let Ok(remote_machine_id) = std::ffi::CString::new(remote_machine_id) else {
        return GpuiRemoteTokenKeychainResult::Failed;
    };
    let token_bytes = token.as_bytes();
    match unsafe {
        GhostexGpuiSaveRemoteGxserverToken(
            remote_machine_id.as_ptr(),
            token_bytes.as_ptr(),
            token_bytes.len(),
        )
    } {
        1 => GpuiRemoteTokenKeychainResult::Success,
        -1 => GpuiRemoteTokenKeychainResult::Unsupported,
        _ => GpuiRemoteTokenKeychainResult::Failed,
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_save_remote_gxserver_token_to_keychain(
    _remote_machine_id: &str,
    _token: &str,
) -> GpuiRemoteTokenKeychainResult {
    GpuiRemoteTokenKeychainResult::Unsupported
}

pub(crate) fn gpui_recent_projects_from_remote_gxserver(
    target: &GpuiRemoteGxserverRequestTarget,
    machine_id: &str,
    machine_name: Option<&str>,
) -> Vec<serde_json::Value> {
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/listRecentProjects",
        &serde_json::json!({}),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| result.get("recentProjects").cloned())
    .and_then(|projects| projects.as_array().cloned())
    .map(|projects| {
        projects
            .iter()
            .filter_map(|project| {
                gpui_recent_project_from_remote_gxserver(project, machine_id, machine_name)
            })
            .collect()
    })
    .unwrap_or_default()
}

pub(crate) fn gpui_recent_project_from_remote_gxserver(
    project: &serde_json::Value,
    machine_id: &str,
    machine_name: Option<&str>,
) -> Option<serde_json::Value> {
    let mut project = gpui_recent_project_from_gxserver(project)?
        .as_object()?
        .clone();
    let project_id = project.get("projectId")?.as_str()?;
    if !gpui_remote_sidebar_project_id_allowed(project_id) {
        return None;
    }
    project.insert(
        "projectId".to_string(),
        serde_json::json!(format!("remote:{machine_id}:project:{project_id}")),
    );
    project.insert("remoteMachineId".to_string(), serde_json::json!(machine_id));
    if let Some(machine_name) = machine_name {
        project.insert(
            "remoteMachineName".to_string(),
            serde_json::json!(machine_name),
        );
    }
    Some(serde_json::Value::Object(project))
}

pub(crate) struct GpuiRemotePreviousSessionSource {
    pub(crate) machine_name: Option<String>,
    pub(crate) remote_machine_id: String,
    pub(crate) target: GpuiRemoteGxserverRequestTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemotePreviousSessionReference {
    pub(crate) remote_machine_id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum GpuiPreviousSessionRestoreResult {
    Local {
        project_id: String,
        session_id: String,
    },
    Remote {
        project_id: String,
        remote_machine_id: String,
        session_id: String,
    },
}

pub(crate) fn gpui_previous_sessions_request_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> GpuiPreviousSessionsRequest {
    let cursor = command
        .get("cursor")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let limit = command
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|limit| limit.min(200).max(1) as usize)
        .unwrap_or(80);
    let query = command
        .get("query")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let request_id = command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let session_tags = command
        .get("sessionTags")
        .and_then(serde_json::Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
    GpuiPreviousSessionsRequest {
        cursor,
        limit,
        query,
        request_id,
        session_tags,
    }
}

pub(crate) fn gpui_previous_sessions_result_message(
    request: GpuiPreviousSessionsRequest,
    remote_sources: Vec<GpuiRemotePreviousSessionSource>,
) -> serde_json::Value {
    /*
    CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
    GPUI Previous Sessions loads real local gxserver history through `/api/listPreviousSessions` with the same bounded previous-only params as the TypeScript sidebar runtime. The response is a transient `previousSessionsResult` sidebarState payload so the shared modal clears loading without replacing the stored hydrate snapshot, and transport/token/network/parser failures return an empty contract-shaped result without logging private daemon data.
    */
    let local_page = gpui_list_previous_sessions_from_gxserver(&request).unwrap_or_default();
    let mut next_cursor = local_page.cursor;
    let mut previous_sessions = local_page.items;
    for remote_source in &remote_sources {
        let remote_page = gpui_list_previous_sessions_from_remote_gxserver(&request, remote_source)
            .unwrap_or_default();
        if next_cursor.is_none() {
            next_cursor = remote_page.cursor;
        }
        previous_sessions.extend(remote_page.items);
    }
    gpui_sort_previous_session_items_by_closed_time(&mut previous_sessions);
    gpui_previous_sessions_result_payload(
        &request.request_id,
        request.query.as_deref(),
        next_cursor.as_deref(),
        previous_sessions,
    )
}

pub(crate) fn gpui_list_previous_sessions_from_remote_gxserver(
    request: &GpuiPreviousSessionsRequest,
    remote_source: &GpuiRemotePreviousSessionSource,
) -> Result<GpuiPreviousSessionsPage, String> {
    let result = gpui_remote_gxserver_rpc_result(
        &remote_source.target,
        "/api/listPreviousSessions",
        &gpui_previous_sessions_list_params(request),
        Duration::from_secs(10),
    )?;
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Remote gxserver returned invalid previous-session results.".to_string())?;
    let history_id_prefix = format!("remote-gxserver:{}", remote_source.remote_machine_id);
    Ok(GpuiPreviousSessionsPage {
        cursor: result
            .get("cursor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        items: results
            .iter()
            .filter_map(|result| {
                gpui_gxserver_search_result_to_previous_session_item_with_options(
                    result,
                    history_id_prefix.as_str(),
                    remote_source.machine_name.as_deref(),
                )
            })
            .collect(),
    })
}

pub(crate) fn gpui_previous_sessions_list_params(request: &GpuiPreviousSessionsRequest) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert("includeActive".to_string(), serde_json::Value::Bool(false));
    params.insert("includePrevious".to_string(), serde_json::Value::Bool(true));
    params.insert(
        "limit".to_string(),
        serde_json::Value::Number(serde_json::Number::from(request.limit as u64)),
    );
    if let Some(cursor) = request.cursor.as_ref() {
        params.insert(
            "cursor".to_string(),
            serde_json::Value::String(cursor.clone()),
        );
    }
    if let Some(query) = request.query.as_ref() {
        params.insert(
            "query".to_string(),
            serde_json::Value::String(query.clone()),
        );
    }
    if let Some(session_tags) = request.session_tags.as_ref() {
        params.insert(
            "sessionTags".to_string(),
            serde_json::Value::Array(
                session_tags
                    .iter()
                    .map(|tag| serde_json::Value::String(tag.clone()))
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_sort_previous_session_items_by_closed_time(previous_sessions: &mut [serde_json::Value]) {
    previous_sessions.sort_by(|left, right| {
        gpui_previous_session_item_closed_time(right)
            .cmp(gpui_previous_session_item_closed_time(left))
    });
}

pub(crate) fn gpui_previous_session_item_closed_time(item: &serde_json::Value) -> &str {
    item.get("closedAt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(crate) fn gpui_previous_sessions_result_payload(
    request_id: &str,
    query: Option<&str>,
    cursor: Option<&str>,
    previous_sessions: Vec<serde_json::Value>,
) -> serde_json::Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "previousSessions".to_string(),
        serde_json::Value::Array(previous_sessions),
    );
    if let Some(query) = query {
        payload.insert(
            "query".to_string(),
            serde_json::Value::String(query.to_string()),
        );
    }
    if let Some(cursor) = cursor {
        payload.insert(
            "cursor".to_string(),
            serde_json::Value::String(cursor.to_string()),
        );
    }
    payload.insert(
        "requestId".to_string(),
        serde_json::Value::String(request_id.to_string()),
    );
    payload.insert(
        "type".to_string(),
        serde_json::Value::String("previousSessionsResult".to_string()),
    );
    serde_json::Value::Object(payload)
}

pub(crate) fn gpui_stashed_prompts_result_message(
    request_id: &str,
    project_id: Option<&str>,
) -> serde_json::Value {
    /*
    CDXC:StashedPrompts 2026-07-29:
    The Prompts modal loads stashed prompt-editor saves through the local
    gxserver `/api/listStashedPrompts` endpoint; a projectId param scopes the
    answer to that project plus its worktree family server-side. The rows are
    forwarded to the modal verbatim as a transient `stashedPromptsResult`
    payload — the prompt bodies are the product here, so Rust must not log,
    store, or reshape them, and transport failures return an empty list.
    */
    let mut params = serde_json::Map::new();
    if let Some(project_id) = project_id {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
    }
    let prompts = gpui_gxserver_rpc_result(
        "/api/listStashedPrompts",
        &serde_json::Value::Object(params),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| result.get("prompts").cloned())
    .filter(|prompts| prompts.is_array())
    .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    serde_json::json!({
        "prompts": prompts,
        "requestId": request_id,
        "type": "stashedPromptsResult",
    })
}

pub(crate) fn gpui_save_stashed_prompt_result_message(
    request_id: &str,
    content: &str,
    prompt_id: Option<&str>,
    project_id: Option<&str>,
    session_id: Option<&str>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "content".to_string(),
        serde_json::Value::String(content.to_string()),
    );
    if let Some(prompt_id) = prompt_id {
        params.insert(
            "promptId".to_string(),
            serde_json::Value::String(prompt_id.to_string()),
        );
    }
    if let Some(project_id) = project_id {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
    }
    if let Some(session_id) = session_id {
        params.insert(
            "sessionId".to_string(),
            serde_json::Value::String(session_id.to_string()),
        );
    }
    let prompt = gpui_gxserver_rpc_result(
        "/api/saveStashedPrompt",
        &serde_json::Value::Object(params),
        Duration::from_secs(10),
    )
    .ok()
    .and_then(|result| result.get("prompt").cloned())
    .filter(serde_json::Value::is_object);
    match prompt {
        Some(prompt) => serde_json::json!({
            "ok": true,
            "prompt": prompt,
            "requestId": request_id,
            "type": "saveStashedPromptResult",
        }),
        None => serde_json::json!({
            "error": "Could not save this prompt.",
            "ok": false,
            "requestId": request_id,
            "type": "saveStashedPromptResult",
        }),
    }
}

pub(crate) fn gpui_remote_previous_session_reference_from_history_id(
    history_id: &str,
) -> Option<GpuiRemotePreviousSessionReference> {
    let payload = history_id.strip_prefix("remote-gxserver:")?;
    let mut parts = payload.split(':');
    let remote_machine_id = parts.next().and_then(gpui_normalize_remote_machine_id)?;
    let project_id = parts.next()?;
    let session_id = parts.next()?;
    if parts.next().is_some()
        || !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_session_id_allowed(session_id)
    {
        return None;
    }
    Some(GpuiRemotePreviousSessionReference {
        remote_machine_id,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    })
}

pub(crate) fn gpui_remote_previous_session_source_for_reference<'a>(
    remote_sources: &'a [GpuiRemotePreviousSessionSource],
    reference: &GpuiRemotePreviousSessionReference,
) -> Option<&'a GpuiRemotePreviousSessionSource> {
    remote_sources
        .iter()
        .find(|source| source.remote_machine_id == reference.remote_machine_id)
}

pub(crate) const GPUI_PREVIOUS_SESSION_RESTORE_DEFAULT_TITLE: &str = "Terminal Session";

#[derive(Clone, Debug)]
pub(crate) struct GpuiPreviousSessionRestoreMetadata {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) title: String,
    pub(crate) session_tag: Option<String>,
    pub(crate) sidebar_order: Option<serde_json::Number>,
    pub(crate) session_persistence_name: Option<String>,
    pub(crate) session_persistence_provider: Option<String>,
}

impl GpuiPreviousSessionRestoreMetadata {
    pub(crate) fn default_title() -> Self {
        Self {
            agent_id: None,
            agent_session_id: None,
            title: GPUI_PREVIOUS_SESSION_RESTORE_DEFAULT_TITLE.to_string(),
            session_tag: None,
            sidebar_order: None,
            session_persistence_name: None,
            session_persistence_provider: None,
        }
    }
}

pub(crate) fn gpui_previous_session_restore_metadata_from_row(
    row: &serde_json::Map<String, serde_json::Value>,
) -> GpuiPreviousSessionRestoreMetadata {
    let title = gpui_trimmed_json_string_field(row, "displayTitle")
        .or_else(|| gpui_trimmed_json_string_field(row, "primaryTitle"))
        .or_else(|| gpui_trimmed_json_string_field(row, "title"))
        .unwrap_or(GPUI_PREVIOUS_SESSION_RESTORE_DEFAULT_TITLE)
        .to_string();
    let agent_id = gpui_trimmed_json_string_field(row, "agentId").map(str::to_string);
    let agent_session_id =
        gpui_trimmed_json_string_field(row, "agentSessionId").map(str::to_string);
    let session_tag = gpui_trimmed_json_string_field(row, "sessionTag").map(str::to_string);
    let sidebar_order = row
        .get("sidebarOrder")
        .and_then(serde_json::Value::as_number)
        .cloned();
    let session_persistence_name =
        gpui_trimmed_json_string_field(row, "sessionPersistenceName").map(str::to_string);
    let session_persistence_provider =
        gpui_trimmed_json_string_field(row, "sessionPersistenceProvider").map(str::to_string);
    GpuiPreviousSessionRestoreMetadata {
        agent_id,
        agent_session_id,
        title,
        session_tag,
        sidebar_order,
        session_persistence_name,
        session_persistence_provider,
    }
}

pub(crate) fn gpui_previous_session_restore_row_matches(
    row: &serde_json::Map<String, serde_json::Value>,
    project_id: &str,
    session_id: &str,
) -> bool {
    gpui_trimmed_json_string_field(row, "projectId") == Some(project_id)
        && gpui_trimmed_json_string_field(row, "sessionId") == Some(session_id)
}

pub(crate) fn gpui_previous_session_restore_row_is_running(
    row: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    json_bool_field(row, "isRunning") == Some(true)
        || json_string_field(row, "lifecycleState") == Some("running")
        || json_string_field(row, "providerSessionState") == Some("running")
}

pub(crate) fn gpui_previous_session_restore_metadata_params(
    project_id: &str,
    session_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "includeActive": false,
        "includePrevious": true,
        "limit": 20,
        "projectId": project_id,
        "query": session_id,
    })
}

pub(crate) fn gpui_previous_session_restore_metadata_from_result(
    result: &serde_json::Value,
    project_id: &str,
    session_id: &str,
) -> Option<GpuiPreviousSessionRestoreMetadata> {
    let results = result
        .get("results")
        .and_then(serde_json::Value::as_array)?;
    let mut running_match = None;
    for row in results.iter().filter_map(serde_json::Value::as_object) {
        if !gpui_previous_session_restore_row_matches(row, project_id, session_id) {
            continue;
        }
        let metadata = gpui_previous_session_restore_metadata_from_row(row);
        if !gpui_previous_session_restore_row_is_running(row) {
            return Some(metadata);
        }
        if running_match.is_none() {
            running_match = Some(metadata);
        }
    }
    running_match
}

pub(crate) fn gpui_previous_session_restore_metadata(
    project_id: &str,
    session_id: &str,
) -> Option<GpuiPreviousSessionRestoreMetadata> {
    let result = gpui_gxserver_rpc_result(
        "/api/listPreviousSessions",
        &gpui_previous_session_restore_metadata_params(project_id, session_id),
        Duration::from_secs(10),
    )
    .ok()?;
    gpui_previous_session_restore_metadata_from_result(&result, project_id, session_id)
}

pub(crate) fn gpui_remote_previous_session_restore_metadata(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
    session_id: &str,
) -> Option<GpuiPreviousSessionRestoreMetadata> {
    let result = gpui_remote_gxserver_rpc_result(
        target,
        "/api/listPreviousSessions",
        &gpui_previous_session_restore_metadata_params(project_id, session_id),
        Duration::from_secs(10),
    )
    .ok()?;
    gpui_previous_session_restore_metadata_from_result(&result, project_id, session_id)
}

pub(crate) fn gpui_previous_session_restore_create_params(
    project_id: &str,
    session_id: &str,
    metadata: GpuiPreviousSessionRestoreMetadata,
) -> serde_json::Value {
    let mut create_params = serde_json::Map::new();
    create_params.insert(
        "kind".to_string(),
        serde_json::Value::String("terminal".to_string()),
    );
    if let Some(agent_id) = metadata.agent_id {
        create_params.insert("agentId".to_string(), serde_json::Value::String(agent_id));
    }
    create_params.insert(
        "lifecycleState".to_string(),
        serde_json::Value::String("running".to_string()),
    );
    create_params.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    create_params.insert(
        "restoredFromSessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    create_params.insert(
        "surface".to_string(),
        serde_json::Value::String("workspace".to_string()),
    );
    create_params.insert(
        "title".to_string(),
        serde_json::Value::String(metadata.title),
    );
    let mut runtime_settings = serde_json::Map::new();
    if let Some(agent_session_id) = metadata.agent_session_id {
        runtime_settings.insert(
            "agentSessionId".to_string(),
            serde_json::Value::String(agent_session_id),
        );
    }
    if let Some(session_persistence_name) = metadata.session_persistence_name {
        runtime_settings.insert(
            "sessionPersistenceName".to_string(),
            serde_json::Value::String(session_persistence_name),
        );
    }
    if let Some(session_persistence_provider) = metadata.session_persistence_provider {
        runtime_settings.insert(
            "sessionPersistenceProvider".to_string(),
            serde_json::Value::String(session_persistence_provider),
        );
    }
    if !runtime_settings.is_empty() {
        create_params.insert(
            "runtimeSettings".to_string(),
            serde_json::Value::Object(runtime_settings),
        );
    }
    if let Some(session_tag) = metadata.session_tag {
        create_params.insert(
            "sessionTag".to_string(),
            serde_json::Value::String(session_tag),
        );
    }
    if let Some(sidebar_order) = metadata.sidebar_order {
        create_params.insert(
            "sidebarOrder".to_string(),
            serde_json::Value::Number(sidebar_order),
        );
    }
    serde_json::Value::Object(create_params)
}

pub(crate) fn gpui_delete_previous_session_from_history_id(
    history_id: &str,
    remote_sources: &[GpuiRemotePreviousSessionSource],
) {
    if let Some(reference) = gpui_remote_previous_session_reference_from_history_id(history_id) {
        let Some(remote_source) =
            gpui_remote_previous_session_source_for_reference(remote_sources, &reference)
        else {
            return;
        };
        let _ = gpui_remote_gxserver_rpc_result(
            &remote_source.target,
            "/api/removeSession",
            &serde_json::json!({
                "projectId": reference.project_id.as_str(),
                "reason": "deletePreviousSession",
                "sessionId": reference.session_id.as_str(),
            }),
            Duration::from_secs(10),
        );
        return;
    }

    let Some((project_id, session_id)) =
        gpui_previous_session_reference_from_history_id(history_id)
    else {
        return;
    };
    let _ = gpui_gxserver_rpc_result(
        "/api/removeSession",
        &serde_json::json!({
            "projectId": project_id,
            "reason": "deletePreviousSession",
            "sessionId": session_id,
        }),
        Duration::from_secs(10),
    );
}

pub(crate) fn gpui_restore_previous_session_from_history_id(
    history_id: &str,
    remote_sources: &[GpuiRemotePreviousSessionSource],
) -> Option<GpuiPreviousSessionRestoreResult> {
    /*
    CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
    Restore/delete commands from the shared Previous Sessions modal are local gxserver mutations only when the modal row carries the canonical `gxserver:<projectId>:<sessionId>` identity created by this projection. Restore creates a replacement workspace terminal with `restoredFromSessionId`, then removes the stopped history row only after create succeeds; unavailable gxserver or malformed history ids remain silent no-ops rather than fake success.
    */
    if let Some(reference) = gpui_remote_previous_session_reference_from_history_id(history_id) {
        let remote_source =
            gpui_remote_previous_session_source_for_reference(remote_sources, &reference)?;
        return gpui_restore_remote_previous_session(&reference, remote_source);
    }

    let (project_id, session_id) = gpui_previous_session_reference_from_history_id(history_id)?;
    let metadata = gpui_previous_session_restore_metadata(project_id, session_id)
        .unwrap_or_else(GpuiPreviousSessionRestoreMetadata::default_title);
    let response = gpui_gxserver_rpc_result(
        "/api/createSession",
        &gpui_previous_session_restore_create_params(project_id, session_id, metadata),
        Duration::from_secs(30),
    )
    .ok()?;
    let _ = gpui_gxserver_rpc_result(
        "/api/removeSession",
        &serde_json::json!({
            "projectId": project_id,
            "reason": "restorePreviousSession",
            "sessionId": session_id,
        }),
        Duration::from_secs(10),
    );
    // macOS opens the restored terminal as the active tab of the focused pane
    // (`createFocusedTabGroupPlacement`); GPUI follows up by focusing the
    // created session through the reviewed sidebar focusSession routing.
    let created = response.get("session")?;
    let created_project_id = created
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&project_id)
        .to_string();
    let created_session_id = created
        .get("sessionId")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    Some(GpuiPreviousSessionRestoreResult::Local {
        project_id: created_project_id,
        session_id: created_session_id,
    })
}

pub(crate) fn gpui_restore_remote_previous_session(
    reference: &GpuiRemotePreviousSessionReference,
    remote_source: &GpuiRemotePreviousSessionSource,
) -> Option<GpuiPreviousSessionRestoreResult> {
    /*
    CDXC:GPUIRemotePreviousSessions 2026-07-04-14:15:
    App-modal remote previous-session restore follows the SidebarApp runtime:
    recreate the workspace session on the owning remote gxserver, copy only
    metadata fields from that remote gxserver's previous-session row, then
    remove the old remote history row. No local gxserver session or renderer
    supplied remote connection details are involved.
    */
    let metadata = gpui_remote_previous_session_restore_metadata(
        &remote_source.target,
        reference.project_id.as_str(),
        reference.session_id.as_str(),
    )
    .unwrap_or_else(GpuiPreviousSessionRestoreMetadata::default_title);
    let response = gpui_remote_gxserver_rpc_result(
        &remote_source.target,
        "/api/createSession",
        &gpui_previous_session_restore_create_params(
            reference.project_id.as_str(),
            reference.session_id.as_str(),
            metadata,
        ),
        Duration::from_secs(30),
    )
    .ok()?;
    let _ = gpui_remote_gxserver_rpc_result(
        &remote_source.target,
        "/api/removeSession",
        &serde_json::json!({
            "projectId": reference.project_id.as_str(),
            "reason": "restorePreviousSession",
            "sessionId": reference.session_id.as_str(),
        }),
        Duration::from_secs(10),
    );
    let created = response.get("session")?;
    let created_project_id = created
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(reference.project_id.as_str())
        .to_string();
    let created_session_id = created
        .get("sessionId")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(created_project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(created_session_id.as_str())
    {
        return None;
    }
    Some(GpuiPreviousSessionRestoreResult::Remote {
        project_id: created_project_id,
        remote_machine_id: reference.remote_machine_id.clone(),
        session_id: created_session_id,
    })
}

pub(crate) fn gpui_combined_presentation_session_focus_id(
    project_id: &str,
    session_id: &str,
) -> Option<String> {
    // The shared projection URI-encodes both id parts of the combined
    // `combined-session:` sidebar id. Build the id only from characters that
    // URI-encode to themselves so Rust never re-implements the encoder; other
    // ids skip the focus follow-up instead of guessing an encoding.
    let encodes_to_itself = |value: &str| {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
    };
    (encodes_to_itself(project_id) && encodes_to_itself(session_id))
        .then(|| format!("combined-session:{project_id}:{session_id}"))
}

pub(crate) fn gpui_close_daemon_session_and_refresh_state(
    project_id: Option<String>,
    session_id: Option<String>,
    active_project_id: Option<&str>,
    focused_session_id: Option<&str>,
) -> serde_json::Value {
    let error_message = match (project_id, session_id) {
        (Some(project_id), Some(session_id)) => {
            /*
            CDXC:GPUIDaemonSessionsModal 2026-06-24-12:00:
            Running Sessions can close only gxserver-owned rows whose modal payload carries both project/workspace id and session id. Use `/api/transitionSession` with `close` instead of `/api/removeSession` so gxserver owns provider shutdown and lifecycle history, while malformed ids or transport failures refresh the list with an honest error and no fake success.
            */
            if gpui_gxserver_rpc_result(
                "/api/transitionSession",
                &serde_json::json!({
                    "action": "close",
                    "projectId": project_id,
                    "reason": "gpuiRunningSessionsModal",
                    "sessionId": session_id,
                }),
                Duration::from_secs(30),
            )
            .is_ok()
            {
                None
            } else {
                Some("GPUI could not close that gxserver session. The Running Sessions list was refreshed without reporting fake success.".to_string())
            }
        }
        _ => Some(
            "GPUI could not identify that gxserver session. The Running Sessions list was refreshed without changing daemon state."
                .to_string(),
        ),
    };
    gpui_daemon_sessions_state_message(error_message, active_project_id, focused_session_id)
}

pub(crate) fn gpui_daemon_sessions_state_message(
    error_message: Option<String>,
    active_project_id: Option<&str>,
    _focused_session_id: Option<&str>,
) -> serde_json::Value {
    /*
    CDXC:GPUIDaemonSessionsModal 2026-06-24-12:00:
    GPUI Running Sessions state is built from real local gxserver health and
    `/api/readPresentationSnapshot` only. If gxserver, auth, health, or
    presentation is unavailable, return the shared daemonSessionsState shape
    with empty rows and an explicit error message; do not invent daemon state,
    terminal text, commands, URLs, tokens, raw responses, or fallback sessions.
    */
    let health = gpui_gxserver_server_health(Duration::from_secs(2)).ok();
    let daemon = health
        .as_ref()
        .and_then(|health| gpui_daemon_info_from_gxserver_health(health).ok());
    let snapshot = match gpui_read_gxserver_presentation_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            let unavailable_message = error_message.unwrap_or_else(|| {
                "Local gxserver is unavailable, so Running Sessions cannot load shared daemon sessions."
                    .to_string()
            });
            return gpui_daemon_sessions_state_payload(
                daemon,
                Vec::new(),
                Some(unavailable_message),
            );
        }
    };
    let sessions =
        gpui_daemon_session_items_from_presentation_snapshot(&snapshot, active_project_id);
    gpui_daemon_sessions_state_payload(daemon, sessions, error_message)
}

pub(crate) fn gpui_daemon_sessions_state_payload(
    daemon: Option<serde_json::Value>,
    sessions: Vec<serde_json::Value>,
    error_message: Option<String>,
) -> serde_json::Value {
    let mut payload = serde_json::Map::new();
    if let Some(daemon) = daemon {
        payload.insert("daemon".to_string(), daemon);
    }
    if let Some(error_message) = error_message {
        payload.insert(
            "errorMessage".to_string(),
            serde_json::Value::String(error_message),
        );
    }
    payload.insert("sessions".to_string(), serde_json::Value::Array(sessions));
    payload.insert(
        "type".to_string(),
        serde_json::Value::String("daemonSessionsState".to_string()),
    );
    serde_json::Value::Object(payload)
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_spawn_remote_runtime(
    target: &SourceCodeServerRuntimeTarget,
    startup_deadline: Instant,
) -> Result<SourceCodeServerRuntimeStartOutput, String> {
    let SourceCodeServerRuntimeEndpoint::Remote {
        component_platform,
        execution_target,
        machine_config,
        ..
    } = &target.endpoint
    else {
        return Err("Remote Source runtime target is invalid.".to_string());
    };
    let store = on_demand_component_store()?
        .ok_or_else(|| "The sealed code-server component manifest is unavailable.".to_string())?;
    let installed =
        store.query_current_for_platform(SOURCE_CODE_SERVER_COMPONENT_NAME, component_platform)?;
    if !installed.installed {
        return Err("The Linux code-server component is not installed.".to_string());
    }
    source_code_server_validate_remote_linux_payload(&installed.path)?;
    source_code_server_ensure_remote_payload(machine_config, execution_target, &installed)?;

    for local_port in source_code_server_remote_candidate_ports() {
        if Instant::now() >= startup_deadline {
            break;
        }
        let askpass = gpui_remote_ssh_askpass_script(machine_config)?;
        let mut arguments = gpui_remote_ssh_client_options(machine_config.has_saved_password);
        arguments.extend([
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-L".to_string(),
            format!("127.0.0.1:{local_port}:127.0.0.1:{SOURCE_CODE_SERVER_REMOTE_PORT}"),
        ]);
        arguments.extend(gpui_remote_ssh_target_arguments(machine_config));
        arguments.push(gpui_remote_command_for_execution_target(
            execution_target,
            source_code_server_remote_launch_command(target.project_path.as_path()).as_str(),
        ));
        let mut command = Command::new("/usr/bin/ssh");
        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(environment) = gpui_remote_ssh_askpass_environment(askpass.as_ref()) {
            command.envs(environment);
        }
        let started_at = Instant::now();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => continue,
        };
        let readiness = source_code_server_wait_until_responsive_at(
            local_port,
            startup_deadline.saturating_duration_since(Instant::now()),
        );
        let child_running = child.try_wait().ok().flatten().is_none();
        if readiness.is_ready() && child_running {
            return Ok(SourceCodeServerRuntimeStartOutput {
                child,
                runtime_origin: format!("http://127.0.0.1:{local_port}"),
                prompt_editor_ipc_ready: readiness.prompt_editor_ipc_ready,
                started_at,
                http_runtime_ready: readiness.http_runtime_ready,
            });
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    Err("Remote Source runtime did not become reachable through SSH.".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_ensure_remote_payload(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    installed: &component_store::InstalledComponent,
) -> Result<(), String> {
    let component_key = format!("{}:{}", installed.version, installed.platform);
    let ready_command = source_code_server_remote_payload_ready_command(component_key.as_str());
    let ready = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        ready_command.as_str(),
        Duration::from_secs(15),
    );
    if ready.exit_code == 0 {
        return Ok(());
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let archive_path = env::temp_dir().join(format!(
        "ghostex-code-server-{}-{unique:x}.tar.gz",
        std::process::id()
    ));
    let archive_result = Command::new("/usr/bin/tar")
        .args(["-czf"])
        .arg(&archive_path)
        .arg("-C")
        .arg(&installed.path)
        .arg(".")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| "Could not package the Linux code-server component.".to_string())?;
    if !archive_result.success() {
        let _ = fs::remove_file(&archive_path);
        return Err("Could not package the Linux code-server component.".to_string());
    }
    let upload_command = source_code_server_remote_payload_upload_command(component_key.as_str());
    let upload = gpui_run_remote_ssh_with_stdin_file_in_execution_target(
        config,
        execution_target,
        upload_command.as_str(),
        archive_path.as_path(),
        Duration::from_secs(180),
    );
    let _ = fs::remove_file(&archive_path);
    if upload.exit_code != 0 {
        return Err("Could not upload the Linux code-server component over SSH.".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_data_root_script() -> &'static str {
    r#"if [ -n "${GHOSTEX_HOME:-}" ] && [ "${GHOSTEX_HOME#/}" != "$GHOSTEX_HOME" ]; then ghostex_data="$GHOSTEX_HOME"; elif [ -n "${XDG_DATA_HOME:-}" ]; then ghostex_data="$XDG_DATA_HOME/ghostex"; else ghostex_data="$HOME/.local/share/ghostex"; fi
code_root="$ghostex_data/code-server""#
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_payload_ready_command(component_key: &str) -> String {
    format!(
        "set -eu\n{}\ntest -f \"$code_root/package/.ghostex-component-key\"\ntest \"$(cat \"$code_root/package/.ghostex-component-key\")\" = {}\ntest -x \"$code_root/package/lib/node\"\ntest -f \"$code_root/package/out/node/entry.js\"\ntest -f \"$code_root/package/lib/vscode/out/server-main.js\"",
        source_code_server_remote_data_root_script(),
        gpui_shell_quote(component_key),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_payload_upload_command(component_key: &str) -> String {
    format!(
        "set -eu\n{}\numask 077\nreleases=\"$code_root/releases\"\nmkdir -p \"$releases\"\nstage=\"$releases/.install-$$\"\nrelease=\"$releases/{}\"\npointer_stage=\"$code_root/.package-$$\"\ncleanup() {{ rm -rf -- \"$stage\"; rm -f -- \"$pointer_stage\"; }}\ntrap cleanup EXIT HUP INT TERM\nfor abandoned in \"$releases\"/.install-* \"$code_root\"/.package-*; do\n  if [ ! -e \"$abandoned\" ] && [ ! -L \"$abandoned\" ]; then continue; fi\n  abandoned_pid=${{abandoned##*-}}\n  case \"$abandoned_pid\" in ''|*[!0-9]*) continue ;; esac\n  if [ ! -d \"/proc/$abandoned_pid\" ]; then rm -rf -- \"$abandoned\"; fi\ndone\nrm -rf -- \"$stage\"\nrm -f -- \"$pointer_stage\"\nmkdir \"$stage\"\ntar -xzf - -C \"$stage\"\ntest -x \"$stage/lib/node\"\ntest -f \"$stage/out/node/entry.js\"\ntest -f \"$stage/lib/vscode/out/server-main.js\"\nprintf '%s' {} >\"$stage/.ghostex-component-key\"\nif [ -e \"$release\" ]; then\n  test -f \"$release/.ghostex-component-key\"\n  test \"$(cat \"$release/.ghostex-component-key\")\" = {}\n  test -x \"$release/lib/node\"\n  test -f \"$release/out/node/entry.js\"\n  test -f \"$release/lib/vscode/out/server-main.js\"\n  rm -rf -- \"$stage\"\nelif ! mv -T \"$stage\" \"$release\"; then\n  test -f \"$release/.ghostex-component-key\"\n  test \"$(cat \"$release/.ghostex-component-key\")\" = {}\n  test -x \"$release/lib/node\"\n  test -f \"$release/out/node/entry.js\"\n  test -f \"$release/lib/vscode/out/server-main.js\"\n  rm -rf -- \"$stage\"\nfi\nprevious_release=$(readlink -f \"$code_root/package\" 2>/dev/null || true)\nln -s \"releases/{}\" \"$pointer_stage\"\nmv -Tf \"$pointer_stage\" \"$code_root/package\"\nfor candidate in \"$releases\"/*; do\n  [ -d \"$candidate\" ] || continue\n  if [ \"$candidate\" = \"$release\" ] || [ \"$candidate\" = \"$previous_release\" ]; then continue; fi\n  if find \"$candidate\" -maxdepth 0 -mmin +10 -print -quit | grep -q .; then rm -rf -- \"$candidate\"; fi\ndone\ntrap - EXIT HUP INT TERM",
        source_code_server_remote_data_root_script(),
        installed_key_path_fragment(component_key),
        gpui_shell_quote(component_key),
        gpui_shell_quote(component_key),
        gpui_shell_quote(component_key),
        installed_key_path_fragment(component_key),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn installed_key_path_fragment(component_key: &str) -> String {
    component_key
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_launch_command(project_path: &Path) -> String {
    format!(
        "set -eu\n{}\npackage=\"$code_root/package\"\nruntime=\"$code_root/runtime\"\nmkdir -p \"$runtime/user-data\" \"$runtime/extensions\"\ncd {}\nchild=\ncleanup() {{ if [ -n \"$child\" ]; then kill \"$child\" 2>/dev/null || true; wait \"$child\" 2>/dev/null || true; fi; }}\ntrap cleanup EXIT HUP INT TERM\n\"$package/lib/node\" \"$package/out/node/entry.js\" --auth none --bind-addr 127.0.0.1:{} --disable-telemetry --disable-update-check --disable-workspace-trust --disable-getting-started-override --ignore-last-opened --app-name 'ghostex Code' --user-data-dir \"$runtime/user-data\" --extensions-dir \"$runtime/extensions\" &\nchild=$!\nwait \"$child\"",
        source_code_server_remote_data_root_script(),
        gpui_shell_quote(gpui_path_string(project_path).as_str()),
        SOURCE_CODE_SERVER_REMOTE_PORT,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn source_code_server_remote_candidate_ports() -> Vec<u16> {
    let range =
        u64::from(SOURCE_CODE_SERVER_TUNNEL_PORT_MAX - SOURCE_CODE_SERVER_TUNNEL_PORT_MIN + 1);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id());
    (0..SOURCE_CODE_SERVER_TUNNEL_ATTEMPTS)
        .map(|offset| SOURCE_CODE_SERVER_TUNNEL_PORT_MIN + ((seed + offset as u64) % range) as u16)
        .collect()
}

pub(crate) fn source_code_server_validate_remote_linux_payload(repo_root: &Path) -> Result<(), String> {
    for relative_path in [
        "lib/node",
        "out/node/entry.js",
        "lib/vscode/out/server-main.js",
    ] {
        if !repo_root.join(relative_path).is_file() {
            return Err("The Linux code-server component is incomplete.".to_string());
        }
    }
    Ok(())
}

pub(crate) fn run_remote_manage_files_bridge_request_for_project_snapshot(
    payload: &str,
    snapshot: Option<&GpuiProjectSnapshot>,
    additional_docs_folders_text: &str,
    reference: &GpuiRemoteProjectReference,
    target: Option<&GpuiRemoteGxserverRequestTarget>,
) -> ManageFilesBridgeOutcome {
    /*
    CDXC:RemoteProjectDocs 2026-08-06:
    Remote Docs uses the owning gxserver as the filesystem boundary. The CEF
    request must still match the active machine-scoped project/editor snapshot,
    then Rust replaces that scoped presentation id with the daemon's raw
    project id and forwards only the fixed Docs action fields through the live
    authenticated tunnel. No remote path, host, token, or generic endpoint
    authority crosses the renderer bridge; a disconnected machine fails
    directly instead of probing the same path on the local Mac.
    */
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let action = manage_request_string(&request, "action").unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    let result = (|| {
        let snapshot =
            snapshot.ok_or_else(|| "No active Docs project is available.".to_string())?;
        manage_validate_request_identity(&request, snapshot)?;
        if !matches!(
            action.as_str(),
            "list"
                | "read"
                | "stat"
                | "save"
                | "rename"
                | "delete"
                | "duplicate"
                | "createFolder"
                | "move"
                | "copyFullPath"
                | "addToSessionContext"
        ) {
            return if action == "revealInFinder" {
                Err("Reveal in Finder is unavailable for remote Docs items.".to_string())
            } else {
                Err("Unsupported Docs file action.".to_string())
            };
        }
        let target =
            target.ok_or_else(|| "Reconnect the remote machine to use Docs.".to_string())?;
        let mut params = serde_json::Map::new();
        params.insert(
            "action".to_string(),
            serde_json::Value::String(action.clone()),
        );
        params.insert(
            "requestId".to_string(),
            serde_json::Value::String(request_id.clone()),
        );
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(reference.project_id.clone()),
        );
        params.insert(
            "additionalDocsFolders".to_string(),
            serde_json::Value::String(additional_docs_folders_text.to_string()),
        );
        for key in ["path", "newPath", "content"] {
            if let Some(value) = manage_request_string(&request, key) {
                params.insert(key.to_string(), serde_json::Value::String(value));
            }
        }
        let response = gpui_remote_gxserver_rpc_result(
            target,
            "/api/runProjectDocsAction",
            &serde_json::Value::Object(params),
            Duration::from_secs(30),
        )?;
        if !response.is_object()
            || response.get("action").and_then(serde_json::Value::as_str) != Some(action.as_str())
            || response
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                != Some(request_id.as_str())
        {
            return Err("The remote Docs service returned an invalid response.".to_string());
        }
        Ok(response)
    })();
    manage_files_bridge_outcome(action, request_id, result)
}

pub(crate) fn read_remote_manage_docs_resource(
    target: Option<&GpuiRemoteGxserverRequestTarget>,
    project_id: &str,
    relative_path: &str,
    additional_docs_folders_text: &str,
) -> Option<Vec<u8>> {
    let target = target?;
    let request_id = format!(
        "docs-resource-{}",
        MANAGE_REMOTE_RESOURCE_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let response = gpui_remote_gxserver_rpc_result(
        target,
        "/api/runProjectDocsAction",
        &serde_json::json!({
            "action": "readResource",
            "additionalDocsFolders": additional_docs_folders_text,
            "path": relative_path,
            "projectId": project_id,
            "requestId": request_id,
        }),
        Duration::from_secs(30),
    )
    .ok()?;
    if !response.is_object()
        || response.get("action").and_then(serde_json::Value::as_str) != Some("readResource")
        || response
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            != Some(request_id.as_str())
    {
        return None;
    }
    let encoded = response
        .get("dataBase64")
        .and_then(serde_json::Value::as_str)?;
    let data = BASE64_STANDARD.decode(encoded).ok()?;
    (data.len() <= MANAGE_REMOTE_RESOURCE_MAX_BYTES).then_some(data)
}

pub(crate) fn gpui_remote_gxserver_rpc_result(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let (status_code, body) =
        gpui_remote_gxserver_post_typed_operation(target, path, params, timeout)?;
    if !(200..300).contains(&status_code) {
        return Err("Remote gxserver request failed.".to_string());
    }
    parse_gpui_gxserver_rpc_result(&body)
}

/*
CDXC:AddProject 2026-07-30:
The add-project dialog is the one gxserver surface that shows the daemon's own
rejection text ("Workspace root is not a directory: …", "Destination path
already exists and is not empty.", a `gh` lookup failure). Generic errors made
the old remote add-project flow unusable: the user could not tell a bad path
from a dead tunnel. Only the daemon's structured `message` is forwarded, and
only for a rejected request — transport failures stay as fixed local copy so no
tunnel host, port, token, or raw response body can reach the renderer.
*/
pub(crate) fn gpui_remote_gxserver_presentation_stream_loop(
    target: GpuiRemoteGxserverRequestTarget,
    cancel: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<GpuiRemoteGxserverPresentationStreamMessage>,
    client_id: String,
    last_revision: Option<u64>,
) {
    let mut last_revision = last_revision;
    for attempt in 0..GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_ATTEMPTS {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        let _ = gpui_remote_gxserver_presentation_stream_once(
            &target,
            cancel.as_ref(),
            &tx,
            client_id.as_str(),
            &mut last_revision,
        );
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        if attempt + 1 < GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_ATTEMPTS {
            thread::sleep(GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_RECONNECT_DELAY);
        }
    }
    if !cancel.load(Ordering::SeqCst) {
        let _ = tx.unbounded_send(GpuiRemoteGxserverPresentationStreamMessage::Failed);
    }
}

pub(crate) fn gpui_remote_gxserver_presentation_stream_once(
    target: &GpuiRemoteGxserverRequestTarget,
    cancel: &AtomicBool,
    tx: &mpsc::UnboundedSender<GpuiRemoteGxserverPresentationStreamMessage>,
    client_id: &str,
    last_revision: &mut Option<u64>,
) -> Result<(), String> {
    /*
    CDXC:GPUIRemotePresentationStreaming 2026-06-24-19:54:
    The remote presentation stream uses gxserver's existing WebSocket protocol through the authenticated localhost tunnel. The only renderer-bound products of this helper are presentation snapshots and deltas already consumed by SidebarApp; eventStreamReady, server ids, auth details, raw response bodies, and transport failures stay native-only.
    */
    let mut stream = gpui_remote_gxserver_open_presentation_websocket(target, cancel)?;
    stream
        .set_read_timeout(Some(GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_READ_TIMEOUT))
        .map_err(|_| "Could not configure remote gxserver event read timeout.".to_string())?;
    let subscribe = gpui_remote_gxserver_presentation_subscribe_message(client_id, *last_revision);
    gpui_websocket_write_text_frame(&mut stream, subscribe.as_bytes())?;
    let mut liveness = GpuiRemoteGxserverPresentationLiveness::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Ok(());
        }
        let Some(frame) = gpui_websocket_read_frame(&mut stream, target, &mut liveness, cancel)?
        else {
            return Ok(());
        };
        match frame {
            GpuiWebSocketFrame::Text(text) => {
                let Ok(event) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
                    continue;
                };
                let Some((payload, revision)) =
                    gpui_remote_gxserver_presentation_event_payload(&event)
                else {
                    continue;
                };
                if let Some(revision) = revision {
                    *last_revision = Some(revision);
                }
                tx.unbounded_send(GpuiRemoteGxserverPresentationStreamMessage::Event(payload))
                    .map_err(|_| "Remote presentation receiver closed.".to_string())?;
            }
            GpuiWebSocketFrame::Ping(payload) => {
                gpui_websocket_write_control_frame(&mut stream, 0xA, &payload)?;
            }
            GpuiWebSocketFrame::Close => {
                return Err("Remote gxserver event stream closed.".to_string());
            }
            GpuiWebSocketFrame::Pong | GpuiWebSocketFrame::Other => {}
        }
    }
}

pub(crate) fn gpui_remote_gxserver_open_presentation_websocket(
    target: &GpuiRemoteGxserverRequestTarget,
    cancel: &AtomicBool,
) -> Result<TcpStream, String> {
    let address = format!("127.0.0.1:{}", target.local_port);
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "Remote gxserver event stream is not reachable.".to_string())?;
    stream
        .set_read_timeout(Some(GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_READ_TIMEOUT))
        .map_err(|_| "Could not configure remote gxserver event read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(
            GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_HANDSHAKE_TIMEOUT,
        ))
        .map_err(|_| "Could not configure remote gxserver event write timeout.".to_string())?;
    let request = format!(
        "GET /api/events?protocolVersion={GPUI_GXSERVER_PROTOCOL_VERSION}&authToken={} HTTP/1.1\r\nHost: {address}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
        target.token.as_str(),
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send remote gxserver event handshake.".to_string())?;
    let headers = gpui_read_websocket_handshake_headers(&mut stream, cancel)?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "Remote gxserver returned an invalid event stream status.".to_string())?;
    if status != 101 {
        return Err("Remote gxserver did not accept the event stream.".to_string());
    }
    Ok(stream)
}

pub(crate) fn gpui_read_websocket_handshake_headers(
    stream: &mut TcpStream,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let deadline = Instant::now() + GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_HANDSHAKE_TIMEOUT;
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while Instant::now() < deadline {
        if cancel.load(Ordering::SeqCst) {
            return Err("Remote gxserver event stream was cancelled.".to_string());
        }
        match stream.read(&mut byte) {
            Ok(0) => return Err("Remote gxserver closed the event handshake.".to_string()),
            Ok(_) => {
                bytes.push(byte[0]);
                if bytes.ends_with(b"\r\n\r\n") {
                    return String::from_utf8(bytes)
                        .map_err(|_| "Remote gxserver event handshake was invalid.".to_string());
                }
                if bytes.len() > 16 * 1024 {
                    return Err("Remote gxserver event handshake was too large.".to_string());
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err("Could not read remote gxserver event handshake.".to_string()),
        }
    }
    Err("Remote gxserver event handshake timed out.".to_string())
}

pub(crate) fn gpui_remote_gxserver_presentation_subscribe_message(
    client_id: &str,
    last_revision: Option<u64>,
) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("clientId".to_string(), serde_json::json!(client_id));
    if let Some(last_revision) = last_revision {
        payload.insert("lastRevision".to_string(), serde_json::json!(last_revision));
    }
    payload.insert(
        "type".to_string(),
        serde_json::json!("subscribePresentation"),
    );
    serde_json::Value::Object(payload).to_string()
}

pub(crate) enum GpuiWebSocketFrame {
    Close,
    Other,
    Ping(Vec<u8>),
    Pong,
    Text(String),
}

pub(crate) struct GpuiRemoteGxserverPresentationLiveness {
    pub(crate) next_health_check: Instant,
}

impl GpuiRemoteGxserverPresentationLiveness {
    pub(crate) fn new() -> Self {
        Self {
            next_health_check: Instant::now() + GPUI_REMOTE_GXSERVER_PRESENTATION_HEALTH_INTERVAL,
        }
    }

    pub(crate) fn confirm_after_read_timeout(
        &mut self,
        target: &GpuiRemoteGxserverRequestTarget,
    ) -> Result<(), String> {
        if Instant::now() < self.next_health_check {
            return Ok(());
        }
        self.next_health_check = Instant::now() + GPUI_REMOTE_GXSERVER_PRESENTATION_HEALTH_INTERVAL;
        if gpui_remote_authenticated_health(target.local_port, target.token.as_str()).is_some() {
            Ok(())
        } else {
            Err("Remote gxserver event stream failed its liveness check.".to_string())
        }
    }
}

pub(crate) fn gpui_websocket_read_frame(
    stream: &mut TcpStream,
    target: &GpuiRemoteGxserverRequestTarget,
    liveness: &mut GpuiRemoteGxserverPresentationLiveness,
    cancel: &AtomicBool,
) -> Result<Option<GpuiWebSocketFrame>, String> {
    let mut header = [0u8; 2];
    if !gpui_websocket_read_exact(stream, &mut header, target, liveness, cancel)? {
        return Ok(None);
    }
    let fin = header[0] & 0x80 != 0;
    let opcode = header[0] & 0x0F;
    if !fin {
        return Err("Remote gxserver sent a fragmented event frame.".to_string());
    }
    let masked = header[1] & 0x80 != 0;
    let mut payload_len = u64::from(header[1] & 0x7F);
    if payload_len == 126 {
        let mut extended = [0u8; 2];
        if !gpui_websocket_read_exact(stream, &mut extended, target, liveness, cancel)? {
            return Ok(None);
        }
        payload_len = u64::from(u16::from_be_bytes(extended));
    } else if payload_len == 127 {
        let mut extended = [0u8; 8];
        if !gpui_websocket_read_exact(stream, &mut extended, target, liveness, cancel)? {
            return Ok(None);
        }
        payload_len = u64::from_be_bytes(extended);
    }
    if payload_len as usize > GPUI_REMOTE_GXSERVER_PRESENTATION_STREAM_FRAME_MAX_BYTES {
        return Err("Remote gxserver event frame was too large.".to_string());
    }
    let mut mask = [0u8; 4];
    if masked && !gpui_websocket_read_exact(stream, &mut mask, target, liveness, cancel)? {
        return Ok(None);
    }
    let mut payload = vec![0u8; payload_len as usize];
    if !payload.is_empty()
        && !gpui_websocket_read_exact(stream, &mut payload, target, liveness, cancel)?
    {
        return Ok(None);
    }
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % mask.len()];
        }
    }
    match opcode {
        0x1 => String::from_utf8(payload)
            .map(GpuiWebSocketFrame::Text)
            .map(Some)
            .map_err(|_| "Remote gxserver event frame was not UTF-8.".to_string()),
        0x8 => Ok(Some(GpuiWebSocketFrame::Close)),
        0x9 => Ok(Some(GpuiWebSocketFrame::Ping(payload))),
        0xA => Ok(Some(GpuiWebSocketFrame::Pong)),
        _ => Ok(Some(GpuiWebSocketFrame::Other)),
    }
}

pub(crate) fn gpui_websocket_read_exact(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    target: &GpuiRemoteGxserverRequestTarget,
    liveness: &mut GpuiRemoteGxserverPresentationLiveness,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let mut offset = 0;
    while offset < buffer.len() {
        if cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        match stream.read(&mut buffer[offset..]) {
            Ok(0) => return Err("Remote gxserver closed the event stream.".to_string()),
            Ok(read_count) => {
                offset += read_count;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                liveness.confirm_after_read_timeout(target)?;
                continue;
            }
            Err(_) => return Err("Could not read remote gxserver event stream.".to_string()),
        }
    }
    Ok(true)
}

pub(crate) fn gpui_websocket_write_text_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    gpui_websocket_write_frame(stream, 0x1, payload)
}

pub(crate) fn gpui_websocket_write_control_frame(
    stream: &mut TcpStream,
    opcode: u8,
    payload: &[u8],
) -> Result<(), String> {
    if payload.len() > 125 {
        return Err("Remote gxserver control frame was too large.".to_string());
    }
    gpui_websocket_write_frame(stream, opcode, payload)
}

pub(crate) fn gpui_websocket_write_frame(
    stream: &mut TcpStream,
    opcode: u8,
    payload: &[u8],
) -> Result<(), String> {
    let mask = gpui_websocket_mask();
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | (opcode & 0x0F));
    if payload.len() <= 125 {
        frame.push(0x80 | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        frame.push(*byte ^ mask[index % mask.len()]);
    }
    stream
        .write_all(&frame)
        .map_err(|_| "Could not write remote gxserver event frame.".to_string())
}

pub(crate) fn gpui_websocket_mask() -> [u8; 4] {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32)
        .wrapping_add(std::process::id())
        .to_be_bytes()
}

pub(crate) fn gpui_remote_gxserver_presentation_event_payload(
    event: &serde_json::Value,
) -> Option<(serde_json::Value, Option<u64>)> {
    let object = event.as_object()?;
    match object.get("type").and_then(serde_json::Value::as_str)? {
        "presentationSnapshot" => {
            let snapshot = object.get("snapshot")?;
            if !gpui_remote_gxserver_presentation_snapshot_allowed(snapshot) {
                return None;
            }
            let revision = gpui_json_revision(snapshot).or_else(|| gpui_json_revision(event));
            Some((
                serde_json::json!({
                    "snapshot": (*snapshot).clone(),
                    "type": "presentationSnapshot",
                }),
                revision,
            ))
        }
        "presentationDelta" => {
            let delta = object.get("delta")?;
            if !gpui_remote_gxserver_presentation_delta_allowed(delta) {
                return None;
            }
            let revision = gpui_json_revision(event)?;
            Some((
                serde_json::json!({
                    "delta": (*delta).clone(),
                    "revision": revision,
                    "type": "presentationDelta",
                }),
                Some(revision),
            ))
        }
        _ => None,
    }
}

pub(crate) fn gpui_remote_gxserver_presentation_snapshot_allowed(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .is_some()
        && object
            .get("projects")
            .and_then(serde_json::Value::as_array)
            .is_some()
        && object
            .get("sessions")
            .and_then(serde_json::Value::as_array)
            .is_some()
        && gpui_json_revision(value).is_some()
}

pub(crate) fn gpui_remote_gxserver_presentation_delta_allowed(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(serde_json::Value::as_str)
        .is_some()
}

pub(crate) fn gpui_json_revision(value: &serde_json::Value) -> Option<u64> {
    value.get("revision").and_then(|revision| {
        revision.as_u64().or_else(|| {
            revision
                .as_i64()
                .and_then(|value| u64::try_from(value).ok())
        })
    })
}

pub(crate) fn gpui_remote_gxserver_post_typed_operation(
    target: &GpuiRemoteGxserverRequestTarget,
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<(u16, String), String> {
    /*
    CDXC:GPUIRemoteMachines 2026-06-24-16:48:
    Remote gxserver RPCs use the live SSH tunnel target and in-memory token captured by Rust after Keychain storage. Keep this helper transport-only and response-unlogged: callers own endpoint allowlists and user-facing generic errors, while tokens, URLs, remote paths, params, stdout/stderr, and daemon bodies are never persisted or copied to renderer globals.
    */
    if !path.starts_with("/api/") {
        return Err("Invalid remote gxserver API path.".to_string());
    }
    let address = format!("127.0.0.1:{}", target.local_port);
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "Remote gxserver tunnel is not reachable.".to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "Could not configure remote gxserver read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "Could not configure remote gxserver write timeout.".to_string())?;

    let body = serde_json::json!({
        "protocolVersion": GPUI_GXSERVER_PROTOCOL_VERSION,
        "params": params,
    })
    .to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\nContent-Length: {}\r\n\r\n{body}",
        target.token.as_str(),
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send remote gxserver request.".to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| "Could not read remote gxserver response.".to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "Remote gxserver returned an invalid HTTP response.".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "Remote gxserver returned an invalid HTTP status.".to_string())?;
    Ok((status_code, gxserver_http_response_body(headers, body)?))
}

pub(crate) fn gpui_remote_gxserver_project_path_by_id(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Result<String, String> {
    /*
    CDXC:GPUIRemoteNativeActions 2026-06-24-19:25:
    Remote project path copy must resolve the path from the owning remote gxserver immediately before writing the local clipboard. React may identify only the saved machine and project id; it must not provide or authorize the remote path string.

    CDXC:GPUIRemoteNativeActions 2026-06-24-20:26:
    Remote IDE opens reuse this resolver so project and changed-file editor launches also receive remote paths only from the owning gxserver, never from React payloads, DOM labels, cached presentation text, or renderer-built URI strings.
    */
    gpui_remote_gxserver_project_path_by_id_from_endpoint(
        target,
        "/api/listProjects",
        "projects",
        project_id,
    )?
    .or_else(|| {
        gpui_remote_gxserver_project_path_by_id_from_endpoint(
            target,
            "/api/listRecentProjects",
            "recentProjects",
            project_id,
        )
        .ok()
        .flatten()
    })
    .ok_or_else(|| "GPUI could not resolve that remote project path.".to_string())
}

pub(crate) fn gpui_remote_gxserver_project_path_by_id_from_endpoint(
    target: &GpuiRemoteGxserverRequestTarget,
    endpoint: &str,
    array_key: &str,
    project_id: &str,
) -> Result<Option<String>, String> {
    let result = gpui_remote_gxserver_rpc_result(
        target,
        endpoint,
        &serde_json::json!({}),
        Duration::from_secs(10),
    )?;
    Ok(result
        .get(array_key)
        .and_then(serde_json::Value::as_array)
        .and_then(|projects| {
            projects
                .iter()
                .filter_map(serde_json::Value::as_object)
                .find_map(|project| {
                    (json_string_field(project, "projectId") == Some(project_id))
                        .then(|| gpui_remote_project_path_from_gxserver_row(project))
                        .flatten()
                })
        }))
}

pub(crate) fn gpui_remote_project_path_from_gxserver_row(
    project: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let path = json_string_field(project, "path")?.trim();
    if path.is_empty()
        || path.chars().count() > GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS
        || path.contains('\0')
        || path.chars().any(char::is_control)
    {
        return None;
    }
    Some(path.to_string())
}

pub(crate) fn gpui_open_remote_existing_project_pull_request_in_browser(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUIRemoteNativeActions 2026-06-24-19:25:
    Remote PR browser opens must re-run `prView` through the saved-machine gxserver tunnel and open only a validated HTTPS GitHub pull-request URL. Renderer URLs, cached PR payloads, Browser titles, command text, SSH details, tokens, and daemon bodies are not launch authority.
    */
    let result = gpui_remote_gxserver_rpc_result(
        target,
        "/api/runGitHubAction",
        &serde_json::json!({
            "action": "prView",
            "projectId": project_id,
        }),
        Duration::from_secs(15),
    )?;
    if gpui_typed_operation_exit_code(&result) != Some(0) {
        return Err("No open remote pull request is available for this project.".to_string());
    }
    let url = gpui_trusted_github_pull_request_url_from_pr_view_stdout(
        gpui_typed_operation_stdout(&result),
    )
    .ok_or_else(|| "No open remote pull request is available for this project.".to_string())?;
    gpui_spawn_os_open(std::ffi::OsStr::new(&url))
        .map_err(|_| "GPUI could not open the remote pull request.".to_string())
}

pub(crate) fn gpui_open_remote_project_in_ide(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    action: GpuiSidebarNativeProjectPathAction,
    project_id: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUIRemoteNativeActions 2026-06-24-20:26:
    Remote project IDE opens are native-owned fixed editor launches. Resolve the remote path from the owning gxserver immediately before launch, derive SSH targeting from saved Settings, and support only reviewed VS Code/Insiders argv or Zed/Zeditor URI paths so custom Settings text, renderer paths, local Finder, and local filesystem paths never become remote-open authority.

    CDXC:GPUIRemoteNativeActions 2026-06-24-21:33:
    Posix-host Zed/Zeditor opens use Zed's documented SSH URI CLI form.
    Windows-host WSL opens instead execute the fixed editor CLI inside the
    retained distribution, matching each editor's documented WSL integration.
    The launcher still rejects custom command text, renderer URI strings, and
    local Finder paths.
    */
    let editor_target = match action {
        GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInVscode => {
            GPUI_WORKSPACE_EDITOR_VSCODE_TARGET
        }
        GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInZed => {
            GPUI_WORKSPACE_EDITOR_ZED_TARGET
        }
        GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInIde => {
            gpui_remote_workspace_editor_target_from_default_settings()?
        }
        _ => {
            return Err("Configured editor is not supported for GPUI remote IDE open.".to_string());
        }
    };
    let project_path = gpui_remote_gxserver_project_path_by_id(target, project_id)?;
    gpui_open_remote_path_in_editor(
        config,
        &target.execution_target,
        editor_target,
        project_path.as_str(),
        GpuiRemoteIdePathKind::Folder,
    )
}

pub(crate) fn gpui_open_remote_sidebar_git_changed_file_in_ide(
    config: &GpuiRemoteMachineConfig,
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
    file_path: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUIRemoteNativeActions 2026-06-24-19:25:
    Remote changed-file opens still revalidate the project-relative candidate against current remote gxserver Git state before any editor side effect. Never open a local path for a remote file and never accept renderer-controlled SSH, path, or URI fallback data.

    CDXC:GPUIRemoteNativeActions 2026-06-24-20:26:
    Remote changed-file opens now use the same Rust-owned fixed editor path as remote project opens after revalidating the relative file candidate against fresh remote Git state. Keep custom editors, local remote paths, renderer URI strings, and unreviewed editor protocols unsupported.
    */
    let relative_file_path = gpui_normalized_relative_git_file_path(file_path)
        .ok_or_else(|| "Choose a changed file from the current remote Git review.".to_string())?;
    let changed_files = gpui_remote_project_git_changed_file_paths(target, project_id)?;
    if !changed_files.contains(&relative_file_path) {
        return Err("Choose a changed file from the current remote Git state.".to_string());
    }
    let project_path = gpui_remote_gxserver_project_path_by_id(target, project_id)?;
    let remote_file_path =
        gpui_join_remote_project_relative_path(project_path.as_str(), relative_file_path.as_str())
            .ok_or_else(|| {
                "Choose a changed file from the current remote Git state.".to_string()
            })?;
    let editor_target = gpui_remote_workspace_editor_target_from_default_settings()?;
    gpui_open_remote_path_in_editor(
        config,
        &target.execution_target,
        editor_target,
        remote_file_path.as_str(),
        GpuiRemoteIdePathKind::File,
    )
}

pub(crate) fn gpui_remote_workspace_editor_target_from_default_settings()
-> Result<GpuiWorkspaceEditorTarget, String> {
    let settings = shared_settings::shared_sidebar_settings_snapshot().external_editor_settings();
    match settings.default_editor_command() {
        shared_settings::SharedDefaultEditorCommand::Code => {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        shared_settings::SharedDefaultEditorCommand::CodeInsiders => {
            Ok(GpuiWorkspaceEditorTarget {
                command: "code-insiders",
                app_names: GPUI_WORKSPACE_EDITOR_VSCODE_INSIDERS_APP_NAMES,
                launch_kind: GpuiWorkspaceEditorLaunchKind::VscodeCompatible,
            })
        }
        shared_settings::SharedDefaultEditorCommand::Zed => Ok(GPUI_WORKSPACE_EDITOR_ZED_TARGET),
        shared_settings::SharedDefaultEditorCommand::Zeditor => Ok(GpuiWorkspaceEditorTarget {
            command: "zeditor",
            app_names: GPUI_WORKSPACE_EDITOR_ZED_APP_NAMES,
            launch_kind: GpuiWorkspaceEditorLaunchKind::ZedCompatible,
        }),
        shared_settings::SharedDefaultEditorCommand::Other
            if settings.editor_command().trim()
                == shared_settings::DEFAULT_DEFAULT_EDITOR_COMMAND =>
        {
            Ok(GPUI_WORKSPACE_EDITOR_VSCODE_TARGET)
        }
        _ => Err("Configured editor is not supported for GPUI remote IDE open.".to_string()),
    }
}

pub(crate) fn gpui_open_remote_path_in_editor(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
    path_kind: GpuiRemoteIdePathKind,
) -> Result<(), String> {
    if !gpui_remote_ide_path_allowed(remote_path) {
        return Err("Remote IDE open could not resolve a valid remote path.".to_string());
    }
    if matches!(
        execution_target,
        GpuiRemoteExecutionTarget::WindowsWsl { .. }
    ) {
        return gpui_open_remote_path_in_windows_wsl_editor(
            config,
            execution_target,
            target,
            remote_path,
            path_kind,
        );
    }
    match target.launch_kind {
        GpuiWorkspaceEditorLaunchKind::VscodeCompatible => {
            gpui_open_remote_path_in_vscode_remote_ssh(config, target, remote_path)
        }
        GpuiWorkspaceEditorLaunchKind::ZedCompatible => {
            gpui_open_remote_path_in_zed_remote_ssh(config, target, remote_path)
        }
        GpuiWorkspaceEditorLaunchKind::DirectPath => {
            Err("Configured editor is not supported for GPUI remote IDE open.".to_string())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiRemoteIdePathKind {
    Folder,
    File,
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_open_remote_path_in_windows_wsl_editor(
    config: &GpuiRemoteMachineConfig,
    execution_target: &GpuiRemoteExecutionTarget,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
    path_kind: GpuiRemoteIdePathKind,
) -> Result<(), String> {
    let GpuiRemoteExecutionTarget::WindowsWsl { distribution } = execution_target else {
        return Err("Remote IDE open could not resolve the retained WSL target.".to_string());
    };
    let executable = match (target.launch_kind, target.command) {
        (GpuiWorkspaceEditorLaunchKind::VscodeCompatible, "code" | "code-insiders")
        | (GpuiWorkspaceEditorLaunchKind::ZedCompatible, "zed" | "zeditor") => target.command,
        _ => {
            return Err(
                "Configured editor is not supported for GPUI remote WSL IDE open.".to_string(),
            );
        }
    };
    let quoted_executable = gpui_shell_single_quote(executable);
    let launch_command = match target.launch_kind {
        GpuiWorkspaceEditorLaunchKind::VscodeCompatible => {
            /*
            VS Code's Windows CLI accepts a WSL remote URI. Include the pinned
            distro authority and an explicit folder/file switch so neither the
            Windows host nor filename-extension guessing can reinterpret the
            gxserver-owned Linux path.
            */
            let remote_uri = format!(
                "vscode-remote://wsl+{}{}",
                gpui_percent_encode_remote_ssh_path(distribution),
                gpui_percent_encode_remote_ssh_path(remote_path)
            );
            let uri_switch = match path_kind {
                GpuiRemoteIdePathKind::Folder => "--folder-uri",
                GpuiRemoteIdePathKind::File => "--file-uri",
            };
            format!(
                "{quoted_executable} --reuse-window {uri_switch} {}",
                gpui_shell_single_quote(remote_uri.as_str())
            )
        }
        GpuiWorkspaceEditorLaunchKind::ZedCompatible => {
            /*
            Zed's Windows CLI detects WSL from the invoking environment. Run it
            inside the retained distro rather than sending an SSH URI to the
            native Windows host, which Zed does not support as a remote server.
            */
            format!(
                "{quoted_executable} {}",
                gpui_shell_single_quote(remote_path)
            )
        }
        GpuiWorkspaceEditorLaunchKind::DirectPath => {
            return Err(
                "Configured editor is not supported for GPUI remote WSL IDE open.".to_string(),
            );
        }
    };
    let remote_command = format!(
        "command -v {quoted_executable} >/dev/null 2>&1 || exit 127; exec {launch_command}"
    );
    let result = gpui_run_remote_ssh_in_execution_target(
        config,
        execution_target,
        remote_command.as_str(),
        Duration::from_secs(15),
    );
    match result.exit_code {
        0 => Ok(()),
        127 => {
            Err("Configured editor is not available in the retained remote WSL target.".to_string())
        }
        _ => Err("Configured editor could not open the retained remote WSL target.".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn gpui_open_remote_path_in_windows_wsl_editor(
    _config: &GpuiRemoteMachineConfig,
    _execution_target: &GpuiRemoteExecutionTarget,
    _target: GpuiWorkspaceEditorTarget,
    _remote_path: &str,
    _path_kind: GpuiRemoteIdePathKind,
) -> Result<(), String> {
    Err("Remote IDE open is unavailable on this platform.".to_string())
}

pub(crate) fn gpui_open_remote_path_in_vscode_remote_ssh(
    config: &GpuiRemoteMachineConfig,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
) -> Result<(), String> {
    if !gpui_remote_ide_path_allowed(remote_path) {
        return Err("Remote IDE open could not resolve a valid remote path.".to_string());
    }
    let remote_authority = gpui_vscode_remote_ssh_authority(config)?;
    if !gpui_command_exists_on_path(target.command) {
        return Err("Configured editor is not available for GPUI remote IDE open.".to_string());
    }
    let mut command = std::process::Command::new("/usr/bin/env");
    command
        .arg(target.command)
        .arg("--reuse-window")
        .arg("--remote")
        .arg(remote_authority)
        .arg(remote_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open the remote target.".to_string())
}

pub(crate) fn gpui_open_remote_path_in_zed_remote_ssh(
    config: &GpuiRemoteMachineConfig,
    target: GpuiWorkspaceEditorTarget,
    remote_path: &str,
) -> Result<(), String> {
    if !gpui_remote_ide_path_allowed(remote_path) {
        return Err("Remote IDE open could not resolve a valid remote path.".to_string());
    }
    if config.ssh_identity_file.is_some() {
        return Err(
            "Remote IDE open requires a saved machine that the selected editor can address by host, user, and port."
                .to_string(),
        );
    }
    if !gpui_command_exists_on_path(target.command) {
        return Err("Configured editor is not available for GPUI remote IDE open.".to_string());
    }
    let remote_uri = gpui_zed_remote_ssh_uri(config, remote_path)?;
    let mut command = std::process::Command::new("/usr/bin/env");
    command
        .arg(target.command)
        .arg(remote_uri)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| "Configured editor could not open the remote target.".to_string())
}

pub(crate) fn gpui_zed_remote_ssh_uri(
    config: &GpuiRemoteMachineConfig,
    remote_path: &str,
) -> Result<String, String> {
    let host = gpui_vscode_remote_ssh_authority_part(config.ssh_host.as_str())
        .ok_or_else(|| "Remote IDE open could not resolve the saved machine host.".to_string())?;
    let user = config
        .ssh_user
        .as_deref()
        .and_then(gpui_vscode_remote_ssh_authority_part);
    let mut authority = match user {
        Some(user) => format!("{user}@{host}"),
        None => host,
    };
    if let Some(port) = config.ssh_port {
        authority.push(':');
        authority.push_str(&port.to_string());
    }
    Ok(format!(
        "ssh://{}{}",
        authority,
        gpui_percent_encode_remote_ssh_path(remote_path)
    ))
}

pub(crate) fn gpui_percent_encode_remote_ssh_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

pub(crate) fn gpui_vscode_remote_ssh_authority(config: &GpuiRemoteMachineConfig) -> Result<String, String> {
    if config.ssh_identity_file.is_some() || config.ssh_port.is_some() {
        return Err(
            "Remote IDE open requires a saved machine that VS Code Remote-SSH can address by host and user."
                .to_string(),
        );
    }
    let host = gpui_vscode_remote_ssh_authority_part(config.ssh_host.as_str())
        .ok_or_else(|| "Remote IDE open could not resolve the saved machine host.".to_string())?;
    let authority = config
        .ssh_user
        .as_deref()
        .and_then(gpui_vscode_remote_ssh_authority_part)
        .map(|user| format!("{user}@{host}"))
        .unwrap_or(host);
    Ok(format!("ssh-remote+{authority}"))
}

pub(crate) fn gpui_vscode_remote_ssh_authority_part(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return None;
    }
    Some(value.to_string())
}

pub(crate) fn gpui_remote_ide_path_allowed(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with('/')
        && value.chars().count() <= GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
}

pub(crate) fn gpui_join_remote_project_relative_path(
    project_path: &str,
    relative_file_path: &str,
) -> Option<String> {
    if !gpui_remote_ide_path_allowed(project_path)
        || gpui_normalized_relative_git_file_path(relative_file_path).as_deref()
            != Some(relative_file_path)
    {
        return None;
    }
    if project_path == "/" {
        return Some(format!("/{relative_file_path}"));
    }
    Some(format!(
        "{}/{}",
        project_path.trim_end_matches('/'),
        relative_file_path
    ))
}

pub(crate) fn gpui_remote_project_git_changed_file_paths(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
) -> Result<HashSet<String>, String> {
    let status = gpui_remote_gxserver_git_action_result(target, project_id, "statusPorcelain")?;
    if gpui_typed_operation_exit_code(&status) != Some(0) {
        return Err("GPUI could not refresh remote changed files.".to_string());
    }
    let mut files = HashSet::new();
    gpui_collect_git_status_porcelain_paths(gpui_typed_operation_stdout(&status), &mut files);

    let diff = gpui_remote_gxserver_git_action_result(target, project_id, "diffNumstat")?;
    if gpui_typed_operation_exit_code(&diff) == Some(0) {
        gpui_collect_git_numstat_paths(gpui_typed_operation_stdout(&diff), &mut files);
    }

    let untracked = gpui_remote_gxserver_git_action_result(target, project_id, "listUntracked")?;
    if gpui_typed_operation_exit_code(&untracked) == Some(0) {
        gpui_collect_git_zero_delimited_paths(gpui_typed_operation_stdout(&untracked), &mut files);
    }
    Ok(files)
}

pub(crate) fn gpui_remote_gxserver_git_action_result(
    target: &GpuiRemoteGxserverRequestTarget,
    project_id: &str,
    action: &str,
) -> Result<serde_json::Value, String> {
    gpui_remote_gxserver_rpc_result(
        target,
        "/api/runGitAction",
        &serde_json::json!({
            "action": action,
            "projectId": project_id,
        }),
        Duration::from_secs(15),
    )
}

