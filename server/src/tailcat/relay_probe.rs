use std::{fs, path::Path, time::Duration};

use serde::Deserialize;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The relay node a server key is pinned to, as the key file names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyRelayNode {
    pub(crate) host: String,
    pub(crate) derp_port: Option<u16>,
    pub(crate) insecure_for_tests: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KeyFile {
    public: KeyPublic,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KeyPublic {
    #[serde(default, rename = "RegionID")]
    region_id: Option<i64>,
    #[serde(default)]
    region: Option<Vec<KeyRegion>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KeyRegion {
    #[serde(default)]
    nodes: Vec<KeyNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KeyNode {
    #[serde(default)]
    host_name: String,
    #[serde(default, rename = "DERPPort")]
    derp_port: Option<u16>,
    #[serde(default)]
    insecure_for_tests: bool,
}

/// The first relay node embedded in a server key (`genkey --region=<host>`), or `None` for a key
/// pinned to a public region by number.
pub(crate) fn read_key_relay_node(key_file: &Path) -> Option<KeyRelayNode> {
    let text = fs::read_to_string(key_file).ok()?;
    parse_key_relay_node(&text)
}

fn parse_key_relay_node(text: &str) -> Option<KeyRelayNode> {
    let key: KeyFile = serde_json::from_str(text).ok()?;
    let node = key
        .public
        .region?
        .into_iter()
        .flat_map(|region| region.nodes)
        .find(|node| !node.host_name.is_empty())?;
    Some(KeyRelayNode {
        host: node.host_name,
        derp_port: node.derp_port.filter(|port| *port != 0),
        insecure_for_tests: node.insecure_for_tests,
    })
}

/// How the log names the relay a key uses: its hostname, or the public region's number.
pub(crate) fn key_relay_label(key_file: &Path) -> String {
    if let Some(node) = read_key_relay_node(key_file) {
        return node.host;
    }
    let region = fs::read_to_string(key_file)
        .ok()
        .and_then(|text| serde_json::from_str::<KeyFile>(&text).ok())
        .and_then(|key| key.public.region_id)
        .filter(|id| *id > 0);
    match region {
        Some(id) => format!("tailcat.dev region {id}"),
        None => "tailcat.dev".to_string(),
    }
}

/// The URL derper answers with an empty 204 (`/generate_204`), the check the relay's own uptime
/// monitor uses too.
pub(crate) fn relay_probe_url(node: &KeyRelayNode) -> String {
    match node.derp_port {
        Some(port) if port != 443 => format!("https://{}:{port}/generate_204", node.host),
        _ => format!("https://{}/generate_204", node.host),
    }
}

/*
CDXC:RemotePairing 2026-11-06 WHY:
The second helper keeps running while its relay is down (tailcat retries the relay by itself), so "is it running" says nothing about whether a phone could reach this computer through it. The QR carries the second address only while the relay answers this check; otherwise it carries the public one, which every phone can still use, and phones that already hold both skip the relay that does not answer.
*/
pub(crate) fn probe_relay(node: &KeyRelayNode) -> bool {
    let mut config = crate::http_agent::config()
        .timeout_global(Some(PROBE_TIMEOUT))
        .http_status_as_error(false);
    if node.insecure_for_tests {
        config = config.tls_config(
            ureq::tls::TlsConfig::builder()
                .disable_verification(true)
                .build(),
        );
    }
    let agent: ureq::Agent = config.build().into();
    match agent.get(&relay_probe_url(node)).call() {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_key_names_its_node_and_a_public_key_its_region() {
        let relay = r#"{"Private":"privkey:00","Public":{"ServerPublic":"nodekey:00","Region":[{"RegionID":0,"Nodes":[{"Name":"","RegionID":0,"HostName":"relay-eu.ghostex.dev"}]}]}}"#;
        assert_eq!(
            parse_key_relay_node(relay),
            Some(KeyRelayNode {
                host: "relay-eu.ghostex.dev".to_string(),
                derp_port: None,
                insecure_for_tests: false,
            })
        );
        let public =
            r#"{"Private":"privkey:00","Public":{"ServerPublic":"nodekey:00","RegionID":902}}"#;
        assert_eq!(parse_key_relay_node(public), None);
    }

    #[test]
    fn the_probe_url_keeps_a_non_default_port() {
        let mut node = KeyRelayNode {
            host: "relay-eu.ghostex.dev".to_string(),
            derp_port: None,
            insecure_for_tests: false,
        };
        assert_eq!(
            relay_probe_url(&node),
            "https://relay-eu.ghostex.dev/generate_204"
        );
        node.derp_port = Some(8443);
        assert_eq!(
            relay_probe_url(&node),
            "https://relay-eu.ghostex.dev:8443/generate_204"
        );
    }
}
