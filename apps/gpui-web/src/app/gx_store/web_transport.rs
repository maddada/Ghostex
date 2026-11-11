//! The browser's gxserver transport: `fetch` for RPC and one `WebSocket` for the event stream, in place of the native client's `ureq` and `tungstenite` thread (`packages/gx-client`). Everything arrives on the page's one thread, so events go into a channel the GPUI task drains.
use futures::channel::mpsc::UnboundedSender;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, Request, RequestInit, Response, WebSocket};

const PROTOCOL_VERSION: u64 = 1;

/// Where the daemon is and the token that opens it, as `ghostex web` hands them out.
#[derive(Clone, Debug)]
pub(crate) struct GxserverEndpoint {
    pub(crate) base_url: String,
    pub(crate) auth_token: String,
}

thread_local! {
    static CURRENT_ENDPOINT: std::cell::RefCell<Option<GxserverEndpoint>> = const { std::cell::RefCell::new(None) };
}

/// The daemon the page is connected to, for the shared files that make their own calls.
pub(crate) fn current_endpoint() -> Option<GxserverEndpoint> {
    CURRENT_ENDPOINT.with(|endpoint| endpoint.borrow().clone())
}

pub(crate) enum StreamEvent {
    Open,
    Frame(String),
    Closed,
}

fn js_error(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::JSON::stringify(&error)
                .ok()
                .and_then(|s| s.as_string())
        })
        .unwrap_or_else(|| "unknown JavaScript error".to_string())
}

async fn post_envelope(url: &str, token: Option<&str>, body: &Value) -> Result<Value, String> {
    post_envelope_with(url, token, None, body).await
}

async fn post_envelope_with(
    url: &str,
    token: Option<&str>,
    bootstrap_secret: Option<&str>,
    body: &Value,
) -> Result<Value, String> {
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(&body.to_string()));
    let request = Request::new_with_str_and_init(url, &init).map_err(js_error)?;
    let headers = request.headers();
    headers
        .set("content-type", "application/json")
        .map_err(js_error)?;
    headers
        .set("x-gxserver-protocol-version", &PROTOCOL_VERSION.to_string())
        .map_err(js_error)?;
    if let Some(token) = token {
        headers
            .set("authorization", &format!("Bearer {token}"))
            .map_err(js_error)?;
    }
    if let Some(secret) = bootstrap_secret {
        headers
            .set("x-ghostex-web-bootstrap", secret)
            .map_err(js_error)?;
    }
    let window = web_sys::window().ok_or("no window")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    // CDXC:RemoteMachines 2026-11-11 WHY: the desktop's transport says when an answer was cut off
    // or can't be read instead of showing a parse error ("gxserver returned invalid JSON.", which
    // a user on a dropping link saw "all the time"); the page says the same instead of the
    // browser's "TypeError: network error" or serde's "EOF while parsing a value".
    let text = JsFuture::from(response.text().map_err(js_error)?)
        .await
        .map_err(|_| {
            // Only a read is safe to ask again (the desktop's `gxserver_request_is_safe_to_repeat`).
            let read = url.rsplit("/api/").next().is_some_and(|name| {
                ["read", "list", "get"]
                    .iter()
                    .any(|verb| name.starts_with(verb))
            });
            if read {
                "Ghostex's answer was cut off: the connection dropped while it arrived. Try again."
            } else {
                "Ghostex's answer didn't arrive: the connection dropped, so what you asked may or may not have happened. Check, then try again if it didn't."
            }
            .to_string()
        })?
        .as_string()
        .unwrap_or_default();
    serde_json::from_str(&text)
        .map_err(|_| "Ghostex's answer couldn't be read. Try again.".to_string())
}

async fn post_json(url: &str, token: Option<&str>, body: &Value) -> Result<Value, String> {
    let envelope = post_envelope(url, token, body).await?;
    if envelope["ok"] == true {
        Ok(envelope["result"].clone())
    } else {
        Err(envelope["message"]
            .as_str()
            .or_else(|| envelope["error"].as_str())
            .unwrap_or("request failed")
            .to_string())
    }
}

/// The per-run key `ghostex web` put in the page's address (`#bootstrap=…`). It is kept in this
/// tab's sessionStorage so a reload still connects, and taken out of the address bar.
fn bootstrap_secret() -> Option<String> {
    const KEY: &str = "ghostex-web-bootstrap";
    let window = web_sys::window()?;
    let storage = window.session_storage().ok().flatten();
    let location = window.location();
    let hash = location.hash().unwrap_or_default();
    if let Some(secret) = hash
        .strip_prefix("#bootstrap=")
        .filter(|secret| !secret.is_empty())
    {
        if let Some(storage) = &storage {
            let _ = storage.set_item(KEY, secret);
        }
        let without_key = format!(
            "{}{}",
            location.pathname().unwrap_or_default(),
            location.search().unwrap_or_default()
        );
        // history.replaceState through Reflect: no reload, no history entry, no extra web-sys feature.
        if let Ok(history) = js_sys::Reflect::get(&window, &JsValue::from_str("history"))
            && let Ok(replace) = js_sys::Reflect::get(&history, &JsValue::from_str("replaceState"))
            && let Some(replace) = replace.dyn_ref::<js_sys::Function>()
        {
            let _ = replace.call3(
                &history,
                &JsValue::NULL,
                &JsValue::from_str(""),
                &JsValue::from_str(&without_key),
            );
        }
        return Some(secret.to_string());
    }
    storage?
        .get_item(KEY)
        .ok()
        .flatten()
        .filter(|secret| !secret.is_empty())
}

/// Asks the page's own server (`ghostex web`, or the dev server proxying to it) for the daemon.
pub(crate) async fn bootstrap() -> Result<GxserverEndpoint, String> {
    // A Ghostex Collab guest has no token of any kind: its calls go to the guest endpoint, which
    // the page carries to the owner's computer (`www/src/collab/tunnel.js`).
    if let Some(guest) = crate::app::collab_guest::guest() {
        let endpoint = GxserverEndpoint {
            base_url: guest.base_url,
            auth_token: "collab-guest".to_string(),
        };
        CURRENT_ENDPOINT.with(|current| *current.borrow_mut() = Some(endpoint.clone()));
        return Ok(endpoint);
    }
    let secret = bootstrap_secret().ok_or(
        "Open Ghostex Web with the link `ghostex web` printed; it carries this run's key.",
    )?;
    let envelope = post_envelope_with("/api/webBootstrap", None, Some(&secret), &json!({})).await?;
    if envelope["ok"] != true {
        return Err(envelope["message"]
            .as_str()
            .or_else(|| envelope["error"].as_str())
            .unwrap_or("Ghostex Web refused this page's key; open the link `ghostex web` printed.")
            .to_string());
    }
    let result = envelope["result"].clone();
    let endpoint = GxserverEndpoint {
        base_url: result["baseUrl"]
            .as_str()
            .ok_or("bootstrap has no baseUrl")?
            .to_string(),
        auth_token: result["authToken"]
            .as_str()
            .ok_or("bootstrap has no authToken")?
            .to_string(),
    };
    CURRENT_ENDPOINT.with(|current| *current.borrow_mut() = Some(endpoint.clone()));
    // The built-in features on for this computer; an older `ghostex web` sends none (all off).
    crate::app::read_aloud::set_computer_features(
        result["builtInFeatures"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str().map(str::to_string))
            .collect(),
    );
    Ok(endpoint)
}

/// One gxserver RPC: `POST {base}{path}` with the `{params, protocolVersion}` envelope.
pub(crate) async fn rpc(
    endpoint: &GxserverEndpoint,
    path: &str,
    params: Value,
) -> Result<Value, String> {
    post_json(
        &format!("{}{path}", endpoint.base_url),
        Some(&endpoint.auth_token),
        &json!({"params": params, "protocolVersion": PROTOCOL_VERSION}),
    )
    .await
}

/// One gxserver RPC for callers that need the daemon's error CODE as well as its message (the chat composer branches on `composerNotReady` and `sendCancelled`).
pub(crate) async fn rpc_envelope(
    endpoint: &GxserverEndpoint,
    path: &str,
    params: Value,
) -> Result<Value, Value> {
    let fail = |message: String| json!({"message": message, "endpoint": path});
    let body = json!({"params": params, "protocolVersion": PROTOCOL_VERSION});
    let started_ms = js_sys::Date::now();
    let mut envelope = loop {
        let envelope = post_envelope(
            &format!("{}{path}", endpoint.base_url),
            Some(&endpoint.auth_token),
            &body,
        )
        .await
        .map_err(fail)?;
        // A gxserver still starting is waited out, as a request in its listen backlog used to be
        // (packages/gx-protocol/src/starting.rs), up to the desktop's `gx_rpc` limit.
        match ghostex_gx_protocol::server_starting_retry_after_value(&envelope) {
            Some(delay)
                if js_sys::Date::now() - started_ms + (delay.as_millis() as f64)
                    < STARTING_WAIT_LIMIT_MS =>
            {
                sleep_ms(delay.as_millis() as i32).await
            }
            _ => break envelope,
        }
    };
    if envelope["ok"] != true {
        return Err(
            json!({"code": envelope["error"], "message": envelope["message"], "endpoint": path, "approval": envelope["approval"]}),
        );
    }
    Ok(envelope["result"].take())
}

/// How long a request waits for a gxserver that is still starting: the desktop's `gx_rpc` limit.
const STARTING_WAIT_LIMIT_MS: f64 = 60_000.0;

/// Resolves after `ms` milliseconds on the page's own timer.
async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// Opens `/api/events`. Browsers cannot set headers on a WebSocket, so the token rides the query string. The returned socket must be kept alive by the caller.
pub(crate) fn open_events(
    endpoint: &GxserverEndpoint,
    events: UnboundedSender<StreamEvent>,
) -> Result<WebSocket, String> {
    let base = endpoint.base_url.replacen("http", "ws", 1);
    let url = format!(
        "{base}/api/events?authToken={}&protocolVersion={PROTOCOL_VERSION}",
        js_sys::encode_uri_component(&endpoint.auth_token)
    );
    let socket = WebSocket::new(&url).map_err(js_error)?;

    let sender = events.clone();
    let on_open = Closure::<dyn FnMut()>::new(move || {
        let _ = sender.unbounded_send(StreamEvent::Open);
    });
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    on_open.forget();

    let sender = events.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            let _ = sender.unbounded_send(StreamEvent::Frame(text));
        }
    });
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    let on_close = Closure::<dyn FnMut()>::new(move || {
        let _ = events.unbounded_send(StreamEvent::Closed);
    });
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();

    Ok(socket)
}
