//! `/api/events` over a real loopback WebSocket: what each `stream` mode delivers, what its
//! client commands may start, and that a closed socket leaves nothing subscribed.

use super::*;
use axum::{routing::any, Router};
use tokio_tungstenite::tungstenite::{self, Message as WsMessage};

type Client =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// How long a socket must stay silent before a test concludes nothing more is coming.
const QUIET: Duration = Duration::from_millis(500);
/// How long a frame the test expects may take (the snapshot is built on a loaded machine too).
const EXPECTED: Duration = Duration::from_secs(5);

async fn serve(state: Arc<AppState>) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let app = Router::new()
        .route("/api/events", any(handle_events))
        .with_state(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    address
}

async fn connect(address: std::net::SocketAddr, token: &str, stream: Option<&str>) -> Client {
    let stream = stream
        .map(|name| format!("&stream={name}"))
        .unwrap_or_default();
    let url = format!(
        "ws://{address}/api/events?authToken={token}&protocolVersion={GXSERVER_PROTOCOL_VERSION}{stream}"
    );
    tokio_tungstenite::connect_async(url)
        .await
        .expect("connect")
        .0
}

/// The next application frame's `type`, or `None` when nothing arrives within `wait`.
async fn type_within(client: &mut Client, wait: Duration) -> Option<String> {
    loop {
        let frame = tokio::time::timeout(wait, client.next()).await.ok()??;
        if let Ok(WsMessage::Text(text)) = frame {
            let value: Value = serde_json::from_str(text.as_str()).expect("json frame");
            return value["type"].as_str().map(str::to_string);
        }
    }
}

async fn next_type(client: &mut Client) -> Option<String> {
    type_within(client, EXPECTED).await
}

/// Every frame type until the socket stays quiet.
async fn drain(client: &mut Client) -> Vec<String> {
    let mut types = Vec::new();
    while let Some(event_type) = type_within(client, QUIET).await {
        types.push(event_type);
    }
    types
}

/// Asserts the socket delivers exactly `expected`: each expected frame may take `EXPECTED`,
/// then the socket must stay quiet for `QUIET`, so a slow frame is not mistaken for silence.
async fn expect_frames(client: &mut Client, expected: &[&str]) {
    let mut types = Vec::with_capacity(expected.len());
    while types.len() < expected.len() {
        let Some(event_type) = next_type(client).await else {
            break;
        };
        types.push(event_type);
    }
    types.extend(drain(client).await);
    assert_eq!(types, expected);
}

async fn send(client: &mut Client, message: Value) {
    client
        .send(WsMessage::Text(message.to_string().into()))
        .await
        .expect("send");
}

/// Everything the hub carries in normal use: a presentation delta, a chat frame, the model
/// catalog, a feed change and the daemon stopping.
fn broadcast_mixed_events(state: &AppState) {
    state
        .event_hub
        .broadcast(json!({ "type": "presentationDelta", "revision": 2 }));
    state.event_hub.broadcast(json!({
        "type": "sessionChatAppended", "projectId": "P-stream", "sessionId": "S-stream",
    }));
    state.event_hub.broadcast(json!({
        "type": crate::agent_model_catalog::AGENT_MODEL_CATALOG_CHANGED_EVENT_TYPE,
    }));
    crate::notification_feed::broadcast_notification_feed_changed(state);
    broadcast_server_stopping(state);
}

#[tokio::test]
async fn notification_stream_keeps_the_handshake_and_only_feed_and_lifecycle_events() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let address = serve(state.clone()).await;
    let mut client = connect(address, &state.auth_token, Some("notifications")).await;

    assert_eq!(
        next_type(&mut client).await.as_deref(),
        Some("eventStreamReady")
    );
    // Commands a full socket answers start nothing here: no snapshot, no chat follower.
    send(
        &mut client,
        json!({ "type": "subscribePresentation", "rendererCommands": true }),
    )
    .await;
    send(
        &mut client,
        json!({ "type": "subscribeSessionChat", "projectId": "P-stream", "sessionId": "S-stream" }),
    )
    .await;
    send(&mut client, json!({ "type": "not json at all" })).await;
    client
        .send(WsMessage::Text("{ malformed".into()))
        .await
        .expect("send malformed");
    tokio::time::sleep(QUIET).await;
    broadcast_mixed_events(&state);

    expect_frames(&mut client, &["notificationFeedChanged", "serverStopping"]).await;
    // The socket is still open after the ignored commands.
    crate::notification_feed::broadcast_notification_feed_changed(&state);
    assert_eq!(
        next_type(&mut client).await.as_deref(),
        Some("notificationFeedChanged")
    );
}

#[tokio::test]
async fn the_full_and_chat_streams_are_unchanged() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let address = serve(state.clone()).await;

    let mut full = connect(address, &state.auth_token, None).await;
    assert_eq!(
        next_type(&mut full).await.as_deref(),
        Some("eventStreamReady")
    );
    // The model catalog may follow when this process has one; the snapshot comes after it.
    let _ = drain(&mut full).await;
    send(&mut full, json!({ "type": "subscribePresentation" })).await;
    assert_eq!(
        next_type(&mut full).await.as_deref(),
        Some("presentationSnapshot")
    );

    let mut chat = connect(address, &state.auth_token, Some("sessionChat")).await;
    assert_eq!(
        next_type(&mut chat).await.as_deref(),
        Some("eventStreamReady")
    );
    send(
        &mut chat,
        json!({ "type": "subscribeSessionChat", "projectId": "P-stream", "sessionId": "S-stream" }),
    )
    .await;
    tokio::time::sleep(QUIET).await;
    let _ = drain(&mut chat).await;
    broadcast_mixed_events(&state);

    expect_frames(
        &mut full,
        &[
            "presentationDelta",
            "sessionChatAppended",
            crate::agent_model_catalog::AGENT_MODEL_CATALOG_CHANGED_EVENT_TYPE,
            "notificationFeedChanged",
            "serverStopping",
        ],
    )
    .await;
    expect_frames(
        &mut chat,
        &[
            "sessionChatAppended",
            crate::agent_model_catalog::AGENT_MODEL_CATALOG_CHANGED_EVENT_TYPE,
        ],
    )
    .await;
}

#[tokio::test]
async fn the_notification_stream_keeps_the_auth_and_protocol_gates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let address = serve(state.clone()).await;
    let status = |url: String| async move {
        match tokio_tungstenite::connect_async(url).await {
            Err(tungstenite::Error::Http(response)) => response.status().as_u16(),
            other => panic!("expected an HTTP refusal, got {:?}", other.map(|_| ())),
        }
    };
    assert_eq!(
        status(format!(
            "ws://{address}/api/events?protocolVersion={GXSERVER_PROTOCOL_VERSION}&stream=notifications"
        ))
        .await,
        401
    );
    assert_eq!(
        status(format!(
            "ws://{address}/api/events?authToken={}&protocolVersion=999&stream=notifications",
            state.auth_token
        ))
        .await,
        426
    );
}

#[tokio::test]
async fn a_closed_notification_socket_leaves_no_hub_subscriber() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let address = serve(state.clone()).await;
    let mut client = connect(address, &state.auth_token, Some("notifications")).await;
    assert_eq!(
        next_type(&mut client).await.as_deref(),
        Some("eventStreamReady")
    );
    assert_eq!(state.event_hub.subscriber_count(), 1);
    client.close(None).await.expect("close");
    drop(client);
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.event_hub.subscriber_count() > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(state.event_hub.subscriber_count(), 0);
}
