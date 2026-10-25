//! Authenticated byte transport for the desktop's machine-scoped browser proxy.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

const FRAME_BYTES: usize = 64 * 1024;
static CONNECTIONS: Semaphore = Semaphore::const_new(128);

/// CDXC:Browser 2026-09-23 WHY:
/// Windows OpenSSH reports a refused IPv6 forwarded connection as successful, so localhost never reaches an IPv4-only listener. The host's Rust TCP connector owns address selection instead; opaque bytes retain HTTP origins, TLS, WebSockets and cross-port requests without rewriting URLs or forcing an address family.
pub(crate) async fn handle_browser_tcp(
    State(state): State<Arc<AppState>>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
    uri: Uri,
) -> Response<Body> {
    let failure = |status, code, message| {
        json_response(status, rpc_error(code, message, Some(request_id(&headers))))
    };
    if !is_authorized_client_headers(&headers, &state.auth_token, &state.device_api_tokens) {
        return failure(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "gxserver authorization is required.",
        );
    }
    let version = read_protocol_version(&headers, &uri, None);
    if !is_expected_protocol_version(version.as_ref()) {
        return json_response(
            StatusCode::UPGRADE_REQUIRED,
            protocol_mismatch_error(version, Some(request_id(&headers))),
        );
    }
    let Ok(ws) = ws else {
        return failure(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "A browser TCP WebSocket is required.",
        );
    };
    let host = query_value(&uri, "host").unwrap_or_default();
    let port = query_value(&uri, "port").and_then(|value| value.parse::<u16>().ok());
    let valid_host = !host.is_empty()
        && host.len() <= 253
        && (host.parse::<std::net::IpAddr>().is_ok()
            || host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')));
    let Some(port) = port.filter(|port| *port != 0 && valid_host) else {
        return failure(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "The browser destination is invalid.",
        );
    };
    let Ok(permit) = CONNECTIONS.try_acquire() else {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "busy",
            "Too many browser connections are open.",
        );
    };
    let mut shutdown = state.shutdown_tx.subscribe();
    // The WebSocket upgrade acknowledges an actual connection, not merely a queued connect.
    let stream = tokio::select! {
        _ = shutdown.recv() => return failure(StatusCode::SERVICE_UNAVAILABLE, "unavailable", "The server is stopping."),
        result = tokio::time::timeout(Duration::from_secs(8), connect_browser_destination(&host, port)) => match result {
            Ok(Ok(stream)) => stream,
            _ => return failure(StatusCode::BAD_GATEWAY, "connectFailed", "The browser destination could not be reached."),
        },
    };
    let _ = stream.set_nodelay(true);
    ws.max_frame_size(FRAME_BYTES)
        .max_message_size(FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            tokio::select! {
                _ = shutdown.recv() => {},
                _ = relay(socket, stream) => {},
            }
        })
}

/// CDXC:Browser 2026-09-23 WHY:
/// Windows may take over two seconds to refuse the first localhost address. Race the resolver's preferred family against the other after 250 ms so an IPv4-only listener remains discoverable within the normal HTTP probe budget; keep resolver order within each family and at most two TCP attempts alive.
async fn connect_browser_destination(host: &str, port: u16) -> std::io::Result<TcpStream> {
    let mut addresses = tokio::net::lookup_host((host, port)).await?;
    let Some(first) = addresses.next() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "The browser destination resolved to no addresses.",
        ));
    };
    let mut preferred = vec![first];
    let mut alternate = Vec::new();
    for address in addresses {
        if address.is_ipv6() == first.is_ipv6() {
            preferred.push(address);
        } else {
            alternate.push(address);
        }
    }
    let preferred = connect_browser_addresses(preferred);
    if alternate.is_empty() {
        return preferred.await;
    }
    tokio::pin!(preferred);
    tokio::select! {
        result = &mut preferred => return match result {
            Ok(stream) => Ok(stream),
            Err(_) => connect_browser_addresses(alternate).await,
        },
        _ = tokio::time::sleep(Duration::from_millis(250)) => {},
    }
    let alternate = connect_browser_addresses(alternate);
    tokio::pin!(alternate);
    tokio::select! {
        result = &mut preferred => match result {
            Ok(stream) => Ok(stream),
            Err(_) => alternate.await,
        },
        result = &mut alternate => match result {
            Ok(stream) => Ok(stream),
            Err(_) => preferred.await,
        },
    }
}

async fn connect_browser_addresses(
    addresses: Vec<std::net::SocketAddr>,
) -> std::io::Result<TcpStream> {
    let mut last_error = std::io::Error::from(std::io::ErrorKind::NotFound);
    for address in addresses {
        match TcpStream::connect(address).await {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn relay(socket: WebSocket, stream: TcpStream) -> Result<()> {
    let (mut browser_tx, mut browser_rx) = socket.split();
    let (mut host_rx, mut host_tx) = stream.into_split();
    let upload = async {
        while let Some(message) = browser_rx.next().await {
            match message? {
                Message::Binary(bytes) => host_tx.write_all(&bytes).await?,
                Message::Text(text) if text.as_str() == "eof" => {
                    host_tx.shutdown().await?;
                    return Ok::<(), anyhow::Error>(());
                }
                Message::Ping(_) | Message::Pong(_) => {}
                _ => return Err(anyhow!("Browser connection closed.")),
            }
        }
        Err(anyhow!("Browser connection closed."))
    };
    let download = async {
        let mut buffer = [0u8; 32 * 1024];
        loop {
            let count = host_rx.read(&mut buffer).await?;
            if count == 0 {
                browser_tx.send(Message::Text("eof".into())).await?;
                return Ok::<(), anyhow::Error>(());
            }
            browser_tx
                .send(Message::Binary(buffer[..count].to_vec().into()))
                .await?;
        }
    };
    tokio::try_join!(upload, download)?;
    Ok(())
}
