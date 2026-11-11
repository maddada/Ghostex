use serde::Serialize;
use serde_json::{json, Value};

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
    /// The pairing door's address: named so a phone knows it as this computer's, not to be
    /// dialled (`easy_connect_addresses_with_doors`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub door: bool,
}

/*
CDXC:RemotePairing 2026-11-06 DECISION:
User (approved plan): a paired phone asks `/api/easyConnectAddresses` with its device token and gets both addresses, ours first, then dials them in that order; `/api/pairDevice` answers with the same list so a phone paired from the QR (which carries one address) has the fallback at once. The second address is listed while its helper runs, even when its relay missed the last check, because the phone checks the relay itself before dialing it; the public one is always last.
SEE-ALSO: apps/mobile/app/src/machines/easyConnectAddresses.ts (the phone's dial order), server/src/tailcat/supervisor.rs.
*/
/*
CDXC:RemotePairing 2026-11-10 WHY (the last place changed on 2026-11-11, see `easy_connect_addresses_with_doors`):
This list never names the door a new phone pairs through among the addresses to dial (tailcat/door.rs): the door stops when the pairing window closes, and a phone dials its saved list in order and waits 20 s on an address that no longer answers before it tries the next. Both helpers below admit a device from the moment it is approved, so they are all it needs; the phone keeps the door's address only as the one it scanned, which it tries last.
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
            door: false,
        });
    }
    if let (true, Some(token)) = (status.running, status.token.as_ref()) {
        addresses.push(EasyConnectAddress {
            address: token.clone(),
            relay: PUBLIC_RELAY_LABEL.to_string(),
            kind: "public",
            reachable: None,
            door: false,
        });
    }
    addresses
}

/*
CDXC:RemotePairing 2026-11-11 WHY:
The list above with the pairing doors' addresses named last (tailcat/door.rs says why: a phone that scans a new code must be able to tell that the door belongs to a computer it already has). Last and flagged `door: true`, because the door answers only while a code is shown: a phone on the old app dials its list in order and so reaches it only after both real addresses failed, which is where the address it scanned already sits for a phone paired since 11.3.0, and the new phone code keeps a flagged address for recognising the computer and never dials it. A list with no real address stays empty.
*/
pub fn easy_connect_addresses_with_doors(
    status: &TailcatStatusPayload,
    doors: Vec<crate::tailcat::DoorAddress>,
) -> Vec<EasyConnectAddress> {
    let mut addresses = easy_connect_addresses(status);
    if addresses.is_empty() {
        return addresses;
    }
    // Every door a code can carry (both since finding 3 of the review, tailcat/door.rs
    // `remember_door_address`), each flagged, so whichever one a code names is on the list.
    for door in doors {
        if addresses.iter().any(|entry| entry.address == door.address) {
            continue;
        }
        let (relay, kind) = match door.relay_host {
            Some(host) => (
                host,
                match status.relay.mode {
                    TailcatRelayMode::Custom => "custom",
                    _ => "ghostex",
                },
            ),
            None => (PUBLIC_RELAY_LABEL.to_string(), "public"),
        };
        addresses.push(EasyConnectAddress {
            address: door.address,
            relay,
            kind,
            reachable: None,
            door: true,
        });
    }
    addresses
}

/*
CDXC:RemotePairing 2026-11-06 DECISION:
User (approved plan, "New pairings"): the pairing QR keeps carrying one address, ours, so it gets no denser; the phone learns the public one from `/api/pairDevice`. Ours goes in only while its relay answered the last check: a QR naming a relay that is down could not pair at all. Copy as text keeps the public address, because computers (connection profiles, `rpc.rs`) and Cloud Boxes stay on it, as the plan allows, and a phone that pastes it learns ours once connected.

CDXC:RemotePairing 2026-11-10 WHY: while a code is shown the QR's one address is the door's (tailcat/door.rs), which sits on our relay under the same rule (only while it answered the last check); `/api/remotePairingCode` falls back to the address below only when the door's helper cannot run.
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

/// The longest error text a phone's dial report keeps.
const DIAL_ERROR_MAX_CHARS: usize = 400;

/// One failed Easy Connect dial a phone reports with its next `/api/easyConnectAddresses` request
/// (`params.failedDial`), as the `easyConnect.dialFailed` record keeps it: the record's details and
/// its error text. `None` when the request carries no report, or one this computer cannot read.
///
/// CDXC:RemotePairing 2026-11-08 WHY:
/// The link log (`server/src/tailcat/link_log.rs`) shows only what reached this computer: on
/// 2026-11-07 it showed the phone's hello arrive on the relay lane and, half a second later, the
/// phone on the public lane, with nothing to say why the phone gave up. The phone's reason was only
/// in its own device log. The phone now sends the reason of the dial it gave up on with the address
/// list request it makes right after connecting another way, so the next fallback names its cause
/// here. The report never carries an address: any word that looks like one is replaced.
/// SEE-ALSO: apps/mobile/app/src/machines/easyConnectDial.ts (`noteEasyConnectDial`).
pub fn failed_dial_report(params: &Value) -> Option<(Value, String)> {
    let report = params.get("failedDial")?.as_object()?;
    let kind = match report.get("kind")?.as_str()? {
        kind @ ("ghostex" | "custom" | "public" | "paired") => kind,
        _ => return None,
    };
    let lane = if matches!(kind, "ghostex" | "custom") {
        "relay"
    } else {
        "public"
    };
    let relay: String = report
        .get("relay")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(100)
        .collect();
    let elapsed_ms = report.get("elapsedMs").and_then(Value::as_u64);
    let seconds_ago = report
        .get("secondsAgo")
        .and_then(Value::as_u64)
        .map(|seconds| seconds.min(24 * 60 * 60));
    let error: String = redact_addresses(
        report
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .chars()
    .take(DIAL_ERROR_MAX_CHARS)
    .collect();
    let mut details = json!({
        "lane": lane,
        "kind": kind,
        "relay": redact_addresses(&relay),
    });
    if let Some(elapsed_ms) = elapsed_ms {
        details["elapsedMs"] = json!(elapsed_ms);
    }
    if let Some(seconds_ago) = seconds_ago {
        details["secondsAgo"] = json!(seconds_ago);
    }
    Some((details, error))
}

/// Replaces every word that looks like a tailcat address (`tc` and a long base64url run) with
/// `[address]`: an address is a secret and never belongs in a log.
fn redact_addresses(text: &str) -> String {
    let is_address_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.len() >= 24 && word.starts_with("tc") {
            out.push_str("[address]");
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if is_address_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
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
