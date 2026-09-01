use serde::{Deserialize, Serialize};

/*
CDXC:Tailcat 2026-09-01:
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
        // PC-to-PC connection profiles (the tailcat transport in rpc.rs).
        ports: vec![22, crate::ghostex_cli::rpc::GXSERVER_LOCAL_API_PORT],
        allowed_client_keys: Vec::new(),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TailcatStateUpdate {
    SetEnabled { enabled: bool },
    SetPorts { ports: Vec<u16> },
    SetAllowedClientKeys { allowed_client_keys: Vec<String> },
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
