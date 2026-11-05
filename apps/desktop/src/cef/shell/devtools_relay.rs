//! Serves the DevTools protocol of this app's own pages to gxserver, which hands it to
//! `ghostex browser mcp` (the `$ghostex-embedded-browser-use` skill) with its token, so CEF's
//! remote-debugging port can stay closed.
//!
//! CDXC:Browser 2026-10-25 DECISION:
//! User (security audit, Q11 "also move the skill off the open port"): see server/src/server/cef_devtools.rs for the route and the wire. This side joins gxserver as the DevTools host with the main token, lists every page CEF shows (its browser id, URL and title), and runs each session through CEF's in-process DevTools API (`SendDevToolsMessage` and a message observer), on the UI thread where CEF wants it. Ghostex's own code sends DevTools commands with small, counting ids (page appearance, screenshots), so a session's command ids are renumbered from 1,500,000,000 and its replies matched back by id; replies to Ghostex's own commands never leave the app. One session per page: a new one replaces the old.
//! A page is listed under the DevTools target id the app learned for it (page_target_id.rs), the id
//! `listBrowserPages` reports, so `ghostex browser mcp` can tell an agent's own project's pages from
//! the rest (agent_pages.rs, CDXC:Browser 2026-10-31); a page whose id is not known yet is listed
//! under its CEF browser id, and either id attaches.
use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, ImplNavigationEntry as _,
    WrapDevToolsMessageObserver, wrap_dev_tools_message_observer,
};
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest as _, http::HeaderValue, protocol::WebSocketConfig,
};

/// Screenshots of long pages arrive as one base64 message.
const RELAY_MESSAGE_BYTES: usize = 128 * 1024 * 1024;
/// The first id a session's commands get, above Ghostex's own counter.
const RELAY_FIRST_MESSAGE_ID: i64 = 1_500_000_000;

/// The sender to the current gxserver connection; `None` while disconnected.
static RELAY_OUTBOX: Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>> = Mutex::new(None);
static RELAY_STARTED: AtomicBool = AtomicBool::new(false);

struct RelaySession {
    browser_id: c_int,
    browser: cef::Browser,
}

thread_local! {
    static RELAY_SESSIONS: RefCell<HashMap<u64, RelaySession>> = RefCell::new(HashMap::new());
    /// One observer per browser that has a session.
    static RELAY_OBSERVERS: RefCell<HashMap<c_int, cef::Registration>> = RefCell::new(HashMap::new());
    /// Renumbered command id -> (session, the id the client used).
    static RELAY_PENDING: RefCell<HashMap<i64, (u64, Value)>> = RefCell::new(HashMap::new());
    static RELAY_NEXT_ID: Cell<i64> = const { Cell::new(RELAY_FIRST_MESSAGE_ID) };
    /// CEF browser id -> the DevTools target id the app learned for that page.
    static RELAY_TARGET_IDS: RefCell<HashMap<c_int, String>> = RefCell::new(HashMap::new());
}

/// The app learned a page's DevTools target id (page_target_id.rs); the page is listed under it.
pub(crate) fn cef_devtools_relay_note_target_id(browser_id: c_int, target_id: &str) {
    RELAY_TARGET_IDS.with(|ids| ids.borrow_mut().insert(browser_id, target_id.to_string()));
}

/// The id a page is listed under: its DevTools target id once known, else its CEF browser id.
fn relay_page_id(browser: &cef::Browser) -> String {
    let browser_id = browser.identifier();
    RELAY_TARGET_IDS
        .with(|ids| ids.borrow().get(&browser_id).cloned())
        .unwrap_or_else(|| browser_id.to_string())
}

fn relay_send(frame: Value) {
    if let Some(sender) = RELAY_OUTBOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
    {
        let _ = sender.send(frame.to_string());
    }
}

/// Starts the connection to the local gxserver once; it reconnects whenever gxserver restarts.
pub(crate) fn start_cef_devtools_relay() {
    if RELAY_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("cef-devtools-relay".into())
        .spawn(|| {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                let mut backoff = std::time::Duration::from_secs(1);
                loop {
                    let connected_for = relay_connection().await;
                    *RELAY_OUTBOX
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                    post_relay_frame(json!({ "type": "detachAll" }).to_string());
                    if connected_for > std::time::Duration::from_secs(10) {
                        backoff = std::time::Duration::from_secs(1);
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(std::time::Duration::from_secs(15));
                }
            });
        });
}

/// One connection to gxserver as the DevTools host; returns how long it lasted.
async fn relay_connection() -> std::time::Duration {
    let started = Instant::now();
    let Ok(token) = crate::app::helpers::read_gpui_gxserver_auth_token() else {
        return started.elapsed();
    };
    let port = crate::app::helpers::gpui_local_gxserver_api_port();
    let connect = async {
        ghostex_paths::loopback_owner::check_host_port("127.0.0.1", port)?;
        let mut request =
            format!("ws://127.0.0.1:{port}/api/cefDevtools/host").into_client_request()?;
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}"))?,
        );
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(RELAY_MESSAGE_BYTES))
            .max_frame_size(Some(RELAY_MESSAGE_BYTES));
        let (socket, _) =
            tokio_tungstenite::client_async_with_config(request, stream, Some(config)).await?;
        Ok::<_, anyhow::Error>(socket)
    };
    let Ok(Ok(socket)) = tokio::time::timeout(std::time::Duration::from_secs(10), connect).await
    else {
        return started.elapsed();
    };
    let (sender, mut outgoing) = tokio::sync::mpsc::unbounded_channel::<String>();
    *RELAY_OUTBOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sender);
    let (mut socket_tx, mut socket_rx) = socket.split();
    let write = async {
        while let Some(frame) = outgoing.recv().await {
            if socket_tx.send(Message::Text(frame.into())).await.is_err() {
                break;
            }
        }
    };
    let read = async {
        while let Some(Ok(message)) = socket_rx.next().await {
            match message {
                Message::Text(text) => post_relay_frame(text.to_string()),
                Message::Close(_) => break,
                _ => {}
            }
        }
    };
    tokio::select! {
        _ = write => {},
        _ = read => {},
    }
    started.elapsed()
}

wrap_task! {
    struct GhostexDevtoolsRelayFrame {
        frame: String,
    }

    impl Task {
        fn execute(&self) {
            handle_relay_frame(&self.frame);
        }
    }
}

/// CEF objects are used on its UI thread, so each frame from gxserver is handled there.
fn post_relay_frame(frame: String) {
    let mut task = GhostexDevtoolsRelayFrame::new(frame);
    post_task(ThreadId::UI, Some(&mut task));
}

fn handle_relay_frame(frame: &str) {
    let Ok(frame) = serde_json::from_str::<Value>(frame) else {
        return;
    };
    let session = frame.get("session").and_then(Value::as_u64);
    match frame.get("type").and_then(Value::as_str) {
        Some("list") => {
            let request_id = frame.get("requestId").cloned().unwrap_or(Value::Null);
            relay_send(json!({ "type": "pages", "requestId": request_id, "pages": relay_pages() }));
        }
        Some("attach") => {
            let page_id = frame.get("pageId").and_then(Value::as_str).unwrap_or("");
            if let Some(session) = session {
                attach_session(session, page_id);
            }
        }
        Some("send") => {
            if let (Some(session), Some(message)) =
                (session, frame.get("message").and_then(Value::as_str))
            {
                send_to_page(session, message);
            }
        }
        Some("detach") => {
            if let Some(session) = session {
                detach_session(session);
            }
        }
        Some("detachAll") => {
            let sessions: Vec<u64> =
                RELAY_SESSIONS.with(|sessions| sessions.borrow().keys().copied().collect());
            for session in sessions {
                detach_session(session);
            }
        }
        _ => {}
    }
}

/// Every browser CEF shows, once each, except DevTools windows.
fn relay_browsers() -> Vec<cef::Browser> {
    let mut seen = HashSet::new();
    CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
        browsers
            .borrow()
            .values()
            .filter(|browser| seen.insert(browser.identifier()))
            .cloned()
            .collect()
    })
}

fn browser_url(browser: &cef::Browser) -> String {
    browser
        .main_frame()
        .map(|frame| CefString::from(&frame.url()).to_string())
        .unwrap_or_default()
}

fn relay_pages() -> Vec<Value> {
    relay_browsers()
        .into_iter()
        .filter_map(|browser| {
            let url = browser_url(&browser);
            if url.starts_with("devtools://") {
                return None;
            }
            let title = browser
                .host()
                .and_then(|host| host.visible_navigation_entry())
                .map(|entry| CefString::from(&entry.title()).to_string())
                .unwrap_or_default();
            Some(json!({ "id": relay_page_id(&browser), "title": title, "url": url }))
        })
        .collect()
}

fn close_session(session: u64, reason: &str) {
    detach_session(session);
    relay_send(json!({ "type": "closed", "session": session, "reason": reason }));
}

fn attach_session(session: u64, page_id: &str) {
    let browser = relay_browsers().into_iter().find(|browser| {
        relay_page_id(browser) == page_id || browser.identifier().to_string() == page_id
    });
    let Some(browser) = browser else {
        relay_send(json!({ "type": "closed", "session": session, "reason": "No such page." }));
        return;
    };
    let browser_id = browser.identifier();
    let previous: Vec<u64> = RELAY_SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .iter()
            .filter(|(_, attached)| attached.browser_id == browser_id)
            .map(|(id, _)| *id)
            .collect()
    });
    for previous in previous {
        close_session(previous, "Another DevTools session took this page.");
    }
    let observed = RELAY_OBSERVERS.with(|observers| observers.borrow().contains_key(&browser_id));
    if !observed {
        let Some(registration) = browser.host().and_then(|host| {
            let mut observer = RelayObserver::new(browser_id);
            host.add_dev_tools_message_observer(Some(&mut observer))
        }) else {
            relay_send(
                json!({ "type": "closed", "session": session, "reason": "The page cannot be inspected." }),
            );
            return;
        };
        RELAY_OBSERVERS.with(|observers| observers.borrow_mut().insert(browser_id, registration));
    }
    RELAY_SESSIONS.with(|sessions| {
        sessions.borrow_mut().insert(
            session,
            RelaySession {
                browser_id,
                browser,
            },
        )
    });
}

fn send_to_page(session: u64, message: &str) {
    let Some(browser) =
        RELAY_SESSIONS.with(|sessions| sessions.borrow().get(&session).map(|s| s.browser.clone()))
    else {
        return;
    };
    let Ok(mut command) = serde_json::from_str::<Value>(message) else {
        return;
    };
    let Some(client_id) = command.get("id").cloned() else {
        return;
    };
    let relay_id = RELAY_NEXT_ID.with(|next| {
        let id = next.get();
        next.set(if id >= i64::from(c_int::MAX) {
            RELAY_FIRST_MESSAGE_ID
        } else {
            id + 1
        });
        id
    });
    command["id"] = json!(relay_id);
    RELAY_PENDING.with(|pending| pending.borrow_mut().insert(relay_id, (session, client_id)));
    let sent = browser
        .host()
        .is_some_and(|host| host.send_dev_tools_message(Some(command.to_string().as_bytes())) != 0);
    if !sent {
        RELAY_PENDING.with(|pending| pending.borrow_mut().remove(&relay_id));
        close_session(session, "The page closed.");
    }
}

fn detach_session(session: u64) {
    let Some(detached) = RELAY_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&session))
    else {
        return;
    };
    RELAY_PENDING.with(|pending| {
        pending
            .borrow_mut()
            .retain(|_, (owner, _)| *owner != session)
    });
    let still_attached = RELAY_SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .values()
            .any(|attached| attached.browser_id == detached.browser_id)
    });
    if !still_attached {
        RELAY_OBSERVERS.with(|observers| observers.borrow_mut().remove(&detached.browser_id));
    }
}

/// A browser went away: its session ends.
pub(crate) fn cef_devtools_relay_browser_closed(browser_id: c_int) {
    RELAY_TARGET_IDS.with(|ids| ids.borrow_mut().remove(&browser_id));
    let sessions: Vec<u64> = RELAY_SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .iter()
            .filter(|(_, attached)| attached.browser_id == browser_id)
            .map(|(id, _)| *id)
            .collect()
    });
    for session in sessions {
        close_session(session, "The page closed.");
    }
}

wrap_dev_tools_message_observer! {
    struct RelayObserver {
        browser_id: c_int,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(
            &self,
            _browser: Option<&mut cef::Browser>,
            message: Option<&[u8]>,
        ) -> c_int {
            let Some(mut parsed) = message.and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            else {
                return 0;
            };
            let session = match parsed.get("id").and_then(Value::as_i64) {
                // A reply: only to a command a session sent; Ghostex's own stay here.
                Some(id) => {
                    let Some((session, client_id)) =
                        RELAY_PENDING.with(|pending| pending.borrow_mut().remove(&id))
                    else {
                        return 0;
                    };
                    parsed["id"] = client_id;
                    session
                }
                // An event: to the page's session.
                None => {
                    let Some(session) = RELAY_SESSIONS.with(|sessions| {
                        sessions
                            .borrow()
                            .iter()
                            .find(|(_, attached)| attached.browser_id == self.browser_id)
                            .map(|(id, _)| *id)
                    }) else {
                        return 0;
                    };
                    session
                }
            };
            relay_send(json!({ "type": "message", "session": session, "message": parsed.to_string() }));
            1
        }
    }
}
