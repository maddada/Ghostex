use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read},
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};

/// A client that resends its hello within this window is the same link, not a new one.
const REPEATED_HELLO_WINDOW: Duration = Duration::from_secs(60);

/// What one line of a `tailcat --verbose serve` log says about an Easy Connect link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkEvent {
    /// A client said hello (tailcat's "meow"): a new link, which starts on the relay.
    Started { peer: String },
    /// The allow-list turned the client away (its hello was recorded as `Started` just before).
    Refused { peer: String },
    /// The link left the relay for a direct path; `path` is `lan`, `internet` or `peerRelay`.
    Direct { peer: String, path: &'static str },
}

impl LinkEvent {
    fn phase(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::Refused { .. } => "refused",
            Self::Direct { .. } => "direct",
        }
    }

    fn peer(&self) -> &str {
        match self {
            Self::Started { peer } | Self::Refused { peer } | Self::Direct { peer, .. } => peer,
        }
    }
}

/*
CDXC:RemotePairing 2026-11-06 DECISION:
User (Q1491, "build and measure together"): gxserver records whether each Easy Connect link goes direct or stays on a relay, and which relay, so the share of relayed links can be counted and tell whether one region is enough. tailcat prints both moments only with `--verbose`: "got meow from <client key>" when a client registers (every link starts on the relay) and magicsock's "now using <ip:port>" when a direct path wins. Each becomes one `easyConnect.link` record (phase `started`, `refused` or `direct`); a started link with no `direct` record stayed relayed. Records keep the peer's short key and whether the direct route is on the LAN or the internet (`route`), never an IP address.
SEE-ALSO: apps/mobile/tailcat-bridge/bridge.go (the phone's side of the same count).
*/
#[derive(Default)]
pub(crate) struct LinkTracker {
    started: HashMap<String, Instant>,
    refused: HashMap<String, Instant>,
    direct: HashSet<String>,
}

impl LinkTracker {
    pub(crate) fn observe(&mut self, line: &str, now: Instant) -> Option<LinkEvent> {
        if let Some(rest) = line.split("ignoring meow from ").nth(1) {
            let peer = short_peer(first_word(rest))?;
            if recently(&self.refused, &peer, now) {
                return None;
            }
            self.refused.insert(peer.clone(), now);
            return Some(LinkEvent::Refused { peer });
        }
        if let Some(rest) = line.split("got meow from ").nth(1) {
            let peer = short_peer(first_word(rest))?;
            if recently(&self.started, &peer, now) {
                return None;
            }
            self.started.insert(peer.clone(), now);
            self.direct.remove(&peer);
            return Some(LinkEvent::Started { peer });
        }
        if line.contains("magicsock: disco: node ") && line.contains(" now using ") {
            let peer = first_word(line.split("magicsock: disco: node ").nth(1)?).to_string();
            if !peer.starts_with('[') {
                return None;
            }
            let endpoint = first_word(line.split(" now using ").nth(1)?);
            if !self.direct.insert(peer.clone()) {
                return None;
            }
            return Some(LinkEvent::Direct {
                peer,
                path: classify_path(endpoint),
            });
        }
        None
    }
}

fn recently(seen: &HashMap<String, Instant>, peer: &str, now: Instant) -> bool {
    seen.get(peer)
        .is_some_and(|at| now.saturating_duration_since(*at) < REPEATED_HELLO_WINDOW)
}

fn first_word(text: &str) -> &str {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .next()
        .unwrap_or_default()
        .trim_end_matches(':')
}

/// magicsock's short form of a node key (`key.NodePublic.ShortString`): `[` and the first five
/// characters of the key's standard base64, `]`, so a hello and a later path change name the same
/// peer. `nodekey:<hex>` in, `None` for anything else.
fn short_peer(word: &str) -> Option<String> {
    let hex = word.strip_prefix("nodekey:")?;
    if hex.len() < 8 {
        return None;
    }
    let mut first = [0u8; 4];
    for (index, byte) in first.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    let encoded = STANDARD.encode(first);
    Some(format!("[{}]", &encoded[..5]))
}

/// `lan` for a private or link-local address, `peerRelay` for a Tailscale peer relay
/// (`ip:port:vni:N`), `internet` for the rest.
fn classify_path(endpoint: &str) -> &'static str {
    if endpoint.contains(":vni:") {
        return "peerRelay";
    }
    let Ok(address) = endpoint.parse::<SocketAddr>() else {
        return "internet";
    };
    let local = match address.ip() {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_loopback(),
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    };
    if local {
        "lan"
    } else {
        "internet"
    }
}

/// Which helper a log belongs to, for the records.
#[derive(Clone)]
pub(crate) struct LinkLogContext {
    pub(crate) lane: &'static str,
    pub(crate) relay: String,
    pub(crate) logger: Option<std::sync::Arc<GxserverLogger>>,
}

/// Reads one helper's log until it closes, recording every link event; the rest is dropped.
pub(crate) fn follow_tailcat_log(stream: impl Read, context: LinkLogContext) {
    let mut tracker = LinkTracker::default();
    let reader = BufReader::new(stream);
    for line in reader.split(b'\n') {
        let Ok(line) = line else {
            return;
        };
        let line = String::from_utf8_lossy(&line);
        if let Some(event) = tracker.observe(&line, Instant::now()) {
            record_link_event(&context, &event);
        }
    }
}

fn record_link_event(context: &LinkLogContext, event: &LinkEvent) {
    let mut details = json!({
        "lane": context.lane,
        "relay": context.relay,
        "phase": event.phase(),
        "peer": event.peer(),
    });
    if let LinkEvent::Direct { path, .. } = event {
        // `path` would be redacted by the logger as a file path.
        details["route"] = json!(path);
    }
    let Some(logger) = context.logger.as_ref() else {
        return;
    };
    let _ = logger.log_record(GxserverLogInput {
        level: LogLevel::Info,
        event: "easyConnect.link".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT: &str = "nodekey:7c4264ba922f98b1b55465c87f1228e98377adab2bf62a1188448542606e7855";

    #[test]
    fn the_hello_names_the_peer_as_magicsock_does() {
        // base64("7c4264ba" bytes) = "fEJkug==": the short form is its first five characters.
        assert_eq!(short_peer(CLIENT).as_deref(), Some("[fEJku]"));
        assert_eq!(short_peer("discokey:00"), None);
    }

    #[test]
    fn a_link_starts_once_then_goes_direct_once() {
        let mut tracker = LinkTracker::default();
        let now = Instant::now();
        let hello = format!("2026/11/06 10:00:00 got meow from {CLIENT}");
        assert_eq!(
            tracker.observe(&hello, now),
            Some(LinkEvent::Started {
                peer: "[fEJku]".to_string()
            })
        );
        assert_eq!(tracker.observe(&hello, now + Duration::from_secs(2)), None);
        let path = "2026/11/06 10:00:01 magicsock: disco: node [fEJku] d:1234abcd now using 203.0.113.7:41641 mtu=1360 tx=a1b2c3";
        assert_eq!(
            tracker.observe(path, now),
            Some(LinkEvent::Direct {
                peer: "[fEJku]".to_string(),
                path: "internet"
            })
        );
        assert_eq!(tracker.observe(path, now), None);
        // A new hello after the window is a new link, which may go direct again.
        let later = now + REPEATED_HELLO_WINDOW + Duration::from_secs(1);
        assert!(matches!(
            tracker.observe(&hello, later),
            Some(LinkEvent::Started { .. })
        ));
        let lan = "magicsock: disco: node [fEJku] d:1234abcd now using 192.168.1.20:41641 mtu=1360";
        assert_eq!(
            tracker.observe(lan, later),
            Some(LinkEvent::Direct {
                peer: "[fEJku]".to_string(),
                path: "lan"
            })
        );
    }

    #[test]
    fn a_refused_client_is_recorded_and_other_lines_are_dropped() {
        let mut tracker = LinkTracker::default();
        let now = Instant::now();
        assert_eq!(
            tracker.observe(
                &format!("ignoring meow from {CLIENT}: not in allowedClients"),
                now
            ),
            Some(LinkEvent::Refused {
                peer: "[fEJku]".to_string()
            })
        );
        assert_eq!(
            tracker.observe("wg: [v2] Received handshake initiation", now),
            None
        );
        assert_eq!(tracker.observe("NetworkMap: {}", now), None);
    }

    #[test]
    fn paths_are_classified_without_keeping_the_address() {
        assert_eq!(classify_path("10.0.0.4:41641"), "lan");
        assert_eq!(classify_path("[fd7a:115c:a1e0::1]:41641"), "lan");
        assert_eq!(classify_path("[2001:db8::1]:41641"), "internet");
        assert_eq!(classify_path("198.51.100.9:3478:vni:7"), "peerRelay");
    }
}
