// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the remote-connect status overlay and
// the core remote type/config definitions. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    fs,
    path::PathBuf,
    process::Child,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use gpui::{AnyElement, FontWeight, div, px, rgb};
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
pub(crate) fn gpui_remote_connect_overlay_labels(
    state: Option<&str>,
) -> (&'static str, Option<&'static str>) {
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
