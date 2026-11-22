//! Server links, the dialing side (GitHub #674; the decision is CDXC:ServerLinks 2026-11-19 in
//! server_links.rs): this gxserver keeps an Easy Connect tunnel to each computer it linked with,
//! asks it for its agents' grant every minute (signed with this computer's pairing device key),
//! registers it as a connected computer, and, after `ghostex computers add` here, asks it to pair
//! back. It also answers `/api/serverLinks` for this computer's own CLI and apps.

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::ghostex_cli::tailcat_tunnel::{TailcatForwardPlan, TailcatTunnel, start_tailcat_tunnel};
use crate::paths::GxserverPaths;
use crate::security::Principal;
use crate::server::AppState;

use super::computer_pairing::{
    ComputerPairingCode, LinkPairing, computer_pairing_status, device_public_key, device_sign,
    load_or_create_device_key, start_computer_pairing_with,
};
use super::server_links::{
    self as links, ConnectBackCode, LinkState, OutgoingLink, ReverseAsk, SERVER_LINK_ENDPOINT,
    ServerLinkAsk,
};

/// How often a linked computer is asked for a fresh grant (it lasts 180 s there).
const RENEW_EVERY: Duration = Duration::from_secs(60);
/// How often a pairing that waits for the owner is asked about.
const PAIRING_POLL_EVERY: Duration = Duration::from_secs(3);
/// A failed ask to pair back is tried again after this long.
const CONNECT_BACK_RETRY: Duration = Duration::from_secs(5 * 60);
/// An ended link stays listed this long, so `ghostex computers add` can say why.
const ENDED_KEPT_MS: u64 = 10 * 60 * 1000;
const ASK_TIMEOUT: Duration = Duration::from_secs(20);
/// The door's helper restarts with a new key; the code waits for it like Connect a Computer does.
const DOOR_READY_TIMEOUT: Duration = Duration::from_secs(5);

/// One live link's tunnel and when it was last renewed.
struct LiveLink {
    tunnel: TailcatTunnel,
    local_port: u16,
    address: String,
    last_grant: Option<Instant>,
    /// The tunnel is new: the next grant rotates the token (it must not outlive its tunnel).
    rotate: bool,
}

fn live() -> &'static Mutex<HashMap<String, LiveLink>> {
    static LIVE: OnceLock<Mutex<HashMap<String, LiveLink>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn polls() -> &'static Mutex<HashMap<String, Instant>> {
    static POLLS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    POLLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Each ask to pair back: when it was last tried and how many tries failed in a row.
fn connect_back_tries() -> &'static Mutex<HashMap<String, (Instant, u32)>> {
    static TRIES: OnceLock<Mutex<HashMap<String, (Instant, u32)>>> = OnceLock::new();
    TRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A link whose last renewal failed waits before the next (review of #674, finding 5): 10 s,
/// then twice as long each time, up to ten minutes. By server id: (retry at, failures in a row).
fn backoff() -> &'static Mutex<HashMap<String, (Instant, u32)>> {
    static BACKOFF: OnceLock<Mutex<HashMap<String, (Instant, u32)>>> = OnceLock::new();
    BACKOFF.get_or_init(|| Mutex::new(HashMap::new()))
}

fn backing_off(server_id: &str) -> bool {
    backoff()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(server_id)
        .is_some_and(|(until, _)| Instant::now() < *until)
}

fn note_failure(server_id: &str) {
    let mut backoff = backoff()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let failures = backoff.get(server_id).map_or(0, |(_, failures)| *failures) + 1;
    let wait = Duration::from_secs(10)
        .saturating_mul(1 << failures.min(6))
        .min(Duration::from_secs(600));
    backoff.insert(server_id.to_string(), (Instant::now() + wait, failures));
}

fn note_success(server_id: &str) {
    backoff()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(server_id);
}

/// Asks to pair back stop after this many failures in a row (`ghostex computers` shows why).
const CONNECT_BACK_MAX_TRIES: u32 = 5;

/// An Easy Connect address as a code carries it: base64url-ish text (`tc…`), never an option
/// (a leading `-`), no spaces or control characters, of a sane length.
pub(crate) fn is_easy_connect_address(address: &str) -> bool {
    !address.is_empty()
        && address.len() <= 4096
        && !address.starts_with('-')
        && address.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '=' | '+' | '/')
        })
}

/// The key file a server link to `address` pairs and dials with.
fn key_file_for(paths: &GxserverPaths, address: &str) -> PathBuf {
    let id = hex::encode(&Sha256::digest(address.trim().as_bytes())[..8]);
    paths
        .root_dir
        .join("pairing")
        .join("link-keys")
        .join(format!("{id}-serverlink.json"))
}

/// Starts pairing this computer with the one behind `code`, asking for a server link. Answers
/// `{ pendingId, words, computerName }` once that computer shows it as waiting. `expected_key`:
/// that computer's pairing device key when it is already known (it asked us to pair back).
pub fn start_link_pairing(
    paths: &GxserverPaths,
    own_server_id: &str,
    code: ComputerPairingCode,
    connect_back: bool,
    expected_key: Option<String>,
    expected_server_id: Option<String>,
) -> Result<Value> {
    // It reaches tailcat's command line: refused before anything runs (review of #674, finding 11).
    if !is_easy_connect_address(&code.address) {
        bail!("That code's Easy Connect address isn't valid.");
    }
    if code.name.chars().count() > 80 || code.name.chars().any(char::is_control) {
        bail!("That code's computer name isn't valid.");
    }
    let key_file = key_file_for(paths, &code.address);
    if let Some(parent) = key_file.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let address = code.address.clone();
    let port = code.port;
    let started = start_computer_pairing_with(
        paths,
        code,
        None,
        Some(LinkPairing {
            own_server_id: own_server_id.to_string(),
            key_file: key_file.clone(),
        }),
    )?;
    let pending_id = started
        .get("pendingId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let cancel = |message: &str| -> Result<Value> {
        super::computer_pairing::cancel_computer_pairing(&pending_id);
        bail!("{message}")
    };
    if started.get("serverLink").and_then(Value::as_bool) != Some(true) {
        return cancel(
            "The other computer's Ghostex is too old for server links. Update Ghostex there, then try again.",
        );
    }
    let server_id = started
        .get("serverId")
        .and_then(Value::as_str)
        .filter(|id| crate::connected_computers::is_server_id(id))
        .map(str::to_string);
    let peer_key = started
        .get("deviceKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_string);
    let (Some(server_id), Some(peer_key)) = (server_id, peer_key) else {
        return cancel("The other computer sent an unreadable answer.");
    };
    if server_id == own_server_id {
        return cancel("That code is this computer's own.");
    }
    if expected_key.as_deref().is_some_and(|key| key != peer_key) {
        return cancel("The computer that answered isn't the one that asked to link.");
    }
    // Pairing back: it must be the server id this computer pinned for it (review of #674, finding 3).
    if expected_server_id
        .as_deref()
        .is_some_and(|expected| expected != server_id)
    {
        return cancel(
            "The computer that answered names another server id than the one it linked with.",
        );
    }
    // A computer the desktop app connects here can't also be a server link (its grant slot and
    // registration are the app's): the same rule the door side keeps.
    let linked_here = {
        let file = links::read(paths);
        file.pin_for_server(&server_id).is_some() || file.outgoing_for_server(&server_id).is_some()
    };
    if !linked_here
        && (crate::connected_computers::find(&server_id).is_some()
            || crate::connected_computers::has_grant(&server_id))
    {
        return cancel(
            "That computer is already connected here through the Ghostex app, so it can't link as a server too.",
        );
    }
    // Another link already holds that server id under a different key: refused, not replaced.
    let held = links::read(paths).outgoing.iter().any(|link| {
        link.server_id.as_deref() == Some(server_id.as_str())
            && link.peer_device_key.as_deref() != Some(peer_key.as_str())
            && link.state != LinkState::Ended
    });
    if held {
        return cancel("Another computer is already linked here under that computer's server id.");
    }
    let name = started
        .get("computerName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let words: Vec<String> = started
        .get("words")
        .and_then(Value::as_array)
        .map(|words| {
            words
                .iter()
                .filter_map(|word| word.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    forget_retry_timers(Some(&server_id));
    links::update(paths, |file| {
        file.outgoing.retain(|link| link.address != address);
        file.outgoing.push(OutgoingLink {
            address: address.clone(),
            gxserver_port: port,
            name: name.clone(),
            key_file,
            server_id: Some(server_id.clone()),
            peer_device_key: Some(peer_key),
            state: LinkState::Pairing,
            pending_id: Some(pending_id.clone()),
            words: words.clone(),
            connect_back,
            reverse: None,
            error: None,
            created_at_ms: crate::security::now_ms(),
        });
    });
    wake();
    Ok(json!({
        "pendingId": pending_id,
        "words": words,
        "computerName": name,
        "serverId": server_id,
    }))
}

/// This gxserver's server id, for asks sent where no request state is at hand (a removal).
static OWN_SERVER_ID: OnceLock<String> = OnceLock::new();

fn wake_signal() -> &'static tokio::sync::Notify {
    static WAKE: OnceLock<tokio::sync::Notify> = OnceLock::new();
    WAKE.get_or_init(tokio::sync::Notify::new)
}

fn wake() {
    wake_signal().notify_one();
}

/// Starts keeping this computer's server links up: every few seconds while a pairing waits, and
/// a fresh grant from each linked computer every minute.
pub(crate) fn start(state: Arc<AppState>) {
    let _ = OWN_SERVER_ID.set(state.metadata.server_id.clone());
    links::note_serving_since(crate::security::now_ms());
    // Reads the store once, so `has_links` knows whether there is anything to keep up.
    let _ = links::read(&state.paths);
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        static RUNNING: AtomicBool = AtomicBool::new(false);
        let mut tick = crate::ticks::steady_interval(PAIRING_POLL_EVERY);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tick.tick() => {}
                () = wake_signal().notified() => {}
            }
            // Nothing to keep up: no sweep, no file read (review of #674, finding 10).
            if !links::has_links() {
                continue;
            }
            if RUNNING.swap(true, Ordering::SeqCst) {
                continue;
            }
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                sweep(&state);
                RUNNING.store(false, Ordering::SeqCst);
            });
        }
        for (_, link) in live()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
        {
            link.tunnel.stop();
        }
    });
}

fn sweep(state: &AppState) {
    let paths = &state.paths;
    let now_ms = crate::security::now_ms();
    if let Ok(db) = crate::storage::open_gxserver_database(paths) {
        links::prune_pins(paths, &db);
    }
    let file = links::read(paths);
    // Ended links are listed a while, then dropped.
    if file
        .outgoing
        .iter()
        .any(|link| link.state == LinkState::Ended && link.created_at_ms + ENDED_KEPT_MS < now_ms)
    {
        links::update(paths, |file| {
            file.outgoing.retain(|link| {
                link.state != LinkState::Ended || link.created_at_ms + ENDED_KEPT_MS >= now_ms
            })
        });
    }
    // Tunnels of links that are gone close.
    {
        let keep: Vec<String> = file
            .outgoing
            .iter()
            .filter(|link| link.state == LinkState::Linked)
            .filter_map(|link| link.server_id.clone())
            .collect();
        let mut live = live()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        live.retain(|server_id, link| {
            let kept = keep.contains(server_id);
            if !kept {
                link.tunnel.stop();
            }
            kept
        });
    }
    for link in file.outgoing {
        match link.state {
            LinkState::Pairing => poll_pairing(state, &link),
            LinkState::Linked => keep_linked(state, &link),
            LinkState::Ended => {}
        }
    }
}

fn poll_pairing(state: &AppState, link: &OutgoingLink) {
    let Some(pending_id) = link.pending_id.clone() else {
        return;
    };
    {
        let mut polls = polls()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if polls
            .get(&pending_id)
            .is_some_and(|at| at.elapsed() < PAIRING_POLL_EVERY)
        {
            return;
        }
        polls.insert(pending_id.clone(), Instant::now());
    }
    let answer = match computer_pairing_status(&state.paths, &pending_id) {
        Ok(answer) => answer,
        Err(error) => {
            end(&state.paths, link, &format!("{error:#}"));
            return;
        }
    };
    let status = answer
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("expired");
    match status {
        "pending" => {}
        "approved" if answer.get("serverLink").and_then(Value::as_str) == Some("linked") => {
            let address = link.address.clone();
            links::update(&state.paths, |file| {
                if let Some(saved) = file.outgoing.iter_mut().find(|l| l.address == address) {
                    saved.state = LinkState::Linked;
                    saved.pending_id = None;
                    saved.error = None;
                }
            });
            crate::audit::record(
                crate::audit::Action::new(
                    "computers.serverLink",
                    format!("Linked this computer's agents with {}", link.name),
                )
                .target(link.name.clone()),
                crate::audit::Actor::Ghostex,
                crate::audit::Outcome::Done,
                None,
            );
            wake();
        }
        "approved" => {
            let why = answer
                .get("serverLinkReason")
                .and_then(Value::as_str)
                .unwrap_or("The other computer paired this one without a server link.");
            end(&state.paths, link, why);
        }
        "denied" => end(&state.paths, link, "It was denied on the other computer."),
        _ => end(
            &state.paths,
            link,
            "Nobody approved it in time. Run ghostex computers add again with a new code.",
        ),
    }
}

fn end(paths: &GxserverPaths, link: &OutgoingLink, why: &str) {
    let address = link.address.clone();
    links::update(paths, |file| {
        if let Some(saved) = file.outgoing.iter_mut().find(|l| l.address == address) {
            saved.state = LinkState::Ended;
            saved.pending_id = None;
            saved.error = Some(why.to_string());
            saved.created_at_ms = crate::security::now_ms();
        }
    });
}

/// The tunnel to a linked computer, opened (or opened again) when needed; its local port.
fn ensure_tunnel(link: &OutgoingLink, server_id: &str) -> Result<u16> {
    let mut live = live()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(open) = live.get(server_id) {
        if open.address == link.address {
            return Ok(open.local_port);
        }
        open.tunnel.stop();
        live.remove(server_id);
    }
    let binary = crate::tailcat::resolve_client_tailcat_binary()
        .ok_or_else(|| anyhow!("no Easy Connect helper on this computer"))?;
    let local_port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let plan = TailcatForwardPlan {
        address_blob: link.address.clone(),
        base_url: format!("http://127.0.0.1:{local_port}"),
        local_port,
        remote_port: link.gxserver_port,
        client_key: Some(link.key_file.clone()),
    };
    let tunnel = start_tailcat_tunnel(&binary, &plan).map_err(|error| anyhow!("{error}"))?;
    live.insert(
        server_id.to_string(),
        LiveLink {
            tunnel,
            local_port,
            address: link.address.clone(),
            last_grant: None,
            rotate: true,
        },
    );
    Ok(local_port)
}

fn drop_tunnel(server_id: &str) {
    if let Some(link) = live()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(server_id)
    {
        link.tunnel.stop();
    }
}

/// A signed ask to the linked computer through its tunnel: the answer, or the refusal's code and
/// words.
fn ask(
    paths: &GxserverPaths,
    local_port: u16,
    own_server_id: &str,
    to_server_id: &str,
    op: &str,
    rotate: bool,
    code: Option<ConnectBackCode>,
) -> std::result::Result<Value, (String, String)> {
    let key =
        load_or_create_device_key(paths).map_err(|e| ("internal".into(), format!("{e:#}")))?;
    let public = device_public_key(&key).map_err(|e| ("internal".into(), format!("{e:#}")))?;
    let mut signed = ServerLinkAsk {
        op: op.to_string(),
        device_key: public,
        to_server_id: to_server_id.to_string(),
        from_server_id: own_server_id.to_string(),
        name: crate::connected_computers::this_computer_name(),
        timestamp_ms: crate::security::now_ms(),
        rotate,
        signature: String::new(),
        code,
    };
    signed.signature =
        device_sign(&key, &signed.payload()).map_err(|e| ("internal".into(), format!("{e:#}")))?;
    let mut params = json!({
        "op": signed.op,
        "deviceKey": signed.device_key,
        "toServerId": signed.to_server_id,
        "fromServerId": signed.from_server_id,
        "name": signed.name,
        "timestampMs": signed.timestamp_ms,
        "rotate": signed.rotate,
        "signature": signed.signature,
    });
    if let Some(code) = &signed.code {
        params["code"] = json!(code);
    }
    let body = json!({
        "requestId": format!("server-link-{}", uuid::Uuid::new_v4().simple()),
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
        "params": params,
    });
    let base = format!("http://127.0.0.1:{local_port}");
    // The tunnel's port must still be ours before anything is sent through it.
    ghostex_paths::loopback_owner::check_url(&base).map_err(|_| {
        (
            "foreignListener".to_string(),
            "Another program holds the tunnel's port.".to_string(),
        )
    })?;
    let response = crate::http_agent::answering_agent()
        .post(format!("{base}{SERVER_LINK_ENDPOINT}"))
        .header("content-type", "application/json")
        .header(
            "x-gxserver-protocol-version",
            crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(ASK_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .send(body.to_string().as_str())
        .map_err(|error| ("unreachable".to_string(), error.to_string()))?;
    let answer: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
    if answer.get("ok") == Some(&Value::Bool(true)) {
        return Ok(answer.get("result").cloned().unwrap_or(Value::Null));
    }
    Err((
        answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unreachable")
            .to_string(),
        answer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("no answer")
            .to_string(),
    ))
}

fn keep_linked(state: &AppState, link: &OutgoingLink) {
    let Some(server_id) = link.server_id.clone() else {
        return;
    };
    let paths = &state.paths;
    let own = state.metadata.server_id.clone();
    // This computer's own switch is off for it: registered off, no tunnel, nothing asked.
    if links::access_off(paths, &server_id) {
        drop_tunnel(&server_id);
        register_off(&server_id, &link.name, false);
        return;
    }
    let due = live()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&server_id)
        .is_none_or(|open| open.last_grant.is_none_or(|at| at.elapsed() >= RENEW_EVERY));
    if !due || backing_off(&server_id) {
        maybe_connect_back(state, link, &server_id);
        return;
    }
    let local_port = match ensure_tunnel(link, &server_id) {
        Ok(port) => port,
        Err(error) => {
            note_failure(&server_id);
            eprintln!(
                "[gxserver] server link to {}: no tunnel ({error:#})",
                link.name
            );
            return;
        }
    };
    let rotate = live()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&server_id)
        .is_some_and(|open| open.rotate);
    match ask(paths, local_port, &own, &server_id, "grant", rotate, None) {
        Ok(answer) => {
            if let Some(open) = live()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_mut(&server_id)
            {
                open.last_grant = Some(Instant::now());
                open.rotate = false;
            }
            note_success(&server_id);
            // Pinned at pairing: an answer naming another server id is refused.
            if answer.get("serverId").and_then(Value::as_str) != Some(server_id.as_str()) {
                eprintln!(
                    "[gxserver] server link to {}: it answered with another server id; refused",
                    link.name
                );
                return;
            }
            let name = answer
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or(&link.name)
                .to_string();
            match (
                answer.get("agentAccess").and_then(Value::as_bool),
                answer.get("peerToken").and_then(Value::as_str),
            ) {
                (Some(true), Some(token)) => {
                    crate::connected_computers::register(
                        crate::connected_computers::Registration {
                            server_id: server_id.clone(),
                            name,
                            base_url: format!("http://127.0.0.1:{local_port}"),
                            token: token.to_string(),
                            direction: crate::connected_computers::Direction::Outbound,
                            agent_access: true,
                            ttl: crate::connected_computers::DEFAULT_REGISTRATION_TTL,
                            owner: false,
                            off_on_other_computer: false,
                            machine_id: None,
                            waiting: None,
                        },
                    );
                }
                _ => register_off(&server_id, &name, true),
            }
            maybe_connect_back(state, link, &server_id);
        }
        // Only that computer, through the tunnel to the address the user's code named, can say
        // this: it removed this computer, so the link ends both ways here too (the user's 3A).
        Err((code, _)) if code == links::NOT_PAIRED => {
            eprintln!(
                "[gxserver] server link to {}: it no longer has this computer; ending the link",
                link.name
            );
            end_both_ways(state, link);
        }
        Err((code, message)) => {
            if code == "unreachable" {
                // A new tunnel next time, and a new token with it.
                drop_tunnel(&server_id);
            }
            note_failure(&server_id);
            eprintln!("[gxserver] server link to {}: {code}: {message}", link.name);
        }
    }
}

fn register_off(server_id: &str, name: &str, other_computer: bool) {
    crate::connected_computers::register(crate::connected_computers::Registration {
        server_id: server_id.to_string(),
        name: name.to_string(),
        base_url: String::new(),
        token: String::new(),
        direction: crate::connected_computers::Direction::Outbound,
        agent_access: false,
        ttl: crate::connected_computers::DEFAULT_REGISTRATION_TTL,
        owner: !other_computer,
        off_on_other_computer: other_computer,
        machine_id: None,
        waiting: None,
    });
}

/// The other computer removed this one: its row here and this side's link go too.
fn end_both_ways(state: &AppState, link: &OutgoingLink) {
    let paths = &state.paths;
    // Only the row this computer pinned for that link (review of #674, finding 1): another
    // pairing of the same computer (Connect a Computer, with its SSH key line and Easy Connect
    // key) is left as it is.
    let pinned = link
        .peer_device_key
        .as_deref()
        .and_then(|key| links::pinned_row_for_key(paths, key));
    if let Some((device_id, pin)) = pinned {
        if let Ok(db) = crate::storage::open_gxserver_database(paths) {
            if let Ok(Some(removed)) = super::paired_devices::remove_paired_device(
                paths,
                &db,
                &state.tailcat_runtime,
                &state.device_api_tokens,
                &device_id,
            ) {
                links::on_device_removed(paths, &removed.id, Some(pin), false);
            }
        }
    }
    forget_link(paths, link, false);
}

/// Stops this side's link (its tunnel, its registration), telling the other computer when
/// `tell` (signed `unlink`, best effort), and drops it from the store.
pub fn forget_link(paths: &GxserverPaths, link: &OutgoingLink, tell: bool) -> bool {
    let mut told = false;
    if let Some(pending_id) = &link.pending_id {
        super::computer_pairing::cancel_computer_pairing(pending_id);
    }
    if let Some(server_id) = &link.server_id {
        if tell && link.state == LinkState::Linked {
            let own = OWN_SERVER_ID.get().cloned();
            if let (Some(own), Ok(port)) = (own, ensure_tunnel(link, server_id)) {
                match ask(paths, port, &own, server_id, "unlink", false, None) {
                    Ok(_) => told = true,
                    Err((code, message)) => eprintln!(
                        "[gxserver] server link to {}: couldn't tell it ({code}: {message}); it ends there at its next renewal",
                        link.name
                    ),
                }
            }
        }
        drop_tunnel(server_id);
        crate::connected_computers::unregister(server_id);
        crate::connected_computers::revoke_grant(server_id, false, Duration::from_secs(10));
    }
    let address = link.address.clone();
    links::update(paths, |file| file.outgoing.retain(|l| l.address != address));
    forget_retry_timers(link.server_id.as_deref());
    // Its own Easy Connect key goes with it (review of #674, finding 12), only from the folder
    // this file makes them in.
    if link
        .key_file
        .starts_with(paths.root_dir.join("pairing").join("link-keys"))
        && link
            .key_file
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("-serverlink.json"))
    {
        let _ = std::fs::remove_file(&link.key_file);
    }
    told
}

/// A link that ends (or starts again) waits on no timer of the one before it.
fn forget_retry_timers(server_id: Option<&str>) {
    if let Some(server_id) = server_id {
        connect_back_tries()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(server_id);
        backoff()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(server_id);
    }
}

/// After `ghostex computers add` here linked this computer to that one: once, ask it to pair back
/// with a code only its device key can spend; its pairing then waits for this computer's owner.
fn maybe_connect_back(state: &AppState, link: &OutgoingLink, server_id: &str) {
    if !link.connect_back || link.reverse.as_ref().is_some_and(|r| r.error.is_none()) {
        return;
    }
    // Already linked back (a pin for it here): nothing to ask.
    if links::read(&state.paths)
        .pin_for_server(server_id)
        .is_some()
    {
        return;
    }
    // Retried after a wait, and not after repeated failures (review of #674, finding 6).
    {
        let mut tries = connect_back_tries()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (last, failures) = tries
            .get(server_id)
            .copied()
            .unwrap_or((Instant::now() - CONNECT_BACK_RETRY, 0));
        if failures >= CONNECT_BACK_MAX_TRIES
            || (failures > 0 && last.elapsed() < CONNECT_BACK_RETRY)
        {
            return;
        }
        tries.insert(server_id.to_string(), (Instant::now(), failures + 1));
    }
    let Some(peer_key) = link.peer_device_key.clone() else {
        return;
    };
    let outcome = (|| -> std::result::Result<Value, String> {
        // The tunnel first: this computer's pairing door opens only once that computer can be
        // asked through it.
        let port = ensure_tunnel(link, server_id).map_err(|error| format!("{error:#}"))?;
        let code = this_computer_code(state, &peer_key)?;
        ask(
            &state.paths,
            port,
            &state.metadata.server_id,
            server_id,
            "connectBack",
            false,
            Some(code),
        )
        .map_err(|(_, message)| message)
    })();
    if outcome.is_ok() {
        // Asked: not asked again unless it fails later on.
        if let Some(entry) = connect_back_tries()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(server_id)
        {
            entry.1 = CONNECT_BACK_MAX_TRIES;
        }
    }
    let reverse = match outcome {
        Ok(answer) => ReverseAsk {
            words: answer
                .get("words")
                .and_then(Value::as_array)
                .map(|words| {
                    words
                        .iter()
                        .filter_map(|word| word.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            asked_at_ms: crate::security::now_ms(),
            error: None,
        },
        Err(error) => ReverseAsk {
            words: Vec::new(),
            asked_at_ms: crate::security::now_ms(),
            error: Some(error),
        },
    };
    let address = link.address.clone();
    links::update(&state.paths, |file| {
        if let Some(saved) = file.outgoing.iter_mut().find(|l| l.address == address) {
            saved.reverse = Some(reverse);
        }
    });
}

/// This computer's Easy Connect code for `peer_key` only, with the pairing door open for it.
fn this_computer_code(
    state: &AppState,
    peer_key: &str,
) -> std::result::Result<ConnectBackCode, String> {
    crate::tailcat::hold_tailcat_pairing_window_open(&state.paths, &state.tailcat_runtime);
    let _ = state.tailcat_runtime.wait_for_open_door(DOOR_READY_TIMEOUT);
    let status =
        crate::tailcat::read_tailcat_status_payload_for_paths(&state.paths, &state.tailcat_runtime);
    let address = super::easy_connect_addresses::public_pairing_address(&status).ok_or_else(|| {
        "Easy Connect is off on this computer. Turn it on (ghostex tailcat enable), then try again."
            .to_string()
    })?;
    let identity = super::probe_cache::cached_remote_access_identity()
        .map_err(|error| format!("{error:#}"))?;
    Ok(ConnectBackCode {
        address,
        port: crate::tailcat::tailcat_gxserver_api_port(),
        secret: state
            .remote_pairing_runtime
            .bind_secret(peer_key, crate::security::now_ms()),
        name: identity.computer_name,
    })
}

/// `/api/serverLinks` for this computer's own CLI and apps: `list`, `add`, `remove`,
/// `setAgentAccess`.
pub(crate) fn answer_local(
    state: &AppState,
    caller: &Principal,
    params: &Value,
) -> std::result::Result<Value, (u16, String)> {
    let paths = &state.paths;
    let op = params.get("op").and_then(Value::as_str).unwrap_or("list");
    let server_id_param = || {
        params
            .get("serverId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .ok_or((400, "serverId is required.".to_string()))
    };
    // Opening a door (a new link, Agent access back on) is for this computer's own clients only:
    // the token file in your terminal, the app's owner session or a paired phone, never an
    // agent's session token (Fable 5.2's review; `may_show_pairing_code`'s rule).
    let may_open =
        || super::http::may_show_pairing_code(&state.security, caller, crate::security::now_ms());
    match op {
        "list" => Ok(list(state)),
        "add" => {
            if !may_open() {
                return Err((
                    403,
                    "Run ghostex computers add in your own terminal, not in a Ghostex session."
                        .to_string(),
                ));
            }
            let text = params
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let code = super::pairing_code::decode_easy_connect_code_text(text)
                .ok_or((400, "That isn't a computer's code. On the other computer run ghostex computer-code (or click Connect a Computer) and paste the code it shows.".to_string()))?;
            let Some(secret) = code.secret.clone() else {
                return Err((400, "That code can't pair a computer by itself. On the other computer run ghostex computer-code and paste the code it prints.".to_string()));
            };
            let code = ComputerPairingCode {
                address: code.address,
                port: code.port,
                secret,
                name: code.name,
            };
            start_link_pairing(paths, &state.metadata.server_id, code, true, None, None)
                .map_err(|error| (400, format!("{error:#}")))
        }
        "remove" => {
            let server_id = server_id_param()?;
            remove(state, &server_id)
        }
        "setAgentAccess" => {
            let server_id = server_id_param()?;
            let on = params.get("on").and_then(Value::as_bool).unwrap_or(true);
            if on && !may_open() {
                return Err((403, "Only you can turn Agent access back on: in Ghostex, on your phone, or with ghostex in your own terminal.".to_string()));
            }
            let known = {
                let file = links::read(paths);
                file.pin_for_server(&server_id).is_some()
                    || file.outgoing_for_server(&server_id).is_some()
            };
            if !known {
                return Err((404, "No server link names that computer.".to_string()));
            }
            links::update(paths, |file| {
                if on {
                    file.access_off.remove(&server_id);
                } else {
                    file.access_off.insert(server_id.clone());
                }
            });
            if !on {
                crate::connected_computers::revoke_grant(
                    &server_id,
                    false,
                    Duration::from_secs(10),
                );
                drop_tunnel(&server_id);
            } else if let Some(open) = live()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_mut(&server_id)
            {
                open.last_grant = None;
            }
            crate::audit::record(
                crate::audit::Action::new(
                    "computers.serverLinkAccess",
                    if on {
                        "Turned Agent access back on for a server link"
                    } else {
                        "Turned Agent access off for a server link"
                    },
                )
                .target(server_id.clone()),
                crate::audit::Actor::current(),
                crate::audit::Outcome::Done,
                None,
            );
            wake();
            Ok(list(state))
        }
        _ => Err((400, "Unknown op.".to_string())),
    }
}

fn remove(state: &AppState, server_id: &str) -> std::result::Result<Value, (u16, String)> {
    let paths = &state.paths;
    let file = links::read(paths);
    let pin = file.pin_for_server(server_id).map(|(id, _)| id.clone());
    let outgoing = file.outgoing_for_server(server_id).cloned();
    if pin.is_none() && outgoing.is_none() {
        return Err((404, "No server link names that computer.".to_string()));
    }
    // Tell it first, while this side's tunnel is still there.
    let told = outgoing
        .as_ref()
        .is_some_and(|link| forget_link(paths, link, true));
    if let Some(device_id) = pin {
        let link_pin = links::pin_of(paths, &device_id);
        let db =
            crate::storage::open_gxserver_database(paths).map_err(|e| (500, format!("{e:#}")))?;
        if let Some(removed) = super::paired_devices::remove_paired_device(
            paths,
            &db,
            &state.tailcat_runtime,
            &state.device_api_tokens,
            &device_id,
        )
        .map_err(|e| (500, format!("{e:#}")))?
        {
            links::on_device_removed(paths, &removed.id, link_pin, false);
        }
    }
    links::update(paths, |file| {
        file.access_off.remove(server_id);
    });
    crate::connected_computers::revoke_grant(server_id, false, Duration::from_secs(10));
    crate::connected_computers::unregister(server_id);
    crate::audit::record(
        crate::audit::Action::new("pairing.remove", "Ended a server link")
            .target(server_id.to_string()),
        crate::audit::Actor::current(),
        crate::audit::Outcome::Done,
        None,
    );
    let mut answer = list(state);
    // Whether the other computer heard it now; otherwise it ends its side when it next asks.
    answer["told"] = json!(told);
    Ok(answer)
}

/// Every server link here, both ways, by the other computer's server id.
fn list(state: &AppState) -> Value {
    let file = links::read(&state.paths);
    let mut rows: Vec<Value> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for link in &file.outgoing {
        let server_id = link.server_id.clone().unwrap_or_default();
        let pin = file.pin_for_server(&server_id);
        seen.push(server_id.clone());
        rows.push(json!({
            "serverId": server_id,
            "name": link.name,
            "state": link.state,
            "words": link.words,
            "error": link.error,
            // This side reaches it (its agents can use that computer).
            "outgoing": link.state == LinkState::Linked,
            "reachable": crate::connected_computers::find(&server_id).is_some_and(|c| c.agent_access),
            // It reaches this side (pinned here, and its grant is live).
            "incoming": pin.is_some(),
            "incomingLive": crate::connected_computers::has_grant(&server_id),
            "deviceId": pin.map(|(id, _)| id.clone()),
            "reverse": link.reverse.as_ref().map(|reverse| json!({
                "words": reverse.words,
                "error": reverse.error,
            })),
            "agentAccess": !file.access_off.contains(&server_id),
        }));
    }
    for (device_id, pin) in &file.door {
        if seen.contains(&pin.server_id) {
            continue;
        }
        rows.push(json!({
            "serverId": pin.server_id,
            "name": pin.name,
            "state": "incomingOnly",
            "outgoing": false,
            "reachable": false,
            "incoming": true,
            "incomingLive": crate::connected_computers::has_grant(&pin.server_id),
            "deviceId": device_id,
            "agentAccess": !file.access_off.contains(&pin.server_id),
        }));
    }
    json!({
        "serverId": state.metadata.server_id,
        // The name the other computer's cards show for this one (its pairing ask's own name).
        "name": super::probe_cache::cached_remote_access_identity()
            .map(|identity| identity.computer_name)
            .unwrap_or_else(|_| crate::connected_computers::this_computer_name()),
        "links": rows,
    })
}
