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

/// Overlay copy for a remote machine's latest connect wire state: the sidebar's own copy
/// (`ghostex_gx_core::machine_state_copy`), so the terminal area and the sidebar never disagree
/// about one machine.
pub(crate) fn gpui_remote_connect_overlay_labels(
    state: Option<&str>,
) -> (&'static str, Option<&'static str>) {
    match state {
        // `disconnected` and the pre-first-status cold-start window describe the same situation
        // as `connecting` here: the machine is not reachable yet and a connect attempt is
        // expected.
        None | Some("disconnected") => ghostex_gx_core::machine_state_copy("connecting"),
        Some(state) => ghostex_gx_core::machine_state_copy(state),
    }
}

/*
CDXC:RemotePairing 2026-09-03:
How this computer reaches a saved machine. `Ssh` dials `ssh_host` directly.
`EasyConnect` has no reachable host at all: the machine is addressed by the
Easy Connect blob from its pairing code, and SSH dials the loopback forwarder
that `easy_connect_forward.rs` runs for it. For such a machine `ssh_host` /
`ssh_port` are the forwarder's `127.0.0.1:<port>` while it is running and
empty / `None` otherwise. An empty host is NOT harmless to ssh (it resolves
to the local sshd), so `gpui_remote_ssh_dial_refusal` in `ssh_exec.rs`
refuses it before any argument list is built.
*/
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuiRemoteMachineTransport {
    Ssh,
    EasyConnect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GpuiRemoteMachineConfig {
    pub(crate) remote_machine_id: String,
    pub(crate) transport: GpuiRemoteMachineTransport,
    /// The Easy Connect address blob (`tc…`); present only for `EasyConnect`.
    pub(crate) easy_connect_address: Option<String>,
    /// The SSH port the forwarder dials on the far side (from the pairing code, 22 by default).
    pub(crate) easy_connect_ssh_port: u16,
    pub(crate) ssh_host: String,
    pub(crate) ssh_identity_file: Option<String>,
    pub(crate) ssh_port: Option<u16>,
    pub(crate) ssh_user: Option<String>,
    pub(crate) has_saved_password: bool,
    pub(crate) wsl_distribution: Option<String>,
    pub(crate) disabled: bool,
    /// The connect's shared SSH login (`GpuiRemoteSshConnectionShare`); set only on the copy one connect runs with.
    pub(crate) ssh_control_path: Option<String>,
}

impl GpuiRemoteMachineConfig {
    pub(crate) fn uses_easy_connect(&self) -> bool {
        self.transport == GpuiRemoteMachineTransport::EasyConnect
    }

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
    WindowsPowerShell,
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

impl GpuiRemoteAttachSessionKey {
    /// The store's key for this session, on its own machine, which an attention acknowledgement
    /// for it is sent to (gx_store/attention/host.rs).
    pub(crate) fn attention_session_key(&self) -> ghostex_gx_core::SessionKey {
        ghostex_gx_core::SessionKey::remote(
            self.remote_machine_id.as_str(),
            self.project_id.as_str(),
            self.session_id.as_str(),
        )
    }
}

#[cfg(test)]
mod attention_session_key_tests {
    use super::GpuiRemoteAttachSessionKey;

    #[test]
    fn a_remote_pane_acknowledges_its_session_on_its_own_machine() {
        let key = GpuiRemoteAttachSessionKey {
            remote_machine_id: "remote-mac".to_string(),
            project_id: "P3lv0".to_string(),
            session_id: "G1pif".to_string(),
        };
        let session = key.attention_session_key();
        assert_eq!(session.machine.remote_id(), Some("remote-mac"));
        assert_eq!(session.project_id, "P3lv0");
        assert_eq!(session.session_id, "G1pif");
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
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub(crate) askpass: Option<GpuiRemoteAskpassScript>,
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
    /// SSH reached the machine and it refused the saved username, password or key. Retrying
    /// cannot fix that, and every retry is another failed login the machine may lock the account
    /// over, so the reconnect ladder stops here until the user reconnects or saves a new password.
    AuthFailed,
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
            | Self::AuthFailed
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
            Self::AuthFailed => "Remote sign-in failed",
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
            Self::AuthFailed => "authFailed",
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
            Self::AuthFailed => "authFailed",
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
        "authFailed" => Some(GpuiRemoteGxserverConnectState::AuthFailed),
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
        "authFailed"
            | "disconnected"
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
    #[cfg(unix)]
    pub(crate) cancel: Arc<AtomicBool>,
    #[cfg(unix)]
    pub(crate) directory: PathBuf,
    #[cfg(unix)]
    pub(crate) password_server: Option<thread::JoinHandle<()>>,
    pub(crate) script: PathBuf,
    #[cfg(windows)]
    pub(crate) remote_machine_id: String,
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) struct GpuiRemoteSpawnedTunnel {
    pub(crate) child: Child,
    pub(crate) _askpass: Option<GpuiRemoteAskpassScript>,
}

#[cfg(unix)]
impl Drop for GpuiRemoteAskpassScript {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(password_server) = self.password_server.take() {
            let _ = password_server.join();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}
