//! Which tokens open gxserver's sockets (the routes outside the HTTP gate, #495): the token file
//! and the desktop's `gxo_` owner session open all of them; a session's own `gxs_` token opens only
//! the two the `ghostex` CLI inside a session uses (`ghostex voice`, `ghostex browser mcp`); a
//! revoked or made-up token opens none.

use super::*;
use axum::{
    routing::{any, get},
    Router,
};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest, Message as WsMessage};

const EXPECTED: Duration = Duration::from_secs(5);

async fn serve(state: Arc<AppState>) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let app = Router::new()
        .route("/api/events", any(handle_events))
        .route("/api/terminal", any(handle_terminal))
        .route("/api/browserTcp", any(browser_tcp::handle_browser_tcp))
        .route("/api/voice", any(crate::voice_agent::handle_voice))
        .route(
            "/api/cefDevtools/host",
            any(cef_devtools::handle_cef_devtools_host),
        )
        .route(
            "/api/cefDevtools/json",
            get(cef_devtools::handle_cef_devtools_pages),
        )
        .route(
            "/api/cefDevtools/page/{page_id}",
            any(cef_devtools::handle_cef_devtools_page),
        )
        .with_state(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    address
}

/// 101 when the socket opened, else the HTTP status gxserver refused it with. The token goes in
/// the `Authorization` header, as the desktop and the CLI send it.
async fn status(address: std::net::SocketAddr, path_and_query: &str, token: &str) -> u16 {
    let mut request = format!("ws://{address}{path_and_query}")
        .into_client_request()
        .expect("request");
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("header"),
    );
    match tokio_tungstenite::connect_async(request).await {
        Ok(_) => 101,
        Err(tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(other) => panic!("{path_and_query}: {other}"),
    }
}

/// The `/api/terminal` socket checks its `authToken` after the upgrade: the first frame's error
/// code, or `None` when the socket said nothing that soon.
async fn terminal_error(address: std::net::SocketAddr, token: &str) -> Option<String> {
    let url = format!(
        "ws://{address}/api/terminal?authToken={token}&protocolVersion={GXSERVER_PROTOCOL_VERSION}"
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("terminal upgrade");
    loop {
        let frame = tokio::time::timeout(EXPECTED, socket.next()).await.ok()??;
        if let Ok(WsMessage::Text(text)) = frame {
            let value: Value = serde_json::from_str(text.as_str()).expect("json frame");
            return value["code"].as_str().map(str::to_string);
        }
    }
}

#[tokio::test]
async fn each_socket_opens_for_exactly_the_tokens_its_callers_hold() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().to_path_buf())));
    let address = serve(state.clone()).await;
    let tokens = &state.security.session_tokens;
    let file = state.auth_token.clone();
    let owner = tokens.mint_owner("mac");
    let (session, _) = tokens.issue_session("P1", "S1", crate::security::SessionRole::Agent);
    let (ended, _) = tokens.issue_session("P1", "S2", crate::security::SessionRole::Agent);
    tokens.revoke_session("P1", "S2");
    let made_up_owner = format!("gxo_{}", "A".repeat(owner.len() - 4));
    let made_up_session = format!("gxs_{}", "A".repeat(session.len() - 4));
    let v = GXSERVER_PROTOCOL_VERSION;

    // (path, what an accepted token gets): 101 opened, or the refusal that comes after the
    // token check (503: no app is connected; 400: no browser destination).
    let everyone = [
        (
            format!("/api/voice?protocolVersion={v}&mode=dictation"),
            101,
        ),
        ("/api/cefDevtools/json".to_string(), 503),
        ("/api/cefDevtools/page/p1".to_string(), 503),
    ];
    let app_only = [
        (format!("/api/events?protocolVersion={v}"), 101),
        (format!("/api/browserTcp?protocolVersion={v}"), 400),
        ("/api/cefDevtools/host".to_string(), 101),
    ];
    for (path, opened) in everyone.iter().chain(app_only.iter()) {
        assert_eq!(
            status(address, path, &file).await,
            *opened,
            "token file on {path}"
        );
        assert_eq!(
            status(address, path, &owner).await,
            *opened,
            "owner on {path}"
        );
        for refused in [&ended, &made_up_owner, &made_up_session] {
            assert_eq!(
                status(address, path, refused).await,
                401,
                "{refused} on {path}"
            );
        }
    }
    for (path, opened) in &everyone {
        assert_eq!(
            status(address, path, &session).await,
            *opened,
            "session on {path}"
        );
    }
    for (path, _) in &app_only {
        assert_eq!(
            status(address, path, &session).await,
            401,
            "session on {path}"
        );
    }

    let unauthorized = Some("unauthorized".to_string());
    assert_ne!(terminal_error(address, &file).await, unauthorized);
    assert_ne!(terminal_error(address, &owner).await, unauthorized);
    for refused in [&session, &ended, &made_up_owner] {
        assert_eq!(
            terminal_error(address, refused).await,
            unauthorized,
            "{refused}"
        );
    }
}
