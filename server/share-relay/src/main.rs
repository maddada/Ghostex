//! Ghostex Collab's share relay (`share.ghostex.dev`).
//!
//! An owner's gxserver keeps one outgoing WebSocket here (`/h`) while Ghostex Collab is on and a
//! chat is shared, and registers each shared chat by its route id. A guest's browser opens the
//! link `/s/<page id>/#<key>` (11.0.0's links: `/s#<key>`), gets the guest page from this relay, hashes the key into the same route id and
//! connects to `/g/<route>`. The relay pairs the two and copies frames between them. It never sees
//! the key (browsers keep the `#fragment` to themselves), so it cannot read the frames: the page
//! and gxserver encrypt them end to end with keys derived from it (`server/collab/src/tunnel.rs`
//! and `apps/gpui-web/www/src/collab/tunnel.js`).
//!
//! CDXC:Collab 2026-11-07 DECISION: the user picked Ghostex's own relay at share.ghostex.dev (Q1611): "gxserver keeps one outgoing connection to it only while Collab is on and a chat is shared ... the key after `#` never reaches a server, the relay sees only its hash and carries end-to-end encrypted traffic it can't read".
//!
//! Host protocol (JSON text frames on `/h`):
//! - host → relay: `{"op":"register","route":R,"secret":S}`, `{"op":"unregister","route":R}`,
//!   `{"op":"data","conn":N,"data":"<base64>"}`, `{"op":"close","conn":N}`, `{"op":"ping"}`.
//! - relay → host: `{"op":"registered","route":R}`, `{"op":"error","route":R,"code":C}`,
//!   `{"op":"open","route":R,"conn":N}`, `{"op":"data","conn":N,"data":"<base64>"}`,
//!   `{"op":"close","conn":N}`, `{"op":"pong"}`.
//!
//! Guest pages (`--pages`): one folder per page, named by its page hash, the SHA-256 of its
//! manifest (every file's SHA-256 and path, `shasum -a 256` lines sorted by path). The relay
//! hashes each folder when it starts and serves only folders whose files match their name, at
//! `/s/<first 16 hex of the hash>/`. Each Ghostex release pins one page hash
//! (`server/collab/src/page_pin.rs`) and checks this relay serves exactly that page before it
//! shares through it, so several releases' pages live here side by side.
//!
//! A guest's frames on `/g/<route>` are binary and opaque. `S` is a random host secret; the relay
//! keeps only its SHA-256, so another party that learns a route id (a guest who has the link)
//! cannot register it in the owner's place.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path as UrlPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

/// Guests one shared chat may have at once; the owner's gxserver applies its own, lower cap.
const MAX_GUESTS_PER_ROUTE: usize = 32;
/// Routes one host connection may register.
const MAX_ROUTES_PER_HOST: usize = 64;
/// One frame: a chat's first page of history can be a few megabytes.
const MAX_FRAME_BYTES: usize = 32 << 20;
/// How long a route's host verifier outlives its host, so a guest cannot claim the route while
/// the owner's computer is briefly offline.
const VERIFIER_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

/// Close codes a guest page understands (`apps/gpui-web/www/src/collab/tunnel.js`).
const CLOSE_OFFLINE: u16 = 4404;
const CLOSE_FULL: u16 = 4429;
const CLOSE_HOST_LEFT: u16 = 4410;
const CLOSE_ENDED: u16 = 4401;

enum GuestOut {
    Data(Vec<u8>),
    Close(u16, &'static str),
}

struct Route {
    host_id: u64,
    host: mpsc::UnboundedSender<Value>,
    guests: HashMap<u64, mpsc::UnboundedSender<GuestOut>>,
}

#[derive(Default)]
struct Tables {
    routes: HashMap<String, Route>,
    /// Route of each guest connection.
    conns: HashMap<u64, String>,
    /// SHA-256 (hex) of the host secret that first registered a route, and when it was last used.
    verifiers: HashMap<String, (String, Instant)>,
}

struct Relay {
    tables: Mutex<Tables>,
    next_id: AtomicU64,
    /// 11.0.0's guest page, at `/s` and its `/assets/…`.
    web_dist: Option<PathBuf>,
    /// Pinned guest pages by page id (the hash's first 16 hex), each checked against its name.
    pages: HashMap<String, PathBuf>,
    /// A development page at `/s/dev/`, unchecked (`--dev-page`).
    dev_page: Option<PathBuf>,
    /// Behind Caddy: the client's address is the first `X-Forwarded-For` entry.
    behind_proxy: bool,
    limits: Mutex<HashMap<(IpAddr, Side), Usage>>,
    route_opens: Mutex<HashMap<String, Vec<Instant>>>,
}

impl Relay {
    fn id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }
}

fn is_route_id(route: &str) -> bool {
    route.len() == 64
        && route
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

struct Args {
    listen: SocketAddr,
    web_dist: Option<PathBuf>,
    pages: Option<PathBuf>,
    dev_page: Option<PathBuf>,
    behind_proxy: bool,
}

// ---- pinned guest pages -----------------------------------------------------------------------

/// Every file under `root` (dot files skipped), as `/`-separated relative paths.
fn page_files(root: &Path) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                folders.push(entry.path());
            } else if kind.is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                files.push(relative);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// A page folder's manifest: `<sha256>  <path>` per file, sorted by path, what
/// `find . -type f | sed 's|^\./||' | LC_ALL=C sort | xargs shasum -a 256` writes.
fn page_manifest(root: &Path) -> std::io::Result<String> {
    let mut manifest = String::new();
    for file in page_files(root)? {
        let bytes = std::fs::read(root.join(&file))?;
        manifest.push_str(&format!(
            "{}  {file}\n",
            hex::encode(Sha256::digest(&bytes))
        ));
    }
    Ok(manifest)
}

/// The page folders under `pages` whose files hash to their folder's name, by page id. A folder
/// that doesn't match is left out and named on stderr.
fn load_pages(pages: &Path) -> anyhow::Result<HashMap<String, PathBuf>> {
    let mut found = HashMap::new();
    for entry in std::fs::read_dir(pages)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.file_type()?.is_dir() || name.starts_with('.') {
            continue;
        }
        let hash = sha256_hex(&page_manifest(&entry.path())?);
        if hash != name {
            eprintln!(
                "ghostex-share-relay: not serving page folder {name}: its files hash to {hash}"
            );
            continue;
        }
        found.insert(name[..16].to_string(), entry.path());
    }
    Ok(found)
}

// ---- rate limits ------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Host,
    Guest,
}

#[derive(Default)]
struct Usage {
    opens: Vec<Instant>,
    open_now: u32,
}

/// Sockets one address may open a minute, and keep open at once, per side.
fn limits(side: Side) -> (usize, u32) {
    match side {
        Side::Guest => (60, 20),
        Side::Host => (30, 8),
    }
}
/// Guest sockets one shared chat takes a minute, whoever opens them.
const ROUTE_OPENS_PER_MINUTE: usize = 120;

/// Holds one open socket against its address's count until dropped.
struct Slot {
    relay: Arc<Relay>,
    key: (IpAddr, Side),
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut limits = self.relay.limits.lock().unwrap();
        if let Some(usage) = limits.get_mut(&self.key) {
            usage.open_now = usage.open_now.saturating_sub(1);
        }
    }
}

fn client_ip(relay: &Relay, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    if relay.behind_proxy {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|first| first.trim().parse::<IpAddr>().ok())
        {
            return ip;
        }
    }
    peer.ip()
}

fn take_slot(relay: &Arc<Relay>, ip: IpAddr, side: Side) -> Option<Slot> {
    let (per_minute, at_once) = limits(side);
    let mut limits = relay.limits.lock().unwrap();
    let usage = limits.entry((ip, side)).or_default();
    usage
        .opens
        .retain(|at| at.elapsed() < Duration::from_secs(60));
    if usage.opens.len() >= per_minute || usage.open_now >= at_once {
        return None;
    }
    usage.opens.push(Instant::now());
    usage.open_now += 1;
    Some(Slot {
        relay: relay.clone(),
        key: (ip, side),
    })
}

fn route_allows(relay: &Relay, route: &str) -> bool {
    let mut opens = relay.route_opens.lock().unwrap();
    let recent = opens.entry(route.to_string()).or_default();
    recent.retain(|at| at.elapsed() < Duration::from_secs(60));
    if recent.len() >= ROUTE_OPENS_PER_MINUTE {
        return false;
    }
    recent.push(Instant::now());
    true
}

fn too_many() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        "Too many connections from this address; try again in a minute.",
    )
        .into_response()
}

fn parse_args() -> anyhow::Result<Args> {
    let mut listen: SocketAddr = "127.0.0.1:58800".parse()?;
    let mut web_dist = None;
    let mut pages = None;
    let mut dev_page = None;
    let mut behind_proxy = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                listen = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--listen needs an address"))?
                    .parse()?
            }
            "--web-dist" => {
                web_dist =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--web-dist needs a folder")
                    })?))
            }
            "--pages" => {
                pages = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--pages needs a folder"))?,
                ))
            }
            "--dev-page" => {
                dev_page =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--dev-page needs a folder")
                    })?))
            }
            "--behind-proxy" => behind_proxy = true,
            "--help" | "-h" => {
                println!(
                    "ghostex-share-relay [--listen 127.0.0.1:58800] [--pages <folder of page-hash folders>] [--web-dist <11.0.0 page>] [--dev-page <gpui-web www/dist>] [--behind-proxy]\n\n\
                     Local dev: run it with --dev-page apps/gpui-web/www/dist, start a debug gxserver with\n\
                     GHOSTEX_COLLAB_UNPINNED_GUEST_PAGE=1 and point it at the relay with\n\
                     GHOSTEX_COLLAB_RELAY_URL=http://127.0.0.1:58800. Production sits behind TLS (Caddy) at\n\
                     https://share.ghostex.dev."
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    Ok(Args {
        listen,
        web_dist,
        pages,
        dev_page,
        behind_proxy,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let pages = match &args.pages {
        Some(folder) => load_pages(folder)?,
        None => HashMap::new(),
    };
    for (id, folder) in &pages {
        eprintln!(
            "ghostex-share-relay: guest page {id} from {}",
            folder.display()
        );
    }
    let relay = Arc::new(Relay {
        tables: Mutex::new(Tables::default()),
        next_id: AtomicU64::new(0),
        web_dist: args.web_dist,
        pages,
        dev_page: args.dev_page,
        behind_proxy: args.behind_proxy,
        limits: Mutex::new(HashMap::new()),
        route_opens: Mutex::new(HashMap::new()),
    });
    {
        let relay = relay.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(3600));
            loop {
                tick.tick().await;
                let mut tables = relay.tables.lock().unwrap();
                let live: Vec<String> = tables.routes.keys().cloned().collect();
                for route in live {
                    if let Some(entry) = tables.verifiers.get_mut(&route) {
                        entry.1 = Instant::now();
                    }
                }
                tables
                    .verifiers
                    .retain(|_, (_, seen)| seen.elapsed() < VERIFIER_TTL);
            }
        });
    }
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/h", get(host_upgrade))
        .route("/g/{route}", get(guest_upgrade))
        .fallback(get(static_file))
        .with_state(relay);
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    eprintln!("ghostex-share-relay listening on http://{}", args.listen);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

async fn health(State(relay): State<Arc<Relay>>) -> impl IntoResponse {
    let tables = relay.tables.lock().unwrap();
    axum::Json(json!({"ok": true, "routes": tables.routes.len(), "guests": tables.conns.len()}))
}

// ---- host side -------------------------------------------------------------------------------

async fn host_upgrade(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(relay): State<Arc<Relay>>,
) -> Response {
    let Some(slot) = take_slot(&relay, client_ip(&relay, peer, &headers), Side::Host) else {
        return too_many();
    };
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            host_session(socket, relay).await;
            drop(slot);
        })
}

async fn host_session(socket: WebSocket, relay: Arc<Relay>) {
    let host_id = relay.id();
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if sink
                .send(Message::Text(frame.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        handle_host_frame(&relay, host_id, &tx, &frame);
    }
    drop_host(&relay, host_id);
    writer.abort();
}

fn handle_host_frame(
    relay: &Relay,
    host_id: u64,
    tx: &mpsc::UnboundedSender<Value>,
    frame: &Value,
) {
    let op = frame["op"].as_str().unwrap_or_default();
    match op {
        "ping" => {
            let _ = tx.send(json!({"op": "pong"}));
        }
        "register" => {
            let route = frame["route"].as_str().unwrap_or_default();
            let secret = frame["secret"].as_str().unwrap_or_default();
            let reply = register(relay, host_id, tx, route, secret);
            let _ = tx.send(match reply {
                Ok(()) => json!({"op": "registered", "route": route}),
                Err(code) => json!({"op": "error", "route": route, "code": code}),
            });
        }
        "unregister" => {
            let route = frame["route"].as_str().unwrap_or_default();
            let mut tables = relay.tables.lock().unwrap();
            if tables
                .routes
                .get(route)
                .is_some_and(|r| r.host_id == host_id)
            {
                remove_route(&mut tables, route, CLOSE_ENDED, "This link has ended");
                tables.verifiers.remove(route);
            }
        }
        "data" => {
            let Some(conn) = frame["conn"].as_u64() else {
                return;
            };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD
                .decode(frame["data"].as_str().unwrap_or_default())
            else {
                return;
            };
            let tables = relay.tables.lock().unwrap();
            if let Some(guest) = guest_of_host(&tables, host_id, conn) {
                let _ = guest.send(GuestOut::Data(bytes));
            }
        }
        "close" => {
            let Some(conn) = frame["conn"].as_u64() else {
                return;
            };
            let mut tables = relay.tables.lock().unwrap();
            if let Some(guest) = guest_of_host(&tables, host_id, conn) {
                let _ = guest.send(GuestOut::Close(CLOSE_ENDED, "Closed by the chat's owner"));
            }
            if let Some(route) = tables.conns.remove(&conn) {
                if let Some(route) = tables.routes.get_mut(&route) {
                    route.guests.remove(&conn);
                }
            }
        }
        _ => {}
    }
}

fn guest_of_host(
    tables: &Tables,
    host_id: u64,
    conn: u64,
) -> Option<mpsc::UnboundedSender<GuestOut>> {
    let route = tables.routes.get(tables.conns.get(&conn)?)?;
    (route.host_id == host_id)
        .then(|| route.guests.get(&conn).cloned())
        .flatten()
}

fn register(
    relay: &Relay,
    host_id: u64,
    tx: &mpsc::UnboundedSender<Value>,
    route: &str,
    secret: &str,
) -> Result<(), &'static str> {
    if !is_route_id(route) || secret.len() < 32 {
        return Err("invalid");
    }
    let verifier = sha256_hex(secret);
    let mut tables = relay.tables.lock().unwrap();
    if let Some((known, _)) = tables.verifiers.get(route) {
        if *known != verifier {
            return Err("taken");
        }
    }
    if tables
        .routes
        .values()
        .filter(|r| r.host_id == host_id)
        .count()
        >= MAX_ROUTES_PER_HOST
        && !tables
            .routes
            .get(route)
            .is_some_and(|r| r.host_id == host_id)
    {
        return Err("too-many");
    }
    if tables
        .routes
        .get(route)
        .is_some_and(|r| r.host_id != host_id)
    {
        // The same owner reconnected before the relay noticed the old socket died.
        remove_route(
            &mut tables,
            route,
            CLOSE_HOST_LEFT,
            "The chat's computer reconnected",
        );
    }
    tables
        .verifiers
        .insert(route.to_string(), (verifier, Instant::now()));
    tables
        .routes
        .entry(route.to_string())
        .or_insert_with(|| Route {
            host_id,
            host: tx.clone(),
            guests: HashMap::new(),
        });
    Ok(())
}

fn remove_route(tables: &mut Tables, route: &str, code: u16, reason: &'static str) {
    if let Some(entry) = tables.routes.remove(route) {
        for (conn, guest) in entry.guests {
            let _ = guest.send(GuestOut::Close(code, reason));
            tables.conns.remove(&conn);
        }
    }
}

fn drop_host(relay: &Relay, host_id: u64) {
    let mut tables = relay.tables.lock().unwrap();
    let owned: Vec<String> = tables
        .routes
        .iter()
        .filter(|(_, r)| r.host_id == host_id)
        .map(|(route, _)| route.clone())
        .collect();
    for route in owned {
        if let Some(entry) = tables.verifiers.get_mut(&route) {
            entry.1 = Instant::now();
        }
        remove_route(
            &mut tables,
            &route,
            CLOSE_HOST_LEFT,
            "The chat's computer went offline",
        );
    }
}

// ---- guest side ------------------------------------------------------------------------------

async fn guest_upgrade(
    ws: WebSocketUpgrade,
    UrlPath(route): UrlPath<String>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(relay): State<Arc<Relay>>,
) -> Response {
    if !is_route_id(&route) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(slot) = take_slot(&relay, client_ip(&relay, peer, &headers), Side::Guest) else {
        return too_many();
    };
    if !route_allows(&relay, &route) {
        return too_many();
    }
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            guest_session(socket, relay, route).await;
            drop(slot);
        })
}

async fn guest_session(socket: WebSocket, relay: Arc<Relay>, route: String) {
    let (mut sink, mut stream) = socket.split();
    let conn = relay.id();
    let (tx, mut rx) = mpsc::unbounded_channel::<GuestOut>();
    let admitted: Result<mpsc::UnboundedSender<Value>, (u16, &'static str)> = {
        let mut tables = relay.tables.lock().unwrap();
        match tables.routes.get_mut(&route) {
            None => Err((
                CLOSE_OFFLINE,
                "This link has ended, or the chat's computer is offline",
            )),
            Some(entry) if entry.guests.len() >= MAX_GUESTS_PER_ROUTE => {
                Err((CLOSE_FULL, "This chat has too many guests"))
            }
            Some(entry) => {
                entry.guests.insert(conn, tx.clone());
                let host = entry.host.clone();
                tables.conns.insert(conn, route.clone());
                let _ = host.send(json!({"op": "open", "route": route, "conn": conn}));
                Ok(host)
            }
        }
    };
    let host = match admitted {
        Ok(host) => host,
        Err((code, reason)) => {
            let _ = sink
                .send(Message::Close(Some(CloseFrame {
                    code,
                    reason: reason.into(),
                })))
                .await;
            return;
        }
    };
    let writer = tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                GuestOut::Data(bytes) => {
                    if sink.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                GuestOut::Close(code, reason) => {
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame {
                            code,
                            reason: reason.into(),
                        })))
                        .await;
                    break;
                }
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        let bytes = match message {
            Message::Binary(bytes) => bytes.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };
        let _ = host.send(json!({
            "op": "data",
            "conn": conn,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        }));
    }
    let still_listed = {
        let mut tables = relay.tables.lock().unwrap();
        let listed = tables.conns.remove(&conn).is_some();
        if let Some(entry) = tables.routes.get_mut(&route) {
            entry.guests.remove(&conn);
        }
        listed
    };
    if still_listed {
        let _ = host.send(json!({"op": "close", "conn": conn}));
    }
    writer.abort();
}

// ---- the guest page --------------------------------------------------------------------------

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// Serves the guest pages: `/s/<page id>/…` from that pinned page's folder (its `index.html` for
/// the folder itself), `/s/dev/…` from `--dev-page`, and 11.0.0's page at `/s` and `/assets/…`.
async fn static_file(State(relay): State<Arc<Relay>>, uri: Uri) -> Response {
    let request = uri.path().trim_start_matches('/');
    let versioned = request.strip_prefix("s/").and_then(|rest| {
        let (id, file) = rest.split_once('/').unwrap_or((rest, ""));
        if id.is_empty() {
            return None;
        }
        let root = if id == "dev" {
            relay.dev_page.as_ref()
        } else {
            relay.pages.get(id)
        };
        Some((id, root, file))
    });
    let (root, relative) = match versioned {
        Some((id, Some(_), file)) if file.is_empty() && !request.ends_with('/') => {
            // `/s/<id>` → `/s/<id>/`, so the page's relative files resolve inside its folder;
            // the browser keeps the link's `#key` across the redirect.
            let mut response = StatusCode::PERMANENT_REDIRECT.into_response();
            if let Ok(location) = HeaderValue::from_str(&format!("/s/{id}/")) {
                response.headers_mut().insert(header::LOCATION, location);
            }
            return response;
        }
        Some((_, Some(root), file)) => (root, if file.is_empty() { "index.html" } else { file }),
        Some((_, None, _)) => {
            return (
                StatusCode::NOT_FOUND,
                "This share link needs a guest page this relay doesn't have. Ask for a new link.",
            )
                .into_response()
        }
        None => {
            let Some(root) = relay.web_dist.as_ref() else {
                return (StatusCode::NOT_FOUND, "No guest page on this relay").into_response();
            };
            (
                root,
                if request.is_empty() || request == "s" || request == "s/" {
                    "index.html"
                } else {
                    request
                },
            )
        }
    };
    let relative = Path::new(relative);
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = root.join(relative);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(&path)),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if relative == Path::new("index.html") {
            "no-store"
        } else if versioned.is_some() {
            // A pinned page's files never change under their folder.
            "public, max-age=86400, immutable"
        } else {
            "public, max-age=300"
        }),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_folders_are_served_only_under_their_own_hash() {
        let temp = std::env::temp_dir().join(format!("gx-relay-pages-{}", std::process::id()));
        let page = temp.join("staging");
        std::fs::create_dir_all(page.join("assets")).unwrap();
        std::fs::write(page.join("index.html"), "<html></html>").unwrap();
        std::fs::write(page.join("assets/a.js"), "1").unwrap();
        let manifest = page_manifest(&page).unwrap();
        assert_eq!(
            manifest,
            format!(
                "{}  assets/a.js\n{}  index.html\n",
                sha256_hex("1"),
                sha256_hex("<html></html>")
            )
        );
        let hash = sha256_hex(&manifest);
        std::fs::rename(&page, temp.join(&hash)).unwrap();
        let pages = load_pages(&temp).unwrap();
        assert_eq!(pages.get(&hash[..16]), Some(&temp.join(&hash)));
        // A changed file: the folder no longer matches its name and is not served.
        std::fs::write(temp.join(&hash).join("assets/a.js"), "2").unwrap();
        assert!(load_pages(&temp).unwrap().is_empty());
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
