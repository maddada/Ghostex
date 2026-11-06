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
    pub relay: TailcatRelaySetting,
}

/// The relay Ghostex runs for Easy Connect, the default second address's relay.
pub const GHOSTEX_EASY_CONNECT_RELAY_HOST: &str = "relay-eu.ghostex.dev";

/// Which relay Easy Connect's second address uses.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TailcatRelayMode {
    /// Ghostex's own relay ([`GHOSTEX_EASY_CONNECT_RELAY_HOST`]), with the public relays behind it.
    #[default]
    Ghostex,
    /// Only Tailscale's public relays: no second address.
    Public,
    /// A relay the user runs, named by `custom_host`, with the public relays behind it.
    Custom,
}

impl TailcatRelayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ghostex => "ghostex",
            Self::Public => "public",
            Self::Custom => "custom",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "public" => Self::Public,
            "custom" => Self::Custom,
            _ => Self::Ghostex,
        }
    }
}

/*
CDXC:RemotePairing 2026-11-06 DECISION:
User (Q1490 B, Q1492): Easy Connect gets one relay Ghostex runs, in the EU, with Tailscale's public relays kept as the fallback; the plan's Settings choice is "Ghostex relay (default) or public relays only, plus an optional custom hostname". The relay is part of an Easy Connect address, so the choice never touches the first address (the one on the public relays that every phone paired so far holds): it only adds, replaces or drops the second one.
SEE-ALSO: server/src/tailcat/supervisor.rs (the second helper), server/src/remote_access/easy_connect_addresses.rs (what phones are told), apps/mobile/app/src/machines/easyConnectAddresses.ts (the phone's dial order).
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TailcatRelaySetting {
    pub mode: TailcatRelayMode,
    /// The user's relay hostname; kept while another mode is picked so switching back restores it.
    pub custom_host: String,
}

impl TailcatRelaySetting {
    /// The hostname the second address uses, or `None` when Easy Connect has only the public one.
    pub fn relay_host(&self) -> Option<String> {
        match self.mode {
            TailcatRelayMode::Ghostex => Some(GHOSTEX_EASY_CONNECT_RELAY_HOST.to_string()),
            TailcatRelayMode::Public => None,
            TailcatRelayMode::Custom => normalize_tailcat_relay_host(&self.custom_host),
        }
    }
}

/// A relay hostname in the form tailcat embeds in an address: lower-case DNS labels with at least
/// one dot, so it doubles as a file name. `None` for anything else, including an empty field.
pub fn normalize_tailcat_relay_host(value: &str) -> Option<String> {
    let host = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.len() > 253 || !host.contains('.') {
        return None;
    }
    let labels_ok = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    labels_ok.then_some(host)
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
        relay: TailcatRelaySetting::default(),
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
    /// Settings > Easy Connect's relay choice (`TailcatRelaySetting`).
    SetRelay {
        mode: TailcatRelayMode,
        #[serde(default, rename = "customHost", alias = "custom_host")]
        custom_host: String,
    },
}

impl TailcatStateUpdate {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::SetEnabled { .. } => "setEnabled",
            Self::SetPorts { .. } => "setPorts",
            Self::SetAllowedClientKeys { .. } => "setAllowedClientKeys",
            Self::SetRelay { .. } => "setRelay",
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
    /// The second address, on the relay Settings picked; `token` above stays the public one.
    #[serde(default)]
    pub relay: TailcatRelayStatus,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailcatRelayStatus {
    pub mode: TailcatRelayMode,
    pub custom_host: String,
    /// The relay the second address uses; `None` with public relays only (or a custom mode
    /// without a valid hostname).
    pub host: Option<String>,
    pub running: bool,
    pub token: Option<String>,
    /// Whether the relay answered its last check (`/generate_204`); `None` before the first one.
    pub reachable: Option<bool>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TailcatLaneSnapshot {
    pub(crate) running: bool,
    pub(crate) token: Option<String>,
    pub(crate) last_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TailcatRuntimeSnapshot {
    pub(crate) running: bool,
    pub(crate) token: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) relay: TailcatLaneSnapshot,
    pub(crate) relay_host: Option<String>,
    pub(crate) relay_reachable: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_hostname_is_normalized_or_refused() {
        assert_eq!(
            normalize_tailcat_relay_host(" Relay.Example.com. ").as_deref(),
            Some("relay.example.com")
        );
        for refused in [
            "",
            "localhost",
            "relay..example.com",
            "-a.example.com",
            "a b.com",
            "../x.y",
            "relay.example.com:443",
        ] {
            assert_eq!(normalize_tailcat_relay_host(refused), None, "{refused}");
        }
    }

    #[test]
    fn each_mode_names_its_relay() {
        let mut setting = TailcatRelaySetting::default();
        assert_eq!(
            setting.relay_host().as_deref(),
            Some(GHOSTEX_EASY_CONNECT_RELAY_HOST)
        );
        setting.mode = TailcatRelayMode::Public;
        assert_eq!(setting.relay_host(), None);
        setting.mode = TailcatRelayMode::Custom;
        assert_eq!(setting.relay_host(), None);
        setting.custom_host = "relay.example.com".to_string();
        assert_eq!(setting.relay_host().as_deref(), Some("relay.example.com"));
        assert_eq!(TailcatRelayMode::parse("bogus"), TailcatRelayMode::Ghostex);
    }

    #[test]
    fn the_relay_update_reads_its_wire_names() {
        let update: TailcatStateUpdate = serde_json::from_value(serde_json::json!({
            "kind": "setRelay",
            "mode": "custom",
            "customHost": "relay.example.com"
        }))
        .expect("setRelay");
        assert_eq!(
            update,
            TailcatStateUpdate::SetRelay {
                mode: TailcatRelayMode::Custom,
                custom_host: "relay.example.com".to_string()
            }
        );
    }
}
