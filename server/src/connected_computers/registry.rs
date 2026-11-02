//! The computers this gxserver can reach for its agents, and the access tokens it gave the
//! computers that may reach it.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::Rng;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

/// A registration lasts this long unless the desktop app that made it renews it.
pub(crate) const DEFAULT_REGISTRATION_TTL: Duration = Duration::from_secs(180);
const MAX_REGISTRATION_TTL: Duration = Duration::from_secs(900);
const MAX_NAME_CHARS: usize = 80;

/// Which side opened the connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    /// This computer's desktop app connected to it (a machine in Settings > Remote > Remote machines).
    Outbound,
    /// It connected to this computer, and its desktop app told this gxserver how to reach it back.
    Inbound,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Direction::Outbound => "outbound",
            Direction::Inbound => "inbound",
        }
    }
}

/// A computer this gxserver forwards its agents' requests to.
///
/// CDXC:RemoteMachines 2026-11-03 DECISION:
/// The user chose that any agent on a computer may start, message and read sessions on every computer it is connected to, in both directions, with no question asked: "connecting a computer is permission enough". The one gate is the computer's Agent access switch in Settings > Remote > Remote machines (on by default), which `agent_access` carries; while it is off every agent request to or from that computer is refused with a plain error.
/// SEE-ALSO: apps/desktop/src/app/remote_conn/connected_computers.rs (registers and renews these), server/src/ghostex_cli/rpc.rs (the CLI's `--computer` and refs of another computer), server/src/coordinators/remote_threads.rs (threads on another computer).
#[derive(Clone)]
pub(crate) struct ConnectedComputer {
    pub(crate) server_id: String,
    pub(crate) name: String,
    pub(crate) base_url: String,
    token: String,
    pub(crate) direction: Direction,
    pub(crate) agent_access: bool,
    expires_at: Instant,
}

impl ConnectedComputer {
    pub(crate) fn token(&self) -> &str {
        &self.token
    }

    /// What `listConnectedComputers` shows: never the token or the address it is reached at.
    pub(crate) fn public_view(&self) -> Value {
        json!({
            "serverId": self.server_id,
            "name": self.name,
            "direction": self.direction.as_str(),
            "agentAccess": self.agent_access,
        })
    }
}

/// A token this gxserver handed to another computer so that computer's agents can reach it.
struct PeerGrant {
    token: String,
    name: String,
    agent_access: bool,
    expires_at: Instant,
}

#[derive(Default)]
struct Registry {
    computers: BTreeMap<String, ConnectedComputer>,
    grants: BTreeMap<String, PeerGrant>,
}

fn registry() -> MutexGuard<'static, Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| Mutex::new(Registry::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the desktop app sends to register a computer.
pub(crate) struct Registration {
    pub(crate) server_id: String,
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) token: String,
    pub(crate) direction: Direction,
    pub(crate) agent_access: bool,
    pub(crate) ttl: Duration,
}

impl Registration {
    /// Reads and checks `registerConnectedComputer`'s params. Errors never quote the token.
    pub(crate) fn from_params(params: &Value, local_server_id: &str) -> Result<Self, String> {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let server_id = text("serverId").ok_or("serverId is required.")?;
        if !is_server_id(&server_id) {
            return Err("serverId is not a gxserver server id.".to_string());
        }
        if server_id == local_server_id {
            return Err("A computer cannot be connected to itself.".to_string());
        }
        let name = text("name")
            .map(|name| name.chars().take(MAX_NAME_CHARS).collect::<String>())
            .unwrap_or_else(|| server_id.clone());
        let direction = match text("direction").as_deref() {
            None | Some("outbound") => Direction::Outbound,
            Some("inbound") => Direction::Inbound,
            Some(_) => return Err("direction must be outbound or inbound.".to_string()),
        };
        let agent_access = params
            .get("agentAccess")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        // A computer with Agent access off is registered only so its refusals can name it: it
        // has no address and no token.
        let (base_url, token) = if agent_access {
            let base_url = text("baseUrl").ok_or("baseUrl is required.")?;
            let base_url = loopback_base_url(&base_url)
                .ok_or("baseUrl must be http://127.0.0.1:<port> or http://localhost:<port>.")?;
            let token = text("token").ok_or("token is required.")?;
            if !is_token(&token) {
                return Err("token is not a gxserver token.".to_string());
            }
            (base_url, token)
        } else {
            (String::new(), String::new())
        };
        let ttl = params
            .get("ttlMs")
            .and_then(Value::as_u64)
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_REGISTRATION_TTL)
            .clamp(Duration::from_secs(10), MAX_REGISTRATION_TTL);
        Ok(Self {
            server_id,
            name,
            base_url,
            token,
            direction,
            agent_access,
            ttl,
        })
    }
}

fn is_token(token: &str) -> bool {
    token.len() <= 512 && !token.chars().any(char::is_whitespace)
}

/// Server ids are `S` plus base-36 digits (`ids::create_server_id`).
pub(crate) fn is_server_id(value: &str) -> bool {
    value.len() >= 2
        && value.len() <= 64
        && value.starts_with('S')
        && value[1..].chars().all(|c| c.is_ascii_alphanumeric())
}

/// Only a loopback address is accepted: a registered computer is always reached through the SSH
/// tunnel's forwarded port on this computer, so nothing can point gxserver at another host.
fn loopback_base_url(value: &str) -> Option<String> {
    let rest = value.trim_end_matches('/').strip_prefix("http://")?;
    let (host, port) = rest.rsplit_once(':')?;
    if !matches!(host, "127.0.0.1" | "localhost") {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    (port != 0).then(|| format!("http://127.0.0.1:{port}"))
}

/// Adds or renews a computer: where this gxserver reaches it and the token that computer gave
/// this one (`issue_grant` on its side).
pub(crate) fn register(registration: Registration) {
    let mut registry = registry();
    let now = Instant::now();
    prune(&mut registry, now);
    registry.computers.insert(
        registration.server_id.clone(),
        ConnectedComputer {
            server_id: registration.server_id,
            name: registration.name,
            base_url: registration.base_url,
            token: registration.token,
            direction: registration.direction,
            agent_access: registration.agent_access,
            expires_at: now + registration.ttl,
        },
    );
}

/// The token `server_id`'s agents use to reach this gxserver: kept and renewed, or a new one when
/// `rotate` (a new way back was opened, or the machine reconnected) or when there is none. With
/// Agent access off the grant is dropped and there is no token.
///
/// CDXC:RemoteMachines 2026-11-03 WHY:
/// The token rotates whenever the way back is (re)opened: a dead reverse forward frees its port on the other computer, where any local user could listen and collect the token the other side keeps sending, so a token must never outlive the tunnel it was handed over through.
pub(crate) fn issue_grant(
    server_id: &str,
    name: &str,
    agent_access: bool,
    rotate: bool,
    ttl: Duration,
) -> Option<String> {
    let mut registry = registry();
    let now = Instant::now();
    prune(&mut registry, now);
    if !agent_access {
        registry.grants.remove(server_id);
        return None;
    }
    let token = registry
        .grants
        .get(server_id)
        .filter(|_| !rotate)
        .map(|grant| grant.token.clone())
        .unwrap_or_else(new_peer_token);
    registry.grants.insert(
        server_id.to_string(),
        PeerGrant {
            token: token.clone(),
            name: name.chars().take(MAX_NAME_CHARS).collect(),
            agent_access,
            expires_at: now + ttl.clamp(Duration::from_secs(10), MAX_REGISTRATION_TTL),
        },
    );
    Some(token)
}

/// Drops the token `server_id` was given, so its agents are refused from now on.
pub(crate) fn revoke_grant(server_id: &str) -> bool {
    registry().grants.remove(server_id).is_some()
}

/// Forgets a computer and the token it was given, so its agents are refused from now on.
pub(crate) fn unregister(server_id: &str) -> bool {
    let mut registry = registry();
    let had_grant = registry.grants.remove(server_id).is_some();
    registry.computers.remove(server_id).is_some() || had_grant
}

/// Every connected computer, without tokens.
pub(crate) fn list() -> Vec<ConnectedComputer> {
    let mut registry = registry();
    prune(&mut registry, Instant::now());
    registry.computers.values().cloned().collect()
}

pub(crate) fn find(server_id: &str) -> Option<ConnectedComputer> {
    let mut registry = registry();
    prune(&mut registry, Instant::now());
    registry.computers.get(server_id).cloned()
}

/// Who a bearer token belongs to when it is not this gxserver's own token.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PeerAuthorization {
    Allowed { server_id: String, name: String },
    AccessOff { name: String },
}

pub(crate) fn authorize_peer_token(provided: &str) -> Option<PeerAuthorization> {
    let mut registry = registry();
    prune(&mut registry, Instant::now());
    registry.grants.iter().find_map(|(server_id, grant)| {
        (grant.token.len() == provided.len()
            && bool::from(grant.token.as_bytes().ct_eq(provided.as_bytes())))
        .then(|| {
            if grant.agent_access {
                PeerAuthorization::Allowed {
                    server_id: server_id.clone(),
                    name: grant.name.clone(),
                }
            } else {
                PeerAuthorization::AccessOff {
                    name: grant.name.clone(),
                }
            }
        })
    })
}

fn prune(registry: &mut Registry, now: Instant) {
    registry
        .computers
        .retain(|_, computer| computer.expires_at > now);
    registry.grants.retain(|_, grant| grant.expires_at > now);
}

fn new_peer_token() -> String {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    format!("gxpeer_{}", URL_SAFE_NO_PAD.encode(random))
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    let mut registry = registry();
    registry.computers.clear();
    registry.grants.clear();
}
