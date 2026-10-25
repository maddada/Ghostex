use serde::{Deserialize, Serialize};

/*
CDXC:RemotePairing 2026-09-01:
tailcat is a control-plane-free remote-access sidecar: gxserver owns the
persistent server key and supervises `tailcat serve`, and the address blob
("token") is DERIVED from that key file at runtime. Persist only the user's
intent — enabled, the served ports, and the allow-list — so a restored or
copied state database can never resurrect a stale address for a key that no
longer exists.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailcatState {
    pub enabled: bool,
    pub ports: Vec<u16>,
    pub allowed_client_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TailcatStateRecord {
    pub state: TailcatState,
    pub updated_at: String,
}

pub fn default_tailcat_state() -> TailcatState {
    TailcatState {
        enabled: false,
        // 22 carries the phone's SSH sessions; the gxserver API port carries
        // Easy Connect pairing and PC-to-PC connection profiles (the tailcat
        // transport in rpc.rs).
        ports: vec![22, tailcat_gxserver_api_port()],
        allowed_client_keys: Vec::new(),
    }
}

/// The port this daemon's local API listens on, which the Easy Connect code advertises.
pub fn tailcat_gxserver_api_port() -> u16 {
    crate::config::read_selected_local_api_port()
        .unwrap_or(crate::constants::GXSERVER_LOCAL_API_PORT)
}

/// CDXC:RemotePairing 2026-10-02 WHY: Native Windows gxserver listens on 58746, not 58744, and both the Easy Connect code and the stored port list hardcoded 58744, so the phone's tunnel reached a port nothing listened on and pairing failed with "unexpected end of stream". The API port is always served because the pairing code always points at it; a stored list written with the wrong port, or a user edit that drops it, must not break pairing.
pub fn tailcat_served_ports(state: &TailcatState) -> Vec<u16> {
    let mut ports = state.ports.clone();
    ports.push(tailcat_gxserver_api_port());
    super::repository::normalize_tailcat_ports(ports)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TailcatStateUpdate {
    SetEnabled {
        enabled: bool,
    },
    SetPorts {
        ports: Vec<u16>,
    },
    /// `rename_all` names only the variants, so the field needs its wire name spelled out: the
    /// desktop's allow-list edits sent `allowedClientKeys` and were all refused as missing the
    /// field until 2026-10-25.
    SetAllowedClientKeys {
        #[serde(rename = "allowedClientKeys", alias = "allowed_client_keys")]
        allowed_client_keys: Vec<String>,
    },
}

impl TailcatStateUpdate {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::SetEnabled { .. } => "setEnabled",
            Self::SetPorts { .. } => "setPorts",
            Self::SetAllowedClientKeys { .. } => "setAllowedClientKeys",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailcatStatusPayload {
    #[serde(default)]
    pub installing: bool,
    #[serde(default)]
    pub install_progress: Option<String>,
    #[serde(default)]
    pub install_error: Option<String>,
    pub enabled: bool,
    pub running: bool,
    pub binary_found: bool,
    pub binary_path: Option<String>,
    pub binary_version: Option<String>,
    pub token: Option<String>,
    pub ports: Vec<u16>,
    pub allowed_client_keys: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TailcatRuntimeSnapshot {
    pub(crate) running: bool,
    pub(crate) token: Option<String>,
    pub(crate) last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailcatLogErrorCode {
    StateUpdateDatabaseUnavailable,
    StateUpdateFailed,
}

impl TailcatLogErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::StateUpdateDatabaseUnavailable => "stateUpdateDatabaseUnavailable",
            Self::StateUpdateFailed => "stateUpdateFailed",
        }
    }
}
