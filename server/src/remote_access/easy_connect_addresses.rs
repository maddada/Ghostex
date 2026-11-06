use serde::Serialize;

use crate::tailcat::{TailcatRelayMode, TailcatStatusPayload};

/// How the log and the phone name the public relays' address.
pub const PUBLIC_RELAY_LABEL: &str = "tailcat.dev";

/// One address a paired phone can dial this computer at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyConnectAddress {
    pub address: String,
    /// The relay the address meets on: a hostname, or [`PUBLIC_RELAY_LABEL`].
    pub relay: String,
    /// `ghostex` (Ghostex's relay), `custom` (the user's) or `public`.
    pub kind: &'static str,
    /// Whether this computer reached the relay at its last check; absent for the public relays,
    /// which it does not check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachable: Option<bool>,
}

/*
CDXC:RemotePairing 2026-11-06 DECISION:
User (approved plan): a paired phone asks `/api/easyConnectAddresses` with its device token and gets both addresses, ours first, then dials them in that order; `/api/pairDevice` answers with the same list so a phone paired from the QR (which carries one address) has the fallback at once. The second address is listed while its helper runs, even when its relay missed the last check, because the phone checks the relay itself before dialing it; the public one is always last.
SEE-ALSO: apps/mobile/app/src/machines/easyConnectAddresses.ts (the phone's dial order), server/src/tailcat/supervisor.rs.
*/
pub fn easy_connect_addresses(status: &TailcatStatusPayload) -> Vec<EasyConnectAddress> {
    let mut addresses = Vec::new();
    if let (true, Some(token), Some(host)) = (
        status.relay.running,
        status.relay.token.as_ref(),
        status.relay.host.as_ref(),
    ) {
        addresses.push(EasyConnectAddress {
            address: token.clone(),
            relay: host.clone(),
            kind: match status.relay.mode {
                TailcatRelayMode::Custom => "custom",
                _ => "ghostex",
            },
            reachable: status.relay.reachable,
        });
    }
    if let (true, Some(token)) = (status.running, status.token.as_ref()) {
        addresses.push(EasyConnectAddress {
            address: token.clone(),
            relay: PUBLIC_RELAY_LABEL.to_string(),
            kind: "public",
            reachable: None,
        });
    }
    addresses
}

/*
CDXC:RemotePairing 2026-11-06 DECISION:
User (approved plan, "New pairings"): the pairing QR keeps carrying one address, ours, so it gets no denser; the phone learns the public one from `/api/pairDevice`. Ours goes in only while its relay answered the last check: a QR naming a relay that is down could not pair at all. Copy as text keeps the public address, because computers (connection profiles, `rpc.rs`) and Cloud Boxes stay on it, as the plan allows, and a phone that pastes it learns ours once connected.
*/
pub fn pairing_qr_address(status: &TailcatStatusPayload) -> Option<String> {
    let relay_ready = status.relay.running && status.relay.reachable == Some(true);
    if relay_ready {
        if let Some(token) = status.relay.token.clone() {
            return Some(token);
        }
    }
    public_pairing_address(status)
}

/// The first address, which Copy as text and every computer use.
pub fn public_pairing_address(status: &TailcatStatusPayload) -> Option<String> {
    status.token.clone().filter(|_| status.running)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tailcat::TailcatRelayStatus;

    fn status(relay_running: bool, reachable: Option<bool>) -> TailcatStatusPayload {
        TailcatStatusPayload {
            installing: false,
            install_progress: None,
            install_error: None,
            enabled: true,
            running: true,
            binary_found: true,
            binary_path: None,
            binary_version: None,
            token: Some("tcPUBLIC".to_string()),
            ports: vec![22, 58744],
            allowed_client_keys: Vec::new(),
            last_error: None,
            relay: TailcatRelayStatus {
                mode: TailcatRelayMode::Ghostex,
                custom_host: String::new(),
                host: Some("relay-eu.ghostex.dev".to_string()),
                running: relay_running,
                token: relay_running.then(|| "tcOURS".to_string()),
                reachable,
                last_error: None,
            },
        }
    }

    #[test]
    fn ours_is_listed_first_and_the_public_address_last() {
        let listed = easy_connect_addresses(&status(true, Some(false)));
        assert_eq!(
            listed
                .iter()
                .map(|entry| (entry.address.as_str(), entry.kind, entry.relay.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("tcOURS", "ghostex", "relay-eu.ghostex.dev"),
                ("tcPUBLIC", "public", PUBLIC_RELAY_LABEL)
            ]
        );
        assert_eq!(listed[0].reachable, Some(false));
    }

    #[test]
    fn without_the_second_helper_only_the_public_address_is_listed() {
        let listed = easy_connect_addresses(&status(false, None));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].address, "tcPUBLIC");
        let mut stopped = status(false, None);
        stopped.running = false;
        assert!(easy_connect_addresses(&stopped).is_empty());
    }

    #[test]
    fn a_custom_relay_is_named_as_such() {
        let mut custom = status(true, Some(true));
        custom.relay.mode = TailcatRelayMode::Custom;
        custom.relay.host = Some("relay.example.com".to_string());
        let listed = easy_connect_addresses(&custom);
        assert_eq!(listed[0].kind, "custom");
        assert_eq!(listed[0].relay, "relay.example.com");
    }

    #[test]
    fn the_qr_carries_ours_only_while_its_relay_answers() {
        assert_eq!(
            pairing_qr_address(&status(true, Some(true))).as_deref(),
            Some("tcOURS")
        );
        assert_eq!(
            pairing_qr_address(&status(true, Some(false))).as_deref(),
            Some("tcPUBLIC")
        );
        assert_eq!(
            pairing_qr_address(&status(true, None)).as_deref(),
            Some("tcPUBLIC")
        );
        assert_eq!(
            pairing_qr_address(&status(false, None)).as_deref(),
            Some("tcPUBLIC")
        );
        assert_eq!(
            public_pairing_address(&status(true, Some(true))).as_deref(),
            Some("tcPUBLIC")
        );
    }
}
