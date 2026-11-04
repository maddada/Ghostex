use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use ureq::config::Config;
use ureq::http::{Response, Uri};
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};
use ureq::Body;
use url::Url;

use crate::tcp_listeners::listener_discovery::TcpListenerDetail;

/// CDXC:Browser 2026-09-12 WHY:
/// Page discovery runs on the computer because its localhost addresses are not reachable from the phone.
/// Bounded concurrent probes keep a long list of non-HTTP listeners from delaying the browser picker indefinitely.
pub(super) fn inspect_ports(listeners: &[TcpListenerDetail]) -> BTreeMap<u16, Value> {
    let mut ports = listeners
        .iter()
        .map(|row| row.port)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    ports.sort_by_key(|port| (probe_priority(listeners, *port), *port));
    let targets = Arc::new(
        ports
            .iter()
            .map(|port| (*port, connect_addresses(listeners, *port)))
            .collect::<HashMap<_, _>>(),
    );
    let cursor = AtomicUsize::new(0);
    let results = Mutex::new(BTreeMap::new());
    let deadline = Instant::now() + Duration::from_secs(6);
    std::thread::scope(|scope| {
        for _ in 0..8.min(ports.len()) {
            let ports = &ports;
            let cursor = &cursor;
            let results = &results;
            let targets = targets.clone();
            scope.spawn(move || {
                let agent = ureq::Agent::with_parts(
                    crate::http_agent::config()
                        .timeout_global(Some(Duration::from_millis(900)))
                        .max_redirects(0)
                        .http_status_as_error(false)
                        .build(),
                    DefaultConnector::default(),
                    ListenerResolver { targets },
                );
                while Instant::now() < deadline {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(port) = ports.get(index) else { break };
                    if let Some(metadata) = inspect_port(&agent, *port, deadline) {
                        results.lock().unwrap().insert(*port, metadata);
                    }
                }
            });
        }
    });
    results.into_inner().unwrap()
}

/// CDXC:Browser 2026-10-05 WHY:
/// Probes connect to the address the port is bound to while the URL, and so the Host header, stays `localhost`. Left to the resolver, `localhost` tries `::1` first, and on Windows a refused loopback connection takes about two seconds, so every port bound only to 127.0.0.1 used up its whole timeout and was never described. Wildcard binds are reached through the loopback address of their family.
fn connect_addresses(listeners: &[TcpListenerDetail], port: u16) -> Vec<SocketAddr> {
    let loopbacks = [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ];
    let mut addresses = listeners
        .iter()
        .filter(|listener| listener.port == port)
        .flat_map(|listener| {
            let address = listener
                .address
                .trim_start_matches('[')
                .trim_end_matches(']');
            let address = address.split('%').next().unwrap_or_default();
            match address.parse::<IpAddr>() {
                Ok(IpAddr::V4(ip)) if ip.is_unspecified() => vec![loopbacks[0]],
                Ok(IpAddr::V6(ip)) if ip.is_unspecified() => vec![loopbacks[1]],
                Ok(ip) => vec![ip],
                Err(_) => loopbacks.to_vec(),
            }
        })
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        addresses = loopbacks.to_vec();
    }
    // Loopback first, IPv4 before IPv6, then any address bound to one interface.
    addresses.sort_by_key(|ip| (!ip.is_loopback(), ip.is_ipv6()));
    addresses.dedup();
    addresses
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect()
}

/// CDXC:Browser 2026-10-05 WHY:
/// The probe budget is shared, and a computer can hold dozens of listeners that never answer HTTP (Windows RPC and service ports, Ghostex's own session daemons) and each hold a probe until it times out. Dev-server runtimes go first and those system listeners last, so they cannot use up the budget before the pages the picker exists for.
fn probe_priority(listeners: &[TcpListenerDetail], port: u16) -> u8 {
    let commands = listeners
        .iter()
        .filter(|listener| listener.port == port)
        .filter_map(|listener| listener.command.as_deref())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let dev_runtime = |command: &str| {
        command.starts_with("python")
            || command.starts_with("next-server")
            || matches!(
                command,
                "node"
                    | "bun"
                    | "deno"
                    | "ruby"
                    | "php"
                    | "java"
                    | "go"
                    | "cargo"
                    | "dotnet"
                    | "tsx"
                    | "npx"
                    | "uvicorn"
                    | "gunicorn"
                    | "puma"
            )
    };
    let system = |command: &str| {
        matches!(
            command,
            "system"
                | "svchost"
                | "lsass"
                | "wininit"
                | "services"
                | "spoolsv"
                | "launchd"
                | "rapportd"
                | "sharingd"
                | "controlcenter"
                | "mdnsresponder"
                | "wmx"
                | "zmx"
        )
    };
    if commands.iter().any(|command| dev_runtime(command)) {
        0
    } else if port < 1024 || commands.iter().any(|command| system(command)) {
        2
    } else {
        1
    }
}

/// `localhost:<port>` goes to the addresses that port's listener is bound to; every other name
/// resolves as usual.
#[derive(Debug)]
struct ListenerResolver {
    targets: Arc<HashMap<u16, Vec<SocketAddr>>>,
}

impl Resolver for ListenerResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let bound = uri
            .host()
            .filter(|host| host.eq_ignore_ascii_case("localhost"))
            .and_then(|_| uri.port_u16())
            .and_then(|port| self.targets.get(&port))
            .filter(|addresses| !addresses.is_empty());
        let Some(addresses) = bound else {
            return DefaultResolver::default().resolve(uri, config, timeout);
        };
        let mut resolved = self.empty();
        for address in addresses {
            if resolved.try_push(*address).is_err() {
                break;
            }
        }
        Ok(resolved)
    }
}

fn response(agent: &ureq::Agent, url: &str) -> Option<Response<Body>> {
    agent
        .get(url)
        .header("User-Agent", "Ghostex-Web-Preview")
        .call()
        .ok()
}

fn inspect_port(agent: &ureq::Agent, port: u16, deadline: Instant) -> Option<Value> {
    for scheme in ["http", "https"] {
        if Instant::now() >= deadline {
            break;
        }
        let origin = Url::parse(&format!("{scheme}://localhost:{port}/")).ok()?;
        let mut page_url = origin.clone();
        let Some(mut reply) = response(agent, page_url.as_str()) else {
            continue;
        };
        // Follow redirects within this listener, so /login and /app expose their own titles.
        for _ in 0..3 {
            if !reply.status().is_redirection() || Instant::now() >= deadline {
                break;
            }
            let Some(next) = crate::http_agent::header(&reply, "Location")
                .and_then(|location| page_url.join(location).ok())
            else {
                break;
            };
            if next.host_str() != origin.host_str()
                || next.port_or_known_default() != Some(port)
                || !matches!(next.scheme(), "http" | "https")
            {
                break;
            }
            let Some(next_reply) = response(agent, next.as_str()) else {
                break;
            };
            page_url = next;
            reply = next_reply;
        }
        let status = reply.status().as_u16();
        let content_type = crate::http_agent::header(&reply, "Content-Type")
            .unwrap_or("")
            .to_string();
        let server = crate::http_agent::header(&reply, "Server")
            .map(|value| value.chars().take(100).collect::<String>());
        let mut bytes = Vec::new();
        let _ = reply
            .into_body()
            .into_reader()
            .take(64 * 1024)
            .read_to_end(&mut bytes);
        let body = String::from_utf8_lossy(&bytes);
        let lower = body.to_ascii_lowercase();
        let html = content_type.contains("text/html")
            || lower.contains("<html")
            || lower.contains("<!doctype html");
        let title = html.then(|| page_title(&body)).flatten();
        let favicon = if html && Instant::now() < deadline {
            let href = crate::project_icon::extract_icon_href(&body)
                .unwrap_or_else(|| "/favicon.ico".into());
            page_url
                .join(&href)
                .ok()
                .filter(|url| url.origin() == page_url.origin())
                .and_then(|url| favicon_data_url(agent, url.as_str()))
        } else {
            None
        };
        return Some(json!({
            "scheme": page_url.scheme(), "status": status, "kind": if html { "page" } else { "service" },
            "title": title, "server": server, "contentType": content_type,
            "faviconDataUrl": favicon,
        }));
    }
    None
}

fn page_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let start = start + lower[start..].find('>')? + 1;
    let end = start + lower[start..].find("</title>")?;
    let text = decode_entities(&html[start..end]);
    let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then(|| title.chars().take(200).collect())
}

fn decode_entities(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 12) else {
            result.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|value| u32::from_str_radix(value, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|value| value.parse().ok())
                })
                .and_then(char::from_u32),
        };
        if let Some(character) = decoded {
            result.push(character);
        } else {
            result.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

fn favicon_data_url(agent: &ureq::Agent, url: &str) -> Option<String> {
    let reply = response(agent, url)?;
    if reply.status() != 200 {
        return None;
    }
    let mut bytes = Vec::new();
    reply
        .into_body()
        .into_reader()
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 16 * 1024 {
        return None;
    }
    // PNG-backed ICO entries can be displayed by both native mobile image decoders.
    if bytes.starts_with(&[0, 0, 1, 0]) && bytes.len() >= 22 {
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        for index in 0..count.min(32) {
            let entry = 6 + index * 16;
            let Some(record) = bytes.get(entry..entry + 16) else {
                break;
            };
            let length = u32::from_le_bytes(record[8..12].try_into().ok()?) as usize;
            let offset = u32::from_le_bytes(record[12..16].try_into().ok()?) as usize;
            if let Some(png) = offset
                .checked_add(length)
                .and_then(|end| bytes.get(offset..end))
            {
                if png.starts_with(b"\x89PNG\r\n\x1a\n") {
                    return Some(format!("data:image/png;base64,{}", STANDARD.encode(png)));
                }
            }
        }
        return None;
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}
