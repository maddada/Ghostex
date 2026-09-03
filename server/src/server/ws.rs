use super::*;
use crate::session_chat_follower::{
    subscribe_session_chat_follower, unsubscribe_session_chat_follower,
};

pub(crate) async fn handle_events(
    State(state): State<Arc<AppState>>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
) -> Response<Body> {
    let request_id = request_id(&headers);
    let Ok(ws) = ws else {
        let message = if method == Method::OPTIONS {
            format!("{} is not a gxserver HTTP endpoint.", uri.path())
        } else {
            format!(
                "No gxserver endpoint for {} {}.",
                method.as_str(),
                uri.path()
            )
        };
        let mut response = json_response(
            StatusCode::NOT_FOUND,
            rpc_error("notFound", message, Some(request_id)),
        );
        apply_cors_headers(&headers, &mut response, &state.config);
        return response;
    };
    /*
    CDXC:StateSync 2026-06-14-20:37:
    Browser WebSocket clients cannot set Authorization headers, so Rust keeps the TypeScript authToken query option and protocolVersion query/header gate for /api/events.

    CDXC:ServerApi 2026-06-22-04:10:
    Plain HTTP requests to the WebSocket-only event path must keep the TypeScript JSON `notFound` envelope. Do not expose Axum WebSocket extractor rejection bodies because clients rely on gxserver's product/protocol/requestId error shape.
    */
    let query_token = query_value(&uri, "authToken");
    let authorized = is_authorized_headers(&headers, &state.auth_token)
        || query_token
            .as_deref()
            .map(|token| is_expected_gxserver_auth_token(token, &state.auth_token))
            .unwrap_or(false);
    if !authorized {
        return json_response(
            StatusCode::UNAUTHORIZED,
            rpc_error(
                "unauthorized",
                "gxserver auth token is required for this endpoint.",
                Some(request_id),
            ),
        );
    }
    let protocol_version = read_protocol_version(&headers, &uri, None);
    if !is_expected_protocol_version(protocol_version.as_ref()) {
        return json_response(
            StatusCode::UPGRADE_REQUIRED,
            protocol_mismatch_error(protocol_version, Some(request_id)),
        );
    }
    ws.on_upgrade(move |socket| handle_event_socket(socket, state))
}

pub(crate) async fn handle_terminal(
    State(state): State<Arc<AppState>>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
) -> Response<Body> {
    let Ok(ws) = ws else {
        let request_id = request_id(&headers);
        let message = if method == Method::OPTIONS {
            format!("{} is not a gxserver HTTP endpoint.", uri.path())
        } else {
            format!(
                "No gxserver endpoint for {} {}.",
                method.as_str(),
                uri.path()
            )
        };
        let mut response = json_response(
            StatusCode::NOT_FOUND,
            rpc_error("notFound", message, Some(request_id)),
        );
        apply_cors_headers(&headers, &mut response, &state.config);
        return response;
    };
    let terminal_state = TerminalWsState {
        auth_token: state.auth_token.clone(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
        paths: state.paths.clone(),
        server_id: state.metadata.server_id.clone(),
    };
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, uri, terminal_state))
}

pub(crate) async fn handle_event_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = state.event_hub.client_channel();
    let mut broadcast_rx = state.event_hub.subscribe();
    if outbound_tx
        .try_send(json!({
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "serverId": state.metadata.server_id.clone(),
            "type": "eventStreamReady",
        }))
        .is_err()
    {
        return;
    }
    /*
    Direct client events (including the subscription snapshot) and hub
    broadcasts must enter one FIFO before socket delivery. A separate unbiased
    select over two ready receivers can send revision R+1 before an already
    queued snapshot R. Forward broadcasts into the client queue instead; the
    snapshot enqueue is sequenced with presentation producers below, so a
    later revision cannot enter this queue first.
    */
    let broadcast_outbound_tx = outbound_tx.clone();
    let broadcast_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(event) => {
                    if broadcast_outbound_tx.try_send(event).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    /*
                    A lagged hub receiver has already lost ordered events.
                    Trigger the same explicit disconnect as a full client FIFO
                    so the client resubscribes from an authoritative snapshot.
                    */
                    broadcast_outbound_tx.signal_overflow();
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    let sender_task = tokio::spawn(async move {
        while let Some(event) = outbound_rx.recv().await {
            if send_event_message(&mut socket_sender, event).await.is_err() {
                break;
            }
        }
    });
    let mut renderer_client_id: Option<String> = None;
    let mut session_chat_subscriptions: HashSet<(String, String)> = HashSet::new();
    loop {
        let message = tokio::select! {
            _ = outbound_tx.wait_for_overflow() => break,
            message = socket_receiver.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let Ok(message) = message else {
            break;
        };
        if !handle_event_client_message(
            &state,
            &outbound_tx,
            &mut renderer_client_id,
            &mut session_chat_subscriptions,
            message,
        )
        .await
        {
            break;
        }
    }
    if let Some(client_id) = renderer_client_id {
        state.event_hub.unregister_renderer_client(&client_id).await;
    }
    // A socket that disappears without unsubscribing must release its
    // session-chat follower refcounts, or followers would tail forever.
    for (project_id, session_id) in session_chat_subscriptions.drain() {
        unsubscribe_session_chat_follower(&state, &project_id, &session_id);
    }
    drop(outbound_tx);
    broadcast_task.abort();
    sender_task.abort();
    let _ = broadcast_task.await;
    let _ = sender_task.await;
}

pub(crate) async fn send_event_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    event: Value,
) -> std::result::Result<(), axum::Error> {
    sender
        .send(Message::Text(format!("{event}\n").into()))
        .await
}

pub(crate) async fn handle_event_client_message(
    state: &AppState,
    outbound_tx: &EventClientSender,
    renderer_client_id: &mut Option<String>,
    session_chat_subscriptions: &mut HashSet<(String, String)>,
    message: Message,
) -> bool {
    let Some(parsed) = parse_event_client_message(message) else {
        return true;
    };
    match parsed.get("type").and_then(Value::as_str) {
        Some("rendererCommandResult") => {
            state
                .event_hub
                .handle_renderer_command_result(&parsed)
                .await;
            true
        }
        Some("subscribePresentation") => {
            if parsed.get("rendererCommands").and_then(Value::as_bool) == Some(true)
                && renderer_client_id.is_none()
            {
                let client_id = format!("renderer-client-{}", Uuid::new_v4());
                *renderer_client_id = Some(
                    state
                        .event_hub
                        .register_renderer_client(client_id, outbound_tx.clone())
                        .await,
                );
            }
            send_presentation_snapshot_for_subscription(state, outbound_tx, &parsed)
        }
        Some("subscribeSessionChat") => {
            if let Some((project_id, session_id)) = event_message_session_ids(&parsed) {
                let limit = parsed
                    .get("limit")
                    .and_then(Value::as_i64)
                    .map(|value| {
                        value.clamp(0, crate::session_chat::SESSION_CHAT_MAX_LIMIT as i64) as usize
                    })
                    .unwrap_or(crate::session_chat::SESSION_CHAT_INITIAL_LIMIT);
                // A duplicate subscribe on the same socket is not a new
                // subscriber, but still gets a fresh authoritative snapshot
                // (and can still raise the follower's window).
                let new_subscriber =
                    session_chat_subscriptions.insert((project_id.clone(), session_id.clone()));
                subscribe_session_chat_follower(
                    state,
                    &project_id,
                    &session_id,
                    limit,
                    new_subscriber,
                );
            }
            true
        }
        Some("unsubscribeSessionChat") => {
            if let Some((project_id, session_id)) = event_message_session_ids(&parsed) {
                if session_chat_subscriptions.remove(&(project_id.clone(), session_id.clone())) {
                    unsubscribe_session_chat_follower(state, &project_id, &session_id);
                }
            }
            true
        }
        _ => true,
    }
}

pub(crate) fn event_message_session_ids(parsed: &Map<String, Value>) -> Option<(String, String)> {
    let project_id = parsed
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let session_id = parsed
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some((project_id.to_string(), session_id.to_string()))
}

pub(crate) fn parse_event_client_message(message: Message) -> Option<Map<String, Value>> {
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok()?,
        Message::Close(_) => return None,
        Message::Ping(_) | Message::Pong(_) => return Some(Map::new()),
    };
    serde_json::from_str::<Value>(&text)
        .ok()?
        .as_object()
        .cloned()
}

pub(crate) fn send_presentation_snapshot_for_subscription(
    state: &AppState,
    outbound_tx: &EventClientSender,
    parsed: &Map<String, Value>,
) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return true;
    };
    /*
    CDXC:StateSync 2026-09-01:
    Every client fetches `/api/readPresentationSnapshot` over HTTP and *then*
    opens this socket quoting the revision it just applied, so the snapshot
    this handler used to build unconditionally was, in the overwhelmingly
    common case, a second full projection of state the client already had —
    paid for on every reconnect, including the flap-y ones.

    A subscriber that names the exact revision the daemon is on has missed
    nothing, so answer it with the revision alone. Anything else takes the
    full-snapshot path unchanged: no `lastRevision`, a malformed one, a client
    that is behind (it missed deltas), and a client that is somehow ahead.

    The revision is read while holding the producer sequencer, for the same
    reason the snapshot below is built while holding it: once this "you are
    current at R" acknowledgement is in the client's FIFO, producers may
    publish R+1 into that same FIFO, never ahead of it.
    */
    if let Some(last_revision) = parsed
        .get("lastRevision")
        .and_then(Value::as_i64)
        .filter(|revision| *revision > 0)
    {
        if let Ok(_event_sequence) = lock_presentation_event_sequence(state) {
            if crate::presentation::read_presentation_revision(&db).ok() == Some(last_revision) {
                let mut event = Map::new();
                if let Some(client_id) = parsed.get("clientId").and_then(Value::as_str) {
                    event.insert("clientId".to_string(), Value::String(client_id.to_string()));
                }
                event.insert(
                    "protocolVersion".to_string(),
                    Value::Number(serde_json::Number::from(GXSERVER_PROTOCOL_VERSION)),
                );
                event.insert(
                    "revision".to_string(),
                    Value::Number(serde_json::Number::from(last_revision)),
                );
                event.insert(
                    "serverId".to_string(),
                    Value::String(state.metadata.server_id.clone()),
                );
                event.insert(
                    "type".to_string(),
                    Value::String("presentationSnapshotCurrent".to_string()),
                );
                return outbound_tx.try_send(Value::Object(event)).is_ok();
            }
        }
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    /*
    CDXC:StateSync 2026-09-01:
    One `list_sessions` feeds both sync passes and the snapshot projection.
    The passes can mutate rows, so re-read only when one of them reports an
    actual change.
    */
    let sessions = repository.list_sessions(None).unwrap_or_default();
    let sidecars_changed =
        sync_session_state_sidecars(state, &db, &repository, &sessions, "presentation-subscribe")
            .unwrap_or(false);
    let identities_changed = sync_live_zmx_process_identities(
        state,
        &db,
        &repository,
        &sessions,
        None,
        "presentation-subscribe",
    )
    .unwrap_or(false);
    let sessions = if sidecars_changed || identities_changed {
        repository.list_sessions(None).unwrap_or(sessions)
    } else {
        sessions
    };
    /*
    Own the producer sequencer at this delivery boundary rather than calling
    read_presentation_snapshot_in_sequence and dropping its guard before the
    event is queued. Once this snapshot R enters the client's single FIFO,
    producers may publish R+1 into that same FIFO, never ahead of it.
    */
    let auto_settle_after_days = session_lifecycle::read_sweep_auto_settle_after_days(&state.paths);
    let sidebar_v2_selected = session_lifecycle::read_sidebar_v2_selected(&state.paths);
    let Ok(_event_sequence) = lock_presentation_event_sequence(state) else {
        return true;
    };
    let Ok(snapshot) = read_presentation_snapshot(
        &db,
        &state.metadata.server_id,
        auto_settle_after_days,
        sidebar_v2_selected,
        sessions,
    ) else {
        return true;
    };
    let revision = snapshot
        .get("revision")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let mut event = Map::new();
    if let Some(client_id) = parsed.get("clientId").and_then(Value::as_str) {
        event.insert("clientId".to_string(), Value::String(client_id.to_string()));
    }
    event.insert(
        "protocolVersion".to_string(),
        Value::Number(serde_json::Number::from(GXSERVER_PROTOCOL_VERSION)),
    );
    event.insert(
        "revision".to_string(),
        Value::Number(serde_json::Number::from(revision)),
    );
    event.insert(
        "serverId".to_string(),
        Value::String(state.metadata.server_id.clone()),
    );
    event.insert("snapshot".to_string(), snapshot);
    event.insert(
        "type".to_string(),
        Value::String("presentationSnapshot".to_string()),
    );
    outbound_tx.try_send(Value::Object(event)).is_ok()
}
